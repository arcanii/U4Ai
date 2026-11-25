#!/usr/bin/env bash
#
# Sign a U4Ai Windows installer or Linux tarball on the Mac with the
# update key, using Sparkle's `sign_update`. Prints the sparkle:edSignature
# for windows/scripts/make-appcast.ps1 -Signature or
# linux/scripts/make-appcast.sh --signature. On Windows, make-appcast.ps1 can
# sign by itself with the key's exported file.
#
# Usage:
#   windows/scripts/sign-release.sh path/to/U4Ai-0.1.0-setup.exe
#
# Notes:
#   * The signature covers the exact bytes, so publish the SAME file you sign.
#     Signing the copy downloaded from the GitHub release proves exactly that.
#   * The key is the Mac app's (its SUPublicEDKey is UPDATE_PUBLIC_KEY), kept
#     in the login Keychain under Sparkle's default account, where the Mac
#     app's own scripts sign with it, so sign_update is run with no --account.
#     macOS may ask to allow access. SIGN_UPDATE_ARGS adds arguments, e.g. to
#     sign straight from an exported key file:
#         SIGN_UPDATE_ARGS="--ed-key-file /path/to/u4ai-update.key" windows/scripts/sign-release.sh <file>
#   * sign_update is looked for in $SIGN_UPDATE, the Mac app's Xcode build
#     (~/Library/Developer/Xcode/DerivedData/Usage4Claude-*/SourcePackages/
#     artifacts/sparkle/Sparkle/bin), /tmp/sparkle-tools/bin, then on PATH.
#   * stdout is only the base64 signature, so it can be captured:
#         SIG=$(windows/scripts/sign-release.sh U4Ai-0.1.0-setup.exe)

set -euo pipefail

file="${1:-}"
if [[ -z "$file" || ! -f "$file" ]]; then
  echo "usage: $(basename "$0") <U4Ai-x.y.z-setup.exe | U4Ai-x.y.z-linux-<arch>.tar.gz>" >&2
  exit 1
fi

tool="${SIGN_UPDATE:-}"
if [[ -z "$tool" ]]; then
  for c in "$HOME"/Library/Developer/Xcode/DerivedData/Usage4Claude-*/SourcePackages/artifacts/sparkle/Sparkle/bin/sign_update \
           /tmp/sparkle-tools/bin/sign_update \
           "$(command -v sign_update 2>/dev/null || true)"; do
    if [[ -n "$c" && -x "$c" ]]; then tool="$c"; break; fi
  done
fi
if [[ -z "$tool" ]]; then
  echo "error: sign_update not found. Set SIGN_UPDATE=/path/to/sign_update" >&2
  echo "       (it ships in the Sparkle release under bin/, and in the Mac app's" >&2
  echo "       Xcode build once Xcode has fetched its Sparkle package)." >&2
  exit 1
fi

args="${SIGN_UPDATE_ARGS:-}"
echo "Signing $file" >&2
echo "  with $tool${args:+ $args}" >&2
# shellcheck disable=SC2086
out="$("$tool" $args "$file")"
echo "  -> $out" >&2

# Sparkle prints:  sparkle:edSignature="BASE64==" length="12345"
sig="$(printf '%s\n' "$out" | sed -n 's/.*sparkle:edSignature="\([^"]*\)".*/\1/p')"
if [[ -z "$sig" ]]; then
  echo "error: could not parse edSignature from the sign_update output above." >&2
  exit 1
fi

echo >&2
echo "Signature (pass to make-appcast.ps1 -Signature or make-appcast.sh --signature):" >&2
printf '%s\n' "$sig"
