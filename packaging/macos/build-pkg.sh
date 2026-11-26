#!/bin/bash
# Builds a standard macOS installer package (text_killer-@VERSION@-macos.pkg)
# from this folder, using Apple's own pkgbuild/productbuild/lipo tools.
# Run it on a Mac:  bash build-pkg.sh
set -euo pipefail
cd "$(dirname "$0")"
VERSION="@VERSION@"
WORK=$(mktemp -d)
ROOT="$WORK/root"
mkdir -p "$ROOT/usr/local/bin" "$ROOT/usr/local/share/man/man1" "$ROOT/usr/local/share/doc/text_killer"
# One "universal" program that runs on both Apple Silicon and Intel Macs.
lipo -create bin/text_killer-arm64 bin/text_killer-x86_64 -output "$ROOT/usr/local/bin/text_killer"
chmod 755 "$ROOT/usr/local/bin/text_killer"
install -m 644 text_killer.1 "$ROOT/usr/local/share/man/man1/text_killer.1"
install -m 644 MANUAL.txt LICENSE "$ROOT/usr/local/share/doc/text_killer/"
install -m 755 uninstall.command "$ROOT/usr/local/share/doc/text_killer/uninstall.command"
xattr -cr "$ROOT" 2>/dev/null || true
pkgbuild --root "$ROOT" --identifier local.text_killer --version "$VERSION" \
  --install-location / "$WORK/component.pkg"
productbuild --package "$WORK/component.pkg" "text_killer-$VERSION-macos.pkg"
rm -rf "$WORK"
echo "Built text_killer-$VERSION-macos.pkg - double-click it to install."
