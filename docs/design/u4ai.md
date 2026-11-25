# U4Ai

The plan agreed with the user on 2026-11-25 ("Yes, scaffold it"; renamed
from U4Claude for Windows and Linux the same day: "Yes, rename it", the
Mac "on the backlog - we already have the U4Claude project"), kept up to
date as it changes. What's planned but not begun is in `BACKLOG.md`.

## What it is

A tray app for Windows and Linux that shows how much of an AI's usage
limits are used, Claude's the first: the 5-hour window, the 7-day window,
the weekly per-model limits the API names (Opus, Sonnet, Fable…) and Extra
Usage. Claude's part follows U4Claude for the Mac
(arcanii/Usage4Claude-Arcanii, itself a fork of f-is-h/Usage4Claude),
whose Swift code is its spec. Nothing of the Mac app compiles here
(SwiftUI, AppKit, WidgetKit, WKWebView, the Keychain), so this is a
rewrite in Rust, laid out as Flow is (arcanii/Flow-Terminal): a core that
never calls an OS API, and a crate per OS. The Mac keeps U4Claude; a Mac
crate here is on the backlog.

## Why its own repository, and its own name

- It's a product of its own, for several AIs, not a port of one app: the
  name says neither Claude (Anthropic's mark, and one provider among
  several to come) nor one system. Upstream reads Codex's limits beside
  Claude's already, so more providers is where this goes.
- The Mac repo is a fork that follows upstream; its port audits and
  upstream pull requests stay clean without a Rust tree in its history,
  and its tags, its "Latest" release and its release workflow stay its
  own. This one starts at 0.1.0 with versions of its own.
- Flow keeps its OSes in one repository because they share `core/`; here
  Windows and Linux share code with each other and nothing with the Mac's
  Swift.

## Layout

```
core/      the providers (core/src/providers/, claude/ the first), what
           their sign-ins and requests share (oauth.rs, http.rs), accounts,
           refresh timing, alerts, the tray's icon and menu as data, the
           update feed, words (no OS calls)
linux/     the tray (StatusNotifierItem over D-Bus, ksni), notifications,
           start at login, Flow's own updater
windows/   the tray (Shell_NotifyIconW), notifications (the tray's
           balloon), start at login (HKCU Run), WinSparkle
src/       the u4ai binary, which hands the core its OS's crate
```

Each OS crate is `#![cfg(...)]`-gated, its dependencies target-only, so a
build on one OS never builds the other's.

## Providers

Each AI is a module of `core/src/providers/` and an arm in `Provider`'s
matches: it signs in (`SignIn`: the page to open, and a wait for it on a
thread), makes a `Reader` from the secret an account keeps, and reads into
a `Reading` of `Meter`s (a key the same in every language, for the alerts;
a label; a `Window`: short, long or a model's; how much is used and when
it resets) and lines of its own. The rings take the short window outside
and the long one inside, in the Mac's colors, whatever the provider; the
menu names each account with its provider ("Claude · me@example.com"),
offers "Sign in with <provider>…" and "<provider> Status" for each, and
the alerts and smart refreshing go by the meters. An account is
`provider:id` everywhere it's kept (settings, the credential store's
names, the alerts), so a second provider needs no migration.

One account shows at a time, as on the Mac. Showing a Claude account's
and a Codex account's limits together comes with the second provider (the
backlog).

## Signing in with Claude

"Sign in with Claude", as the Mac app's OAuth path does it: PKCE in
the system browser, Claude Code's public client, the redirect caught on
`http://localhost:1456/callback` (or 1458). The listener binds the
loopback addresses alone (127.0.0.1 and ::1), so no other machine can
reach it, where the Mac's binds every interface and drops other peers.
The refresh token goes in the system's credential store (Credential
Manager, the Secret Service), split into pieces where it's long, as
Flow's `secrets::store_long` does for Windows' 2,560 bytes an entry.

The session-cookie path (claude.ai's `sessionKey`, scraped from a web
view or pasted) is left out: it meets Cloudflare's check, which turns
away clients that aren't browsers, and reading the cookie needs an
embedded web view. OAuth reads `api.anthropic.com` with a Bearer token
and meets neither.

Both apps lean on endpoints Anthropic doesn't document
(`/api/oauth/usage`, `/api/oauth/profile`). The usage decoder's tests are
the Swift tests' JSON, so a change in the API shows in both.

## Updates

As Flow does them. For now with the key that signs U4Claude's DMG (its
public half is `SUPublicEDKey` in the Mac app's `Config/Info.plist`, here
`UPDATE_PUBLIC_KEY`); before the first release, the user chooses between
keeping it and giving U4Ai a key of its own, as Flow has its own (the
backlog).

- **Windows: WinSparkle** (0.9.3, vendored), loaded from beside
  `u4ai.exe` at run time. It reads `appcast-windows.xml`, checks the
  installer's Ed25519 signature, asks U4Ai to quit
  (`shutdown_request`) and runs it. The portable zip leaves the DLL out,
  so it never updates itself.
- **Linux: Flow's own updater**, ported: it reads
  `appcast-linux-<arch>.xml` (Sparkle's form), checks the tarball's
  signature, checks the `u4ai` inside says the feed's build, and runs
  the tarball's own `install.sh` into the folder it was installed in
  (`~/.local` unless chosen), then starts the new one. A copy not put
  there by `install.sh` has no updater. There's no .deb yet; Flow's runs
  `sudo apt` in a terminal tab, which a tray app hasn't got, so one would
  go through `pkexec`.
- **Only this project's own builds update**: the updater is built in only
  with `U4AI_UPDATES=1`, which the release scripts set only with the
  key at hand (Flow's `docs/design/updates-for-releases.md`), so a fork's
  build never offers this project's release in its place.
- The feeds live in this repository's `main`, as the Mac's `appcast.xml`
  lives in its own; Flow needs a separate releases repository only because
  its source was private. Releases are tagged `v<ver>-windows` and
  `v<ver>-linux`, so each system goes out on its own day.
- The private key never goes to CI. It's in the Mac's login Keychain, where
  Sparkle's `sign_update` signs the Windows installer and the Linux
  tarball (`windows/scripts/sign-release.sh`); a copy exported once to
  `~\.u4ai-signing\u4ai-update.key` lets Windows sign by itself.

## Refreshing

As the Mac app: smart mode steps from every minute to every 3, 5 and 10
after 3, 6 and 12 unchanged reads, and back to every minute on any change;
fixed mode reads every 1, 3, 5 or 10 minutes. A 429 holds every read until
the server's `Retry-After` (kept within 30 s and 30 min), else 2, 4, 8… up
to 30 minutes, until a read succeeds. A clock that jumped ahead (the
computer slept) reads again a few seconds after.

## Alerts

A limit reaching 90% says so once, until it falls below 90% again. A limit
that had reached 90% says it reset once its window starts again (its reset
time moves later and its usage falls) or it falls 30 points. Each account
keeps its own, so switching accounts never compares one with another, and an
account's first read after U4Ai starts only notes where it stands (the
tray shows it).
(The Mac app also tells of a reset below 90%; this one keeps quiet there.)

## The scaffold

The tray icon (two rings drawn in code, in the Mac's colors), its tooltip,
and a menu with whose usage it is, each limit, Refresh, the accounts,
Sign in with Claude, Sign out, Start at login, the update item, Claude
Status and Quit. Updates, alerts and both OSes' release scripts. What
comes next is `BACKLOG.md`.

## Words

Every word goes through `i18n::t` and `tf`, from `core/locales/<code>.json`
in the Mac's five languages (English, Japanese, Korean, Simplified and
Traditional Chinese). Strings the Mac app has are its translations; the
few new ones (Sign out, Start at login, the updater's states, the
providers' sign-in and status items) were
translated for the scaffold and want a native speaker's look.
