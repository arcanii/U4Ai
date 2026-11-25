// Used by core/build.rs.
//
// The build number is the git commit count plus `build_offset` under
// `[workspace.metadata.u4ai]` in the root Cargo.toml (0 where it isn't
// given), so it goes up with every commit and matches on every machine,
// which the updaters rely on. Outside a git checkout, or before the first
// commit, it is 0.

use std::path::Path;
use std::process::Command;

/// The build number for the checkout containing `dir`. Also asks Cargo to
/// rerun the build script when HEAD moves.
pub fn build_number(dir: &Path) -> u32 {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    };
    // The crate sits below the repo root, so ask git where .git is.
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        for file in ["HEAD", "packed-refs"] {
            let path = Path::new(&git_dir).join(file);
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
        // Every commit writes logs/HEAD. Named even before the first commit,
        // when it isn't there yet (and Cargo runs this each build until it
        // is), or that commit's build would keep build 0: HEAD itself still
        // says the same branch.
        println!("cargo:rerun-if-changed={}", Path::new(&git_dir).join("logs/HEAD").display());
    }
    let Some(count) = git(&["rev-list", "--count", "HEAD"]).and_then(|s| s.parse::<u32>().ok()) else {
        return 0;
    };
    let offset = git(&["rev-parse", "--show-toplevel"])
        .and_then(|root| metadata(Path::new(&root), "build_offset"))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    count.saturating_add(offset)
}

/// `key` under `[workspace.metadata.u4ai]` in the root Cargo.toml in
/// `root`, as written. Also asks Cargo to rerun the build script when that
/// file changes.
fn metadata(root: &Path, key: &str) -> Option<String> {
    let manifest = root.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let mut in_table = false;
    for line in std::fs::read_to_string(&manifest).unwrap_or_default().lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_table = line == "[workspace.metadata.u4ai]";
        } else if let Some((k, value)) = line.split_once('=').filter(|_| in_table) {
            if k.trim() == key {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

/// The version people see: `version`, then a patch release's number
/// (`patch` under `[workspace.metadata.u4ai]`), as 0.1.0.1; `version`
/// alone when there's none.
pub fn shown_version(root: &Path, version: &str) -> String {
    match metadata(root, "patch").and_then(|p| p.parse::<u32>().ok()).unwrap_or(0) {
        0 => version.to_string(),
        patch => format!("{version}.{patch}"),
    }
}

/// Whether the work toward `version` is still going on (`dev = true`), so
/// U4Ai shows it as 0.1.0.DEV.
pub fn dev(root: &Path) -> bool {
    metadata(root, "dev").as_deref() == Some("true")
}
