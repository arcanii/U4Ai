//! What each OS crate gives the core (`Platform`), and the events that
//! reach the core's loop from the tray, the updater and the threads that
//! read and sign in (`Event`, through a `Waker`).

use std::sync::mpsc::Sender;

use crate::providers::{NewAccount, Provider, ReadError, Reading, SignInError};
use crate::tray::TrayView;

/// What a menu item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Refresh,
    SignIn(Provider),
    CancelSignIn,
    /// Show the account at this place in the list.
    Switch(usize),
    SignOut,
    StartAtLogin,
    /// The outer ring's percentage in the icon, on or off.
    ShowPercent,
    CheckUpdates,
    InstallUpdate,
    /// Start the copy an update put in place, and quit.
    Restart,
    /// The provider's status page.
    OpenStatus(Provider),
    Quit,
}

pub enum Event {
    Act(Action),
    /// A read of the account with this key came back.
    Read { account: String, result: Result<Reading, ReadError> },
    SignedIn(Result<NewAccount, SignInError>),
    /// The updater's state changed.
    Update,
    /// The tray found nowhere to show (no StatusNotifierWatcher).
    NoTray,
    Quit,
}

/// Hands events to the core's loop from any thread.
#[derive(Clone)]
pub struct Waker(pub(crate) Sender<Event>);

impl Waker {
    pub fn send(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

/// How an updater stands, for the menu (Linux's; WinSparkle shows its own).
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateState {
    /// Nothing asked yet, or a quiet check that found nothing.
    Idle,
    Checking,
    UpToDate,
    /// A newer release, as "0.1.1 (build 160)".
    Available { version: String },
    Installing { version: String },
    /// In place: the next start is the new version.
    Installed { version: String },
    /// In the user's language.
    Failed(String),
}

/// What kind of updater a copy has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Updater {
    /// None: a build from the tree, a fork's, or a copy that can't replace
    /// itself (the portable zip, a copy install.sh didn't put in place).
    None,
    /// The core shows its states (`update_state`) in the menu.
    Own,
    /// It shows its own windows (WinSparkle): the menu only asks it to check.
    Shows,
}

pub trait Platform {
    /// Shows the tray as `view` says; called on the core's thread each time
    /// it changes.
    fn show(&mut self, view: &TrayView);
    fn notify(&self, title: &str, body: &str);
    /// Opens a web page (only https ones are ever given).
    fn open_url(&self, url: &str);
    /// The system's languages, most wanted first ("ja-JP").
    fn languages(&self) -> Vec<String>;
    fn updater(&self) -> Updater {
        Updater::None
    }
    fn update_state(&self) -> Option<UpdateState> {
        None
    }
    /// A quiet check changes nothing that shows unless it finds a release.
    fn check_updates(&self, _quiet: bool) {}
    fn install_update(&self) {}
    /// Starts the copy an update put in place; false where it couldn't.
    fn relaunch(&self) -> bool {
        false
    }
    /// Whether U4Ai starts at login, where the platform can say.
    fn start_at_login(&self) -> Option<bool> {
        None
    }
    fn set_start_at_login(&self, _on: bool) -> Result<(), String> {
        Err("not here".into())
    }
    /// The core's loop is ending.
    fn exiting(&mut self) {}
}
