//! The words U4Ai shows, in each language: a JSON file each in
//! `core/locales`, built in, nested objects whose leaves are strings looked
//! up by their dotted path ("menu.refresh"). `{name}` in a string is a value
//! [`tf`] fills in. A key a language lacks falls back to English, and one
//! English lacks shows as the key itself (the tests make sure neither
//! happens). The Mac app's five languages, with its translations where it
//! has the words. Flow's i18n, made smaller.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// The languages: code and file. English comes first and is what the others
/// fall back to.
const FILES: [(&str, &str); 5] = [
    ("en", include_str!("../locales/en.json")),
    ("ja", include_str!("../locales/ja.json")),
    ("ko", include_str!("../locales/ko.json")),
    ("zh-Hans", include_str!("../locales/zh-Hans.json")),
    ("zh-Hant", include_str!("../locales/zh-Hant.json")),
];

/// What the settings' `language` holds to follow the system's languages.
pub const AUTO: &str = "auto";

static CURRENT: AtomicUsize = AtomicUsize::new(0);

// Tests run side by side on their own threads, so each keeps its own
// language rather than switching everyone's.
#[cfg(test)]
thread_local! {
    static TEST_LANGUAGE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

type Table = HashMap<String, String>;

fn tables() -> &'static [Table] {
    static TABLES: OnceLock<Vec<Table>> = OnceLock::new();
    TABLES.get_or_init(|| FILES.iter().map(|(code, json)| parse(code, json)).collect())
}

fn parse(code: &str, json: &str) -> Table {
    fn flatten(prefix: &str, value: &serde_json::Value, out: &mut Table) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                    flatten(&key, v, out);
                }
            }
            serde_json::Value::String(s) => {
                out.insert(prefix.to_string(), s.clone());
            }
            other => log::warn!("{prefix} in a locale file is {other}, not a string"),
        }
    }
    let mut table = Table::new();
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => flatten("", &value, &mut table),
        Err(err) => log::warn!("locales/{code}.json doesn't parse: {err}"),
    }
    table
}

fn current() -> usize {
    #[cfg(test)]
    if let Some(i) = TEST_LANGUAGE.get() {
        return i;
    }
    CURRENT.load(Ordering::Relaxed)
}

/// The language in use.
pub fn language() -> &'static str {
    FILES[current()].0
}

/// Which of the languages a system's language tag ("ja-JP", "zh_TW.UTF-8",
/// "zh-Hans-CN") is: Chinese by its script where it says one, else by its
/// region (Taiwan, Hong Kong and Macau write Traditional).
fn index_of(tag: &str) -> Option<usize> {
    let tag = tag.split('.').next().unwrap_or("").replace('_', "-").to_ascii_lowercase();
    let mut parts = tag.split('-');
    let code = match parts.next()? {
        "zh" => {
            let rest: Vec<&str> = parts.collect();
            if rest.contains(&"hant") || rest.iter().any(|r| matches!(*r, "tw" | "hk" | "mo")) && !rest.contains(&"hans") {
                "zh-Hant"
            } else {
                "zh-Hans"
            }
        }
        other => other,
    };
    FILES.iter().position(|(c, _)| c.eq_ignore_ascii_case(code))
}

/// Picks the language: `setting` is a code, or [`AUTO`] for the first of
/// `preferred` (the system's languages, most wanted first) there are words
/// for, else English.
pub fn set_language(setting: &str, preferred: &[String]) {
    let i = if setting == AUTO { preferred.iter().find_map(|p| index_of(p)) } else { index_of(setting) }.unwrap_or(0);
    #[cfg(test)]
    TEST_LANGUAGE.set(Some(i));
    #[cfg(not(test))]
    CURRENT.store(i, Ordering::Relaxed);
}

/// The words for `key` in the language in use.
pub fn t(key: &str) -> &'static str {
    let tables = tables();
    match tables[current()].get(key).or_else(|| tables[0].get(key)) {
        Some(s) => s,
        None => missing(key),
    }
}

/// [`t`] with each `{name}` filled in from `args`.
pub fn tf(key: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut s = t(key).to_string();
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), &value.to_string());
    }
    s
}

/// A key no language has: shown as itself, kept once so it can be returned
/// for good, and noted for the tests.
fn missing(key: &str) -> &'static str {
    #[cfg(test)]
    MISSED.with(|m| m.borrow_mut().push(key.to_string()));
    static MISSING: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut seen = MISSING.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = seen.get(key) {
        return s;
    }
    log::warn!("no words for {key}");
    let s: &'static str = Box::leak(key.to_string().into_boxed_str());
    seen.insert(key.to_string(), s);
    s
}

#[cfg(test)]
thread_local! {
    static MISSED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The keys this thread looked up that no language has, for tests.
#[cfg(test)]
pub fn missed() -> Vec<String> {
    MISSED.with(|m| m.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<&str> {
        s.split('{').skip(1).filter_map(|rest| rest.split_once('}').map(|(name, _)| name)).collect()
    }

    #[test]
    fn every_language_has_every_english_key_with_the_same_placeholders() {
        let tables = tables();
        let english = &tables[0];
        assert!(english.len() > 40, "{}", english.len());
        for (i, (code, _)) in FILES.iter().enumerate().skip(1) {
            let table = &tables[i];
            let lacking: Vec<_> = english.keys().filter(|k| !table.contains_key(*k)).collect();
            assert!(lacking.is_empty(), "{code} lacks {lacking:?}");
            let extra: Vec<_> = table.keys().filter(|k| !english.contains_key(*k)).collect();
            assert!(extra.is_empty(), "{code} has keys English doesn't: {extra:?}");
            for (key, words) in table {
                assert_eq!(placeholders(words), placeholders(&english[key]), "{code} {key}");
            }
        }
    }

    /// Every "section.key" string in the sources, for a section the
    /// language files have, is a key English has.
    #[test]
    fn the_keys_in_the_sources_are_in_english() {
        let sections: BTreeSet<&str> = tables()[0].keys().filter_map(|k| k.split('.').next()).collect();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        let mut dirs = vec![root.join("src"), root.join("../linux/src"), root.join("../windows/src")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        let mut checked = 0;
        for file in &files {
            let source = std::fs::read_to_string(file).unwrap();
            for literal in source.split('"').skip(1).step_by(2) {
                let section = literal.split('.').next().unwrap_or("");
                let keyish = literal.contains('.') && literal.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
                if matches!(literal.rsplit('.').next(), Some("png" | "json" | "log" | "txt" | "xml" | "rs" | "desktop" | "lock" | "sh")) {
                    continue;
                }
                if keyish && sections.contains(section) {
                    assert!(tables()[0].contains_key(literal), "{} uses {literal}, which en.json lacks", file.display());
                    checked += 1;
                }
            }
        }
        assert!(checked > 30, "only {checked} keys found in the sources");
    }

    #[test]
    fn automatic_follows_the_first_system_language_there_are_words_for() {
        let langs = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        set_language(AUTO, &langs(&["ja-JP", "en-US"]));
        assert_eq!(language(), "ja");
        set_language(AUTO, &langs(&["fr-FR", "ko_KR.UTF-8"]));
        assert_eq!(language(), "ko");
        set_language(AUTO, &langs(&["zh_TW.UTF-8"]));
        assert_eq!(language(), "zh-Hant");
        set_language(AUTO, &langs(&["zh-Hant-HK"]));
        assert_eq!(language(), "zh-Hant");
        set_language(AUTO, &langs(&["zh-CN"]));
        assert_eq!(language(), "zh-Hans");
        set_language(AUTO, &langs(&["zh-Hans-HK"]));
        assert_eq!(language(), "zh-Hans", "a script said wins over the region");
        set_language(AUTO, &langs(&["fr-FR"]));
        assert_eq!(language(), "en");
        set_language("ja", &langs(&["ko-KR"]));
        assert_eq!(language(), "ja");
    }

    #[test]
    fn words_come_in_the_language_in_use_with_their_values_filled_in() {
        set_language("ja", &[]);
        assert_eq!(t("menu.refresh"), "更新");
        set_language("en", &[]);
        assert_eq!(tf("notification.warning_body", &[("limit", &"5-Hour Limit"), ("percent", &92)]), "5-Hour Limit has reached 92% usage");
        assert!(missed().is_empty());
        assert_eq!(t("nowhere.at_all"), "nowhere.at_all");
        assert_eq!(missed(), ["nowhere.at_all"]);
    }
}
