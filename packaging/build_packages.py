#!/usr/bin/env python3
"""Builds every text_killer release package from prebuilt binaries.

Run from the project root after cross-compiling (see packaging/README.txt):

    python3 packaging/build_packages.py

Output goes to dist/<version>/:
  portable:   linux (x86_64, arm64) .tar.gz, windows .zip, macos .zip (x2)
  installers: .deb, .rpm, Arch .pkg.tar.zst (+PKGBUILD), Windows setup .exe
              (also zipped),
              macOS installer .zip (install.command + build-pkg.sh)
"""

import gzip
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PKG = ROOT / "packaging"
VERSION = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M).group(1)
OUT = ROOT / "dist" / VERSION
TARGET = ROOT / "target"
NOTICES = "THIRD-PARTY-LICENSES.txt"

BINARIES = {
    "linux-x86_64": TARGET / "x86_64-unknown-linux-musl/release/text_killer",
    "linux-arm64": TARGET / "aarch64-unknown-linux-musl/release/text_killer",
    "windows-x86_64": TARGET / "x86_64-pc-windows-gnu/release/text_killer.exe",
    "macos-arm64": TARGET / "aarch64-apple-darwin/release/text_killer",
    "macos-x86_64": TARGET / "x86_64-apple-darwin/release/text_killer",
}

SUMMARY = "Terminal text editor with an IRC-style interface"
DESCRIPTION = (
    "text_killer is a terminal text editor with an IRC-client look. It shows\n"
    "documents as side-by-side pages (US Letter by default), highlights 93\n"
    "languages, opens office documents and e-books (.odt, .docx, .xlsx, .pptx,\n"
    ".rtf, .epub and more) as editable text, and saves .odt, .docx and .rtf."
)


def run(cmd, **kw):
    print("  $", " ".join(str(c) for c in cmd))
    subprocess.run([str(c) for c in cmd], check=True, **kw)


def sub(text: str) -> str:
    return text.replace("@VERSION@", VERSION)


def write(path: Path, text: str, mode: int = 0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    path.chmod(mode)


def copy(src: Path, dst: Path, mode: int = 0o644):
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(src, dst)
    dst.chmod(mode)


def tar_gz(src_dir: Path, out: Path):
    with tarfile.open(out, "w:gz") as tf:
        tf.add(src_dir, arcname=src_dir.name, filter=_root_owned)


def _root_owned(ti: tarfile.TarInfo) -> tarfile.TarInfo:
    ti.uid = ti.gid = 0
    ti.uname = ti.gname = "root"
    return ti


def zip_dir(src_dir: Path, out: Path):
    """Zips a folder, keeping Unix permissions (so programs stay runnable)."""
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zf:
        for path in sorted(src_dir.rglob("*")):
            arc = path.relative_to(src_dir.parent).as_posix()
            if path.is_dir():
                info = zipfile.ZipInfo(arc + "/")
                info.external_attr = (0o40755 << 16) | 0x10
                zf.writestr(info, "")
            else:
                info = zipfile.ZipInfo(arc)
                info.external_attr = (0o100000 | (path.stat().st_mode & 0o777)) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                zf.writestr(info, path.read_bytes())


INSTALL_LINUX = """text_killer @VERSION@ for Linux (@ARCH@)
==========================================
A single self-contained program (statically linked): no libraries needed,
works on any 64-bit Linux distribution.

Easiest: use the installer package for your distribution instead of this
folder if one exists (.deb for Debian/Ubuntu/Mint, .rpm for Fedora/openSUSE,
.pkg.tar.zst for Arch/Manjaro).

Install from this folder (any distribution):
    ./install.sh              system-wide into /usr/local (asks for sudo)
    ./install.sh --user       just for you, into ~/.local
Uninstall:
    ./install.sh --uninstall  (add --user if you installed with --user)

Or run it without installing:
    ./text_killer myfile.txt

Press F1 inside the editor for help. The full manual is MANUAL.txt.
"""

INSTALL_WINDOWS = """text_killer @VERSION@ for Windows (64-bit) - portable
====================================================
This is the portable version: nothing is installed. For a normal
installation (Start-menu entry, added to PATH, uninstall from Settings >
Apps) use text_killer-@VERSION@-windows-x86_64-setup.exe instead.

text_killer is a terminal program; it works best inside Windows Terminal
(built into Windows 11, free in the Microsoft Store for Windows 10).
Run it from this folder in Windows Terminal or PowerShell:
    .\\text_killer.exe myfile.txt

Windows SmartScreen may warn about an unknown publisher because the program
is not code-signed: choose "More info" > "Run anyway".

Requires Windows 10 or 11 (64-bit). Press F1 inside the editor for help.
"""

INSTALL_MAC_PORTABLE = """text_killer @VERSION@ for macOS (@WHICH@) - portable
=================================================
This is the portable version. For a normal installation use
text_killer-@VERSION@-macos-installer.zip instead.

1. Open Terminal and go to this folder, for example:
       cd ~/Downloads/@FOLDER@
2. Remove macOS's download quarantine once (the app is not notarized):
       xattr -d com.apple.quarantine text_killer
3. Run it:
       ./text_killer myfile.txt

In Terminal.app turn on Settings > Profiles > Keyboard > "Use Option as
Meta key" so Alt shortcuts work. Press F1 inside the editor for help.
"""

README_MAC_INSTALLER = """text_killer @VERSION@ installer for macOS (Apple Silicon and Intel)
================================================================

Option 1 - install now (recommended)
  1. Right-click (or Control-click) "install.command" and choose Open.
     (A plain double-click is blocked the first time because the file was
     downloaded and is not notarized by Apple; right-click > Open allows it.)
  2. A Terminal window opens and asks for your password.
  It installs text_killer to /usr/local/bin, plus its manual page.
  Afterwards run it from Terminal:   text_killer myfile.txt

  Alternatively, in Terminal:  bash ~/Downloads/@FOLDER@/install.command

Option 2 - build a standard .pkg installer
  In Terminal:  bash ~/Downloads/@FOLDER@/build-pkg.sh
  This uses Apple's own packaging tools (already on every Mac) to create
  text_killer-@VERSION@-macos.pkg, a normal double-click installer you can
  keep or share. Unsigned packages need right-click > Open the first time.

Uninstall: bash /usr/local/share/doc/text_killer/uninstall.command

Tip: in Terminal.app turn on Settings > Profiles > Keyboard > "Use Option as
Meta key" so Alt shortcuts work. Press F1 inside the editor for help.
"""


def build_portable(work: Path):
    print("Portable packages")
    for arch, label in [("x86_64", "linux-x86_64"), ("arm64", "linux-arm64")]:
        d = work / f"text_killer-{VERSION}-{label}"
        copy(BINARIES[label], d / "text_killer", 0o755)
        copy(ROOT / "MANUAL.txt", d / "MANUAL.txt")
        copy(ROOT / "LICENSE", d / "LICENSE")
        copy(ROOT / NOTICES, d / NOTICES)
        write(d / "text_killer.1", sub((PKG / "text_killer.1").read_text()))
        copy(PKG / "text_killer.desktop", d / "text_killer.desktop")
        copy(PKG / "linux/install.sh", d / "install.sh", 0o755)
        write(d / "INSTALL.txt", sub(INSTALL_LINUX).replace("@ARCH@", arch))
        tar_gz(d, OUT / f"{d.name}.tar.gz")

    d = work / f"text_killer-{VERSION}-windows-x86_64"
    copy(BINARIES["windows-x86_64"], d / "text_killer.exe", 0o755)
    write(d / "MANUAL.txt", (ROOT / "MANUAL.txt").read_text().replace("\n", "\r\n"))
    write(d / "LICENSE.txt", (ROOT / "LICENSE").read_text().replace("\n", "\r\n"))
    write(d / NOTICES, (ROOT / NOTICES).read_text().replace("\n", "\r\n"))
    write(d / "INSTALL.txt", sub(INSTALL_WINDOWS).replace("\n", "\r\n"))
    zip_dir(d, OUT / f"{d.name}.zip")

    for key, label, which in [
        ("macos-arm64", "macos-apple-silicon", "Apple Silicon: M1 and newer"),
        ("macos-x86_64", "macos-intel", "Intel"),
    ]:
        d = work / f"text_killer-{VERSION}-{label}"
        copy(BINARIES[key], d / "text_killer", 0o755)
        copy(ROOT / "MANUAL.txt", d / "MANUAL.txt")
        copy(ROOT / "LICENSE", d / "LICENSE")
        copy(ROOT / NOTICES, d / NOTICES)
        text = sub(INSTALL_MAC_PORTABLE).replace("@WHICH@", which).replace("@FOLDER@", d.name)
        write(d / "INSTALL.txt", text)
        zip_dir(d, OUT / f"{d.name}.zip")


def build_deb(work: Path):
    print("Debian/Ubuntu .deb packages")
    for label, arch in [("linux-x86_64", "amd64"), ("linux-arm64", "arm64")]:
        root = work / f"deb-{arch}"
        copy(BINARIES[label], root / "usr/bin/text_killer", 0o755)
        man = sub((PKG / "text_killer.1").read_text()).encode()
        (root / "usr/share/man/man1").mkdir(parents=True, exist_ok=True)
        (root / "usr/share/man/man1/text_killer.1.gz").write_bytes(gzip.compress(man, mtime=0))
        copy(PKG / "text_killer.desktop", root / "usr/share/applications/text_killer.desktop")
        docdir = root / "usr/share/doc/text-killer"
        copy(ROOT / "MANUAL.txt", docdir / "MANUAL.txt")
        copy(ROOT / NOTICES, docdir / NOTICES)
        write(
            docdir / "copyright",
            "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n"
            "Upstream-Name: text_killer\n\nFiles: *\nCopyright: 2026 the text_killer authors\nLicense: MIT\n "
            + "\n ".join(l if l else "." for l in (ROOT / "LICENSE").read_text().splitlines()[2:])
            + "\n\nFiles: bundled third-party components\nComment: The text_killer program includes open-source "
            "libraries and runtime code\n whose copyright and license notices are reproduced in\n "
            "/usr/share/doc/text-killer/THIRD-PARTY-LICENSES.txt\n",
        )
        size_kb = sum(p.stat().st_size for p in root.rglob("*") if p.is_file()) // 1024 + 1
        long_desc = "\n".join(" " + (l if l else ".") for l in DESCRIPTION.splitlines())
        write(
            root / "DEBIAN/control",
            f"Package: text-killer\nVersion: {VERSION}\nArchitecture: {arch}\n"
            f"Maintainer: text_killer <text_killer@localhost>\nInstalled-Size: {size_kb}\n"
            f"Section: editors\nPriority: optional\nDescription: {SUMMARY}\n{long_desc}\n",
        )
        for p in root.rglob("*"):
            if p.is_dir():
                p.chmod(0o755)
        run(["dpkg-deb", "--root-owner-group", "-Zxz", "--build", root, OUT / f"text-killer_{VERSION}_{arch}.deb"])


def build_rpm(work: Path):
    print("Fedora/openSUSE .rpm packages")
    for label, arch in [("linux-x86_64", "x86_64"), ("linux-arm64", "aarch64")]:
        top = work / f"rpm-{arch}"
        src = top / "SOURCES"
        copy(BINARIES[label], src / "text_killer", 0o755)
        write(src / "text_killer.1", sub((PKG / "text_killer.1").read_text()))
        copy(PKG / "text_killer.desktop", src / "text_killer.desktop")
        copy(ROOT / "MANUAL.txt", src / "MANUAL.txt")
        copy(ROOT / "LICENSE", src / "LICENSE")
        copy(ROOT / NOTICES, src / NOTICES)
        spec = top / "SPECS/text_killer.spec"
        write(spec, sub((PKG / "rpm/text_killer.spec.in").read_text()))
        run(["rpmbuild", "-bb", "--quiet", "--target", f"{arch}-linux", "--define", f"_topdir {top}",
             "--define", "_build_id_links none", spec])
        built = next((top / "RPMS").rglob("*.rpm"))
        shutil.copyfile(built, OUT / built.name)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build_arch(work: Path):
    print("Arch Linux packages (makepkg)")
    d = work / "arch"
    d.mkdir()
    tarballs = {a: OUT / f"text_killer-{VERSION}-linux-{a}.tar.gz" for a in ("x86_64", "arm64")}
    pkgbuild = sub((PKG / "arch/PKGBUILD.in").read_text())
    pkgbuild = pkgbuild.replace("@SHA_X86_64@", sha256(tarballs["x86_64"])).replace("@SHA_ARM64@", sha256(tarballs["arm64"]))
    write(d / "PKGBUILD", pkgbuild)
    write(OUT / "PKGBUILD", pkgbuild)
    for t in tarballs.values():
        shutil.copyfile(t, d / t.name)
    # makepkg refuses to run as root; use the unprivileged "builder" account.
    shutil.chown(d, "builder", "builder")
    for p in d.iterdir():
        shutil.chown(p, "builder", "builder")
    conf = Path("/etc/makepkg.conf").read_text()
    for carch in ("x86_64", "aarch64"):
        c = re.sub(r'^CARCH=.*$', f'CARCH="{carch}"', conf, flags=re.M)
        c = re.sub(r'^CHOST=.*$', f'CHOST="{carch}-pc-linux-gnu"', c, flags=re.M)
        c = re.sub(r"^PKGEXT=.*$", "PKGEXT='.pkg.tar.zst'", c, flags=re.M)
        write(d / f"makepkg-{carch}.conf", c)
        shutil.chown(d / f"makepkg-{carch}.conf", "builder", "builder")
        run(["su", "builder", "-c",
             f"cd {d} && PACKAGER='text_killer <text_killer@localhost>' makepkg --config makepkg-{carch}.conf -f --nodeps --noconfirm"],
            stdout=subprocess.DEVNULL)
    for p in d.glob("*.pkg.tar.zst"):
        shutil.copyfile(p, OUT / p.name)


def build_windows_installer(work: Path):
    print("Windows setup .exe (NSIS)")
    d = work / "nsis"
    copy(BINARIES["windows-x86_64"], d / "text_killer.exe", 0o755)
    write(d / "MANUAL.txt", (ROOT / "MANUAL.txt").read_text().replace("\n", "\r\n"))
    write(d / "LICENSE.txt", (ROOT / "LICENSE").read_text().replace("\n", "\r\n"))
    write(d / NOTICES, (ROOT / NOTICES).read_text().replace("\n", "\r\n"))
    out = OUT / f"text_killer-{VERSION}-windows-x86_64-setup.exe"
    run(["makensis", "-V2", f"-DVERSION={VERSION}", f"-DSRCDIR={d}", f"-DOUTFILE={out}", PKG / "windows/installer.nsi"])
    # Some browsers and mail filters block direct .exe downloads; also ship it zipped.
    with zipfile.ZipFile(out.with_suffix(".zip"), "w", zipfile.ZIP_DEFLATED) as zf:
        info = zipfile.ZipInfo(out.name)
        info.external_attr = 0o100755 << 16
        info.compress_type = zipfile.ZIP_DEFLATED
        zf.writestr(info, out.read_bytes())


def build_mac_installer(work: Path):
    print("macOS installer bundle")
    d = work / f"text_killer-{VERSION}-macos-installer"
    copy(BINARIES["macos-arm64"], d / "bin/text_killer-arm64", 0o755)
    copy(BINARIES["macos-x86_64"], d / "bin/text_killer-x86_64", 0o755)
    for name, mode in [("install.command", 0o755), ("uninstall.command", 0o755), ("build-pkg.sh", 0o755)]:
        write(d / name, sub((PKG / "macos" / name).read_text()), mode)
    write(d / "text_killer.1", sub((PKG / "text_killer.1").read_text()))
    copy(ROOT / "MANUAL.txt", d / "MANUAL.txt")
    copy(ROOT / "LICENSE", d / "LICENSE")
    copy(ROOT / NOTICES, d / NOTICES)
    write(d / "README.txt", sub(README_MAC_INSTALLER).replace("@FOLDER@", d.name))
    zip_dir(d, OUT / f"{d.name}.zip")


def main():
    # The notices must match the dependencies being shipped.
    run([sys.executable, PKG / "third_party_licenses.py", "--check"])
    missing = [str(p) for p in BINARIES.values() if not p.exists()]
    if missing:
        sys.exit("missing binaries (cross-compile first):\n  " + "\n  ".join(missing))
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix="tk-pkg-") as tmp:
        work = Path(tmp)
        work.chmod(0o755)
        build_portable(work)
        build_deb(work)
        build_rpm(work)
        build_arch(work)
        build_windows_installer(work)
        build_mac_installer(work)
    sums = "".join(f"{sha256(p)}  {p.name}\n" for p in sorted(OUT.iterdir()) if p.is_file())
    (OUT / "SHA256SUMS.txt").write_text(sums)
    print(f"\nPackages in {OUT}:")
    for p in sorted(OUT.iterdir()):
        print(f"  {p.stat().st_size:>10,}  {p.name}")


if __name__ == "__main__":
    main()
