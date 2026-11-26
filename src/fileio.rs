//! Loading and saving files with encoding / line-ending detection.
//!
//! Supported on load: UTF-8 (with or without BOM), UTF-16 LE/BE (with BOM),
//! Latin-1 fallback for legacy 8-bit text, and a read-only hex view for
//! binary files. Files are written back in the encoding they were read with.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::document::Document;

/// Files above this size are refused to protect memory.
pub const MAX_FILE_SIZE: u64 = 256 * 1024 * 1024;
/// Binary files above this size are truncated in the hex view.
const MAX_HEX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Latin1,
    Binary,
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
            Self::Latin1 => "Latin-1",
            Self::Binary => "binary",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
        }
    }
}

impl fmt::Display for LineEnding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Lf => "LF",
            Self::CrLf => "CRLF",
            Self::Cr => "CR",
        })
    }
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Makes a path absolute without requiring it to exist.
pub fn absolute(path: &Path) -> PathBuf {
    if let Ok(p) = fs::canonicalize(path) {
        return p;
    }
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map(|d| d.join(path)).unwrap_or_else(|_| path.to_path_buf())
    }
}

/// Expands a leading `~` to the home directory.
pub fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix('~') {
        if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
            if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
                return PathBuf::from(home).join(rest.trim_start_matches(['/', '\\']));
            }
        }
    }
    PathBuf::from(s)
}

fn looks_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.contains(&0) {
        return true;
    }
    // Many control characters (other than common whitespace) suggests binary.
    let ctrl = sample
        .iter()
        .filter(|&&b| b < 0x20 && !matches!(b, b'\n' | b'\r' | b'\t' | 0x0c | 0x1b | 0x08))
        .count();
    !sample.is_empty() && ctrl * 10 > sample.len()
}

fn decode_utf16(bytes: &[u8], le: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|c| {
            let pair = [c[0], *c.get(1).unwrap_or(&0)];
            if le { u16::from_le_bytes(pair) } else { u16::from_be_bytes(pair) }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Decodes raw bytes into text plus the detected encoding.
pub fn decode(bytes: &[u8]) -> (String, Encoding) {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return (String::from_utf8_lossy(rest).into_owned(), Encoding::Utf8Bom);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return (decode_utf16(rest, true), Encoding::Utf16Le);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return (decode_utf16(rest, false), Encoding::Utf16Be);
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        if !looks_binary(bytes) {
            return (s.to_owned(), Encoding::Utf8);
        }
    }
    if looks_binary(bytes) {
        return (hex_dump(bytes), Encoding::Binary);
    }
    (bytes.iter().map(|&b| b as char).collect(), Encoding::Latin1)
}

/// Normalises line endings to `\n` and reports the dominant original style.
pub fn normalize_newlines(text: &str) -> (String, LineEnding) {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count() - crlf;
    let cr = text.matches('\r').count() - crlf;
    let ending = if crlf > lf && crlf >= cr {
        LineEnding::CrLf
    } else if cr > lf && cr > crlf {
        LineEnding::Cr
    } else {
        LineEnding::Lf
    };
    if crlf == 0 && cr == 0 {
        return (text.to_owned(), ending);
    }
    (text.replace("\r\n", "\n").replace('\r', "\n"), ending)
}

pub fn hex_dump(bytes: &[u8]) -> String {
    let shown = &bytes[..bytes.len().min(MAX_HEX_BYTES)];
    let mut out = String::with_capacity(shown.len() * 4 + 64);
    for (i, chunk) in shown.chunks(16).enumerate() {
        out.push_str(&format!("{:08x}  ", i * 16));
        for j in 0..16 {
            match chunk.get(j) {
                Some(b) => out.push_str(&format!("{b:02x} ")),
                None => out.push_str("   "),
            }
            if j == 7 {
                out.push(' ');
            }
        }
        out.push_str(" |");
        for &b in chunk {
            out.push(if (0x20..0x7f).contains(&b) { b as char } else { '.' });
        }
        out.push_str("|\n");
    }
    if bytes.len() > shown.len() {
        out.push_str(&format!("... truncated: showing {} of {} bytes\n", shown.len(), bytes.len()));
    }
    out.push_str(&format!("{:08x}", bytes.len()));
    out
}

/// Loads a file into a new document.
pub fn load(path: &Path) -> io::Result<Document> {
    let meta = fs::metadata(path)?;
    if meta.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "is a directory"));
    }
    if meta.len() > MAX_FILE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("file too large ({} MiB)", meta.len() / (1024 * 1024)),
        ));
    }
    let bytes = fs::read(path)?;
    let abs = absolute(path);
    let name = file_name(&abs);
    // Rich documents (.odt, .docx, .rtf, .epub, ...) are converted to text.
    if let Some(result) = crate::docformats::import(&name, &bytes) {
        let imp = result.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let (text, ending) = normalize_newlines(&imp.text);
        let mut doc = Document::from_text(name, &text, Some(abs));
        doc.set_language(imp.language);
        doc.encoding = imp.encoding.unwrap_or(Encoding::Utf8);
        doc.line_ending = ending;
        doc.format = Some(imp.kind.label());
        doc.lossy_import = imp.lossy;
        if meta.permissions().readonly() {
            doc.readonly = true;
        }
        return Ok(doc);
    }
    let (text, encoding) = decode(&bytes);
    let (text, ending) = normalize_newlines(&text);
    let mut doc = Document::from_text(name, &text, Some(abs));
    doc.encoding = encoding;
    doc.line_ending = ending;
    if encoding == Encoding::Binary {
        doc.readonly = true;
        doc.set_language(crate::syntax::by_name("hex").expect("hex language"));
    }
    if meta.permissions().readonly() {
        doc.readonly = true;
    }
    Ok(doc)
}

/// Encodes the document text in its original encoding.
pub fn encode(doc: &Document) -> Result<Vec<u8>, String> {
    let text = doc.lines.join(doc.line_ending.as_str());
    Ok(match doc.encoding {
        Encoding::Utf8 => text.into_bytes(),
        Encoding::Utf8Bom => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            v
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let le = doc.encoding == Encoding::Utf16Le;
            let mut v = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
            for u in text.encode_utf16() {
                v.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
            }
            v
        }
        Encoding::Latin1 => {
            if let Some(c) = text.chars().find(|&c| c as u32 > 0xFF) {
                return Err(format!("character '{c}' cannot be stored as Latin-1; use /encoding utf-8"));
            }
            text.chars().map(|c| c as u8).collect()
        }
        Encoding::Binary => return Err("binary files are read-only".into()),
    })
}

/// Writes atomically: write to a temp file in the same directory, then rename.
pub fn save(doc: &Document, path: &Path) -> Result<usize, String> {
    // Document formats are chosen by the destination's extension.
    let bytes = match crate::docformats::export(path, &doc.lines.join("\n"), || encode(doc)) {
        Some(result) => result?,
        None => encode(doc)?,
    };
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let tmp = dir.join(format!(".{}.text_killer-tmp", file_name(path)));
    let write = || -> io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        if let Ok(meta) = fs::metadata(path) {
            // Preserve permissions of the file being replaced.
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, path)
    };
    if let Err(e) = write() {
        let _ = fs::remove_file(&tmp);
        // Fall back to a direct write (e.g. on filesystems disallowing rename).
        fs::write(path, &bytes).map_err(|e2| format!("{e2} (atomic save failed: {e})"))?;
    }
    Ok(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_encodings() {
        assert_eq!(decode(b"abc").1, Encoding::Utf8);
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'a']).0, "a");
        let (s, e) = decode(&[0xFF, 0xFE, b'h', 0, b'i', 0]);
        assert_eq!((s.as_str(), e), ("hi", Encoding::Utf16Le));
        let (s, e) = decode(&[b'c', b'a', b'f', 0xE9]);
        assert_eq!((s.as_str(), e), ("café", Encoding::Latin1));
        assert_eq!(decode(&[0, 1, 2, 3]).1, Encoding::Binary);
    }

    #[test]
    fn line_endings() {
        let (t, e) = normalize_newlines("a\r\nb\r\n");
        assert_eq!((t.as_str(), e), ("a\nb\n", LineEnding::CrLf));
        let (t, e) = normalize_newlines("a\nb");
        assert_eq!((t.as_str(), e), ("a\nb", LineEnding::Lf));
    }

    #[test]
    fn roundtrip_save_load() {
        let dir = std::env::temp_dir().join(format!("text_killer-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.txt");
        fs::write(&p, b"one\r\ntwo\r\n").unwrap();
        let doc = load(&p).unwrap();
        assert_eq!(doc.lines, vec!["one", "two", ""]);
        save(&doc, &p).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"one\r\ntwo\r\n");
        fs::remove_dir_all(&dir).unwrap();
    }
}
