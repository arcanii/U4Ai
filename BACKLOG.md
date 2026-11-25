# Backlog

What's planned for U4Ai and not begun, roughly in order. The plan for what
is built is [docs/design/u4ai.md](docs/design/u4ai.md).

## Next

- **A popover window** (egui), as the Mac's U4Claude has: the rings large,
  a row per limit with its reset time, the manual paste for a sign-in whose
  redirect never came back (`oauth::pasted` reads it already), and
  settings (refresh mode, alerts, language).
- **Sparklines**: the Mac's 24-hour history, one NDJSON line a read.
- **Windows' own toasts** with an AppUserModelID (Flow's
  `windows/src/notify.rs`), in place of the tray's balloons.

## Providers

- **Codex** (ChatGPT's): upstream f-is-h/Usage4Claude reads its limits
  beside Claude's, and so does the Tauri port plan on upstream (issue 95,
  which also names Antigravity). A module in `core/src/providers/`.
- **Gemini**, and others where their limits can be read.
- **Several accounts shown at once**, once there's a second provider: a
  Claude account's and a Codex account's limits together, each under its
  provider, rather than one account at a time as now.

## Systems

- **macOS**, as a third OS crate like Flow's `macos/` (a menu-bar item,
  Sparkle). Not now: the Mac has U4Claude (arcanii/Usage4Claude-Arcanii),
  whose desktop widgets and SwiftUI popover don't come for free in Rust
  (widgets need a Swift extension in the bundle).
- **Linux**: a .deb (installed through `pkexec`, as a tray app has no
  terminal tab for Flow's `sudo apt`), and aarch64.
- **GNOME without the AppIndicator extension** has no tray at all (Ubuntu
  turns it on; Fedora doesn't), and on Wayland a window can't be put
  beside the tray: the popover's window stands in for the tray there.

## Before the first release

- **The update key**: keep U4Claude's (the Mac app's `SUPublicEDKey`, now
  `UPDATE_PUBLIC_KEY`), or give U4Ai a key of its own, as Flow has its own,
  made on the Mac with Sparkle's `generate_keys --account U4Ai`.
- **An icon of its own**: `assets/icon/` is U4Claude's for now.
- **A try on Windows**: the tray, its balloons, Start at login and
  WinSparkle have only been type-checked from Linux, and `release.ps1`,
  `make-appcast.ps1` and `installer.iss` have never run.
- **The new strings' translations** (Japanese, Korean, Chinese) looked over
  by a native speaker.

## Not planned

- Widgets: Windows 11's need an MSIX package, and Linux has none.
- claude.ai's session-cookie sign-in: it meets Cloudflare's check and needs
  an embedded web view; OAuth needs neither.
