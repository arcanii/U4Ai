#!/usr/bin/env bash
#
# Write the appcast for one Linux tarball of a U4Ai release, which its
# updater (linux/src/updater.rs) reads. Commit the file this writes as
# appcast-linux-<arch>.xml at the root of this repo's main branch, one per
# architecture, once the tarball is up on the release.
#
#   linux/scripts/make-appcast.sh --tarball target/linux/U4Ai-0.1.0-linux-x86_64.tar.gz \
#       --tag v0.1.0-linux --signature <base64>
#
# The signature is the update key's over the tarball's exact bytes, as
# Sparkle's: made on the Mac with windows/scripts/sign-release.sh <tarball>,
# or here with --key-file <file> (the key's base64 seed, as
# examples/update_key.rs reads it). The private key never goes in the repo.
#
# Options: --repo <owner/name> (default arcanii/U4Ai),
# --notes <url> (default: the release page), --out <file> (default
# target/linux/appcast-linux-<arch>.xml).
#
# Checks first that the tarball's u4ai reports the version the tag names
# (so run it on the architecture the tarball is for) and updates itself
# (package.sh --updates), that it holds install.sh, and that the signature
# verifies against UPDATE_PUBLIC_KEY, because each mistake would only show
# up as failed updates on users' computers.

set -euo pipefail

die() { echo "error: $*" >&2; exit 1; }

tarball=""; tag=""; sig=""; key_file=""; repo_name="arcanii/U4Ai"; notes=""; out=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --tarball) tarball="${2:-}"; shift 2 ;;
    --tag) tag="${2:-}"; shift 2 ;;
    --signature) sig="${2:-}"; shift 2 ;;
    --key-file) key_file="${2:-}"; shift 2 ;;
    --repo) repo_name="${2:-}"; shift 2 ;;
    --notes) notes="${2:-}"; shift 2 ;;
    --out) out="${2:-}"; shift 2 ;;
    *) die "unknown option $1 (see the top of $0)" ;;
  esac
done
[[ -n "$tarball" && -n "$tag" ]] || die "usage: $(basename "$0") --tarball <U4Ai-x.y.z-linux-<arch>.tar.gz> --tag v<x.y.z>-linux (--signature <base64> | --key-file <file>)"
[[ -f "$tarball" ]] || die "no tarball at $tarball"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?-linux$ ]] || die "--tag should look like v0.1.0-linux (v0.1.0.1-linux for a patch release), not $tag"
[[ -n "$sig" || -n "$key_file" ]] || die "give --signature (from windows/scripts/sign-release.sh on the Mac) or --key-file"

# This script is in linux/scripts; the repo root is two levels up.
repo="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
tarball="$(CDPATH= cd -- "$(dirname -- "$tarball")" && pwd)/$(basename -- "$tarball")"
file="$(basename -- "$tarball")"
# U4Ai-0.1.0-linux-x86_64.tar.gz -> 0.1.0 and x86_64. A test build's
# 0.1.0.DEV doesn't match, so it never goes in a feed.
short="$(printf '%s\n' "$file" | sed -n 's/^U4Ai-\([0-9.]*\)-linux-\([a-z0-9_]*\)\.tar\.gz$/\1/p')"
arch="$(printf '%s\n' "$file" | sed -n 's/^U4Ai-\([0-9.]*\)-linux-\([a-z0-9_]*\)\.tar\.gz$/\2/p')"
[[ -n "$short" && -n "$arch" ]] || die "$file isn't named as package.sh names a release's tarball"
[[ "$tag" == "v$short-linux" ]] || die "the tarball is U4Ai $short, but the tag is $tag"
[[ -n "$out" ]] || out="$repo/target/linux/appcast-linux-$arch.xml"

# The build number, from the tarball's own u4ai.
work="$(mktemp -d "${TMPDIR:-/tmp}/u4ai-appcast.XXXXXX")"
trap 'rm -rf "$work"' EXIT
tar -xzf "$tarball" -C "$work"
program="$work/u4ai-$short/bin/u4ai"
[[ -x "$program" ]] || die "$file holds no u4ai-$short/bin/u4ai"
reported="$("$program" --version 2>/dev/null)" || die "its u4ai doesn't run here; make the $arch appcast on a $arch computer"
build="$(printf '%s\n' "$reported" | sed -n 's/^U4Ai \([0-9.]*\) (build \([0-9]*\))$/\2/p')"
version="$(printf '%s\n' "$reported" | sed -n 's/^U4Ai \([0-9.]*\) (build \([0-9]*\))$/\1/p')"
[[ -n "$build" && "$version" == "$short" ]] || die "its u4ai says \"$reported\", not U4Ai $short"
# Every copy that reads the feed updates to it, so it must update itself
# too, or they'd all stop there (package.sh --updates).
said="$(U4AI_SAY_UPDATES=1 "$program" --version 2>/dev/null | sed -n 2p)"
[[ "$said" == "updates: on" ]] || die "its u4ai says \"${said:-nothing}\" of updates, not \"updates: on\": make it with package.sh --updates"
[[ -x "$work/u4ai-$short/install.sh" ]] || die "$file holds no install.sh, which the updater runs"

key_tool() {
  cargo run --quiet --locked --example update_key --manifest-path "$repo/Cargo.toml" -- "$@"
}
if [[ -z "$sig" ]]; then
  [[ -f "$key_file" ]] || die "no key file at $key_file"
  echo "Signing $file with the key in $key_file"
  sig="$(key_tool sign "$tarball" "$key_file")"
fi
sig="$(printf '%s' "$sig" | tr -d '[:space:]')"
key_tool verify "$tarball" "$sig" || die "the signature check failed. Sign this exact file with the update key."

len="$(stat -c %s "$tarball")"
url="https://github.com/$repo_name/releases/download/$tag/$file"
[[ -n "$notes" ]] || notes="https://github.com/$repo_name/releases/tag/$tag"
pub="$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')"
mkdir -p "$(dirname -- "$out")"
cat > "$out" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>U4Ai (Linux, $arch) Updates</title>
    <link>https://raw.githubusercontent.com/$repo_name/main/appcast-linux-$arch.xml</link>
    <description>Most recent updates to U4Ai for Linux on $arch</description>
    <language>en</language>
    <item>
      <title>Version $short</title>
      <sparkle:releaseNotesLink>$notes</sparkle:releaseNotesLink>
      <pubDate>$pub</pubDate>
      <enclosure url="$url"
                 sparkle:version="$build"
                 sparkle:shortVersionString="$short"
                 sparkle:edSignature="$sig"
                 length="$len"
                 type="application/gzip"/>
    </item>
  </channel>
</rss>
EOF
echo "Wrote $out"
echo "  version=$short build=$build arch=$arch length=$len"
echo "  url=$url"
echo "Next: upload $file to the $tag release in $repo_name (if it isn't there yet), then commit this file as appcast-linux-$arch.xml at the root of main."
