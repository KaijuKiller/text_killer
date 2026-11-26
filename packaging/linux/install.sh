#!/bin/sh
# text_killer installer for any Linux distribution.
#   ./install.sh            install system-wide to /usr/local (asks for sudo)
#   ./install.sh --user     install for the current user to ~/.local
#   ./install.sh --uninstall [--user]
set -eu
here=$(cd "$(dirname "$0")" && pwd)
mode=system
action=install
for a in "$@"; do
  case "$a" in
    --user) mode=user ;;
    --uninstall) action=uninstall ;;
    -h|--help) sed -n '2,5p' "$0"; exit 0 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done
if [ "$mode" = user ]; then
  prefix="$HOME/.local"; sudo=""
else
  prefix="/usr/local"
  if [ "$(id -u)" -eq 0 ]; then sudo=""; else sudo="sudo"; fi
fi
bin="$prefix/bin/text_killer"
man="$prefix/share/man/man1/text_killer.1"
desktop="$prefix/share/applications/text_killer.desktop"
doc="$prefix/share/doc/text_killer"
if [ "$action" = uninstall ]; then
  $sudo rm -f "$bin" "$man" "$desktop"
  $sudo rm -rf "$doc"
  echo "text_killer removed from $prefix"
  exit 0
fi
$sudo mkdir -p "$prefix/bin" "$prefix/share/man/man1" "$prefix/share/applications" "$doc"
$sudo install -m 755 "$here/text_killer" "$bin"
$sudo install -m 644 "$here/text_killer.1" "$man"
$sudo install -m 644 "$here/text_killer.desktop" "$desktop"
$sudo install -m 644 "$here/MANUAL.txt" "$here/LICENSE" "$here/THIRD-PARTY-LICENSES.txt" "$doc/"
echo "text_killer installed to $bin"
case ":$PATH:" in
  *":$prefix/bin:"*) ;;
  *) echo "Note: $prefix/bin is not on your PATH. Add it, e.g.:"
     echo "  echo 'export PATH=\"$prefix/bin:\$PATH\"' >> ~/.profile" ;;
esac
echo "Run: text_killer myfile.txt   (uninstall: $0 --uninstall$( [ "$mode" = user ] && echo ' --user'))"
