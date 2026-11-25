//! Start at login: a value named U4Ai under HKCU's
//! `Software\Microsoft\Windows\CurrentVersion\Run`, holding this program's
//! quoted path. The installer takes it away at uninstall.

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};

use crate::wide;

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "U4Ai";

pub fn on() -> bool {
    let (key, value) = (wide(RUN), wide(VALUE));
    // SAFETY: asks only whether the value is there, reading nothing back.
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) == ERROR_SUCCESS
    }
}

pub fn set(on: bool) -> Result<(), String> {
    let (key, value) = (wide(RUN), wide(VALUE));
    let done = if on {
        let program = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = wide(&format!("\"{}\"", program.display()));
        // SAFETY: a REG_SZ of the command's bytes, its ending nothing too.
        unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), REG_SZ, command.as_ptr().cast(), (command.len() * 2) as u32) }
    } else {
        // SAFETY: deletes the one value, if it's there.
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) } {
            2 => ERROR_SUCCESS, // ERROR_FILE_NOT_FOUND: it wasn't.
            other => other,
        }
    };
    if done == ERROR_SUCCESS { Ok(()) } else { Err(format!("the Run key said {done}")) }
}
