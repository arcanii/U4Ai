#Requires -Version 7
# Build a Windows x64 release of U4Ai into target\dist:
#   U4Ai-<ver>-setup.exe                 Inno Setup installer, the build WinSparkle updates
#   U4Ai-<ver>-windows-x64-portable.zip  u4ai.exe without the updater; updated by hand
#   SHA256SUMS.txt
# <ver> is a patch release's too (0.1.0.1), as the app shows it.
# Commit first: the build number is the commit count (plus build_offset), so it
# has to name HEAD.
#
#   pwsh windows\scripts\release.ps1              build and package
#   pwsh windows\scripts\release.ps1 -SmokeTest   also install to a temp dir, check, uninstall
#
# U4Ai updates itself only in a build made with the update key at hand
# (docs\design\u4ai.md, Updates): this builds with
# U4AI_UPDATES=1 where -KeyFile, which make-appcast.ps1 signs with, holds
# the key the build trusts (UPDATE_PUBLIC_KEY), and a release made without it
# stops unless given -NoUpdates.
[CmdletBinding()]
param(
    [switch]$SkipBuild,    # package the u4ai.exe already in target\x86_64-pc-windows-msvc\release
    [switch]$AllowDirty,   # allow uncommitted changes and dev = true, for test builds only
    [switch]$SmokeTest,
    [switch]$NoUpdates,    # a release that never updates itself, made without the update key
    [string]$KeyFile = (Join-Path $env:USERPROFILE '.u4ai-signing\u4ai-update.key')
)
$ErrorActionPreference = 'Stop'
# This script is in windows\scripts; the repo root is two levels up.
$root = Split-Path -Path (Split-Path -Path $PSScriptRoot -Parent) -Parent
$win = Join-Path $root 'windows'
$dist = Join-Path $root 'target\dist'
$stage = Join-Path $dist 'stage'
$manifest = Join-Path $root 'Cargo.toml'
# Always x64, even on an ARM64 PC, where a plain build would be ARM64: the
# installer, the zip and WinSparkle.dll are x64, and ARM64 Windows runs them
# emulated.
$target = 'x86_64-pc-windows-msvc'
$exe = Join-Path $root "target\$target\release\u4ai.exe"

# Runs one of the repo's examples. With installer detection on, Windows takes
# an exe that has no manifest and "update" in its name (update_key.exe) for
# an installer, and won't start it without elevation; RunAsInvoker starts it
# with the user's own rights instead.
function Invoke-Example([string]$Name, [string[]]$Arguments) {
    $layer = $env:__COMPAT_LAYER
    $env:__COMPAT_LAYER = 'RunAsInvoker'
    try {
        cargo run --quiet --locked --example $Name --manifest-path $manifest -- @Arguments
        if ($LASTEXITCODE) { throw "examples\$Name.rs failed (exit $LASTEXITCODE)" }
    } finally {
        $env:__COMPAT_LAYER = $layer
    }
}

# What `u4ai.exe --version` prints, a line each, with the line that says
# whether it updates itself (U4AI_SAY_UPDATES=1). It's a window's
# program, which writes nowhere a shell sees unless its output is sent to a
# file.
function Get-SaidVersion([string]$Path) {
    $said = [System.IO.Path]::GetTempFileName()
    $env:U4AI_SAY_UPDATES = '1'
    try {
        $p = Start-Process -FilePath $Path -ArgumentList '--version' -NoNewWindow -Wait -PassThru -RedirectStandardOutput $said
        if ($p.ExitCode) { throw "$Path --version exited with $($p.ExitCode)" }
        @(Get-Content -LiteralPath $said)
    } finally {
        Remove-Item Env:U4AI_SAY_UPDATES -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $said -ErrorAction SilentlyContinue
    }
}

# Marketing version from [workspace.package] in Cargo.toml; build number =
# commit count plus `build_offset` under [workspace.metadata.u4ai], as
# core\build_number.rs has it. $full, which the installer and WinSparkle
# compare, leaves a patch release's number out; $shown has it.
$ver = $null
$patch = 0
$offset = 0
$dev = $false
$table = ''
foreach ($line in Get-Content -LiteralPath $manifest) {
    $line = ($line -split '#')[0].Trim()
    if ($line.StartsWith('[')) { $table = $line }
    elseif ($table -eq '[workspace.package]' -and $line -match '^version\s*=\s*"([^"]+)"$') { $ver = $Matches[1] }
    elseif ($table -eq '[workspace.metadata.u4ai]' -and $line -match '^patch\s*=\s*(\d+)$') { $patch = [int]$Matches[1] }
    elseif ($table -eq '[workspace.metadata.u4ai]' -and $line -match '^build_offset\s*=\s*(\d+)$') { $offset = [int]$Matches[1] }
    elseif ($table -eq '[workspace.metadata.u4ai]' -and $line -match '^dev\s*=\s*true$') { $dev = $true }
}
if (-not $ver) { throw 'No version under [workspace.package] in Cargo.toml.' }
$shown = if ($patch) { "$ver.$patch" } else { $ver }
# Work toward a version (`dev = true` there) isn't released: U4Ai says
# 0.1.0.DEV until releasing sets it to false. A test build keeps it.
if ($dev) {
    if (-not $AllowDirty) {
        throw "U4Ai is $shown.DEV, still worked toward: set dev = false under [workspace.metadata.u4ai] and commit to release it (or pass -AllowDirty for a test build)."
    }
    $shown = "$shown.DEV"
}
$count = git -C $root rev-list --count HEAD
if ($LASTEXITCODE) { throw 'git rev-list failed (no commit yet?)' }
$build = ([int]"$count".Trim() + $offset).ToString()
$full = "$ver.$build"
Write-Host "U4Ai $shown (build $build)"

if (-not $AllowDirty -and (git -C $root status --porcelain)) {
    throw 'Uncommitted changes. Commit first, since the build number names HEAD (or pass -AllowDirty for a test build).'
}
foreach ($f in 'README.md', 'LICENSE') {
    if (-not (Test-Path -LiteralPath (Join-Path $root $f))) { throw "No $f at the repo's root, which the release carries." }
}

# Updates, only with the update key at hand: the key file's public half must
# be the one the build trusts, so a fork's release never offers this
# project's in its place.
$updates = $false
if (-not $NoUpdates) {
    $trusted = Invoke-Example update_key @('trusted') | Select-Object -Last 1
    $held = $null
    if (Test-Path -LiteralPath $KeyFile) {
        $held = Invoke-Example update_key @('public', $KeyFile) | Select-Object -Last 1
    }
    if ($held -and "$held".Trim() -eq "$trusted".Trim()) { $updates = $true }
    elseif ($AllowDirty) { Write-Host "note: $KeyFile doesn't hold the update key (UPDATE_PUBLIC_KEY), so this test build won't update itself." }
    else { throw "$KeyFile doesn't hold the update key (UPDATE_PUBLIC_KEY; windows\RELEASING.md), so this release couldn't update itself. Pass -NoUpdates for a release that never does." }
}
$saidUpdates = if ($updates) { 'updates: on' } else { 'updates: off' }
Write-Host "This build: $saidUpdates"
$env:U4AI_UPDATES = if ($updates) { '1' } else { '0' }

if (-not $SkipBuild) {
    if (Get-Process u4ai -ErrorAction SilentlyContinue | Where-Object Path -like "*\target\$target\release\u4ai.exe") {
        throw "U4Ai is running from target\$target\release. Quit it so the build can replace u4ai.exe."
    }
    if (-not (rustup target list --installed | Where-Object { $_ -eq $target })) {
        throw "The $target Rust target isn't installed. Add it: rustup target add $target"
    }
    # The build's own folders out of the program (panics name their source
    # files): the user's profile as ~ (Cargo's crates are under it) and the
    # repo as u4ai, so a release names no one's account. Encoded, parted
    # by 0x1f, as a folder may hold spaces.
    $env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=~" + [char]0x1f + "--remap-path-prefix=$root=u4ai"
    cargo build --release --locked --target $target -p u4ai --bin u4ai --manifest-path $manifest
    if ($LASTEXITCODE) { throw "cargo build failed (exit $LASTEXITCODE)" }
}
if (-not (Test-Path -LiteralPath $exe)) { throw "No $exe. Build it, or leave out -SkipBuild." }
$said = Get-SaidVersion $exe
if ($said[0] -ne "U4Ai $shown (build $build)") { throw "target\$target\release\u4ai.exe says '$($said[0])', not 'U4Ai $shown (build $build)'. Rebuild without -SkipBuild." }
if ($said[1] -ne $saidUpdates) { throw "target\$target\release\u4ai.exe says '$($said[1])', not '$saidUpdates'. Rebuild without -SkipBuild." }

if (Test-Path -LiteralPath $dist) { Remove-Item -LiteralPath $dist -Recurse -Force }
New-Item -ItemType Directory -Path $stage | Out-Null
Copy-Item -LiteralPath $exe, (Join-Path $win 'third_party\winsparkle\x64\WinSparkle.dll'),
    (Join-Path $root 'assets\icon\u4ai.ico'), (Join-Path $root 'README.md'), (Join-Path $root 'LICENSE') -Destination $stage

# THIRD-PARTY-NOTICES.txt: WinSparkle.dll and what's built into it, then
# every crate compiled into u4ai.exe.
$crates = Join-Path $dist 'crates.txt'
Invoke-Example third_party_notices @('--target', $target, '--binary', 'u4ai.exe', '--out', $crates)
$head = @'
U4Ai - third-party notices
==============================

U4Ai is built from the open-source Rust crates listed below, and its
installer carries WinSparkle.dll, which updates it. Their copyright notices
and licenses follow.

-------------------------------------------------------------------------------
WinSparkle.dll (in the installed copy, not the portable zip)
-------------------------------------------------------------------------------

WinSparkle 0.9.3, the automatic-update library
    Copyright (C) 2009-2026 Vaclav Slavik
    License: MIT (text below)
    Source:  https://github.com/vslavik/winsparkle

WinSparkle.dll has these libraries built in:

wxWidgets (user interface)
    License: wxWindows Library Licence, Version 3.1
    Source:  https://www.wxwidgets.org/about/licence/

Expat (XML parser)
    Copyright (c) Thai Open Source Software Center Ltd, Clark Cooper and the
    Expat maintainers
    License: MIT (text below)
    Source:  https://libexpat.github.io/

OpenSSL (cryptography)
    License: OpenSSL License and the original SSLeay License
    Source:  https://www.openssl.org/source/license-openssl-ssleay.txt
    This product includes software developed by the OpenSSL Project for use
    in the OpenSSL Toolkit (http://www.openssl.org/).
    This product includes cryptographic software written by Eric Young
    (eay@cryptsoft.com).

MIT License (WinSparkle and Expat)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

'@
$notices = ($head + (Get-Content -LiteralPath $crates -Raw)) -replace "`r`n", "`n" -replace "`n", "`r`n"
[System.IO.File]::WriteAllText((Join-Path $stage 'THIRD-PARTY-NOTICES.txt'), $notices, [System.Text.UTF8Encoding]::new($false))
Remove-Item -LiteralPath $crates

$iscc = "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe" | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 not found. Install it: winget install JRSoftware.InnoSetup' }
$iss = Join-Path $win 'packaging\installer.iss'
& $iscc /Q "/DMyVer=$ver" "/DMyShown=$shown" "/DMyBuild=$build" $iss
if ($LASTEXITCODE) { throw "ISCC failed (exit $LASTEXITCODE)" }
$setup = Join-Path $dist "U4Ai-$shown-setup.exe"

# The portable zip leaves WinSparkle.dll out: an update runs the installer,
# which would put a second copy in %LOCALAPPDATA%\Programs instead of
# updating this one.
$zip = Join-Path $dist "U4Ai-$shown-windows-x64-portable.zip"
Compress-Archive -Path (Join-Path $stage 'u4ai.exe'), (Join-Path $stage 'README.md'), (Join-Path $stage 'LICENSE'),
    (Join-Path $stage 'THIRD-PARTY-NOTICES.txt') -DestinationPath $zip

$sums = foreach ($f in $setup, $zip) {
    '{0}  {1}' -f (Get-FileHash -LiteralPath $f -Algorithm SHA256).Hash.ToLowerInvariant(), (Split-Path $f -Leaf)
}
# LF endings, so `sha256sum -c` works on macOS and Linux too.
[System.IO.File]::WriteAllText((Join-Path $dist 'SHA256SUMS.txt'), ($sums -join "`n") + "`n")

if ($SmokeTest) {
    if (Get-Process u4ai -ErrorAction SilentlyContinue) {
        throw 'Quit U4Ai first: setup refuses to run while it holds its mutex.'
    }
    # The installer is per user, so a test install shares a real one's
    # uninstall entry and would leave that install unlisted once uninstalled.
    $appId = (Select-String -LiteralPath $iss -Pattern '^AppId=\{\{([0-9A-Fa-f-]+)\}' | Select-Object -First 1).Matches[0].Groups[1].Value
    $entry = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{$appId}_is1"
    if (Test-Path -LiteralPath $entry) {
        throw "U4Ai is installed for this user. Uninstall it first, or smoke-test on another account."
    }
    # Uninstalling takes Start at login's value away; another copy's is put back.
    $runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
    $runWas = (Get-ItemProperty -LiteralPath $runKey -Name U4Ai -ErrorAction SilentlyContinue).U4Ai
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) "u4ai-smoke-$build"
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Recurse -Force }
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        # Run a local copy: Windows asks before starting an exe from a network share.
        Copy-Item -LiteralPath $setup -Destination $tmp
        $app = Join-Path $tmp 'app'
        # /NOLAUNCH: a silent install starts U4Ai, as after an update, unless told not to.
        $p = Start-Process -FilePath (Join-Path $tmp (Split-Path $setup -Leaf)) -Wait -PassThru -ArgumentList `
            '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOLAUNCH', "/DIR=`"$app`"", "/LOG=`"$tmp\setup.log`""
        if ($p.ExitCode) { throw "Setup exited with $($p.ExitCode); see $tmp\setup.log" }
        foreach ($f in 'u4ai.exe', 'WinSparkle.dll', 'u4ai.ico', 'README.md', 'LICENSE', 'THIRD-PARTY-NOTICES.txt', 'unins000.exe') {
            if (-not (Test-Path -LiteralPath (Join-Path $app $f))) { throw "Setup didn't install $f" }
        }
        if (Get-Process u4ai -ErrorAction SilentlyContinue) { throw 'Setup started U4Ai despite /NOLAUNCH' }
        $installed = Get-SaidVersion (Join-Path $app 'u4ai.exe')
        if ($installed[0] -ne "U4Ai $shown (build $build)") { throw "Installed u4ai.exe --version printed '$($installed[0])'" }
        $stamped = (Get-Item -LiteralPath $setup).VersionInfo.FileVersion.Trim()
        if ($stamped -ne $full) { throw "The setup exe is version $stamped, not $full" }
        Set-ItemProperty -LiteralPath $runKey -Name U4Ai -Value "`"$app\u4ai.exe`""
        $u = Start-Process -FilePath (Join-Path $app 'unins000.exe') -Wait -PassThru `
            -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'
        if ($u.ExitCode) { throw "Uninstall exited with $($u.ExitCode)" }
        # The uninstaller hands off to a copy of itself in %TEMP%, so wait for the files to go.
        $deadline = (Get-Date).AddSeconds(30)
        while ((Test-Path -LiteralPath (Join-Path $app 'u4ai.exe')) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 250 }
        if (Test-Path -LiteralPath (Join-Path $app 'u4ai.exe')) { throw "Uninstall left $app\u4ai.exe behind" }
        if ($null -ne (Get-ItemProperty -LiteralPath $runKey -Name U4Ai -ErrorAction SilentlyContinue)) {
            throw 'Uninstall left the Start at login value (HKCU\...\Run\U4Ai) behind'
        }
        if (Test-Path -LiteralPath $entry) { throw 'Uninstall left its entry in Apps & features behind' }
    } finally {
        if ($null -ne $runWas) { Set-ItemProperty -LiteralPath $runKey -Name U4Ai -Value $runWas }
        else { Remove-ItemProperty -LiteralPath $runKey -Name U4Ai -ErrorAction SilentlyContinue }
    }
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "Smoke test passed: installed $full per-user, then uninstalled it."
}

Write-Host ''
Write-Host "Built in ${dist}:"
Get-ChildItem -LiteralPath $dist -File | ForEach-Object { '  {0,-48} {1,8:N1} MB' -f $_.Name, ($_.Length / 1MB) }
if ($dev) {
    Write-Host "`nA test build of work toward $ver, not for release."
    return
}
if (-not $updates) {
    Write-Host "`nIt never updates itself (-NoUpdates): don't put it in appcast-windows.xml, where every copy would update to it and then stop."
}
Write-Host @"

Next (windows\RELEASING.md has the details):
  1. gh release create v$shown-windows --repo arcanii/U4Ai --title "U4Ai $shown for Windows" target\dist\U4Ai-$shown-setup.exe target\dist\U4Ai-$shown-windows-x64-portable.zip target\dist\SHA256SUMS.txt
  2. pwsh windows\scripts\make-appcast.ps1 -Version $full -Tag v$shown-windows -SetupExe target\dist\U4Ai-$shown-setup.exe
     (signs with ~\.u4ai-signing\u4ai-update.key; for a Mac signature add -Signature <sig>)
  3. Commit target\dist\appcast-windows.xml as appcast-windows.xml at the root of main.
"@
