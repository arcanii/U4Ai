//! The version shown to people and to the updaters (stamped by
//! core/build.rs).

/// The marketing version from `[workspace.package]` in the root Cargo.toml,
/// with a patch release's number after it: "0.1.0" or "0.1.0.1", numbers
/// alone, DEV or not.
pub const VERSION: &str = env!("U4AI_VERSION");
/// The work toward `VERSION` goes on (`dev = true` under
/// `[workspace.metadata.u4ai]`): not released, shown as "0.1.0.DEV".
pub const DEV: bool = matches!(env!("U4AI_DEV").as_bytes(), b"true");
/// The build number: the git commit count at build time plus
/// `build_offset`. Linux's updater compares it with its feed's.
pub const BUILD: &str = env!("U4AI_BUILD");
/// "0.1.0.42": what WinSparkle compares with its feed's `sparkle:version`.
/// The marketing version without the patch, so the build number stays the
/// fourth number and keeps rising.
pub const FULL: &str = env!("U4AI_VERSION_FULL");

/// Made as one of this project's releases, whose updater reads its feeds:
/// `U4AI_UPDATES=1` as core built, which the release scripts set only
/// with the update key at hand. A plain `cargo build --release`, a fork's
/// among them, has no updater, so it never offers this project's release in
/// its place.
pub const UPDATES: bool = matches!(env!("U4AI_UPDATES").as_bytes(), b"true");

/// The releases, every system's.
pub const RELEASES: &str = "https://github.com/arcanii/U4Ai/releases";

/// The version as people see it: `VERSION`, with ".DEV" after it while
/// it's still worked toward.
pub fn shown() -> String {
    if DEV { format!("{VERSION}.DEV") } else { VERSION.to_string() }
}

/// "0.1.0 (build 42)", or "0.1.0.DEV (build 42)" while it's worked toward.
pub fn display() -> String {
    format!("{} (build {BUILD})", shown())
}

/// Whether the updater starts: a release build made for updates, or any
/// build `U4AI_UPDATER=1` asks it of, for testing; never otherwise a
/// debug build, which would offer to "update" to the last release.
pub fn updater_on(updates: bool, debug: bool, forced: bool) -> bool {
    forced || (updates && !debug)
}

/// Whether this run starts the updater (`updater_on` for this build).
pub fn updater_wanted() -> bool {
    updater_on(UPDATES, cfg!(debug_assertions), std::env::var("U4AI_UPDATER").as_deref() == Ok("1"))
}

/// `u4ai --version`'s answer: "U4Ai 0.1.0 (build 42)", and with
/// `say_updates` (`U4AI_SAY_UPDATES=1`) a line after it saying whether
/// the build updates itself, which the release scripts read to check that
/// what they package is as they decided. Linux's updater reads the first
/// line's "(build N)" of a release it's about to install.
pub fn answer(say_updates: bool) -> String {
    let version = format!("{} {}", crate::APP_NAME, display());
    if !say_updates {
        return version;
    }
    format!("{version}\nupdates: {}", if UPDATES { "on" } else { "off" })
}

/// This version's release page: each system tags its own, `v0.1.0-windows`
/// and `v0.1.0-linux`. A DEV build's is the list, which is always there.
pub fn release_page() -> String {
    if DEV {
        return RELEASES.to_string();
    }
    let system = if cfg!(windows) { "windows" } else { "linux" };
    format!("{RELEASES}/tag/v{VERSION}-{system}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_release_made_for_updates_or_a_build_asked_to_starts_the_updater() {
        assert!(updater_on(true, false, false), "a release");
        assert!(!updater_on(false, false, false), "a plain cargo build --release, a fork's");
        assert!(!updater_on(true, true, false), "a debug build would offer the last release");
        assert!(updater_on(false, true, true) && updater_on(false, false, true), "U4AI_UPDATER=1, for testing");
    }

    #[test]
    fn the_version_says_whether_the_build_updates_only_when_asked() {
        assert_eq!(answer(false), format!("U4Ai {}", display()));
        let said = answer(true);
        let (first, second) = said.split_once('\n').expect("two lines");
        assert_eq!(first, answer(false));
        assert_eq!(second, if UPDATES { "updates: on" } else { "updates: off" });
        assert!(first.ends_with(&format!("(build {BUILD})")), "Linux's updater reads the build there");
    }

    #[test]
    fn a_dev_build_says_so_after_its_version_and_only_there() {
        assert!(VERSION.split('.').all(|n| n.parse::<u32>().is_ok()), "{VERSION}: numbers alone");
        assert_eq!(shown().ends_with(".DEV"), DEV);
        assert!(FULL.starts_with(env!("CARGO_PKG_VERSION")) && FULL.ends_with(&format!(".{BUILD}")));
    }

    #[test]
    fn a_release_links_to_its_own_systems_tag() {
        let page = release_page();
        if DEV {
            assert_eq!(page, RELEASES);
        } else {
            assert!(page.starts_with(&format!("{RELEASES}/tag/v{VERSION}-")), "{page}");
        }
    }
}
