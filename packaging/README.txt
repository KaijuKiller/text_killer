Building the text_killer release packages
=========================================

1. Cross-compile the five release binaries (from the project root). This
   uses cargo-zigbuild, with Zig installed from PyPI. rust-toolchain.toml
   pins the Rust version and makes rustup install the needed targets and
   the rust-src component (used for the standard library's notices):

     pip install ziglang
     cargo install cargo-zigbuild
     for t in x86_64-unknown-linux-musl aarch64-unknown-linux-musl \
              x86_64-pc-windows-gnu x86_64-apple-darwin aarch64-apple-darwin; do
       cargo zigbuild --release --target $t
     done

2. Install the packaging tools (Debian/Ubuntu names):

     sudo apt install dpkg rpm nsis makepkg pacman-package-manager \
         libarchive-tools zstd fakeroot

   makepkg refuses to run as root, so build_packages.py runs it as a user
   named "builder" (create it with: sudo useradd -m builder).

3. Build everything into dist/<version>/:

     python3 packaging/build_packages.py

Files in this folder
  text_killer.1           manual page (installed by every installer)
  text_killer.desktop     Linux application-menu entry (opens in a terminal)
  linux/install.sh        installer for any Linux, shipped in the .tar.gz
  arch/PKGBUILD.in        Arch package recipe (makepkg -si)
  rpm/text_killer.spec.in RPM recipe
  windows/installer.nsi   NSIS script for the Windows setup .exe
  macos/install.command   double-click installer for macOS
  macos/uninstall.command
  macos/build-pkg.sh      builds a native .pkg on a Mac with Apple's tools
  third_party_licenses.py regenerates ../THIRD-PARTY-LICENSES.txt; run it
                          after changing dependencies (the build checks it)
