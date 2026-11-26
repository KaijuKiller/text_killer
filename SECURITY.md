# Security policy

## Reporting a vulnerability

Please **do not** report security problems in public issues.

Instead, use GitHub's private reporting: open the repository's
**Security** tab and choose **Report a vulnerability**
(<https://github.com/KaijuKiller/text_killer/security/advisories/new>).
Only the maintainer can see the report.

Useful details to include:

- the text_killer version (`text_killer --version`) and operating system
- what an attacker could do (for example: a crafted .docx, .odt, .epub or
  other file that crashes the editor, hangs it, or affects files it
  shouldn't)
- a sample file or steps to reproduce, if you can share them safely

## Supported versions

Fixes are made in the latest release only.

## Scope

text_killer opens untrusted files, including ZIP-based documents (.odt,
.docx, .xlsx, .pptx, .epub), RTF and gzip, so problems in reading those
formats are in scope. Its installers are not code-signed; warnings from
Windows SmartScreen or macOS Gatekeeper about that are expected and are
not vulnerabilities.
