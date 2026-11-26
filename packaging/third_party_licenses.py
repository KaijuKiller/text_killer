#!/usr/bin/env python3
"""Generates THIRD-PARTY-LICENSES.txt: the copyright and license notices of
every third-party component compiled into the text_killer program.

    python3 packaging/third_party_licenses.py          # (re)write the file
    python3 packaging/third_party_licenses.py --check  # fail if out of date

The crate list comes from `cargo tree` (runtime dependencies for all target
platforms; build-time-only crates such as proc-macros are excluded because
none of their code ends up in the program). License texts are read from
each crate's published source in the cargo registry. Notices for the
statically linked runtime pieces (Rust standard library, musl, mingw-w64,
Zig's compiler runtime) are read from the installed toolchains.
"""

import html
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD-PARTY-LICENSES.txt"
RULE = "=" * 78
THIN = "-" * 78

LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|copyright|notice|unlicense)", re.I)


def sh(*cmd) -> str:
    return subprocess.run(cmd, cwd=ROOT, check=True, capture_output=True, text=True).stdout


def runtime_crates():
    out = sh("cargo", "tree", "-e", "normal,no-proc-macro", "--target", "all", "--prefix", "none",
             "--format", "{p}|{l}")
    crates = {}
    for line in out.splitlines():
        line = line.replace(" (*)", "").strip()
        if not line or "|" not in line:
            continue
        pkg, lic = line.split("|", 1)
        name, ver = pkg.split()[:2]
        if name == "text_killer":
            continue
        crates[(name, ver.lstrip("v"))] = lic
    return crates


def crate_dirs():
    meta = json.loads(sh("cargo", "metadata", "--format-version", "1", "--locked"))
    return {(p["name"], p["version"]): (Path(p["manifest_path"]).parent, p.get("repository") or "")
            for p in meta["packages"]}


def is_apache2(text: str) -> bool:
    return "Apache License" in text and "Version 2.0, January 2004" in text


def norm(text: str) -> str:
    text = text.replace("\r\n", "\n").replace("\t", "    ")
    return "\n".join(l.rstrip() for l in text.strip().split("\n"))


def license_texts(d: Path):
    files = sorted(p for p in d.iterdir() if p.is_file() and LICENSE_FILE.match(p.name))
    out = []
    for p in files:
        t = norm(p.read_text(errors="replace"))
        if is_apache2(t):
            out.append((p.name, "[Apache License 2.0: full text in Appendix A]"))
        else:
            out.append((p.name, t))
    return out


def toolchain_paths():
    sysroot = Path(sh("rustc", "--print", "sysroot").strip())
    rust_doc = sysroot / "share/doc/rust"
    zig = None
    try:
        import ziglang  # installed with `pip install ziglang` for cross-compiling

        zig = Path(ziglang.__file__).parent
    except ImportError:
        pass
    return rust_doc, zig


def html_to_text(s: str) -> str:
    # License texts in <pre> blocks repeat (often the same Apache text many
    # times): show Apache via Appendix A and every other text only once.
    seen = {}

    def pre(m):
        body = norm(html.unescape(m.group(1)))
        if is_apache2(body):
            return "\n[Apache License 2.0: full text in Appendix A]\n"
        if body in seen:
            return f"\n[Same text as license text #{seen[body]} above]\n"
        seen[body] = len(seen) + 1
        marked = "\n".join("    " + l if l else "" for l in body.split("\n"))
        return f"\n[License text #{seen[body]}]\n" + marked.replace("<", "&lt;") + "\n"

    s = re.sub(r"<pre[^>]*>(.*?)</pre>", pre, s, flags=re.S)
    s = re.sub(r"<(script|style)[^>]*>.*?</\1>", "", s, flags=re.S)
    s = re.sub(r"<br\s*/?>|</(p|li|h\d|tr|div)>", "\n", s)
    s = html.unescape(re.sub(r"<[^>]+>", "", s))
    lines = [l.rstrip() if l.startswith("    ") else re.sub(r"\s+", " ", l).strip() for l in s.split("\n")]
    out = []
    for l in lines:
        if l or (out and out[-1]):
            out.append(l)
    return "\n".join(out).strip()


def generate() -> str:
    version = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M).group(1)
    crates = runtime_crates()
    dirs = crate_dirs()
    rust_doc, zig = toolchain_paths()

    # Crates whose package contains no license file reuse the texts of
    # their parent project (same authors and declared license).
    borrow = {"winapi-i686-pc-windows-gnu": "winapi", "winapi-x86_64-pc-windows-gnu": "winapi"}

    groups = {}  # license texts -> list of crate descriptions
    for (name, ver), lic in sorted(crates.items()):
        d, repo = dirs[(name, ver)]
        texts = license_texts(d)
        note = ""
        if not texts and name in borrow:
            parent = next(k for k in dirs if k[0] == borrow[name])
            texts = license_texts(dirs[parent][0])
            note = f" (ships no license file; text from the {borrow[name]} crate by the same authors)"
        if not texts:
            raise SystemExit(f"no license text found for {name} {ver}")
        key = tuple(texts)
        groups.setdefault(key, []).append(f"{name} {ver}  [{lic}]{(' ' + repo) if repo else ''}{note}")

    parts = [
        RULE,
        f"THIRD-PARTY SOFTWARE NOTICES for text_killer {version}",
        RULE,
        "",
        "text_killer itself is Copyright (c) 2026 kaiju_killer and is licensed under",
        "the MIT License (see LICENSE).",
        "",
        "The text_killer program includes the third-party components listed below.",
        "Their copyright and license notices are reproduced here as their licenses",
        "require. Where a component is offered under a choice of licenses (for",
        "example \"MIT OR Apache-2.0\"), text_killer uses it under the MIT License;",
        "the other license texts the component ships are included for completeness.",
        "",
        "Contents",
        "  1. Rust libraries (crates)",
        "  2. Runtime code linked into the program by the toolchain",
        "  3. Credits",
        "  Appendix A. Apache License, Version 2.0",
        "",
        "This file is generated by packaging/third_party_licenses.py from Cargo.lock.",
        "",
        "",
        RULE,
        "1. RUST LIBRARIES (CRATES)",
        RULE,
    ]
    for texts, members in sorted(groups.items(), key=lambda kv: kv[1][0].lower()):
        parts += ["", THIN]
        parts += members
        parts.append(THIN)
        for fname, t in texts:
            parts += ["", f"--- {fname} ---", "", t]

    parts += ["", "", RULE, "2. RUNTIME CODE LINKED INTO THE PROGRAM BY THE TOOLCHAIN", RULE, ""]
    parts += [THIN, "Rust standard library (all builds)  [Apache-2.0 OR MIT]", THIN, ""]
    std_copyright = rust_doc / "COPYRIGHT-library.html"
    if std_copyright.exists():
        parts.append(html_to_text(std_copyright.read_text()))
    else:
        parts.append("Copyright: The Rust Project Developers (see https://thanks.rust-lang.org)")
    # Texts of the licenses named above (Apache-2.0 is in Appendix A).
    for fname, title in [("MIT.txt", "MIT License"), ("Unicode-3.0.txt", "Unicode License v3 (Unicode-3.0)"),
                         ("BSD-2-Clause.txt", "BSD 2-Clause License")]:
        f = rust_doc / "licenses" / fname
        if f.exists():
            parts += ["", f"--- {title} ---", "", norm(f.read_text())]

    if zig:
        libc = zig / "lib/libc"
        parts += ["", THIN, "musl C library (Linux builds)  [MIT]", THIN, "", norm((libc / "musl/COPYRIGHT").read_text())]
        parts += ["", THIN, "mingw-w64 runtime (Windows build)  [ZPL-2.1, with public-domain and BSD parts]", THIN, "",
                  norm((libc / "mingw/COPYING").read_text())]
        parts += ["", THIN, "Zig compiler runtime (compiler_rt), used when cross-compiling  [MIT]", THIN, "",
                  norm((zig / "LICENSE").read_text())]
    else:
        raise SystemExit("ziglang is not installed; it provides the musl/mingw-w64/compiler_rt notices")
    parts += [
        "",
        THIN,
        "LLVM libunwind and compiler-rt (via the Rust toolchain)  [Apache-2.0 WITH LLVM-exception]",
        THIN,
        "",
        "Licensed under the Apache License v2.0 with LLVM Exceptions (Appendix A plus",
        "the exception below). The exception waives the attribution requirement for",
        "compiled binaries; the notice is included here for completeness.",
    ]
    exc = rust_doc / "licenses/LLVM-exception.txt"
    if exc.exists():
        parts += ["", norm(exc.read_text())]

    parts += [
        "",
        "",
        RULE,
        "3. CREDITS",
        RULE,
        "",
        "The \"Frost\" colour theme uses colours from the Nord palette",
        "(https://www.nordtheme.com), released under the MIT License.",
        "The \"Solar\" colour theme uses colours from Solarized by Ethan Schoonover",
        "(https://ethanschoonover.com/solarized), released under the MIT License.",
        "The \"Irssi\" and \"WeeChat\" themes are original colour schemes inspired",
        "by the look of those IRC clients; the names belong to their owners. The",
        "\"Classic\" theme is an original light scheme in the style of classic IRC",
        "clients.",
        "",
        "",
        RULE,
        "APPENDIX A. APACHE LICENSE, VERSION 2.0",
        RULE,
        "",
    ]
    apache = rust_doc / "licenses/Apache-2.0.txt"
    parts.append(norm(apache.read_text()))
    return "\n".join(parts) + "\n"


def main():
    text = generate()
    if "--check" in sys.argv:
        if not OUT.exists() or OUT.read_text() != text:
            sys.exit("THIRD-PARTY-LICENSES.txt is out of date: run python3 packaging/third_party_licenses.py")
        print("THIRD-PARTY-LICENSES.txt is up to date")
        return
    OUT.write_text(text)
    print(f"wrote {OUT.relative_to(ROOT)} ({len(text):,} bytes)")


if __name__ == "__main__":
    main()
