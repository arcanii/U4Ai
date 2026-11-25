//! U4Ai on Linux: the tray as a StatusNotifierItem over D-Bus (KDE's,
//! and GNOME's with the AppIndicator extension), desktop notifications,
//! Start at login through an XDG autostart entry, and Flow's own updater
//! for a copy install.sh put in place.

#![cfg(target_os = "linux")]

mod autostart;
mod notify;
mod tray;
mod updater;

use std::process::{Command, Stdio};

use u4ai_core::tray::TrayView;
use u4ai_core::{Platform, UpdateState, Updater, Waker};

struct Linux {
    tray: tray::Tray,
}

impl Platform for Linux {
    fn show(&mut self, view: &TrayView) {
        self.tray.show(view);
    }

    fn notify(&self, title: &str, body: &str) {
        notify::notify(title, body);
    }

    fn open_url(&self, url: &str) {
        // A web address alone, never a word an opener could read as an
        // option.
        if !url.starts_with("https://") {
            log::warn!("not opened: {url}");
            return;
        }
        if let Err(err) = Command::new("xdg-open").arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            log::warn!("xdg-open {url}: {err}");
        }
    }

    fn languages(&self) -> Vec<String> {
        languages(|name| std::env::var(name).ok())
    }

    fn updater(&self) -> Updater {
        if updater::available() {
            Updater::Own
        } else {
            Updater::None
        }
    }

    fn update_state(&self) -> Option<UpdateState> {
        updater::state()
    }

    fn check_updates(&self, quiet: bool) {
        updater::check(quiet);
    }

    fn install_update(&self) {
        updater::install();
    }

    fn relaunch(&self) -> bool {
        updater::relaunch()
    }

    fn start_at_login(&self) -> Option<bool> {
        Some(autostart::on())
    }

    fn set_start_at_login(&self, on: bool) -> Result<(), String> {
        autostart::set(on)
    }

    fn exiting(&mut self) {
        self.tray.shut_down();
    }
}

/// The system's languages as the C library reads them: `LANGUAGE`'s list
/// first, then `LC_ALL`, `LC_MESSAGES` and `LANG`, the C locale none.
fn languages(var: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut out: Vec<String> = var("LANGUAGE").map(|l| l.split(':').map(String::from).collect()).unwrap_or_default();
    out.extend(["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|n| var(n)));
    out.retain(|l| !l.is_empty() && l != "C" && l != "POSIX" && !l.starts_with("C."));
    out
}

pub fn run() {
    u4ai_core::run(|waker: Waker| {
        if u4ai_core::version::updater_wanted() {
            updater::init(waker.clone());
        }
        Box::new(Linux { tray: tray::Tray::new(waker) })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }

    #[test]
    fn the_languages_are_languages_list_then_the_locale_without_c() {
        assert_eq!(languages(env(&[("LANGUAGE", "ja:en"), ("LANG", "en_US.UTF-8")])), ["ja", "en", "en_US.UTF-8"]);
        assert!(languages(env(&[("LC_ALL", "C"), ("LANG", "C.UTF-8")])).is_empty());
        assert_eq!(languages(env(&[("LC_MESSAGES", "zh_TW.UTF-8")])), ["zh_TW.UTF-8"]);
    }
}
