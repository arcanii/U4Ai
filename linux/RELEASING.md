# Releasing U4Ai (Linux)

For Windows, see [windows/RELEASING.md](../windows/RELEASING.md). The Mac
app releases from its own repository.

Linux has no Sparkle, so U4Ai updates itself there with an updater of
its own (`linux/src/updater.rs`, Flow's ported). It reads an **EdDSA-signed
appcast** in Sparkle's form, downloads the newer tarball, checks it with the
update key (the one Sparkle and WinSparkle check with), checks that its
`bin/u4ai --version` says the feed's build, and runs the tarball's
`install.sh` into the prefix it was installed in, then starts the new one.
Commands below run from the repo root.

- Feeds: one per architecture, `appcast-linux-<arch>.xml` at the root of
  this repo's `main` (`x86_64` for now), read from
  `https://raw.githubusercontent.com/arcanii/U4Ai/main/appcast-linux-<arch>.xml`.
- Releases: `v<ver>-linux`, created with `--latest=false`, so Windows'
  keeps the Latest badge. `<ver>` is what `u4ai --version` shows, a
  patch's fourth number and all (`patch` under
  `[workspace.metadata.u4ai]`).
- Version: the feed's `sparkle:version` is the build number (the commit
  count plus `build_offset`), which the updater compares with its own, and
  `sparkle:shortVersionString` is `<ver>`.
- Who updates: a copy `install.sh` put in place (`<prefix>/bin/u4ai`
  with `<prefix>/share/applications/com.arcanii.U4Ai.desktop`).
  Copies run from `target/` have no updater, and only a build made with
  `U4AI_UPDATES=1` has one at all.
- Key: the update key's private half is on the Mac (the login Keychain)
  and in the Windows PC's key file, never here, so a Linux tarball is
  signed on the Mac.

## One-time setup

- Rust, a C compiler and `gh` signed in to the arcanii account.
- **zig** and **cargo-zigbuild**, with which `package.sh` builds against
  glibc 2.31 (a program linked on a new Linux needs that Linux's glibc;
  built so, it runs on Ubuntu 20.04, Debian 11 and everything since).
  Neither needs root:
  ```sh
  # zig 0.16.0 from https://ziglang.org/download/ (check the SHA-256 its
  # index.json gives), unpacked under ~/.local/share/zig, on the PATH:
  ln -s ~/.local/share/zig/zig-x86_64-linux-0.16.0/zig ~/.cargo/bin/zig
  cargo install cargo-zigbuild --version 0.24.0 --locked
  ```
- On the Mac, Sparkle's `sign_update` (the Mac app's Xcode build has it),
  for `windows/scripts/sign-release.sh`.

## Per release

1. **Commit everything** at the version to release, with `dev = false`
   under `[workspace.metadata.u4ai]`: `package.sh` refuses a DEV build
   and a dirty tree, since the build number names the commit.
2. **Build**:
   ```sh
   linux/scripts/package.sh --updates
   ```
   `--updates` builds it with `U4AI_UPDATES=1`, as the key isn't here
   to say so; a release must say `--updates` or `--no-updates` (one that
   never updates itself, which mustn't go in the feed). It checks that
   `u4ai` needs no glibc newer than 2.31 and says `updates: on` as asked
   (`U4AI_SAY_UPDATES=1 u4ai --version`), and writes
   `target/linux/U4Ai-<ver>-linux-x86_64.tar.gz` and its `.sha256`. It
   unpacks to `u4ai-<ver>/`: `install.sh`, `bin/u4ai`, the menu
   entry and icons under `share/`, and `share/doc/u4ai/` (README,
   LICENSE, THIRD-PARTY-NOTICES.txt). `--allow-dirty` makes a test build
   (`0.1.0.DEV`, no updates unless `--updates`), `--native` builds with
   plain cargo for this computer's glibc. To try one without touching
   `~/.local`, unpack it and run `install.sh --prefix <scratch dir>`, then
   `install.sh --uninstall --prefix <scratch dir>`.
3. **Publish the release**:
   ```sh
   gh release create v<ver>-linux --repo arcanii/U4Ai --latest=false \
      --title "U4Ai <ver> for Linux" --notes "..." target/linux/U4Ai-<ver>-linux-x86_64.tar.gz*
   ```
   No installed copy sees it until the feed changes in step 5.
4. **Sign it on the Mac**, from the release, which proves the published
   file is the one signed:
   ```sh
   curl -fLO https://github.com/arcanii/U4Ai/releases/download/v<ver>-linux/U4Ai-<ver>-linux-x86_64.tar.gz
   windows/scripts/sign-release.sh U4Ai-<ver>-linux-x86_64.tar.gz
   ```
   Then, back here, write the feed with what it printed:
   ```sh
   linux/scripts/make-appcast.sh --tarball target/linux/U4Ai-<ver>-linux-x86_64.tar.gz \
       --tag v<ver>-linux --signature <sig>
   ```
   It checks that the tarball's `u4ai` says the tag's version and
   `updates: on`, that the tarball holds `install.sh`, and that the
   signature verifies against `UPDATE_PUBLIC_KEY`, before it writes
   `target/linux/appcast-linux-x86_64.xml`. With the key's file at hand,
   `--key-file <file>` signs here instead.
5. **Publish the feed**: copy it to the repo's root as
   `appcast-linux-x86_64.xml`, commit and push to `main`. Publishing it
   updates every installed copy, so do it only when the release is ready.
6. Once every system's release is out, bump `version` and set `dev = true`
   again.
