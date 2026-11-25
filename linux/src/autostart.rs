//! Start at login: an XDG autostart entry,
//! `$XDG_CONFIG_HOME/autostart/com.arcanii.U4Ai.desktop`, that runs
//! this copy by its full path. install.sh's `--uninstall` takes it away
//! where it runs that prefix's copy, so it's written as install.sh reads it
//! (`Exec="<path>"`).

use std::path::{Path, PathBuf};

use u4ai_core::{APP_ID, APP_NAME};

fn entry_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| dirs::home_dir().map(|h| h.join(".config")))?;
    Some(config.join("autostart").join(format!("{APP_ID}.desktop")))
}

/// Whether the entry is there and not turned off (`Hidden=true`, or GNOME's
/// own switch off).
pub fn on() -> bool {
    entry_path().and_then(|p| std::fs::read_to_string(p).ok()).is_some_and(|text| enabled(&text))
}

fn enabled(text: &str) -> bool {
    !text.lines().any(|l| matches!(l.trim(), "Hidden=true" | "X-GNOME-Autostart-enabled=false"))
}

/// This program's path: after an update the running one's is marked
/// " (deleted)", and the new one is where it was.
fn program() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let text = exe.to_string_lossy();
    Ok(PathBuf::from(text.strip_suffix(" (deleted)").unwrap_or(&text)))
}

/// A path as a desktop entry's Exec takes it: quoted, with `"`, `` ` ``,
/// `$` and `\` escaped, and `%` (a field code) doubled.
fn exec_word(path: &Path) -> String {
    let mut out = String::from("\"");
    for c in path.to_string_lossy().chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '%' => out.push_str("%%"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn entry(program: &Path) -> String {
    format!("[Desktop Entry]\nType=Application\nName={APP_NAME}\nExec={}\nIcon={APP_ID}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n", exec_word(program))
}

pub fn set(on: bool) -> Result<(), String> {
    let path = entry_path().ok_or("no home folder")?;
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(path.parent().expect("in autostart/")).map_err(|e| e.to_string())?;
    let temp = path.with_extension("desktop.new");
    std::fs::write(&temp, entry(&program()?)).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_runs_this_copy_by_its_quoted_path_as_install_sh_reads_it() {
        let text = entry(Path::new("/home/me/.local/bin/u4ai"));
        assert!(text.contains("\nExec=\"/home/me/.local/bin/u4ai\"\n"), "{text}");
        assert!(enabled(&text));
        assert_eq!(exec_word(Path::new("/odd $place/50%/a\"b")), r#""/odd \$place/50%%/a\"b""#);
        assert!(!enabled("[Desktop Entry]\nHidden=true\n"));
    }
}
