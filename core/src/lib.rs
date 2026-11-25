//! U4Ai's core, shared by Windows and Linux and calling no OS API: the AIs
//! whose limits it reads (`providers`, Claude the first, with what their
//! sign-ins and requests share in `oauth` and `http`), the accounts and
//! settings (`config`, `secrets`), when to read (`refresh`), when to tell of
//! a limit (`alerts`), the tray as data (`tray`, `icon`), the update feed
//! (`feed`), the words (`i18n`) and the loop that runs them (`app`). Each OS
//! crate implements `platform::Platform` and hands it to [`run`].
//! docs/design/u4ai.md has the plan.

pub mod alerts;
pub mod app;
pub mod config;
pub mod feed;
pub mod http;
pub mod i18n;
pub mod icon;
pub mod oauth;
pub mod platform;
pub mod providers;
pub mod refresh;
pub mod secrets;
pub mod tray;
pub mod version;

use std::fs::File;
use std::path::Path;

pub use platform::{Action, Event, Platform, UpdateState, Updater, Waker};

pub const APP_NAME: &str = "U4Ai";
/// Linux's app id and desktop file, the credential store's service, and
/// the Mac app's bundle id.
pub const APP_ID: &str = "com.arcanii.U4Ai";
/// The Ed25519 key every update must be signed with: the Mac app's
/// `SUPublicEDKey`, so one key signs all three systems' releases.
pub const UPDATE_PUBLIC_KEY: &str = "hGTiB0kyn45HOB8WWKdAHc28+Bthe8Rv8O7asa4nG2c=";

/// Answers what the command line asks before anything starts, as
/// `--version` (which Linux's updater and the release scripts ask of a
/// copy about to be installed, with no display); true where it was asked.
pub fn answer_command_line() -> bool {
    if std::env::args().skip(1).any(|a| a == "--version" || a == "-V") {
        println!("{}", version::answer(std::env::var("U4AI_SAY_UPDATES").as_deref() == Ok("1")));
        return true;
    }
    false
}

/// Logs to stderr in a debug build (`RUST_LOG` works), and to
/// `u4ai.log` in U4Ai's folder in a release, which has no console
/// on Windows.
fn start_logging(dir: &Path) {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if !cfg!(debug_assertions) {
        if let Ok(file) = File::create(dir.join("u4ai.log")) {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
    }
    let _ = builder.try_init();
}

/// Set for a copy an updater started in its own place: it waits for the
/// one quitting to let go of the instance lock.
pub const HANDED_OVER: &str = "U4AI_HANDED_OVER";
/// How long a copy handed over waits.
const HANDOVER_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// The lock only one running U4Ai holds, so a second start (by hand
/// beside the one started at login) adds no second icon; none where
/// another holds it.
fn only_one(dir: &Path) -> Option<File> {
    let file = File::options().create(true).truncate(false).write(true).open(dir.join("instance.lock")).ok()?;
    let handed_over = std::env::var_os(HANDED_OVER).is_some();
    // Not for the programs it starts.
    std::env::remove_var(HANDED_OVER);
    let until = std::time::Instant::now() + HANDOVER_WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Some(file),
            Err(std::fs::TryLockError::WouldBlock) if handed_over && std::time::Instant::now() < until => {
                std::thread::sleep(std::time::Duration::from_millis(100))
            }
            Err(std::fs::TryLockError::WouldBlock) => return None,
            // A filesystem without locks: let it run.
            Err(std::fs::TryLockError::Error(err)) => {
                log::warn!("no instance lock: {err}");
                return Some(file);
            }
        }
    }
}

/// Runs U4Ai on the platform `make` builds, until it quits.
pub fn run(make: impl FnOnce(Waker) -> Box<dyn Platform>) {
    let dir = config::dir();
    let _ = std::fs::create_dir_all(&dir);
    // Before the log, which a release starts afresh: a second start says
    // nothing and leaves the first one's log as it is.
    let Some(_lock) = only_one(&dir) else { return };
    start_logging(&dir);
    log::info!("{} {} on {}", APP_NAME, version::display(), std::env::consts::OS);
    let (tx, rx) = std::sync::mpsc::channel();
    let waker = Waker(tx);
    let platform = make(waker.clone());
    app::App::new(platform, waker, dir).run(rx);
}
