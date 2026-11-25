//! Updates through WinSparkle, as Flow's (windows/RELEASING.md): it reads
//! the EdDSA-signed `appcast-windows.xml` from this repository's `main`,
//! checks the installer against the project's key, asks U4Ai to quit
//! and runs it.
//!
//! WinSparkle.dll is loaded from beside u4ai.exe at run time, so builds
//! without it (dev builds, the portable zip) have no updater, and only a
//! build made as one of the project's releases starts it
//! (`version::UPDATES`).

use std::ffi::{c_char, c_int};
use std::os::windows::ffi::OsStrExt;
use std::sync::OnceLock;
use std::time::Duration;

use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use u4ai_core::{Event, Waker};

/// The feed, on this repository's `main`.
const APPCAST_URL: &[u8] = b"https://raw.githubusercontent.com/arcanii/U4Ai/main/appcast-windows.xml\0";

type VoidFn = unsafe extern "C" fn();
type IntFn = unsafe extern "C" fn() -> c_int;
type StrFn = unsafe extern "C" fn(*const c_char);
type StrIntFn = unsafe extern "C" fn(*const c_char) -> c_int;
type DetailsFn = unsafe extern "C" fn(*const u16, *const u16, *const u16);
type SetIntCbFn = unsafe extern "C" fn(IntFn);
type SetVoidCbFn = unsafe extern "C" fn(VoidFn);

struct Api {
    check_with_ui: VoidFn,
    cleanup: VoidFn,
}

static API: OnceLock<Api> = OnceLock::new();
static WAKER: OnceLock<Waker> = OnceLock::new();

unsafe extern "C" fn can_shutdown() -> c_int {
    1
}

/// WinSparkle (on its own thread) needs us gone before it runs the installer.
unsafe extern "C" fn shutdown_request() {
    if let Some(w) = WAKER.get() {
        w.send(Event::Quit);
    }
    // A clean close normally wins; this makes sure a stuck one can't keep
    // u4ai.exe locked and make the update fail.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(2500));
        std::process::exit(0);
    });
}

/// Loads WinSparkle and starts its periodic checks. False if unavailable.
pub fn init(waker: Waker) -> bool {
    if API.get().is_some() {
        return true;
    }
    let Some(dll) = std::env::current_exe().ok().map(|p| p.with_file_name("WinSparkle.dll")) else {
        return false;
    };
    if !dll.is_file() {
        log::info!("updater off: {} not found", dll.display());
        return false;
    }
    let wide: Vec<u16> = dll.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: plain Win32 loading of a DLL by full path; symbols are cast to
    // the signatures in winsparkle.h (cdecl == the x64 calling convention).
    unsafe {
        let module = LoadLibraryW(wide.as_ptr());
        if module.is_null() {
            log::warn!("updater off: could not load {}", dll.display());
            return false;
        }
        macro_rules! sym {
            ($name:literal, $ty:ty) => {
                match GetProcAddress(module, concat!($name, "\0").as_ptr()) {
                    Some(f) => std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(f),
                    None => {
                        log::warn!("updater off: {} missing from WinSparkle.dll", $name);
                        return false;
                    }
                }
            };
        }
        let set_url = sym!("win_sparkle_set_appcast_url", StrFn);
        let set_details = sym!("win_sparkle_set_app_details", DetailsFn);
        let set_key = sym!("win_sparkle_set_eddsa_public_key", StrIntFn);
        let set_can_shutdown = sym!("win_sparkle_set_can_shutdown_callback", SetIntCbFn);
        let set_shutdown_request = sym!("win_sparkle_set_shutdown_request_callback", SetVoidCbFn);
        let init = sym!("win_sparkle_init", VoidFn);
        let check_with_ui = sym!("win_sparkle_check_update_with_ui", VoidFn);
        let cleanup = sym!("win_sparkle_cleanup", VoidFn);

        let _ = WAKER.set(waker);
        set_url(APPCAST_URL.as_ptr().cast());
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (company, app, version) = (wide("arcanii"), wide(u4ai_core::APP_NAME), wide(u4ai_core::version::FULL));
        // Reports marketing.build (e.g. 0.1.0.42) to compare with sparkle:version.
        set_details(company.as_ptr(), app.as_ptr(), version.as_ptr());
        let key: Vec<u8> = u4ai_core::UPDATE_PUBLIC_KEY.bytes().chain(std::iter::once(0)).collect();
        if set_key(key.as_ptr().cast()) == 0 {
            log::warn!("updater off: WinSparkle rejected the EdDSA public key");
            return false;
        }
        set_can_shutdown(can_shutdown);
        set_shutdown_request(shutdown_request);
        init();
        let _ = API.set(Api { check_with_ui, cleanup });
    }
    log::info!("updater on ({})", u4ai_core::version::FULL);
    true
}

pub fn available() -> bool {
    API.get().is_some()
}

/// "Check for Updates…": WinSparkle shows its own progress and result.
pub fn check_with_ui() {
    if let Some(api) = API.get() {
        unsafe { (api.check_with_ui)() }
    }
}

pub fn cleanup() {
    if let Some(api) = API.get() {
        unsafe { (api.cleanup)() }
    }
}
