//! Stamps the version and build number into the core for `version.rs`:
//! the marketing version (`version` under `[workspace.package]` in the root
//! Cargo.toml, with a patch release's number after it), whether it's still
//! worked toward (`dev`), the build number (the git commit count), and
//! whether the build updates itself: `U4AI_UPDATES=1` while core builds,
//! which the release scripts set only with the update key at hand
//! (docs/design/u4ai.md, Updates).

mod build_number;

fn main() {
    let dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_number.rs");
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let build = build_number::build_number(&dir);
    let root = dir.parent().expect("core/ sits in the workspace");
    println!("cargo:rustc-env=U4AI_VERSION={}", build_number::shown_version(root, &version));
    println!("cargo:rustc-env=U4AI_DEV={}", build_number::dev(root));
    println!("cargo:rustc-env=U4AI_BUILD={build}");
    println!("cargo:rustc-env=U4AI_VERSION_FULL={version}.{build}");
    println!("cargo:rerun-if-env-changed=U4AI_UPDATES");
    println!("cargo:rustc-env=U4AI_UPDATES={}", std::env::var("U4AI_UPDATES").as_deref() == Ok("1"));
}
