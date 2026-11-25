//! U4Ai on Windows: the tray icon (Shell_NotifyIconW) with its menu and
//! balloons, Start at login (HKCU's Run key), web pages opened through the
//! shell, and WinSparkle for updates, in a copy the installer put in place.

#![cfg(windows)]

mod autostart;
mod tray;
mod updater;

use u4ai_core::tray::TrayView;
use u4ai_core::{Platform, Updater, Waker};
use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The mutex the installer's `AppMutex` looks for, so setup asks to close
/// a running U4Ai before replacing it.
const RUNNING: &str = "U4Ai.Running";

struct Windows;

impl Platform for Windows {
    fn show(&mut self, view: &TrayView) {
        tray::show(view);
    }

    fn notify(&self, title: &str, body: &str) {
        tray::balloon(title, body);
    }

    fn open_url(&self, url: &str) {
        if !url.starts_with("https://") {
            log::warn!("not opened: {url}");
            return;
        }
        let (verb, file) = (wide("open"), wide(url));
        // SAFETY: the shell's own "open" of a web address, the strings
        // outliving the call.
        let done = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
        if (done as isize) <= 32 {
            log::warn!("could not open {url}: {}", done as isize);
        }
    }

    fn languages(&self) -> Vec<String> {
        languages()
    }

    fn updater(&self) -> Updater {
        if updater::available() {
            Updater::Shows
        } else {
            Updater::None
        }
    }

    fn check_updates(&self, quiet: bool) {
        // WinSparkle keeps its own daily check; the menu's is the one asked.
        if !quiet {
            updater::check_with_ui();
        }
    }

    fn start_at_login(&self) -> Option<bool> {
        Some(autostart::on())
    }

    fn set_start_at_login(&self, on: bool) -> Result<(), String> {
        autostart::set(on)
    }

    fn exiting(&mut self) {
        tray::remove();
        updater::cleanup();
    }
}

/// The user's display languages, most wanted first ("ja-JP").
fn languages() -> Vec<String> {
    let (mut count, mut length) = (0u32, 0u32);
    // SAFETY: asked first for the length, then into a buffer that long.
    unsafe {
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut length) == 0 || length == 0 {
            return Vec::new();
        }
        let mut buffer = vec![0u16; length as usize];
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buffer.as_mut_ptr(), &mut length) == 0 {
            return Vec::new();
        }
        buffer.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
    }
}

pub fn run() {
    // SAFETY: a named mutex held for the life of the process, never closed.
    unsafe { CreateMutexW(std::ptr::null(), 0, wide(RUNNING).as_ptr()) };
    u4ai_core::run(|waker: Waker| {
        tray::prepare(waker.clone());
        if u4ai_core::version::updater_wanted() {
            updater::init(waker);
        }
        Box::new(Windows)
    });
}
