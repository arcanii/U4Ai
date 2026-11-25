# U4Ai

<img src="assets/icon/256.png" width="128" alt="U4Ai icon">

How much of your AI's usage limits you've used, in the tray on Windows and
Linux. Claude first: the 5-hour window, the 7-day window, the weekly
per-model limits (Opus, Sonnet, Fable…) and Extra Usage, with a
notification when a limit reaches 90% and when it starts again. Other AIs
come next ([the backlog](BACKLOG.md)).

Claude's part follows
[U4Claude for the Mac](https://github.com/arcanii/Usage4Claude-Arcanii),
itself a fork of [f-is-h/Usage4Claude](https://github.com/f-is-h/Usage4Claude).
All credit for the app's design goes to [@f-is-h](https://github.com/f-is-h).
The Mac app is Swift and SwiftUI, none of which builds here, so this is a
rewrite in Rust that follows it. On a Mac, use U4Claude.

> **Status: a scaffold.** The tray icon, its menu, sign-in, refreshing,
> alerts and self-updates work; the popover window with its large rings and
> settings comes next. See [the plan](docs/design/u4ai.md).

## What it does

- **Two rings in the tray**, as the Mac's: the 5-hour window outside (green,
  orange from 70%, red from 90%) and the 7-day inside. On a plan without a
  5-hour window, the 7-day takes the outer ring.
- **A menu** with whose usage it is, each limit and how long until it
  starts again, Refresh, your accounts, Sign in with Claude, Sign out,
  Start at login, updates, Claude Status and Quit.
- **Sign in with Claude** in your own browser (Google, Microsoft, SSO and
  passkeys all work), through Claude's OAuth with PKCE. The refresh token
  stays in the system's credential store: Windows' Credential Manager, or
  the Secret Service (GNOME Keyring, KWallet) on Linux.
- **Smart refreshing**: every minute while usage moves, slowing to every 10
  minutes while it doesn't; it backs off when the API asks it to.
- **Updates itself** from signed releases: WinSparkle on Windows, its own
  updater on Linux. Every update is checked against the project's Ed25519
  key before it's installed.
- In English, 日本語, 한국어, 简体中文 and 繁體中文.

Network calls go to `console.anthropic.com` and `api.anthropic.com` (your
usage and profile), and `raw.githubusercontent.com` and `github.com` for
updates. Signing in opens a short-lived listener on `localhost`, on the
loopback addresses only. No telemetry.

## Install

From [Releases](https://github.com/arcanii/U4Ai/releases):

- **Windows 10 and 11 (x64)**: `U4Ai-<ver>-setup.exe` installs for you
  alone, no admin needed, and keeps itself up to date. The portable zip
  runs from anywhere and doesn't update itself.
- **Linux (x86-64, glibc 2.31 or newer)**: unpack
  `U4Ai-<ver>-linux-x86_64.tar.gz` and run `./install.sh` from it, which
  puts U4Ai in `~/.local` (`--prefix` for elsewhere, `--uninstall` to
  take it away). That copy keeps itself up to date.
  The tray is a StatusNotifierItem: KDE shows it; on GNOME, turn on the
  AppIndicator extension (Ubuntu has it on already).

## Build

```bash
cargo run                       # a debug build, logging to stderr
cargo test --workspace
cargo clippy --workspace --all-targets
cargo clippy --target x86_64-pc-windows-msvc --workspace --all-targets   # checks Windows' code from Linux
```

Releases are built by `windows/scripts/release.ps1` and
`linux/scripts/package.sh`; see [windows/RELEASING.md](windows/RELEASING.md)
and [linux/RELEASING.md](linux/RELEASING.md).

## Documentation

- [The plan](docs/design/u4ai.md): why it's its own repository and name,
  how providers, signing in and updates work.
- [The backlog](BACKLOG.md): what comes next, other AIs and the Mac among
  it.
- [Releasing on Windows](windows/RELEASING.md) and
  [on Linux](linux/RELEASING.md).

## License

MIT, as the Mac app and f-is-h's original: see [LICENSE](LICENSE).
