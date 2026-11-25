#!/bin/sh
#
# Install U4Ai from the folder this script is in, as the release tarball
# unpacks it, or take it away again.
#
#   ./install.sh [--prefix <dir>]
#   ./install.sh --uninstall [--prefix <dir>]
#
# The prefix is ~/.local unless given, which needs no root: u4ai goes in
# <prefix>/bin, its menu entry and icons in <prefix>/share, and its notices
# in <prefix>/share/doc/u4ai. For every user, run it as root with
# --prefix /usr/local. The updater (linux/src/updater.rs) runs the new
# tarball's install.sh into the prefix it was installed in.
#
# Uninstalling takes away what this put there, and the entry that Start at
# login wrote in ~/.config/autostart where it runs this prefix's u4ai.
# U4Ai's settings and the sign-in in the system's credential store stay
# where they are either way.

set -eu
# Folders readable by everyone, as an install for every user needs, whatever
# the umask of the root shell running it; files get their modes from install.
umask 022

usage() {
  echo "usage: $(basename "$0") [--uninstall] [--prefix <dir>]" >&2
  exit 1
}

prefix=""
uninstall=0
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) [ $# -ge 2 ] || usage; prefix="$2"; shift 2 ;;
    --uninstall) uninstall=1; shift ;;
    *) usage ;;
  esac
done
[ -n "$prefix" ] || prefix="${HOME:?}/.local"
case "$prefix" in
  /*) ;;
  *) echo "error: the prefix must be an absolute path: $prefix" >&2; exit 1 ;;
esac
# The menu entry names u4ai by its path, where these would need escaping
# (and sed below would read & and | as its own).
nl='
'
case "$prefix" in
  *[\"\`\$\\%\&\|]* | *"$nl"*) echo "error: pick a prefix without \" \` \$ \\ % & | or a new line: $prefix" >&2; exit 1 ;;
esac
# One slash at its end (or a lone /) would name the program //bin/u4ai.
while [ "$prefix" != "/" ] && [ "${prefix%/}" != "$prefix" ]; do
  prefix="${prefix%/}"
done
[ "$prefix" != "/" ] || prefix=""

id=com.arcanii.U4Ai
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
program="$prefix/bin/u4ai"
apps="$prefix/share/applications"
icons="$prefix/share/icons/hicolor"
docs="$prefix/share/doc/u4ai"
autostart="${XDG_CONFIG_HOME:-${HOME:?}/.config}/autostart/$id.desktop"

# Asks the desktop to read the menu entries and icons again, where it can.
refresh() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$apps" 2>/dev/null || true
  fi
  if [ -d "$icons" ]; then
    touch "$icons" 2>/dev/null || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -f "$icons/index.theme" ]; then
    gtk-update-icon-cache -q -t "$icons" 2>/dev/null || true
  fi
}

# Copies a file to a new name beside its place and moves it over the old
# one, so a running u4ai keeps the file it started from (and the updater
# sees its /proc/self/exe marked deleted).
put() {
  mode="$1"; from="$2"; to="$3"
  install -m "$mode" "$from" "$(dirname -- "$to")/.$(basename -- "$to").new"
  mv -f "$(dirname -- "$to")/.$(basename -- "$to").new" "$to"
}

if [ "$uninstall" = 1 ]; then
  rm -f "$program" "$apps/$id.desktop"
  for icon in "$icons"/*/apps/"$id".png; do
    rm -f "$icon"
  done
  rm -f "$docs/README.md" "$docs/LICENSE" "$docs/THIRD-PARTY-NOTICES.txt"
  rmdir "$docs" 2>/dev/null || true
  # Start at login's entry, where it starts this copy (another prefix's
  # copy may have written it).
  if [ -f "$autostart" ]; then
    run="$(sed -n 's/^Exec=//p' "$autostart" | head -n 1)"
    case "$run" in
      "$program" | "$program "* | "\"$program\"" | "\"$program\" "*) rm -f "$autostart" ;;
    esac
  fi
  refresh
  echo "U4Ai is uninstalled from ${prefix:-/}."
  exit 0
fi

[ -x "$here/bin/u4ai" ] || { echo "error: no bin/u4ai beside this script" >&2; exit 1; }
[ -f "$here/share/applications/$id.desktop" ] || { echo "error: no share/applications/$id.desktop beside this script" >&2; exit 1; }
mkdir -p "$prefix/bin" "$apps"
put 755 "$here/bin/u4ai" "$program"
for icon in "$here"/share/icons/hicolor/*/apps/"$id".png; do
  [ -f "$icon" ] || continue
  size="$(basename "$(dirname "$(dirname "$icon")")")"
  mkdir -p "$icons/$size/apps"
  put 644 "$icon" "$icons/$size/apps/$id.png"
done
for doc in "$here"/share/doc/u4ai/*; do
  [ -f "$doc" ] || continue
  mkdir -p "$docs"
  put 644 "$doc" "$docs/$(basename -- "$doc")"
done
# The menu runs u4ai by its full path: a launcher's PATH may lack
# <prefix>/bin.
sed "s|^Exec=.*|Exec=\"$program\"|" "$here/share/applications/$id.desktop" > "$apps/.$id.desktop.new"
chmod 644 "$apps/.$id.desktop.new"
mv -f "$apps/.$id.desktop.new" "$apps/$id.desktop"
refresh

echo "U4Ai is installed in ${prefix:-/}: it's in your applications as U4Ai."
case ":$PATH:" in
  *":$prefix/bin:"*) ;;
  *) echo "To start it as 'u4ai' from a shell, add $prefix/bin to your PATH." ;;
esac
