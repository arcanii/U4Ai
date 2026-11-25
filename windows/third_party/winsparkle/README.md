# WinSparkle

`x64/WinSparkle.dll` and `winsparkle.h` are WinSparkle 0.9.3, the same files
vendored in the sibling apps (Flow, RabbitEars). Source and releases:
<https://github.com/vslavik/winsparkle>. MIT licensed: `COPYING` is its
license as WinSparkle 0.9.3 has it, and the same notice ships in each
release's `THIRD-PARTY-NOTICES.txt`.

U4Ai loads the DLL at run time from next to `u4ai.exe` (see
`windows/src/updater.rs`), so nothing links against it.
`windows/scripts/release.ps1` copies it into the installer, and leaves it out
of the portable zip, which then has no updater (`windows/RELEASING.md`).
