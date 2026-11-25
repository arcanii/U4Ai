//! The AIs whose limits U4Ai reads. Each provider is a module of its own
//! (`claude`, the first) that signs in (`sign_in`, giving a `SignIn`: the
//! page to open, and a wait for it on a thread), makes a `Reader` from the
//! secret an account keeps, and reads into a `Reading`: its limits as
//! `Meter`s, each with a key the same in every language (for the alerts), a
//! label, the kind of window it is (which ring and colors it takes), and
//! how much is used; and lines of its own (Claude's Extra Usage). What the
//! rest of U4Ai knows of a provider is here, so a second one (Codex,
//! Gemini…) is a module and an arm in `Provider`'s matches.

pub mod claude;

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::i18n::{t, tf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Claude,
}

impl Provider {
    pub const ALL: [Provider; 1] = [Provider::Claude];

    /// Its id in settings and in the credential store's names.
    pub fn id(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
        }
    }

    /// Its name as the menu shows it (a name, the same in every language).
    pub fn name(self) -> &'static str {
        match self {
            Provider::Claude => "Claude",
        }
    }

    /// Its status page, for the menu's "<provider> Status".
    pub fn status_page(self) -> &'static str {
        match self {
            Provider::Claude => claude::STATUS_PAGE,
        }
    }

    /// Begins a sign-in: the browser is to open `SignIn::url`.
    pub fn sign_in(self) -> Result<SignIn, SignInError> {
        match self {
            Provider::Claude => claude::sign_in(),
        }
    }

    /// What reads an account's usage, from the secret it keeps.
    pub fn reader(self, secret: String) -> Box<dyn Reader> {
        match self {
            Provider::Claude => claude::reader(secret),
        }
    }
}

/// One limit: how much is used, and when it starts again.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limit {
    /// 0–100.
    pub percent: f64,
    /// Rounded to the second where the provider's times wobble.
    pub resets_at: Option<Timestamp>,
}

/// What kind of window a limit is: the short one takes the outer ring, the
/// long one the inner (or the outer, without a short one), and a model's
/// shows as a row alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Short,
    Long,
    Model,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Meter {
    /// The same in every language, for the alerts: "five_hour".
    pub key: String,
    pub label: String,
    pub window: Window,
    pub limit: Limit,
}

/// One read of an account's usage.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub meters: Vec<Meter>,
    /// Rows of the provider's own under the limits (Claude's Extra Usage).
    pub lines: Vec<String>,
}

impl Reading {
    /// The first limit of a kind.
    pub fn first(&self, window: Window) -> Option<&Meter> {
        self.meters.iter().find(|m| m.window == window)
    }

    /// The limit the outer ring shows: the short window where there is
    /// one, else the long; what smart refreshing watches.
    pub fn primary(&self) -> Option<&Meter> {
        self.first(Window::Short).or_else(|| self.first(Window::Long))
    }
}

/// Why a read gave nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadError {
    Network(String),
    /// The sign-in was refused: the user signs in again.
    Unauthorized,
    /// 429, with the server's `Retry-After` where it gave one.
    RateLimited(Option<Duration>),
    Http(u16),
    Decoding(String),
    /// The plan has no usage data (not a failed sign-in).
    NoDashboard,
    /// The secret couldn't be read from the credential store.
    Keyring(String),
}

impl ReadError {
    /// In the user's words.
    pub fn say(&self) -> String {
        match self {
            ReadError::Network(_) => t("error.network").to_string(),
            ReadError::Unauthorized => t("error.unauthorized").to_string(),
            ReadError::RateLimited(_) => t("error.rate_limited").to_string(),
            ReadError::Http(status) => tf("error.http", &[("status", status)]),
            ReadError::Decoding(_) => t("error.decoding").to_string(),
            ReadError::NoDashboard => t("error.no_dashboard").to_string(),
            ReadError::Keyring(why) => tf("error.keyring", &[("why", why)]),
        }
    }
}

/// Reads an account's usage, on the reading thread.
pub trait Reader: Send {
    fn read(&mut self) -> Result<Reading, ReadError>;
    /// A secret the provider turned over since the last look (a refresh
    /// token rotated), to keep in its place; once.
    fn take_rotated(&mut self) -> Option<String>;
}

/// Why a sign-in didn't happen.
#[derive(Debug, Clone, PartialEq)]
pub enum SignInError {
    /// The user called it off.
    Cancelled,
    /// In the user's words.
    Failed(String),
}

/// An account signed in to: its provider, its id there (Claude's
/// organization, else the email), the name to show, and the secret to
/// keep in the credential store.
#[derive(Debug, Clone, PartialEq)]
pub struct NewAccount {
    pub provider: Provider,
    pub id: String,
    pub name: String,
    pub secret: String,
}

/// A sign-in begun: the page to open, and what waits for it to come back.
pub struct SignIn {
    pub url: String,
    #[allow(clippy::type_complexity)]
    wait: Box<dyn FnOnce(&AtomicBool) -> Result<NewAccount, SignInError> + Send>,
}

impl SignIn {
    pub fn new(url: String, wait: impl FnOnce(&AtomicBool) -> Result<NewAccount, SignInError> + Send + 'static) -> SignIn {
        SignIn { url, wait: Box::new(wait) }
    }

    /// Waits for the browser and finishes the sign-in, on a thread of its
    /// own; given up when `cancel` is set.
    pub fn finish(self, cancel: &AtomicBool) -> Result<NewAccount, SignInError> {
        (self.wait)(cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meter(key: &str, window: Window, percent: f64) -> Meter {
        Meter { key: key.into(), label: key.into(), window, limit: Limit { percent, resets_at: None } }
    }

    #[test]
    fn the_short_window_is_primary_and_the_long_stands_in_without_one() {
        let both = Reading { meters: vec![meter("model", Window::Model, 9.0), meter("long", Window::Long, 7.0), meter("short", Window::Short, 5.0)], lines: vec![] };
        assert_eq!(both.primary().unwrap().key, "short");
        let long = Reading { meters: vec![meter("long", Window::Long, 7.0)], lines: vec![] };
        assert_eq!(long.primary().unwrap().key, "long");
        assert!(Reading { meters: vec![meter("model", Window::Model, 1.0)], lines: vec![] }.primary().is_none());
    }

    #[test]
    fn each_provider_has_an_id_a_name_and_a_status_page() {
        for p in Provider::ALL {
            assert!(p.id().chars().all(|c| c.is_ascii_lowercase()), "{p:?}");
            assert!(!p.name().is_empty() && p.status_page().starts_with("https://"));
            assert_eq!(serde_json::to_string(&p).unwrap(), format!("\"{}\"", p.id()), "settings name it by its id");
        }
    }
}
