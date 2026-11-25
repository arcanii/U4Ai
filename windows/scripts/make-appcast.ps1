#Requires -Version 7
# Write the WinSparkle appcast for a U4Ai release: commit the file this
# writes as appcast-windows.xml at the root of this repo's main branch, once
# the installer is up on the release.
#
#   pwsh windows\scripts\make-appcast.ps1 -Version 0.1.0.12 -Tag v0.1.0-windows -SetupExe target\dist\U4Ai-0.1.0-setup.exe
#
# Without -Signature it signs the installer here with the update key
# (-KeyFile, by default ~\.u4ai-signing\u4ai-update.key). With
# -Signature it uses one made on the Mac by windows/scripts/sign-release.sh.
# Either way it checks first that the installer is the build named by
# -Version and that the signature verifies against the updater's public key
# (UPDATE_PUBLIC_KEY), because either mistake would only show up as failed
# updates on users' computers.
#
# The feed asks WinSparkle to run the installer with /SILENT, so an update
# shows only setup's progress, and setup starts U4Ai again after it.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Version,    # marketing.build, e.g. 0.1.0.12 (what WinSparkle compares)
    [Parameter(Mandatory)][string]$Tag,        # the release tag, e.g. v0.1.0-windows, or v0.1.0.1-windows for a patch release
    [Parameter(Mandatory)][string]$SetupExe,   # the installer users download
    [string]$Signature,                        # sparkle:edSignature from the Mac; omit to sign here
    [string]$KeyFile = (Join-Path $env:USERPROFILE '.u4ai-signing\u4ai-update.key'),
    [string]$Repo = 'arcanii/U4Ai',
    [string]$Notes,                            # release-notes URL; defaults to the release page
    [string]$Out                               # defaults to target\dist\appcast-windows.xml
)
$ErrorActionPreference = 'Stop'
# This script is in windows\scripts; the repo root is two levels up.
$root = Split-Path -Path (Split-Path -Path $PSScriptRoot -Parent) -Parent
if (-not (Test-Path -LiteralPath $SetupExe)) { throw "Setup exe not found: $SetupExe" }
$SetupExe = (Resolve-Path -LiteralPath $SetupExe).Path
if (-not $Out) { $Out = Join-Path $root 'target\dist\appcast-windows.xml' }

if ($Version -notmatch '^\d+\.\d+\.\d+\.\d+$') { throw "-Version should be the version and the build, as 0.1.0.12, not $Version." }
# Inno Setup pads its version strings with spaces.
$stamped = (Get-Item -LiteralPath $SetupExe).VersionInfo.FileVersion.Trim()
if ($stamped -ne $Version) { throw "$SetupExe is version $stamped, not $Version." }
$tagged = [regex]::Match($Tag, '^v(\d+\.\d+\.\d+(\.\d+)?)-windows$')
if (-not $tagged.Success) { throw "-Tag should look like v0.1.0-windows (or v0.1.0.1-windows for a patch release), not $Tag." }
# The version people see is the tag's; its first three numbers start -Version.
$shown = $tagged.Groups[1].Value
if (-not $Version.StartsWith((($shown -split '\.')[0..2] -join '.') + '.')) { throw "-Tag $Tag doesn't match -Version $Version." }
$file = Split-Path -Path $SetupExe -Leaf
if ($file -ne "U4Ai-$shown-setup.exe") { throw "$file isn't U4Ai-$shown-setup.exe, as release.ps1 names this release's installer." }

function Invoke-UpdateKey {
    # With installer detection on, Windows takes an exe that has no manifest and
    # "update" in its name for an installer, and won't start it without
    # elevation. RunAsInvoker starts it with the user's own rights instead.
    $layer = $env:__COMPAT_LAYER
    $env:__COMPAT_LAYER = 'RunAsInvoker'
    try {
        cargo run --quiet --locked --example update_key --manifest-path (Join-Path $root 'Cargo.toml') -- @args
    } finally {
        $env:__COMPAT_LAYER = $layer
    }
}
if (-not $Signature) {
    if (-not (Test-Path -LiteralPath $KeyFile)) { throw "No -Signature given, and no key file at $KeyFile to sign with." }
    $Signature = Invoke-UpdateKey sign $SetupExe $KeyFile | Select-Object -Last 1
    if ($LASTEXITCODE) { throw "Signing with $KeyFile failed." }
    Write-Host "Signed with $KeyFile"
}
$Signature = "$Signature".Trim()
Invoke-UpdateKey verify $SetupExe $Signature
if ($LASTEXITCODE) { throw "The signature check failed. Sign this exact file with the update key." }

$len = (Get-Item -LiteralPath $SetupExe).Length
$url = "https://github.com/$Repo/releases/download/$Tag/$file"
if (-not $Notes) { $Notes = "https://github.com/$Repo/releases/tag/$Tag" }
$pub = (Get-Date).ToUniversalTime().ToString(
    'ddd, dd MMM yyyy HH:mm:ss +0000', [System.Globalization.CultureInfo]::InvariantCulture)
$xml = @"
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>U4Ai (Windows) Updates</title>
    <link>https://raw.githubusercontent.com/$Repo/main/appcast-windows.xml</link>
    <description>Most recent updates to U4Ai for Windows</description>
    <language>en</language>
    <item>
      <title>Version $shown</title>
      <sparkle:releaseNotesLink>$Notes</sparkle:releaseNotesLink>
      <pubDate>$pub</pubDate>
      <enclosure url="$url"
                 sparkle:version="$Version"
                 sparkle:shortVersionString="$shown"
                 sparkle:installerArguments="/SILENT /SP- /NOCANCEL"
                 sparkle:edSignature="$Signature"
                 length="$len"
                 type="application/octet-stream"/>
    </item>
  </channel>
</rss>
"@
$dir = Split-Path -Path $Out -Parent
if ($dir -and -not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
[System.IO.File]::WriteAllText($Out, $xml + "`n", [System.Text.UTF8Encoding]::new($false))
Write-Host "Wrote $Out"
Write-Host "  version=$Version  shown=$shown  length=$len"
Write-Host "  url=$url"
Write-Host "Next: upload $file to the $Tag release in $Repo (if it isn't there yet), then commit this file as appcast-windows.xml at the root of main."
