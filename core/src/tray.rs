//! What the tray shows, as data each OS's tray draws its own way: the
//! rings, the tooltip, and the menu (whose usage it is, each limit, the
//! provider's own lines, Refresh, the accounts, signing in with each
//! provider and out, Start at login, the update, each provider's status
//! page, the version and Quit). Made afresh from how things stand, so the
//! tests read it without a tray.

use jiff::Timestamp;

use crate::config::Account;
use crate::i18n::{t, tf};
use crate::icon::{five_hour_color, seven_day_color, Ring, Rings};
use crate::platform::{Action, UpdateState, Updater};
use crate::providers::{Limit, Provider, ReadError, Reading, Window};

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// Words that do nothing.
    Text(String),
    Item { label: String, action: Action, enabled: bool },
    Check { label: String, action: Action, checked: bool },
    Separator,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrayView {
    pub rings: Rings,
    pub title: String,
    /// The tooltip's lines, under the title.
    pub tooltip: Vec<String>,
    pub menu: Vec<Entry>,
}

/// How things stand, for the view.
pub struct Status<'a> {
    pub accounts: &'a [Account],
    pub current: Option<usize>,
    pub reading: Option<&'a Reading>,
    pub error: Option<&'a ReadError>,
    /// A read is under way with nothing shown yet.
    pub loading: bool,
    pub signing_in: bool,
    pub updater: Updater,
    pub update: Option<&'a UpdateState>,
    pub start_at_login: Option<bool>,
    /// The outer ring's percentage drawn in the icon.
    pub percent_in_icon: bool,
    pub now: Timestamp,
}

/// An account as the menu names it: its provider, then its name.
pub fn account_label(account: &Account) -> String {
    tf("usage.account", &[("provider", &account.provider.name()), ("account", &account.name)])
}

/// How long until `resets_at`, as the Mac's compact rows say it.
pub fn left(resets_at: Option<Timestamp>, now: Timestamp) -> Option<String> {
    let seconds = resets_at?.as_second() - now.as_second();
    let (d, h, m) = (seconds / 86_400, seconds % 86_400 / 3600, seconds % 3600 / 60);
    Some(if seconds < 60 {
        t("usage.resets_soon").to_string()
    } else if seconds < 3600 {
        tf("usage.left_minutes", &[("m", &m)])
    } else if seconds < 86_400 {
        tf("usage.left_hours", &[("h", &h), ("m", &m)])
    } else {
        tf("usage.left_days", &[("d", &d), ("h", &h)])
    })
}

/// A limit's row: its label, how much is used, and when it starts again.
fn row(label: &str, limit: &Limit, now: Timestamp) -> String {
    let percent = limit.percent.round() as i64;
    match left(limit.resets_at, now) {
        Some(resets) => tf("usage.row_resets", &[("limit", &label), ("percent", &percent), ("resets", &resets)]),
        None => tf("usage.row", &[("limit", &label), ("percent", &percent)]),
    }
}

/// The rings for a reading: the short window outside and the long one
/// inside, or the long one alone where there's no short one; with
/// `percent`, the outer one's percentage in the middle.
pub fn rings(reading: Option<&Reading>, percent: bool) -> Rings {
    let Some(reading) = reading else { return Rings::default() };
    let ring = |limit: Limit, color: fn(f64) -> crate::icon::Rgb| Ring { fill: limit.percent / 100.0, color: color(limit.percent) };
    let long = reading.first(Window::Long).map(|m| ring(m.limit, seven_day_color));
    let number = reading.primary().filter(|_| percent).map(|m| m.limit.percent.clamp(0.0, 100.0).round() as u32);
    match reading.first(Window::Short) {
        Some(short) => Rings { outer: Some(ring(short.limit, five_hour_color)), inner: long, number },
        None => Rings { outer: long, inner: None, number },
    }
}

fn update_entries(updater: Updater, state: Option<&UpdateState>) -> Vec<Entry> {
    let check = || Entry::Item { label: t("menu.check_updates").into(), action: Action::CheckUpdates, enabled: true };
    match (updater, state) {
        (Updater::None, _) => Vec::new(),
        (Updater::Shows, _) | (Updater::Own, None | Some(UpdateState::Idle)) => vec![check()],
        (Updater::Own, Some(UpdateState::Checking)) => vec![Entry::Item { label: t("update.checking").into(), action: Action::CheckUpdates, enabled: false }],
        (Updater::Own, Some(UpdateState::UpToDate)) => vec![Entry::Text(tf("update.up_to_date", &[("version", &crate::version::display())])), check()],
        (Updater::Own, Some(UpdateState::Failed(why))) => vec![Entry::Text(why.clone()), check()],
        (Updater::Own, Some(UpdateState::Available { version })) => {
            vec![Entry::Item { label: tf("update.install", &[("version", version)]), action: Action::InstallUpdate, enabled: true }]
        }
        (Updater::Own, Some(UpdateState::Installing { version })) => {
            vec![Entry::Item { label: tf("update.installing", &[("version", version)]), action: Action::InstallUpdate, enabled: false }]
        }
        (Updater::Own, Some(UpdateState::Installed { version })) => {
            vec![Entry::Item { label: tf("update.restart", &[("version", version)]), action: Action::Restart, enabled: true }]
        }
    }
}

pub fn view(s: &Status) -> TrayView {
    let current = s.current.and_then(|i| s.accounts.get(i));
    let mut lines = Vec::new();
    if let Some(account) = current {
        lines.push(account_label(account));
    }
    if let Some(reading) = s.reading {
        for meter in &reading.meters {
            lines.push(row(&meter.label, &meter.limit, s.now));
        }
        lines.extend(reading.lines.iter().cloned());
    }
    let mut notes = Vec::new();
    if current.is_none() {
        notes.push(t("usage.not_signed_in").to_string());
    } else if let Some(error) = s.error {
        if s.reading.is_some() {
            notes.push(t("usage.cached").to_string());
        }
        notes.push(error.say());
    } else if s.reading.is_none() && s.loading {
        notes.push(t("usage.loading").to_string());
    }
    if s.signing_in {
        notes.push(t("sign_in.waiting").to_string());
    }

    let mut menu: Vec<Entry> = lines.iter().chain(&notes).cloned().map(Entry::Text).collect();
    menu.push(Entry::Separator);
    menu.push(Entry::Item { label: t("menu.refresh").into(), action: Action::Refresh, enabled: current.is_some() && !s.loading });
    menu.push(Entry::Separator);
    if s.accounts.len() > 1 {
        for (i, account) in s.accounts.iter().enumerate() {
            menu.push(Entry::Check { label: account_label(account), action: Action::Switch(i), checked: Some(i) == s.current });
        }
    }
    if s.signing_in {
        menu.push(Entry::Item { label: t("menu.cancel_sign_in").into(), action: Action::CancelSignIn, enabled: true });
    } else {
        for provider in Provider::ALL {
            menu.push(Entry::Item { label: tf("menu.sign_in", &[("provider", &provider.name())]), action: Action::SignIn(provider), enabled: true });
        }
    }
    if let Some(account) = current {
        menu.push(Entry::Item { label: tf("menu.sign_out", &[("account", &account.name)]), action: Action::SignOut, enabled: true });
    }
    menu.push(Entry::Separator);
    menu.push(Entry::Check { label: t("menu.show_percent").into(), action: Action::ShowPercent, checked: s.percent_in_icon });
    if let Some(on) = s.start_at_login {
        menu.push(Entry::Check { label: t("menu.start_at_login").into(), action: Action::StartAtLogin, checked: on });
    }
    menu.extend(update_entries(s.updater, s.update));
    for provider in Provider::ALL {
        menu.push(Entry::Item { label: tf("menu.status", &[("provider", &provider.name())]), action: Action::OpenStatus(provider), enabled: true });
    }
    menu.push(Entry::Separator);
    menu.push(Entry::Text(tf("menu.version", &[("version", &crate::version::display())])));
    menu.push(Entry::Item { label: t("menu.quit").into(), action: Action::Quit, enabled: true });

    let mut tooltip = lines;
    tooltip.extend(notes);
    TrayView { rings: rings(s.reading, s.percent_in_icon), title: crate::APP_NAME.into(), tooltip, menu }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::set_language;
    use crate::providers::Meter;

    fn now() -> Timestamp {
        "2026-11-25T10:00:00Z".parse().unwrap()
    }

    fn later(seconds: i64) -> Option<Timestamp> {
        Some(Timestamp::from_second(now().as_second() + seconds).unwrap())
    }

    fn reading() -> Reading {
        let meter = |key: &str, label: &str, window, percent, resets_at| Meter { key: key.into(), label: label.into(), window, limit: Limit { percent, resets_at } };
        Reading {
            meters: vec![
                meter("five_hour", "5-Hour Limit", Window::Short, 42.4, later(2 * 3600 + 10 * 60)),
                meter("seven_day", "7-Day Limit", Window::Long, 76.0, later(3 * 86_400 + 4 * 3600)),
                meter("model:Fable", "7D Fable Limit", Window::Model, 13.0, None),
            ],
            lines: vec!["Extra Usage: $12.34 / $50".into()],
        }
    }

    fn accounts() -> Vec<Account> {
        vec![Account { provider: Provider::Claude, id: "1".into(), name: "a@example.com".into() }, Account { provider: Provider::Claude, id: "2".into(), name: "b@example.com".into() }]
    }

    fn status<'a>(accounts: &'a [Account], reading: Option<&'a Reading>) -> Status<'a> {
        Status {
            accounts,
            current: (!accounts.is_empty()).then_some(0),
            reading,
            error: None,
            loading: false,
            signing_in: false,
            updater: Updater::None,
            update: None,
            start_at_login: None,
            percent_in_icon: true,
            now: now(),
        }
    }

    fn texts(view: &TrayView) -> Vec<String> {
        view.menu.iter().filter_map(|e| if let Entry::Text(t) = e { Some(t.clone()) } else { None }).collect()
    }

    fn actions(view: &TrayView) -> Vec<(Action, bool)> {
        view.menu
            .iter()
            .filter_map(|e| match e {
                Entry::Item { action, enabled, .. } => Some((*action, *enabled)),
                Entry::Check { action, checked, .. } => Some((*action, *checked)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn whose_usage_it_is_then_each_limit_with_when_it_starts_again() {
        set_language("en", &[]);
        let accounts = &accounts()[..1];
        let reading = reading();
        let view = view(&status(accounts, Some(&reading)));
        assert_eq!(
            texts(&view)[..5],
            ["Claude · a@example.com", "5-Hour Limit: 42% · 2h 10m left", "7-Day Limit: 76% · 3d 4h left", "7D Fable Limit: 13%", "Extra Usage: $12.34 / $50"]
        );
        assert_eq!(view.tooltip, texts(&view)[..5]);
        assert_eq!(
            actions(&view),
            [
                (Action::Refresh, true),
                (Action::SignIn(Provider::Claude), true),
                (Action::SignOut, true),
                (Action::ShowPercent, true),
                (Action::OpenStatus(Provider::Claude), true),
                (Action::Quit, true)
            ]
        );
        let items: Vec<&str> = view.menu.iter().filter_map(|e| if let Entry::Item { label, .. } = e { Some(label.as_str()) } else { None }).collect();
        assert!(items.contains(&"Sign in with Claude…") && items.contains(&"Claude Status"), "{items:?}");
        assert!(view.rings.inner.is_some() && (view.rings.outer.unwrap().fill - 0.424).abs() < 1e-9);
        assert_eq!(view.rings.number, Some(42), "the 5-hour window's, rounded");
        let off = super::view(&Status { percent_in_icon: false, ..status(accounts, Some(&reading)) });
        assert_eq!(off.rings.number, None);
        assert!(actions(&off).contains(&(Action::ShowPercent, false)));
    }

    #[test]
    fn signed_out_it_asks_to_sign_in_and_reads_nothing() {
        set_language("en", &[]);
        let view = view(&status(&[], None));
        assert_eq!(texts(&view)[0], "Authentication not configured");
        assert_eq!(actions(&view)[..2], [(Action::Refresh, false), (Action::SignIn(Provider::Claude), true)]);
        assert_eq!(view.rings, Rings::default());
    }

    #[test]
    fn several_accounts_are_checks_and_a_failed_read_keeps_the_last_usage() {
        set_language("en", &[]);
        let accounts = accounts();
        let reading = reading();
        let error = ReadError::RateLimited(None);
        let s = Status { current: Some(1), error: Some(&error), signing_in: true, ..status(&accounts, Some(&reading)) };
        let view = view(&s);
        let texts = texts(&view);
        assert!(texts.contains(&"Showing cached data — refresh failed".into()) && texts.contains(&error.say()), "{texts:?}");
        assert!(actions(&view).contains(&(Action::Switch(0), false)) && actions(&view).contains(&(Action::Switch(1), true)));
        assert!(actions(&view).contains(&(Action::CancelSignIn, true)) && !actions(&view).iter().any(|(a, _)| matches!(a, Action::SignIn(_))));
        assert_eq!(view.tooltip[0], "Claude · b@example.com", "whose usage it is");
    }

    #[test]
    fn the_updaters_state_is_one_item_that_does_the_next_thing() {
        set_language("en", &[]);
        let items = |updater, state: Option<UpdateState>| actions(&view(&Status { updater, update: state.as_ref(), ..status(&[], None) }));
        assert!(!items(Updater::None, None).iter().any(|(a, _)| *a == Action::CheckUpdates));
        assert!(items(Updater::Shows, None).contains(&(Action::CheckUpdates, true)));
        assert!(items(Updater::Own, Some(UpdateState::Checking)).contains(&(Action::CheckUpdates, false)));
        assert!(items(Updater::Own, Some(UpdateState::Available { version: "0.1.1 (build 9)".into() })).contains(&(Action::InstallUpdate, true)));
        assert!(items(Updater::Own, Some(UpdateState::Installing { version: "0.1.1".into() })).contains(&(Action::InstallUpdate, false)));
        assert!(items(Updater::Own, Some(UpdateState::Installed { version: "0.1.1".into() })).contains(&(Action::Restart, true)));
        let failed = view(&Status { updater: Updater::Own, update: Some(&UpdateState::Failed("no network".into())), ..status(&[], None) });
        assert!(texts(&failed).contains(&"no network".into()) && actions(&failed).contains(&(Action::CheckUpdates, true)));
        let login = view(&Status { start_at_login: Some(true), ..status(&[], None) });
        assert!(actions(&login).contains(&(Action::StartAtLogin, true)));
    }

    #[test]
    fn time_left_reads_as_the_macs_compact_rows() {
        set_language("en", &[]);
        assert_eq!(left(later(30), now()).as_deref(), Some("Resetting soon"));
        assert_eq!(left(later(-5), now()).as_deref(), Some("Resetting soon"));
        assert_eq!(left(later(25 * 60), now()).as_deref(), Some("25m left"));
        assert_eq!(left(later(26 * 3600 + 60), now()).as_deref(), Some("1d 2h left"));
        assert_eq!(left(None, now()), None);
        set_language("ja", &[]);
        assert_eq!(left(later(3 * 3600 + 5 * 60), now()).as_deref(), Some("残り3時間5分"));
    }

    #[test]
    fn without_a_short_window_the_long_one_takes_the_outer_ring() {
        let mut reading = reading();
        reading.meters.remove(0);
        let rings = rings(Some(&reading), true);
        assert_eq!((rings.outer.unwrap().fill, rings.inner, rings.number), (0.76, None, Some(76)));
        assert_eq!(super::rings(None, true), Rings::default(), "nothing read, no number");
    }
}
