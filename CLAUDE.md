# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

U4Ai is a tray app in Rust for Windows and Linux that shows how much of an
AI's usage limits are used, Claude's the first of its providers. Claude's
part ports the Mac's U4Claude (arcanii/Usage4Claude-Arcanii, Swift, a fork
of f-is-h/Usage4Claude), whose Swift code is its spec; the Mac keeps
U4Claude, and a Mac crate here is on the backlog. It's laid out as Flow is
(arcanii/Flow-Terminal, checked out as `../Flow_Terminal`), whose
updaters, credential storage, loopback sign-in and release scripts it
took. The plan, kept up to date as it changes, is `docs/design/u4ai.md`;
a new feature's design goes in `docs/design/<feature>.md` too, and what's
planned but not begun in `BACKLOG.md`.

## Commands

```bash
cargo run                                   # debug build, logs to stderr (RUST_LOG works)
cargo test --workspace                      # core's tests, and the OS crate's for this system
cargo test -p u4ai-core usage           # one module
cargo clippy --workspace --all-targets
cargo clippy --target x86_64-pc-windows-msvc --workspace --all-targets   # Windows' code, type-checked from Linux
cargo run --example update_key -- trusted   # the update key's public half (also new, public, sign, verify)
U4AI_SAY_UPDATES=1 cargo run -- --version
```

- Don't run `cargo fmt`; match the surrounding style (long lines are fine).
- `cargo clippy` is clean on both targets; keep it so.
- Each OS crate is `#![cfg(...)]`-gated with target-only dependencies, so a
  build on one OS never builds the other's. Windows' code can be
  type-checked from Linux (above): core takes ureq's native-tls on Windows
  (Schannel, no C compiler), rustls elsewhere.
- A release build logs to `u4ai.log` in U4Ai's folder
  (`~/.config/U4Ai`, `%APPDATA%\U4Ai`), beside `settings.json`.
- Try it live on Linux with a scratch home, so it touches neither your
  settings nor your keyring: `HOME=<dir> XDG_CONFIG_HOME=<dir>/.config
  XDG_CACHE_HOME=<dir>/.cache ./target/debug/u4ai`, and read its item
  over D-Bus (`org.kde.StatusNotifierWatcher`'s
  `RegisteredStatusNotifierItems`, then the item's `/MenuBar`
  `com.canonical.dbusmenu.GetLayout`). Stop it by its PID.

## Architecture

- **core/** (`u4ai-core`) calls no OS API. `providers/` holds the AIs
  (`claude/` the first) and what U4Ai knows of any: `Provider` (its id,
  name, status page, `sign_in`, `reader`), `Reading` of `Meter`s (key,
  label, `Window`: short, long or a model's, `Limit`) and lines,
  `ReadError`, the `Reader` trait, `SignIn` and `NewAccount`. A second
  provider is a module and an arm in each of `Provider`'s matches.
  Accounts are known by `provider:id` (`Account::key`) in the settings,
  the credential store's names and the alerts. `app.rs` is the loop: one
  thread holding how things stand, taking `platform::Event`s through a
  `Waker` (menu actions, reads and sign-ins back from their threads, the
  updater), reading when `refresh.rs` says, telling of limits
  (`alerts.rs`), and handing the OS crate a `tray::TrayView` (rings,
  tooltip, menu entries as data) whenever it changes. Network and
  credential-store calls run on threads of their own.
- `providers/claude/`: `api.rs`, Anthropic's OAuth endpoints (token,
  usage, profile) and `Session`, which keeps an access token and trades the
  refresh token again on a 401; `usage.rs`, the usage answer, ported from
  the Mac's `ClaudeAPIResponseModels.swift` with its tests' JSON (the
  5-hour window optional, the 7-day a placeholder where missing, the two
  weekly slots and `limits[]`' named models); `sign_in.rs`, Claude Code's
  public client on `localhost:1456` or 1458; `mod.rs`, its meters. Shared:
  `oauth.rs` (PKCE, the loopback listener on the loopback addresses alone,
  a pasted callback link), `http.rs` (the client, statuses, `Retry-After`).
  `secrets.rs`: each account's secret in the credential store, in pieces
  where long.
  `config.rs`: `settings.json`. `icon.rs`: the rings drawn in code at any
  size, and the outer one's percentage in a pixel font of its own on whole
  pixels (`docs/design/tray-numbers.md`; `U4AI_ICON_PICTURES=<dir>` has a
  test write icons out as raw RGBA). `feed.rs`: Sparkle's appcast and the Ed25519 check. `i18n.rs` and
  `core/locales/*.json`: every word the user reads, five languages; the
  tests check every locale has every key with the same placeholders, and
  that every `"section.key"` literal in the sources exists.
  `version.rs`: stamped by `core/build.rs` (build number = commit count +
  `build_offset`; `U4AI_UPDATES=1` at build time turns the updater on).
- **linux/**: the tray through ksni (StatusNotifierItem over D-Bus, no
  GTK; the user is told of no tray only where none answered at the
  start, `tray::no_tray_at_all`, since GNOME's AppIndicator extension
  is off while the screen is locked and ksni registers again when it's
  back; its clicks are in BACKLOG.md's drop-down), notifications through gdbus/notify-send, Start at login as an XDG
  autostart entry, and `updater.rs`, Flow's own Linux updater ported (signed
  tarball, its `install.sh` run into the prefix it was installed in).
- **windows/**: the tray through Shell_NotifyIconW on a thread with a hidden
  window (Flow's `tray.rs`), balloons for notifications, Start at login in
  HKCU's Run key, WinSparkle (`updater.rs`, Flow's), and the `U4Ai.Running`
  mutex the installer's AppMutex looks for.
- **src/main.rs** answers `--version` before anything starts, then runs the
  OS crate.

## Releases

- Version: `version` under `[workspace.package]`; `patch`, `dev` and
  `build_offset` under `[workspace.metadata.u4ai]`, as Flow's. While
  work goes on toward a version, `dev = true` (shown as `0.1.0.DEV`);
  releasing sets it false, and once every system's release is out the next
  commit bumps `version` and sets it true again.
- One Ed25519 key signs all three systems' updates: the Mac app's
  `SUPublicEDKey` is `UPDATE_PUBLIC_KEY` in `core/src/lib.rs`. Its private
  half is in the Mac's login Keychain (and a copy at
  `~\.u4ai-signing\u4ai-update.key` on Windows); never in the repo or
  in CI. Linux tarballs are signed on the Mac (`windows/scripts/sign-release.sh`).
- Releases go in this repository, tagged `v<ver>-windows` and `v<ver>-linux`;
  the feeds are `appcast-windows.xml` and `appcast-linux-<arch>.xml` on
  `main`. Publishing a feed updates every installed copy, so publish only
  when asked. Follow `windows/RELEASING.md` and `linux/RELEASING.md`.

## Conventions

- Comments and docs are plain English that say why; each module starts with
  a `//!` summary. Tests are named as sentences.
- Words the user reads go through `i18n::t` / `tf`, in every locale file.
- Commit messages: an imperative subject and a body that says why, ending
  with what was checked, on which OS, and what wasn't.
