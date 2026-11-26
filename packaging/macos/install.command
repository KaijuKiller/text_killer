#!/bin/bash
# text_killer installer for macOS (Apple Silicon and Intel).
# Double-click this file in Finder, or run:  bash install.command
set -euo pipefail
cd "$(dirname "$0")"
VERSION="@VERSION@"
case "$(uname -m)" in
  arm64) BIN="bin/text_killer-arm64" ;;
  x86_64) BIN="bin/text_killer-x86_64" ;;
  *) echo "Unsupported Mac processor: $(uname -m)"; exit 1 ;;
esac
PREFIX=/usr/local
echo "Installing text_killer $VERSION to $PREFIX/bin"
echo "(macOS will ask for your password to write there.)"
echo
sudo mkdir -p "$PREFIX/bin" "$PREFIX/share/man/man1" "$PREFIX/share/doc/text_killer"
sudo install -m 755 "$BIN" "$PREFIX/bin/text_killer"
sudo install -m 644 text_killer.1 "$PREFIX/share/man/man1/text_killer.1"
sudo install -m 644 MANUAL.txt LICENSE THIRD-PARTY-LICENSES.txt "$PREFIX/share/doc/text_killer/"
sudo install -m 755 uninstall.command "$PREFIX/share/doc/text_killer/uninstall.command"
# The program was downloaded, so clear the quarantine flag for it.
sudo xattr -d com.apple.quarantine "$PREFIX/bin/text_killer" 2>/dev/null || true
echo
if "$PREFIX/bin/text_killer" --version >/dev/null 2>&1; then
  echo "Installed: $("$PREFIX/bin/text_killer" --version)"
else
  echo "Installed, but the program did not start. Please report this."
fi
case ":$PATH:" in
  *":$PREFIX/bin:"*) ;;
  *) echo "Note: add $PREFIX/bin to your PATH (it normally is by default)." ;;
esac
echo
echo "Open Terminal and run:   text_killer myfile.txt"
echo "Manual:                  man text_killer   or   $PREFIX/share/doc/text_killer/MANUAL.txt"
echo "To uninstall later run:  bash $PREFIX/share/doc/text_killer/uninstall.command"
echo
read -r -p "Press Return to close this window. " _ || true
