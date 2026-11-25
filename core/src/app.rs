//! The core's loop: one thread that holds how things stand, takes `Event`s
//! from the tray, the updater and the threads that read and sign in, reads
//! the usage when `refresh` says, tells of limits, and hands the tray a new
//! `TrayView` whenever it changes (at least once a minute, for the time
//! left). Network and credential-store calls run on threads of their own,
//! so the tray never waits on them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use jiff::Timestamp;

use crate::alerts::{Alert, Alerts};
use crate::config::{Account, Settings};
use crate::i18n::{t, tf};
use crate::platform::{Action, Event, Platform, Updater, Waker};
use crate::providers::{NewAccount, Provider, ReadError, Reader, Reading, SignInError};
use crate::refresh::Refresh;
use crate::secrets;
use crate::tray::{self, Status, TrayView};

/// How often the tray is drawn again with nothing else happening: the time
/// left moves by the minute.
const REDRAW_EVERY: Duration = Duration::from_secs(60);
/// How often the updater looks for a release by itself.
const UPDATE_EVERY: Duration = Duration::from_secs(24 * 3600);
/// A wall clock ahead of the steady one by more than this means the
/// computer slept.
const SLEPT: Duration = Duration::from_secs(120);
/// How soon after waking to read.
const AFTER_WAKE: Duration = Duration::from_secs(3);

/// An account's reader, made on the reading thread the first time (the
/// credential store may be slow), and kept (Claude's keeps its access token).
type ReaderSlot = Arc<Mutex<Option<Box<dyn Reader>>>>;

pub struct App {
    platform: Box<dyn Platform>,
    waker: Waker,
    dir: PathBuf,
    settings: Settings,
    /// Each account's reader, by its key.
    readers: HashMap<String, ReaderSlot>,
    /// The last usage read, and whose (its account's key).
    reading: Option<(String, Reading)>,
    error: Option<ReadError>,
    /// The key of the account a read is under way for.
    reading_for: Option<String>,
    signing_in: Option<Arc<AtomicBool>>,
    refresh: Refresh,
    alerts: Alerts,
    next_read: Instant,
    next_update_check: Option<Instant>,
    shown: Option<TrayView>,
    /// The steady and the wall clock at the last turn, to see a sleep.
    clocks: (Instant, SystemTime),
    told_no_tray: bool,
    quit: bool,
}

impl App {
    pub fn new(platform: Box<dyn Platform>, waker: Waker, dir: PathBuf) -> App {
        let settings = Settings::load(&dir);
        crate::i18n::set_language(&settings.language, &platform.languages());
        let refresh = Refresh::new(settings.refresh);
        let now = Instant::now();
        let next_update_check = (platform.updater() == Updater::Own).then_some(now);
        App {
            platform,
            waker,
            dir,
            settings,
            readers: HashMap::new(),
            reading: None,
            error: None,
            reading_for: None,
            signing_in: None,
            refresh,
            alerts: Alerts::default(),
            next_read: now,
            next_update_check,
            shown: None,
            clocks: (now, SystemTime::now()),
            told_no_tray: false,
            quit: false,
        }
    }

    /// Runs until Quit.
    pub fn run(mut self, events: Receiver<Event>) {
        self.turn();
        while !self.quit {
            let now = Instant::now();
            let due = [Some(self.next_read), self.next_update_check, Some(now + REDRAW_EVERY)].into_iter().flatten().min().unwrap_or(now);
            match events.recv_timeout(due.saturating_duration_since(now)) {
                Ok(event) => self.handle(event),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.turn();
        }
        self.platform.exiting();
        log::info!("quitting");
    }

    /// What's due: a read, a look for updates, and the tray drawn again
    /// where it changed.
    fn turn(&mut self) {
        let now = Instant::now();
        self.notice_sleep(now);
        if now >= self.next_read {
            self.read();
        }
        if self.next_update_check.is_some_and(|at| now >= at) {
            self.platform.check_updates(true);
            self.next_update_check = Some(now + UPDATE_EVERY);
        }
        self.show();
    }

    /// A wall clock that went further than the steady one: the computer
    /// slept, and the usage is read again soon after it woke.
    fn notice_sleep(&mut self, now: Instant) {
        let wall = SystemTime::now();
        let (was_now, was_wall) = self.clocks;
        self.clocks = (now, wall);
        let steady = now.duration_since(was_now);
        if wall.duration_since(was_wall).is_ok_and(|w| w > steady + SLEPT) {
            log::info!("the computer slept; reading again");
            self.refresh.hurry();
            self.next_read = self.next_read.min(now + AFTER_WAKE);
        }
    }

    fn current(&self) -> Option<&Account> {
        self.settings.current_account()
    }

    fn save(&self) {
        if let Err(err) = self.settings.save(&self.dir) {
            log::warn!("settings not saved: {err}");
        }
    }

    /// Reads the shown account's usage on a thread, unless one already is,
    /// or a 429's hold stands.
    fn read(&mut self) {
        let now = Instant::now();
        let Some(account) = self.current().cloned() else {
            self.next_read = now + Duration::from_secs(3600);
            return;
        };
        if self.reading_for.is_some() {
            return;
        }
        if let Some(until) = self.refresh.limited_until(now) {
            self.next_read = until;
            return;
        }
        // Until it comes back: a read that never does is tried again later.
        self.next_read = now + self.refresh.interval();
        let key = account.key();
        self.reading_for = Some(key.clone());
        let slot = self.readers.entry(key.clone()).or_default().clone();
        let waker = self.waker.clone();
        std::thread::Builder::new()
            .name("u4ai-read".into())
            .spawn(move || {
                let result = read_on_thread(&slot, account.provider, &key);
                waker.send(Event::Read { account: key, result });
            })
            .expect("a thread to read on");
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::Act(action) => self.act(action),
            Event::Read { account, result } => self.read_back(account, result),
            Event::SignedIn(result) => self.signed_in(result),
            Event::Update => {}
            Event::NoTray => {
                if !self.told_no_tray {
                    self.told_no_tray = true;
                    self.platform.notify(crate::APP_NAME, t("notification.no_tray"));
                }
            }
            Event::Quit => self.quit = true,
        }
    }

    fn read_back(&mut self, account: String, result: Result<Reading, ReadError>) {
        if self.reading_for.as_deref() == Some(account.as_str()) {
            self.reading_for = None;
        }
        // An account switched away from (or signed out of) meanwhile.
        if self.current().map(Account::key).as_deref() != Some(account.as_str()) {
            self.next_read = Instant::now();
            return;
        }
        let now = Instant::now();
        match result {
            Ok(reading) => {
                if let Some(primary) = reading.primary() {
                    self.refresh.read(primary.limit.percent);
                }
                self.next_read = now + self.refresh.interval();
                let alerts = self.alerts.check(&account, &reading.meters);
                if self.settings.alerts {
                    for alert in alerts {
                        match alert {
                            Alert::Warning { limit, percent } => {
                                self.platform.notify(t("notification.warning_title"), &tf("notification.warning_body", &[("limit", &limit), ("percent", &percent)]))
                            }
                            Alert::Reset { limit } => self.platform.notify(t("notification.reset_title"), &tf("notification.reset_body", &[("limit", &limit)])),
                        }
                    }
                }
                self.reading = Some((account, reading));
                self.error = None;
            }
            Err(error) => {
                log::warn!("reading the usage: {error:?}");
                self.next_read = match &error {
                    ReadError::RateLimited(retry) => self.refresh.rate_limited(*retry, now),
                    _ => now + self.refresh.interval(),
                };
                if matches!(error, ReadError::Unauthorized | ReadError::Keyring(_)) {
                    // Read the secret afresh next time: the user may have
                    // signed in again.
                    self.readers.remove(&account);
                }
                self.error = Some(error);
            }
        }
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::Refresh => {
                self.refresh.hurry();
                self.next_read = Instant::now();
            }
            Action::SignIn(provider) => self.sign_in(provider),
            Action::CancelSignIn => {
                if let Some(cancel) = self.signing_in.take() {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Action::Switch(i) => {
                if let Some(account) = self.settings.accounts.get(i) {
                    self.settings.current = Some(account.key());
                    self.save();
                    self.shown_changed();
                }
            }
            Action::SignOut => {
                if let Some(key) = self.current().map(Account::key) {
                    if let Err(err) = secrets::forget_long(&secrets::token_account(&key)) {
                        log::warn!("the secret wasn't forgotten: {err}");
                    }
                    self.settings.remove(&key);
                    self.readers.remove(&key);
                    self.alerts.forget(&key);
                    self.save();
                    self.shown_changed();
                }
            }
            Action::StartAtLogin => {
                let on = self.platform.start_at_login().unwrap_or(false);
                if let Err(err) = self.platform.set_start_at_login(!on) {
                    log::warn!("start at login: {err}");
                    self.platform.notify(crate::APP_NAME, &err);
                }
            }
            Action::CheckUpdates => self.platform.check_updates(false),
            Action::InstallUpdate => self.platform.install_update(),
            Action::Restart => {
                if self.platform.relaunch() {
                    self.quit = true;
                }
            }
            Action::OpenStatus(provider) => self.platform.open_url(provider.status_page()),
            Action::Quit => self.quit = true,
        }
    }

    /// Another account shows (or none): nothing of the last one stays.
    fn shown_changed(&mut self) {
        self.reading = None;
        self.error = None;
        self.refresh.hurry();
        self.next_read = Instant::now();
    }

    fn sign_in(&mut self, provider: Provider) {
        if self.signing_in.is_some() {
            return;
        }
        let pending = match provider.sign_in() {
            Ok(pending) => pending,
            Err(SignInError::Failed(why)) => return self.platform.notify(crate::APP_NAME, &why),
            Err(SignInError::Cancelled) => return,
        };
        self.platform.open_url(&pending.url);
        let cancel = Arc::new(AtomicBool::new(false));
        self.signing_in = Some(cancel.clone());
        let waker = self.waker.clone();
        std::thread::Builder::new()
            .name("u4ai-sign-in".into())
            .spawn(move || {
                let result = pending.finish(&cancel).and_then(|account| {
                    let key = Account { provider: account.provider, id: account.id.clone(), name: String::new() }.key();
                    secrets::store_long(&secrets::token_account(&key), &account.secret)
                        .map_err(|why| SignInError::Failed(tf("sign_in.kept_failed", &[("why", &why)])))?;
                    Ok(account)
                });
                waker.send(Event::SignedIn(result));
            })
            .expect("a thread to sign in on");
    }

    fn signed_in(&mut self, result: Result<NewAccount, SignInError>) {
        self.signing_in = None;
        match result {
            Ok(new) => {
                let account = Account { provider: new.provider, id: new.id, name: new.name };
                log::info!("signed in as {}", account.key());
                self.readers.remove(&account.key());
                self.platform.notify(crate::APP_NAME, &tf("sign_in.added", &[("account", &tray::account_label(&account))]));
                self.settings.add(account);
                self.save();
                self.shown_changed();
            }
            Err(SignInError::Cancelled) => log::info!("sign-in called off"),
            Err(SignInError::Failed(why)) => self.platform.notify(crate::APP_NAME, &why),
        }
    }

    /// Hands the tray its view, where it changed.
    fn show(&mut self) {
        let current = self.settings.current.as_ref().and_then(|key| self.settings.accounts.iter().position(|a| &a.key() == key));
        let shown = self.current().map(Account::key);
        let reading = self.reading.as_ref().filter(|(key, _)| Some(key) == shown.as_ref()).map(|(_, reading)| reading);
        let update = self.platform.update_state();
        let status = Status {
            accounts: &self.settings.accounts,
            current,
            reading,
            error: self.error.as_ref(),
            loading: self.reading_for.is_some(),
            signing_in: self.signing_in.is_some(),
            updater: self.platform.updater(),
            update: update.as_ref(),
            start_at_login: self.platform.start_at_login(),
            now: Timestamp::now(),
        };
        let view = tray::view(&status);
        if self.shown.as_ref() != Some(&view) {
            self.platform.show(&view);
            self.shown = Some(view);
        }
    }
}

/// A read on its own thread: the account's reader, made by its provider from
/// the secret in the credential store the first time, and a secret the
/// provider turned over kept there again.
fn read_on_thread(slot: &ReaderSlot, provider: Provider, key: &str) -> Result<Reading, ReadError> {
    let mut reader = slot.lock().unwrap_or_else(|e| e.into_inner());
    if reader.is_none() {
        match secrets::load_long(&secrets::token_account(key)) {
            Ok(Some(secret)) => *reader = Some(provider.reader(secret)),
            Ok(None) => return Err(ReadError::Unauthorized),
            Err(why) => return Err(ReadError::Keyring(why)),
        }
    }
    let reader = reader.as_mut().expect("made above");
    let result = reader.read();
    if let Some(rotated) = reader.take_rotated() {
        if let Err(err) = secrets::store_long(&secrets::token_account(key), &rotated) {
            log::warn!("the turned-over secret wasn't kept: {err}");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::UpdateState;
    use std::sync::mpsc;

    /// The views shown, the notifications and the pages opened.
    type Handed = (Vec<TrayView>, Vec<(String, String)>, Vec<String>);

    /// A platform that keeps what it's handed.
    #[derive(Default, Clone)]
    struct Kept(Arc<Mutex<Handed>>);

    impl Platform for Kept {
        fn show(&mut self, view: &TrayView) {
            self.0.lock().unwrap().0.push(view.clone());
        }
        fn notify(&self, title: &str, body: &str) {
            self.0.lock().unwrap().1.push((title.into(), body.into()));
        }
        fn open_url(&self, url: &str) {
            self.0.lock().unwrap().2.push(url.into());
        }
        fn languages(&self) -> Vec<String> {
            vec!["en-US".into()]
        }
        fn updater(&self) -> Updater {
            Updater::Own
        }
        fn update_state(&self) -> Option<UpdateState> {
            Some(UpdateState::Idle)
        }
    }

    fn app(dir: &std::path::Path) -> (App, Kept, Receiver<Event>) {
        let kept = Kept::default();
        let (tx, rx) = mpsc::channel();
        let app = App::new(Box::new(kept.clone()), Waker(tx), dir.to_path_buf());
        (app, kept, rx)
    }

    fn data(percent: f64) -> Reading {
        let usage = crate::providers::claude::usage::parse(&format!(r#"{{"five_hour": {{"utilization": {percent}, "resets_at": null}}}}"#)).unwrap().unwrap();
        crate::providers::claude::reading(&usage)
    }

    fn account(id: &str) -> Account {
        Account { provider: Provider::Claude, id: id.into(), name: format!("{id}@example.com") }
    }

    #[test]
    fn a_read_for_the_shown_account_shows_and_warns_and_one_for_another_is_let_go() {
        let dir = std::env::temp_dir().join(format!("u4ai-app-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (mut app, kept, _rx) = app(&dir);
        app.settings.add(account("a"));
        app.read_back("claude:a".into(), Ok(data(50.0)));
        app.read_back("claude:a".into(), Ok(data(93.0)));
        app.read_back("claude:b".into(), Ok(data(99.0)));
        app.show();
        let kept = kept.0.lock().unwrap();
        assert_eq!(kept.1, [("Usage Warning".to_string(), "5-Hour Limit has reached 93% usage".to_string())]);
        let view = kept.0.last().unwrap();
        assert_eq!(view.tooltip[0], "Claude · a@example.com");
        assert!(view.tooltip[1].starts_with("5-Hour Limit: 93%"), "{:?}", view.tooltip);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_429_holds_the_next_read_and_switching_accounts_forgets_the_last_usage() {
        let dir = std::env::temp_dir().join(format!("u4ai-app-test-429-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (mut app, _kept, _rx) = app(&dir);
        app.settings.add(account("a"));
        app.settings.add(account("b"));
        app.read_back("claude:b".into(), Ok(data(10.0)));
        let before = Instant::now();
        app.read_back("claude:b".into(), Err(ReadError::RateLimited(Some(Duration::from_secs(600)))));
        assert!(app.next_read >= before + Duration::from_secs(599));
        assert!(app.reading.is_some() && app.error.is_some(), "the last usage stays, with why it's old");
        app.act(Action::Switch(0));
        assert_eq!((app.current().unwrap().id.as_str(), app.reading.is_none(), app.error.is_none()), ("a", true, true));
        assert_eq!(Settings::load(&dir).current.as_deref(), Some("claude:a"), "saved");
        app.act(Action::OpenStatus(Provider::Claude));
        app.act(Action::Quit);
        assert!(app.quit);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
