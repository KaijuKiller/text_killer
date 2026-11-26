#!/bin/bash
# Removes text_killer installed by install.command or the .pkg.
set -euo pipefail
PREFIX=/usr/local
echo "Removing text_killer from $PREFIX (macOS will ask for your password)."
sudo rm -f "$PREFIX/bin/text_killer" "$PREFIX/share/man/man1/text_killer.1"
sudo rm -rf "$PREFIX/share/doc/text_killer"
sudo pkgutil --forget local.text_killer >/dev/null 2>&1 || true
echo "Done. Your settings (~/Library/Application Support/text_killer) were left in place."
read -r -p "Press Return to close this window. " _ || true
