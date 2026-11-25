# Releasing U4Ai (Windows x64)

For Linux, see [linux/RELEASING.md](../linux/RELEASING.md). The Mac app
releases from its own repository.

U4Ai updates itself with **WinSparkle** (0.9.3, vendored in
`windows/third_party/winsparkle`), loaded from beside `u4ai.exe`. It reads
an **EdDSA-signed appcast**, then downloads and runs the new installer,
silently (the feed's `sparkle:installerArguments`), and setup starts
U4Ai again. Commands below run from the repo root.

- Feed: `appcast-windows.xml` at the root of this repo's `main`, read from
  `https://raw.githubusercontent.com/arcanii/U4Ai/main/appcast-windows.xml`.
- Releases: `v<ver>-windows` in
  [arcanii/U4Ai](https://github.com/arcanii/U4Ai/releases),
  marked Latest (Linux's go out with `--latest=false`).
- Version: `u4ai --version` says `U4Ai 0.1.0 (build N)`. WinSparkle
  compares `0.1.0.N` (the marketing version and the build) with the feed's
  `sparkle:version`.
  - The marketing version is `version` under `[workspace.package]` in the
    root `Cargo.toml`. A patch release sets `patch` under
    `[workspace.metadata.u4ai]` and shows as `0.1.0.1 (build N)`; its
    tag, files and the feed's `sparkle:shortVersionString` say `0.1.0.1`,
    while WinSparkle still compares `0.1.0.N`. `patch` goes back to 0 when
    `version` goes up.
  - The build number is `git rev-list --count HEAD` plus `build_offset`
    there, so a rebuild of a commit keeps its number.
  - `dev = true` there while work toward `version` goes on (it shows as
    `0.1.0.DEV`). Releasing sets it to false.

## The update key

One Ed25519 key signs the Mac's, Windows' and Linux's updates: its public
half is the Mac app's `SUPublicEDKey`, here `UPDATE_PUBLIC_KEY` in
`core/src/lib.rs`.

- Its private half is in the Mac's login Keychain, under Sparkle's default
  account, where the Mac app's scripts sign with it.
- A copy on Windows lets this PC sign by itself:
  `~\.u4ai-signing\u4ai-update.key`, the base64 seed, as Sparkle
  writes it. Export it once on the Mac with Sparkle's `generate_keys -x
  u4ai-update.key` (it ships in Sparkle's `bin/`, beside `sign_update`),
  carry it over on a USB stick rather than the network, and delete the
  carried copy. Check it: `cargo run --example update_key -- public
  <file>` must print the same as `cargo run --example update_key -- trusted`.
- Never in a repo or in CI. Lose it and every installed copy can't update.

## Installer vs portable zip

WinSparkle updates by running the installer, so **only installed copies
update themselves**. The portable zip leaves `WinSparkle.dll` out, so it has
no updater, rather than an update putting a second copy in
`%LOCALAPPDATA%\Programs` and leaving the portable one stale.

The installer is per user, with no admin rights
(`%LOCALAPPDATA%\Programs\U4Ai`), so neither it nor an update asks for
them. It adds a Start menu shortcut, starts U4Ai when it's done, and
its uninstaller takes away Start at login's value
(`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\U4Ai`).

## One-time setup

- Inno Setup 6 (`winget install JRSoftware.InnoSetup`), PowerShell 7
  (`pwsh`), Rust, and `gh` signed in to the arcanii account.
- The x64 target: `rustup target add x86_64-pc-windows-msvc`. Releases are
  always x64; on an ARM64 PC that's a cross build, which needs Visual
  Studio's x64 build tools too.
- The key file, as above.

## Per release

1. **Set `dev = false`** (or `patch`, for a patch release) and **commit
   everything**: `release.ps1` refuses a DEV build and a dirty tree, since
   the build number names the commit. `<ver>` below is the version people
   see, a patch's number and all.
2. **Build**:
   ```powershell
   pwsh windows\scripts\release.ps1 -SmokeTest
   ```
   With the update key in `-KeyFile` (by default
   `~\.u4ai-signing\u4ai-update.key`) the build updates itself
   (`U4AI_UPDATES=1`); without it the script stops, unless given
   `-NoUpdates` for a release that never updates itself (which mustn't go in
   the feed). It checks that `u4ai.exe --version` says the version and,
   with `U4AI_SAY_UPDATES=1`, `updates: on` or `off` as it should, and
   writes to `target\dist`:
   - `U4Ai-<ver>-setup.exe`
   - `U4Ai-<ver>-windows-x64-portable.zip`
   - `SHA256SUMS.txt`

   `-SmokeTest` installs into a temp folder silently (`/NOLAUNCH`, so it
   doesn't start), checks the files and the version, uninstalls and checks
   that Start at login's value went with it. Quit U4Ai first, and run it
   where U4Ai isn't installed for this user: the test install would take
   the real one's place in Apps & features. `-AllowDirty` makes a test build
   (`0.1.0.DEV`, no updates) of whatever the tree holds.
3. **Publish the release** (the script prints the command):
   ```powershell
   gh release create v<ver>-windows --repo arcanii/U4Ai --latest `
      --title "U4Ai <ver> for Windows" --notes "..." `
      target\dist\U4Ai-<ver>-setup.exe target\dist\U4Ai-<ver>-windows-x64-portable.zip `
      target\dist\SHA256SUMS.txt
   ```
   No installed copy sees it until the feed changes in step 5.
4. **Sign and write the feed.** On Windows, with the key file:
   ```powershell
   pwsh windows\scripts\make-appcast.ps1 -Version <version>.<build> -Tag v<ver>-windows `
        -SetupExe target\dist\U4Ai-<ver>-setup.exe
   ```
   Or sign on the Mac, from the release, which proves the published file is
   the one signed:
   ```sh
   curl -fLO https://github.com/arcanii/U4Ai/releases/download/v<ver>-windows/U4Ai-<ver>-setup.exe
   windows/scripts/sign-release.sh U4Ai-<ver>-setup.exe
   ```
   and pass what it prints as `-Signature <sig>`. Either way the script
   checks that the installer's version is `-Version` and that the signature
   verifies against `UPDATE_PUBLIC_KEY` before it writes
   `target\dist\appcast-windows.xml`.
5. **Publish the feed**: copy it to the repo's root as `appcast-windows.xml`,
   commit and push to `main`. WinSparkle sees it at its next check, or at
   *Check for Updates…*. Publishing it updates every installed copy, so do
   it only when the release is ready.
6. Once every system's release is out, bump `version` and set `dev = true`
   again.

## Not covered yet

- **Authenticode.** Neither `u4ai.exe` nor the installer is
  code-signed, so SmartScreen warns on first run (More info, Run anyway).
  The EdDSA signature protects updates only. With a certificate,
  `signtool sign` the exe before `release.ps1` packages it, then the
  installer, then make the EdDSA signature over the final bytes.
- **ARM64.** Only x64, which runs emulated on ARM64. A native build would
  need its own feed and an ARM64 `WinSparkle.dll`.
