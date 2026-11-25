//! What U4Ai keeps between runs, `settings.json` in its folder
//! (`~/.config/U4Ai` on Linux, `%APPDATA%\U4Ai` on Windows): the
//! accounts signed in to (their ids and names; their refresh tokens are in
//! the credential store, `secrets`), the one shown, how often to read, and
//! whether to tell of limits. Written whole, then renamed into place.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::providers::Provider;
use crate::refresh::Mode;

/// An account signed in to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    #[serde(default)]
    pub provider: Provider,
    /// Its id at its provider (Claude's organization, else the email, as
    /// the Mac app keys them), so signing in again replaces rather than adds.
    pub id: String,
    /// What the menu shows: the email.
    pub name: String,
}

impl Account {
    /// What U4Ai knows it by, its provider's and its own: `claude:org-1`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.provider.id(), self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub accounts: Vec<Account>,
    /// The key of the account shown (`Account::key`).
    pub current: Option<String>,
    pub refresh: Mode,
    /// Tell of a limit at 90% and of its reset.
    pub alerts: bool,
    /// The outer ring's percentage drawn in the tray's icon.
    pub percent_in_icon: bool,
    /// A language's code, or "auto" for the system's.
    pub language: String,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { accounts: Vec::new(), current: None, refresh: Mode::Smart, alerts: true, percent_in_icon: true, language: crate::i18n::AUTO.into() }
    }
}

/// U4Ai's own folder.
pub fn dir() -> PathBuf {
    dirs::config_dir().or_else(|| dirs::home_dir().map(|h| h.join(".config"))).unwrap_or_else(std::env::temp_dir).join(crate::APP_NAME)
}

const FILE: &str = "settings.json";

impl Settings {
    /// The settings in `dir`; the defaults where there are none, or where
    /// they don't read (kept aside as `settings.json.unread`, so nothing
    /// the user had is lost).
    pub fn load(dir: &Path) -> Settings {
        let path = dir.join(FILE);
        let Ok(text) = std::fs::read_to_string(&path) else { return Settings::default() };
        match serde_json::from_str(&text) {
            Ok(settings) => settings,
            Err(err) => {
                log::warn!("{} doesn't read ({err}); starting from the defaults", path.display());
                let _ = std::fs::rename(&path, dir.join(format!("{FILE}.unread")));
                Settings::default()
            }
        }
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let temp = dir.join(format!(".{FILE}.{}", std::process::id()));
        std::fs::write(&temp, text + "\n").map_err(|e| e.to_string())?;
        std::fs::rename(&temp, dir.join(FILE)).map_err(|e| e.to_string())
    }

    /// The account shown, where there is one.
    pub fn current_account(&self) -> Option<&Account> {
        let key = self.current.as_deref()?;
        self.accounts.iter().find(|a| a.key() == key)
    }

    /// Adds `account`, or puts it in the place of one with its key, and
    /// shows it.
    pub fn add(&mut self, account: Account) {
        self.current = Some(account.key());
        match self.accounts.iter_mut().find(|a| a.key() == account.key()) {
            Some(old) => *old = account,
            None => self.accounts.push(account),
        }
    }

    /// Takes the account with `key` away; the first left is shown in its
    /// place.
    pub fn remove(&mut self, key: &str) {
        self.accounts.retain(|a| a.key() != key);
        if self.current.as_deref() == Some(key) || self.current_account().is_none() {
            self.current = self.accounts.first().map(Account::key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, name: &str) -> Account {
        Account { provider: Provider::Claude, id: id.into(), name: name.into() }
    }

    #[test]
    fn signing_in_again_replaces_the_account_and_signing_out_shows_the_next() {
        let mut s = Settings::default();
        s.add(account("org-1", "a@example.com"));
        s.add(account("org-2", "b@example.com"));
        assert_eq!(s.current_account().unwrap().key(), "claude:org-2");
        s.add(account("org-1", "a2@example.com"));
        assert_eq!(s.accounts.len(), 2);
        assert_eq!(s.current_account().unwrap().name, "a2@example.com");
        s.remove("claude:org-1");
        assert_eq!(s.current_account().unwrap().id, "org-2");
        s.remove("claude:org-2");
        assert_eq!((s.current_account(), s.current.clone()), (None, None));
    }

    #[test]
    fn settings_come_back_as_saved_and_ones_that_dont_read_are_kept_aside() {
        let dir = std::env::temp_dir().join(format!("u4ai-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Settings::load(&dir), Settings::default(), "none yet");
        let mut s = Settings { refresh: Mode::Fixed(5), alerts: false, ..Settings::default() };
        s.add(account("org-1", "a@example.com"));
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        assert!(std::fs::read_to_string(dir.join(FILE)).unwrap().contains(r#""provider": "claude""#));
        // A file from a later version, with keys this one hasn't got, and
        // one from before a key was added, which takes its default.
        std::fs::write(dir.join(FILE), r#"{"alerts": false, "someday": 1}"#).unwrap();
        let read = Settings::load(&dir);
        assert!(!read.alerts && read.percent_in_icon);
        std::fs::write(dir.join(FILE), "{broken").unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());
        assert_eq!(std::fs::read_to_string(dir.join("settings.json.unread")).unwrap(), "{broken");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
