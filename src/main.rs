//! The `u4ai` binary: hands the core the crate for the system it was
//! built for, Windows' or Linux's. On a Mac, U4Claude is the app
//! (arcanii/Usage4Claude-Arcanii); a Mac crate here is on the backlog.

// No console window behind the tray in a release on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    if u4ai_core::answer_command_line() {
        return;
    }
    #[cfg(windows)]
    u4ai_windows::run();
    #[cfg(target_os = "linux")]
    u4ai_linux::run();
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        eprintln!("U4Ai is for Windows and Linux; on a Mac, use U4Claude (arcanii/Usage4Claude-Arcanii).");
        std::process::exit(1);
    }
}
