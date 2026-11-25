//! Lists the Rust crates compiled into u4ai, with their licenses, for a
//! release's THIRD-PARTY-NOTICES.txt. linux/scripts/package.sh puts a title
//! in front of it, and windows/scripts/release.ps1 the notice of
//! WinSparkle.dll, which the installer carries.
//!
//!   cargo run --example third_party_notices -- --target x86_64-unknown-linux-gnu \
//!       [--target <triple>]... [--binary u4ai] --out notices.txt
//!
//! The crates are those the workspace's root package (u4ai) uses on the
//! targets named, through normal dependencies: the OS crate of each, never
//! the other's.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context};
use serde_json::Value;

fn main() -> anyhow::Result<()> {
    let mut targets = Vec::new();
    let (mut out, mut binary) = (None, "u4ai".to_string());
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--target" => targets.push(args.next().context("--target needs a value")?),
            "--out" => out = Some(args.next().context("--out needs a value")?),
            "--binary" => binary = args.next().context("--binary needs a value")?,
            other => bail!("unknown argument {other}"),
        }
    }
    let (Some(out), false) = (out, targets.is_empty()) else {
        bail!("usage: third_party_notices --target <triple> [--target <triple>]... [--binary <name>] --out <file>");
    };

    // name+version -> package; the crates are the union over the targets.
    let mut crates: BTreeMap<(String, String), Value> = BTreeMap::new();
    for target in &targets {
        let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["metadata", "--format-version", "1", "--locked", "--filter-platform", target, "--manifest-path"])
            .arg(&manifest)
            .output()
            .context("running cargo metadata")?;
        if !output.status.success() {
            bail!("cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        let meta: Value = serde_json::from_slice(&output.stdout)?;
        // Our own workspace crates (u4ai-core, u4ai-linux) have no
        // source registry: leave them out.
        for p in shipped(&meta)?.into_iter().filter(|p| !p["source"].is_null()) {
            let key = (str_field(&p, "name"), str_field(&p, "version"));
            crates.insert(key, p);
        }
    }

    // Identical license texts (most Apache-2.0 copies) are printed once and numbered.
    let mut text_index: HashMap<String, usize> = HashMap::new();
    let mut texts: Vec<(String, Vec<String>)> = Vec::new();
    let mut list = Vec::new();
    for ((name, version), p) in &crates {
        let dir = Path::new(p["manifest_path"].as_str().unwrap_or_default()).parent().map(Path::to_path_buf);
        let mut refs = Vec::new();
        let mut files: Vec<_> = dir
            .into_iter()
            .filter_map(|d| std::fs::read_dir(d).ok())
            .flatten()
            .flatten()
            .filter(|e| e.path().is_file() && is_license_file(&e.file_name().to_string_lossy()))
            .map(|e| e.path())
            .collect();
        files.sort();
        for file in files {
            let Ok(raw) = std::fs::read_to_string(&file) else { continue };
            let text = raw.replace("\r\n", "\n").lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            let n = *text_index.entry(text.clone()).or_insert_with(|| {
                texts.push((text, Vec::new()));
                texts.len()
            });
            texts[n - 1].1.push(format!("{name} {version}"));
            if !refs.contains(&n) {
                refs.push(n);
            }
        }
        let source = p["repository"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("https://crates.io/crates/{name}/{version}"));
        let license = p["license"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| format!("see {}", p["license_file"].as_str().unwrap_or("the crate")));
        list.push((format!("{name} {version}"), license, source, refs));
    }
    // A crate that ships no license text but may be had under Apache-2.0 is
    // pointed at that license's own text, which names no author, as another
    // crate shipped it. MIT and the like carry their authors' copyright, so
    // they can't be borrowed.
    let apache = texts.iter().position(|(text, _)| text.starts_with("Apache License") && text.contains("Version 2.0, January 2004"));
    let list: Vec<String> = list
        .into_iter()
        .map(|(crate_, license, source, mut refs)| {
            let borrowed = refs.is_empty() && license.split([' ', '/', '(', ')']).any(|id| id == "Apache-2.0");
            let text_line = match apache {
                Some(n) if borrowed => {
                    texts[n].1.push(format!("{crate_} (which ships no text of its own)"));
                    refs.push(n + 1);
                    format!("[{}], the license's own text (none in the published crate)", n + 1)
                }
                _ if refs.is_empty() => "not in the published crate; see Source".to_string(),
                _ => refs.iter().map(|n| format!("[{n}]")).collect::<Vec<_>>().join(" "),
            };
            format!("{crate_}\n    License: {license}\n    Source:  {source}\n    Text:    {text_line}\n")
        })
        .collect();

    let rule = "-".repeat(79);
    let mut doc = format!("{rule}\nRust crates compiled into {binary} ({})\n{rule}\n\n", crates.len());
    for entry in &list {
        doc.push_str(entry);
        doc.push('\n');
    }
    doc.push_str(&format!("{rule}\nLicense texts (numbered as in the Text: lines above)\n{rule}\n"));
    for (i, (text, users)) in texts.iter().enumerate() {
        doc.push_str(&format!("\n[{}] {}\n\n{text}\n", i + 1, users.join(", ")));
    }
    std::fs::write(&out, doc).with_context(|| format!("writing {out}"))?;
    println!("Wrote {out} ({} crates, {} license texts)", crates.len(), texts.len());
    Ok(())
}

/// Packages reached from the workspace's root package through normal
/// dependencies. Build and dev dependencies don't ship, and neither do
/// proc-macro crates or what only they pull in.
fn shipped(meta: &Value) -> anyhow::Result<Vec<Value>> {
    let packages: HashMap<&str, &Value> = meta["packages"]
        .as_array()
        .context("no packages in cargo metadata")?
        .iter()
        .map(|p| (p["id"].as_str().unwrap_or_default(), p))
        .collect();
    let nodes: HashMap<&str, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .context("no resolve graph in cargo metadata")?
        .iter()
        .map(|n| (n["id"].as_str().unwrap_or_default(), n))
        .collect();
    let root = meta["resolve"]["root"].as_str().context("no root package")?;
    let is_proc_macro = |id: &str| {
        packages.get(id).and_then(|p| p["targets"].as_array()).is_some_and(|targets| {
            targets.iter().any(|t| t["kind"].as_array().is_some_and(|k| k.iter().any(|k| k == "proc-macro")))
        })
    };
    let (mut seen, mut queue) = (HashSet::new(), VecDeque::from([root]));
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        for dep in nodes.get(id).and_then(|n| n["deps"].as_array()).into_iter().flatten() {
            let normal = dep["dep_kinds"].as_array().is_some_and(|kinds| kinds.iter().any(|k| k["kind"].is_null()));
            let pkg = dep["pkg"].as_str().unwrap_or_default();
            if normal && !is_proc_macro(pkg) {
                queue.push_back(pkg);
            }
        }
    }
    seen.remove(root);
    Ok(seen.into_iter().filter_map(|id| packages.get(id).map(|p| (*p).clone())).collect())
}

fn is_license_file(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE", "PATENTS"].iter().any(|p| upper.starts_with(p))
}

fn str_field(p: &Value, key: &str) -> String {
    p[key].as_str().unwrap_or_default().to_string()
}
