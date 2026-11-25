#!/usr/bin/env bash
#
# Make U4Ai's Linux release: a tarball holding u4ai, its menu entry,
# its icons, its notices and install.sh.
#
#   linux/scripts/package.sh [--out <dir>] [--glibc <version> | --native] [--no-build] [--allow-dirty]
#       [--updates | --no-updates]
#
# Run from anywhere in the repo. It builds u4ai in release mode (unless
# --no-build, which takes what the build folder holds), then writes
# <out>/U4Ai-<version>-linux-<arch>.tar.gz and its .sha256 (<out> is
# target/linux unless given). The tarball unpacks to u4ai-<version>/,
# laid out as it installs:
#
#   install.sh
#   bin/u4ai
#   share/applications/com.arcanii.U4Ai.desktop
#   share/icons/hicolor/<size>/apps/com.arcanii.U4Ai.png
#   share/doc/u4ai/README.md, LICENSE, THIRD-PARTY-NOTICES.txt
#
# The updater (linux/src/updater.rs) relies on that: it finds the folder
# holding install.sh, asks its bin/u4ai for its build, and runs its
# install.sh into the prefix it was installed in.
#
# A program linked on a new Linux needs that Linux's glibc, and older ones
# refuse to start it. So it builds with cargo-zigbuild and zig against glibc
# 2.31 (--glibc picks another), which Ubuntu 20.04, Debian 11 and everything
# since have: the result goes in target/<triple>/release. --native builds
# with plain cargo into target/release instead, for a tarball that runs on
# this computer's Linux and newer only. linux/RELEASING.md says how to get
# zig and cargo-zigbuild.
#
# The build number is the commit count (plus build_offset), so a tree with
# changes is refused unless --allow-dirty: its build would claim a commit it
# doesn't match. So is a build still worked toward (dev = true).
#
# U4Ai updates itself only in a build made as one of its releases
# (U4AI_UPDATES=1 as it's built). The update key isn't on Linux (the
# tarball is signed on the Mac afterwards), so a release says which it is:
# --updates for the project's own, --no-updates for one that never updates
# itself (a fork's). A test build (--allow-dirty) is made without updates
# unless given --updates. Nothing is published.

set -euo pipefail

usage() {
  echo "usage: $(basename "$0") [--out <dir>] [--glibc <version> | --native] [--no-build] [--allow-dirty] [--updates | --no-updates]" >&2
  exit 1
}
die() { echo "error: $*" >&2; exit 1; }

# This script is in linux/scripts; the repo root is two levels up.
repo="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
out="$repo/target/linux"; build=1; allow_dirty=0; glibc=2.31; updates=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) out="${2:-}"; [[ -n "$out" ]] || usage; shift 2 ;;
    --glibc) glibc="${2:-}"; [[ "$glibc" =~ ^2\.[0-9]+$ ]] || usage; shift 2 ;;
    --native) glibc=""; shift ;;
    --no-build) build=0; shift ;;
    --allow-dirty) allow_dirty=1; shift ;;
    --updates) updates=1; shift ;;
    --no-updates) updates=0; shift ;;
    *) usage ;;
  esac
done
mkdir -p "$out"
out="$(CDPATH= cd -- "$out" && pwd)"
cd "$repo"

if [[ "$allow_dirty" = 0 && -n "$(git status --porcelain)" ]]; then
  die "the tree has changes; commit them, or pass --allow-dirty for a build that's only for this computer"
fi
# Work toward a version (`dev = true` under [workspace.metadata.u4ai])
# isn't released: U4Ai says 0.1.0.DEV until releasing sets it to false.
# A test build keeps it, and its files say so.
if [[ "$allow_dirty" = 0 ]] && awk '/^\[workspace\.metadata\.u4ai\]/ { f = 1; next } /^\[/ { f = 0 } f && /^dev *= *true/ { found = 1 } END { exit !found }' Cargo.toml; then
  die "U4Ai is still worked toward (dev = true under [workspace.metadata.u4ai]): set it to false and commit to release it, or pass --allow-dirty for a test build"
fi
if [[ "$allow_dirty" = 0 && -z "$updates" ]]; then
  die "say whether this release updates itself: --updates for the project's own (its tarball signed with the update key on the Mac), or --no-updates for one that never does"
fi
for f in README.md LICENSE; do
  [[ -f "$repo/$f" ]] || die "no $f at the repo's root, which the tarball carries"
done
said_updates="updates: off"
[[ "$updates" = 1 ]] && said_updates="updates: on"
echo "This build: $said_updates"
export U4AI_UPDATES="${updates:-0}"
triple="$(rustc -vV | sed -n 's/^host: //p')"
[[ -n "$triple" ]] || die "rustc names no host target"
# The build's own folders out of the program (panics name their source
# files): the home folder as ~ (Cargo's crates are under it) and the repo as
# u4ai, so a release names no one's account. Encoded, parted by 0x1f, as
# a folder may hold spaces.
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$HOME=~"$'\x1f'"--remap-path-prefix=$repo=u4ai"
if [[ -n "$glibc" ]]; then
  bin="$repo/target/$triple/release"
  if [[ "$build" = 1 ]]; then
    command -v zig >/dev/null && cargo zigbuild --help >/dev/null 2>&1 \
      || die "zig and cargo-zigbuild are needed to build for glibc $glibc (linux/RELEASING.md), or pass --native"
    cargo zigbuild --release --locked -p u4ai --bin u4ai --target "$triple.$glibc"
  fi
else
  bin="$repo/target/release"
  if [[ "$build" = 1 ]]; then
    cargo build --release --locked -p u4ai --bin u4ai
  fi
fi
[[ -x "$bin/u4ai" ]] || die "no $bin/u4ai; build it, or leave out --no-build"
# It mustn't need a newer glibc than the one built for.
if [[ -n "$glibc" ]]; then
  need="$(objdump -T "$bin/u4ai" | grep -oE 'GLIBC_2\.[0-9]+' | sort -t. -k2 -n -u | tail -1)"
  [[ "${need#GLIBC_2.}" -le "${glibc#2.}" ]] || die "u4ai needs $need, newer than glibc $glibc"
fi

# "U4Ai 0.1.0 (build 42)" -> 0.1.0 and 42; a patch release says
# 0.1.0.1, which names the files as its tag does, and a test build of work
# toward it 0.1.0.DEV.
reported="$("$bin/u4ai" --version)"
version="$(printf '%s\n' "$reported" | sed -n 's/^U4Ai \([^ ]*\) (build \([0-9]*\))$/\1/p')"
build_number="$(printf '%s\n' "$reported" | sed -n 's/^U4Ai \([^ ]*\) (build \([0-9]*\))$/\2/p')"
[[ -n "$version" && -n "$build_number" ]] || die "unexpected version output: $reported"
said="$(U4AI_SAY_UPDATES=1 "$bin/u4ai" --version | sed -n 2p)"
[[ "$said" == "$said_updates" ]] || die "$bin/u4ai says \"$said\", not \"$said_updates\"; build it again, without --no-build"
# What it was built for, which a 32-bit system on a 64-bit kernel tells
# apart and `uname -m` doesn't.
arch="${triple%%-*}"

id=com.arcanii.U4Ai
name="u4ai-$version"
stage="$(mktemp -d "${TMPDIR:-/tmp}/u4ai-package.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
dir="$stage/$name"
mkdir -p "$dir/bin" "$dir/share/applications" "$dir/share/doc/u4ai"

install -m 755 "$repo/linux/scripts/install.sh" "$dir/install.sh"
install -m 755 "$bin/u4ai" "$dir/bin/u4ai"
install -m 644 "$repo/linux/packaging/$id.desktop" "$dir/share/applications/$id.desktop"
# Sizes the hicolor theme lists that assets/icon has.
for size in 16 32 64 128 256 512; do
  mkdir -p "$dir/share/icons/hicolor/${size}x${size}/apps"
  install -m 644 "$repo/assets/icon/$size.png" "$dir/share/icons/hicolor/${size}x${size}/apps/$id.png"
done
install -m 644 "$repo/README.md" "$repo/LICENSE" "$dir/share/doc/u4ai/"

# THIRD-PARTY-NOTICES.txt: every crate compiled into u4ai.
cargo run --quiet --locked --example third_party_notices -- --target "$triple" \
  --binary u4ai --out "$stage/crates.txt"
{
  cat <<EOF
U4Ai - third-party notices
==============================

U4Ai is built from the open-source Rust crates listed below. Their
copyright notices and licenses follow.

EOF
  cat "$stage/crates.txt"
} > "$dir/share/doc/u4ai/THIRD-PARTY-NOTICES.txt"
chmod 644 "$dir/share/doc/u4ai/THIRD-PARTY-NOTICES.txt"
rm "$stage/crates.txt"

tarball="U4Ai-$version-linux-$arch.tar.gz"
tar -C "$stage" --sort=name --owner=0 --group=0 --numeric-owner --mode=u+rwX,go+rX,go-w -czf "$out/$tarball" "$name"
(cd "$out" && sha256sum "$tarball" > "$tarball.sha256")
if [[ -n "$glibc" ]]; then runs="glibc $glibc and newer"; else runs="this computer's glibc and newer"; fi
echo "$out/$tarball ($reported, $triple, for $runs)"
