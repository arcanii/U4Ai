//! The updater on Linux, where there's no Sparkle: Flow's own, ported. It
//! reads the signed appcast for this computer's architecture from this
//! repository's `main` (linux/RELEASING.md), and installs a newer release
//! by running the release's own install.sh into the folder U4Ai runs
//! from, once the download's Ed25519 signature is the project's key's and
//! the u4ai inside says the feed's build. Only a copy install.sh put in
//! place updates itself, and only a build made as one of the project's
//! releases starts this (`version::UPDATES`). Its states show in the menu.

use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use std::io::Read;

use u4ai_core::feed::{self, Release};
use u4ai_core::i18n::tf;
use u4ai_core::{Event, UpdateState, Waker, APP_ID};

/// The feed for this architecture ("x86_64", "aarch64"). U4AI_UPDATE_FEED
/// points elsewhere, to try a feed before it's published: what it offers
/// must still be signed with the project's key.
fn feed_url() -> String {
    std::env::var("U4AI_UPDATE_FEED").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| {
        format!("https://raw.githubusercontent.com/arcanii/U4Ai/main/appcast-linux-{}.xml", std::env::consts::ARCH)
    })
}

/// The biggest download taken: a release is a few MB.
const MAX_DOWNLOAD: u64 = 128 << 20;

struct Updater {
    /// The folder install.sh put U4Ai in: it is `<prefix>/bin/u4ai`.
    prefix: PathBuf,
    feed: String,
    waker: Waker,
    state: Mutex<UpdateState>,
    /// What the last check found, for `install`.
    found: Mutex<Option<Release>>,
}

static UPDATER: OnceLock<Updater> = OnceLock::new();

/// Starts the updater if this copy is one install.sh put in place; run from
/// where it was built, or by a package manager's files, there's nothing for
/// it to replace.
pub fn init(waker: Waker) -> bool {
    if UPDATER.get().is_some() {
        return true;
    }
    let Some(prefix) = std::env::current_exe().ok().as_deref().and_then(install_prefix) else {
        log::info!("updater off: not a copy install.sh installed");
        return false;
    };
    let feed = feed_url();
    log::info!("updater on ({}, {feed})", u4ai_core::version::display());
    let _ = UPDATER.set(Updater { prefix, feed, waker, state: Mutex::new(UpdateState::Idle), found: Mutex::new(None) });
    true
}

/// The prefix of a program at `exe` that install.sh put there:
/// `<prefix>/bin/u4ai` beside its menu entry.
fn install_prefix(exe: &Path) -> Option<PathBuf> {
    let bin = exe.parent()?;
    let prefix = bin.parent()?;
    let installed = bin.file_name()? == "bin" && prefix.join(format!("share/applications/{APP_ID}.desktop")).is_file();
    installed.then(|| prefix.to_path_buf())
}

pub fn available() -> bool {
    UPDATER.get().is_some()
}

pub fn state() -> Option<UpdateState> {
    UPDATER.get().map(|u| lock(&u.state).clone())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Looks for a newer release on a thread of its own. A quiet check (the
/// daily one) changes nothing that shows unless it finds one.
pub fn check(quiet: bool) {
    let Some(u) = UPDATER.get() else { return };
    {
        let mut state = lock(&u.state);
        if matches!(*state, UpdateState::Checking | UpdateState::Installing { .. } | UpdateState::Installed { .. }) {
            return;
        }
        if !quiet {
            *state = UpdateState::Checking;
        }
    }
    u.waker.send(Event::Update);
    std::thread::spawn(move || {
        let current: u64 = u4ai_core::version::BUILD.parse().unwrap_or(0);
        let result = newest_release(&u.feed);
        let mut state = lock(&u.state);
        if matches!(*state, UpdateState::Installing { .. } | UpdateState::Installed { .. }) {
            return;
        }
        match result {
            Ok(Some(release)) if release.build > current => {
                log::info!("update: {} is out", release.display());
                *state = UpdateState::Available { version: release.display() };
                *lock(&u.found) = Some(release);
            }
            Ok(_) => {
                log::info!("update: {} is the newest", u4ai_core::version::display());
                if !quiet {
                    *state = UpdateState::UpToDate;
                }
            }
            Err(error) => {
                log::warn!("update check failed: {error}");
                *state = if quiet { UpdateState::Idle } else { UpdateState::Failed(error) };
            }
        }
        drop(state);
        u.waker.send(Event::Update);
    });
}

/// Downloads, checks and installs what the last check found, on a thread of
/// its own.
pub fn install() {
    let Some(u) = UPDATER.get() else { return };
    let Some(release) = lock(&u.found).clone() else { return };
    let version = release.display();
    {
        let mut state = lock(&u.state);
        if !matches!(*state, UpdateState::Available { .. }) {
            return;
        }
        // Installed as root, say: the user installs it as they did this one.
        // Checked before anything is replaced.
        if !install_writable(&u.prefix) {
            let folder = u.prefix.display().to_string();
            *state = UpdateState::Failed(tf("update.not_writable", &[("version", &version), ("folder", &folder)]));
            drop(state);
            u.waker.send(Event::Update);
            return;
        }
        *state = UpdateState::Installing { version: version.clone() };
    }
    u.waker.send(Event::Update);
    std::thread::spawn(move || {
        let next = match install_release(&release, &u.prefix, u4ai_core::UPDATE_PUBLIC_KEY, &cache_dir()) {
            Ok(()) => {
                log::info!("update: installed {version} in {}", u.prefix.display());
                UpdateState::Installed { version }
            }
            Err(error) => {
                log::warn!("update of {version} failed: {error:?}");
                UpdateState::Failed(error.say(&version))
            }
        };
        *lock(&u.state) = next;
        u.waker.send(Event::Update);
    });
}

/// Starts the copy install.sh just put in place; this one then quits.
pub fn relaunch() -> bool {
    let Some(u) = UPDATER.get() else { return false };
    let program = u.prefix.join("bin/u4ai");
    // Told it's handed over, the new one waits for this one to let go of
    // the instance lock rather than stopping at it.
    match Command::new(&program).env(u4ai_core::HANDED_OVER, "1").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(_) => true,
        Err(err) => {
            log::warn!("could not start {}: {err}", program.display());
            false
        }
    }
}

/// The user's own cache folder, where the download is unpacked: unlike
/// /tmp, no one else can put anything in the way there.
fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir);
    base.join(APP_ID)
}

/// Whether install.sh can write everywhere it puts something under
/// `prefix`. A folder it would make is tried by the nearest one there is.
fn install_writable(prefix: &Path) -> bool {
    let icons = prefix.join("share/icons/hicolor");
    let mut dirs = vec![prefix.join("bin"), prefix.join("share/applications"), prefix.join("share/doc/u4ai"), icons.clone()];
    dirs.extend(std::fs::read_dir(&icons).into_iter().flatten().flatten().map(|e| e.path().join("apps")));
    dirs.iter().all(|dir| dir.ancestors().find(|d| d.is_dir()).is_some_and(writable))
}

/// Whether U4Ai can put files in `dir`: tried, not guessed from modes.
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".u4ai-update-{}", std::process::id()));
    let ok = std::fs::OpenOptions::new().write(true).create_new(true).open(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(10 * 60)))
        .user_agent(format!("U4Ai/{} (linux)", u4ai_core::version::VERSION))
        .build()
        .into()
}

/// The newest release the feed at `url` offers, or None if it offers none.
/// The error is for the user, in their language.
fn newest_release(url: &str) -> Result<Option<Release>, String> {
    let failed = |error: &dyn std::fmt::Display| tf("update.check_failed", &[("error", &error)]);
    let text = match agent().get(url).call() {
        Ok(mut resp) => resp.body_mut().with_config().limit(1 << 20).read_to_string().map_err(|e| failed(&e))?,
        // No feed for this architecture yet.
        Err(ureq::Error::StatusCode(404)) => return Err(tf("update.no_release", &[("arch", &std::env::consts::ARCH)])),
        Err(e) => return Err(failed(&e)),
    };
    Ok(feed::newest(&text))
}

/// Why an install didn't happen.
#[derive(Debug)]
enum UpdateError {
    Download(String),
    /// The download isn't what the project's key signed.
    NotSigned,
    Install(String),
}

impl UpdateError {
    fn say(&self, version: &str) -> String {
        match self {
            UpdateError::Download(e) => tf("update.download_failed", &[("version", &version), ("error", e)]),
            UpdateError::NotSigned => tf("update.not_signed", &[("version", &version)]),
            UpdateError::Install(e) => tf("update.install_failed", &[("version", &version), ("error", e)]),
        }
    }
}

/// Downloads `release`, checks that `key` (base64) signed it, unpacks it
/// under `temp`, checks its u4ai is that build, and runs its install.sh
/// into `prefix`. Nothing is unpacked unless the signature holds.
fn install_release(release: &Release, prefix: &Path, key: &str, temp: &Path) -> Result<(), UpdateError> {
    if release.length == 0 || release.length > MAX_DOWNLOAD {
        return Err(UpdateError::Download(format!("the feed gives its size as {} bytes", release.length)));
    }
    let bytes = download(&release.url, release.length)?;
    if !feed::signed(&bytes, &release.signature, key) {
        return Err(UpdateError::NotSigned);
    }
    // A folder of its own, made new and private: a name someone else put
    // there first (a link, say) is an error, not a place to unpack into.
    std::fs::create_dir_all(temp).map_err(|e| UpdateError::Install(e.to_string()))?;
    let dir = temp.join(format!("update-{}-{}", std::process::id(), release.build));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new().mode(0o700).create(&dir).map_err(|e| UpdateError::Install(e.to_string()))?;
    let result = unpack_and_install(&bytes, &dir, release, prefix);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn download(url: &str, length: u64) -> Result<Vec<u8>, UpdateError> {
    let mut resp = agent().get(url).call().map_err(|e| UpdateError::Download(e.to_string()))?;
    let mut bytes = Vec::with_capacity(length as usize);
    resp.body_mut().with_config().limit(length + 1).reader().read_to_end(&mut bytes).map_err(|e| UpdateError::Download(e.to_string()))?;
    if bytes.len() as u64 != length {
        return Err(UpdateError::Download(format!("{} bytes came, not {length}", bytes.len())));
    }
    Ok(bytes)
}

fn unpack_and_install(bytes: &[u8], dir: &Path, release: &Release, prefix: &Path) -> Result<(), UpdateError> {
    let fail = |e: &dyn std::fmt::Display| UpdateError::Install(e.to_string());
    let tarball = dir.join("release.tar.gz");
    std::fs::write(&tarball, bytes).map_err(|e| fail(&e))?;
    run(Command::new("tar").arg("-xzf").arg(&tarball).arg("-C").arg(dir))?;
    // u4ai-<version>/, as package.sh lays it out.
    let top = std::fs::read_dir(dir)
        .map_err(|e| fail(&e))?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir() && p.join("install.sh").is_file())
        .ok_or_else(|| fail(&"it holds no install.sh"))?;
    // The u4ai inside must be the build the feed named, as the release
    // scripts check before publishing.
    let out = Command::new(top.join("bin/u4ai")).arg("--version").stdin(Stdio::null()).output().map_err(|e| fail(&e))?;
    let reported = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("").trim().to_string();
    if !reported.ends_with(&format!("(build {})", release.build)) {
        return Err(fail(&format!("its u4ai says {reported:?}")));
    }
    run(Command::new("sh").arg(top.join("install.sh")).arg("--prefix").arg(prefix))
}

/// Runs `cmd`, failing with the last line it wrote to stderr.
fn run(cmd: &mut Command) -> Result<(), UpdateError> {
    let out = cmd.stdin(Stdio::null()).output().map_err(|e| UpdateError::Install(e.to_string()))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
    Err(UpdateError::Install(if last.is_empty() { out.status.to_string() } else { last }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    use ed25519_dalek::{Signer, SigningKey};

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[3u8; 32])
    }

    fn public(key: &SigningKey) -> String {
        STANDARD.encode(key.verifying_key().to_bytes())
    }

    /// A release laid out as package.sh lays one out, whose u4ai says
    /// it's `build`, with the repo's own install.sh and menu entry.
    fn tarball(work: &Path, build: u64) -> Vec<u8> {
        let top = work.join("u4ai-9.9.9");
        for dir in ["bin", "share/applications", "share/icons/hicolor/32x32/apps", "share/doc/u4ai"] {
            std::fs::create_dir_all(top.join(dir)).unwrap();
        }
        let program = top.join("bin/u4ai");
        std::fs::write(&program, format!("#!/bin/sh\necho 'U4Ai 9.9.9 (build {build})'\n")).unwrap();
        std::fs::set_permissions(&program, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        std::fs::copy(here.join(format!("packaging/{APP_ID}.desktop")), top.join(format!("share/applications/{APP_ID}.desktop"))).unwrap();
        std::fs::copy(here.join("../assets/icon/32.png"), top.join(format!("share/icons/hicolor/32x32/apps/{APP_ID}.png"))).unwrap();
        std::fs::write(top.join("share/doc/u4ai/LICENSE"), "MIT\n").unwrap();
        std::fs::copy(here.join("scripts/install.sh"), top.join("install.sh")).unwrap();
        let out = work.join("release.tar.gz");
        assert!(Command::new("tar").arg("-czf").arg(&out).arg("-C").arg(work).arg("u4ai-9.9.9").status().unwrap().success());
        std::fs::read(out).unwrap()
    }

    fn feed(base: &str, build: u64, bytes: &[u8], signature: &str) -> Vec<u8> {
        format!(
            r#"<rss xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle"><channel><item>
<enclosure url="{base}/release.tar.gz" sparkle:version="{build}" sparkle:shortVersionString="9.9.9" sparkle:edSignature="{signature}" length="{}" type="application/octet-stream"/>
</item></channel></rss>"#,
            bytes.len()
        )
        .into_bytes()
    }

    /// Serves `files` (made from the server's own base URL) until the test
    /// ends; anything else is a 404. Its base URL.
    fn serve_with(files: impl FnOnce(&str) -> Vec<(String, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let files = files(&base);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                let _ = reader.read_line(&mut first);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                }
                let path = first.split_whitespace().nth(1).unwrap_or("");
                let mut out = stream;
                match files.iter().find(|(p, _)| p == path) {
                    Some((_, body)) => {
                        let _ = write!(out, "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
                        let _ = out.write_all(body);
                    }
                    None => {
                        let _ = write!(out, "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
                    }
                }
            }
        });
        base
    }

    #[test]
    fn only_a_copy_install_sh_put_in_place_counts_as_installed() {
        let root = std::env::temp_dir().join(format!("u4ai-prefix-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("bin")).unwrap();
        assert_eq!(install_prefix(&root.join("bin/u4ai")), None, "no menu entry beside it");
        std::fs::create_dir_all(root.join("share/applications")).unwrap();
        std::fs::write(root.join(format!("share/applications/{APP_ID}.desktop")), "").unwrap();
        assert_eq!(install_prefix(&root.join("bin/u4ai")), Some(root.clone()));
        assert_eq!(install_prefix(&root.join("target/release/u4ai")), None);
        assert!(install_writable(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_signed_release_installs_over_u4ai_and_a_tampered_one_doesnt() {
        let root = std::env::temp_dir().join(format!("u4ai-updater-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (work, prefix, temp) = (root.join("work"), root.join("prefix with space"), root.join("temp"));
        for d in [&work, &prefix, &temp] {
            std::fs::create_dir_all(d).unwrap();
        }
        let build = 99_999;
        let bytes = tarball(&work, build);
        let key = key();
        let signature = STANDARD.encode(key.sign(&bytes).to_bytes());
        let mut tampered = bytes.clone();
        *tampered.last_mut().unwrap() ^= 1;

        let good = serve_with(|base| vec![("/feed.xml".into(), feed(base, build, &bytes, &signature)), ("/release.tar.gz".into(), bytes.clone())]);
        let release = newest_release(&format!("{good}/feed.xml")).unwrap().expect("a release");
        assert_eq!(release.display(), "9.9.9 (build 99999)");
        assert!(matches!(newest_release(&format!("{good}/none.xml")), Err(e) if e.contains(std::env::consts::ARCH)));

        // A tampered download is turned away before anything is unpacked.
        let bad = serve_with(|base| vec![("/feed.xml".into(), feed(base, build, &tampered, &signature)), ("/release.tar.gz".into(), tampered.clone())]);
        let bad = newest_release(&format!("{bad}/feed.xml")).unwrap().unwrap();
        assert!(matches!(install_release(&bad, &prefix, &public(&key), &temp), Err(UpdateError::NotSigned)));
        assert!(!prefix.join("bin/u4ai").exists());
        // As is one signed by another key, the project's own among them.
        assert!(matches!(install_release(&release, &prefix, u4ai_core::UPDATE_PUBLIC_KEY, &temp), Err(UpdateError::NotSigned)));

        // A feed naming another build than the tarball's u4ai is refused.
        let wrong = Release { build: build + 1, ..release.clone() };
        let result = install_release(&wrong, &prefix, &public(&key), &temp);
        assert!(matches!(result, Err(UpdateError::Install(ref e)) if e.contains("99999")), "{result:?}");
        assert!(!prefix.join("bin/u4ai").exists());

        install_release(&release, &prefix, &public(&key), &temp).unwrap();
        let out = Command::new(prefix.join("bin/u4ai")).arg("--version").output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "U4Ai 9.9.9 (build 99999)");
        let entry = std::fs::read_to_string(prefix.join(format!("share/applications/{APP_ID}.desktop"))).unwrap();
        assert!(entry.contains(&format!("Exec=\"{}/bin/u4ai\"", prefix.display())), "{entry}");
        assert!(prefix.join(format!("share/icons/hicolor/32x32/apps/{APP_ID}.png")).is_file());
        assert_eq!(install_prefix(&prefix.join("bin/u4ai")), Some(prefix.clone()), "and it updates itself next time");
        assert_eq!(std::fs::read_dir(&temp).unwrap().count(), 0, "nothing left behind");
        let _ = std::fs::remove_dir_all(&root);
    }
}
