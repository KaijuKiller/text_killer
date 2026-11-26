//! Document (non-plain-text) file formats.
//!
//! Rich documents are converted to editable Markdown-style text on open
//! (see [`markup`]) and converted back when saved in a writable format.
//!
//! | Format                     | Extensions                     | Open | Save |
//! |----------------------------|--------------------------------|------|------|
//! | OpenDocument Text          | .odt .ott .fodt                | yes  | .odt |
//! | OpenDocument Spreadsheet   | .ods .ots .fods                | yes  | no   |
//! | OpenDocument Presentation  | .odp .otp .fodp                | yes  | no   |
//! | Word (Office Open XML)     | .docx .docm .dotx              | yes  | yes  |
//! | Excel (Office Open XML)    | .xlsx .xlsm .xltx              | yes  | no   |
//! | PowerPoint (Office Open XML)| .pptx .ppsx .potx             | yes  | no   |
//! | EPUB e-book                | .epub                          | yes  | no   |
//! | Rich Text Format           | .rtf                           | yes  | yes  |
//! | gzip-compressed text       | .gz                            | yes  | yes  |

mod epub;
mod inflate;
pub mod markup;
mod odf;
mod ooxml;
mod rtf;
mod xml;
mod zip;

use std::path::Path;

use crate::syntax::{self, Language};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Odt,
    Ods,
    Odp,
    FlatOdf,
    Docx,
    Xlsx,
    Pptx,
    Epub,
    Rtf,
    Gzip,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Odt => "ODT document",
            Self::Ods => "ODS spreadsheet",
            Self::Odp => "ODP slides",
            Self::FlatOdf => "flat ODF",
            Self::Docx => "DOCX document",
            Self::Xlsx => "XLSX spreadsheet",
            Self::Pptx => "PPTX slides",
            Self::Epub => "EPUB e-book",
            Self::Rtf => "RTF document",
            Self::Gzip => "gzip",
        }
    }

    /// Whether text_killer can write this format.
    pub fn writable(self) -> bool {
        matches!(self, Self::Odt | Self::Docx | Self::Rtf | Self::Gzip)
    }

    /// Whether saving loses information compared with the original file.
    fn lossy(self) -> bool {
        !matches!(self, Self::Gzip)
    }
}

/// The result of importing a document.
pub struct Imported {
    pub text: String,
    pub kind: Kind,
    pub language: &'static Language,
    /// Original encoding for gzip-wrapped text; UTF-8 otherwise.
    pub encoding: Option<crate::fileio::Encoding>,
    pub lossy: bool,
}

fn ext_of(name: &str) -> String {
    Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

pub fn kind_from_name(name: &str) -> Option<Kind> {
    Some(match ext_of(name).as_str() {
        "odt" | "ott" => Kind::Odt,
        "ods" | "ots" => Kind::Ods,
        "odp" | "otp" => Kind::Odp,
        "fodt" | "fods" | "fodp" | "fodg" => Kind::FlatOdf,
        "docx" | "docm" | "dotx" | "dotm" => Kind::Docx,
        "xlsx" | "xlsm" | "xltx" | "xltm" => Kind::Xlsx,
        "pptx" | "pptm" | "ppsx" | "potx" => Kind::Pptx,
        "epub" => Kind::Epub,
        "rtf" => Kind::Rtf,
        "gz" | "gzip" => Kind::Gzip,
        _ => return None,
    })
}

/// Identifies a document from its contents when the extension is unknown.
fn sniff(data: &[u8]) -> Option<Kind> {
    if data.starts_with(b"{\\rtf") {
        return Some(Kind::Rtf);
    }
    if data.starts_with(&[0x1f, 0x8b]) {
        return Some(Kind::Gzip);
    }
    if !zip::is_zip(data) {
        return None;
    }
    let z = zip::ZipArchive::parse(data).ok()?;
    if let Ok(m) = z.read_text("mimetype") {
        let m = m.trim();
        return match m {
            _ if m.starts_with("application/vnd.oasis.opendocument.text") => Some(Kind::Odt),
            _ if m.starts_with("application/vnd.oasis.opendocument.spreadsheet") => Some(Kind::Ods),
            _ if m.starts_with("application/vnd.oasis.opendocument.presentation") => Some(Kind::Odp),
            "application/epub+zip" => Some(Kind::Epub),
            _ => None,
        };
    }
    if z.find("word/document.xml").is_some() {
        Some(Kind::Docx)
    } else if z.find("xl/workbook.xml").is_some() {
        Some(Kind::Xlsx)
    } else if z.find("ppt/presentation.xml").is_some() {
        Some(Kind::Pptx)
    } else {
        None
    }
}

/// Imports `data` if it is a supported document format. Returns `None` for
/// ordinary text/binary files.
pub fn import(name: &str, data: &[u8]) -> Option<Result<Imported, String>> {
    let kind = kind_from_name(name).or_else(|| sniff(data))?;
    let markdown = syntax::by_name("markdown").expect("markdown language");
    let doc = |text: String| Imported { text, kind, language: markdown, encoding: None, lossy: kind.lossy() };
    Some(match kind {
        Kind::Odt | Kind::Ods | Kind::Odp => odf::import_package(data).map(doc),
        Kind::FlatOdf => Ok(doc(odf::to_text(&String::from_utf8_lossy(data), None))),
        Kind::Docx => ooxml::import_docx(data).map(doc),
        Kind::Xlsx => ooxml::import_xlsx(data).map(doc),
        Kind::Pptx => ooxml::import_pptx(data).map(doc),
        Kind::Epub => epub::import_epub(data).map(doc),
        Kind::Rtf => rtf::import_rtf(data).map(doc),
        Kind::Gzip => import_gzip(name, data),
    })
}

fn import_gzip(name: &str, data: &[u8]) -> Result<Imported, String> {
    let raw = inflate::gunzip(data, crate::fileio::MAX_FILE_SIZE as usize)?;
    let inner = name.strip_suffix(".gz").or_else(|| name.strip_suffix(".gzip")).unwrap_or(name);
    // A compressed document (e.g. report.rtf.gz) is imported as that document.
    if let Some(Ok(mut imp)) = import(inner, &raw).filter(|_| kind_from_name(inner).is_some_and(|k| k != Kind::Gzip)) {
        imp.lossy = true;
        imp.kind = Kind::Gzip;
        return Ok(imp);
    }
    let (text, enc) = crate::fileio::decode(&raw);
    if enc == crate::fileio::Encoding::Binary {
        return Err("compressed file is binary, not text".into());
    }
    let language = syntax::detect(inner, text.lines().next().unwrap_or(""));
    Ok(Imported {
        text,
        kind: Kind::Gzip,
        language,
        encoding: Some(enc),
        lossy: false,
    })
}

/// Encodes `text` for `path` when its extension names a document format.
/// Returns `None` for plain-text destinations.
pub fn export(path: &Path, text: &str, enc_text: impl FnOnce() -> Result<Vec<u8>, String>) -> Option<Result<Vec<u8>, String>> {
    let name = path.to_string_lossy();
    let kind = kind_from_name(&name)?;
    Some(match kind {
        Kind::Odt => Ok(odf::export_odt(text)),
        Kind::Docx => Ok(ooxml::export_docx(text)),
        Kind::Rtf => Ok(rtf::export_rtf(text)),
        Kind::Gzip => enc_text().map(|raw| gzip_stored(&raw)),
        other => Err(format!(
            "{} files can be opened but not written; use Save As with .odt, .docx, .rtf, .md or .txt",
            other.label()
        )),
    })
}

/// Whether a file with this name can be written by text_killer.
pub fn writable(path: &Path) -> bool {
    kind_from_name(&path.to_string_lossy()).is_none_or(Kind::writable)
}

/// The label shown in the status bar for a document saved at `path`.
pub fn label_for(path: &Path) -> Option<&'static str> {
    kind_from_name(&path.to_string_lossy()).map(Kind::label)
}

/// Suggests a writable file name for a read-only document format.
pub fn suggest_name(name: &str) -> String {
    let stem = Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    match kind_from_name(name) {
        Some(Kind::Ods | Kind::Xlsx) => format!("{stem}.md"),
        _ => format!("{stem}.odt"),
    }
}

/// Wraps data in a gzip container using uncompressed DEFLATE blocks
/// (valid gzip that any tool can read).
fn gzip_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
    let chunks: Vec<&[u8]> = if data.is_empty() { vec![&[][..]] } else { data.chunks(65_535).collect() };
    let n = chunks.len();
    for (i, c) in chunks.into_iter().enumerate() {
        out.push(u8::from(i + 1 == n));
        let len = c.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(c);
    }
    out.extend_from_slice(&inflate::crc32(data).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GZ: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0xc2, 0x86, 0xfa, 0x6a, 0x02, 0xff, 0x0b, 0xc9, 0x48, 0x55, 0x28, 0x2c, 0xcd, 0x4c, 0xce,
        0x56, 0x48, 0x2a, 0xca, 0x2f, 0xcf, 0x53, 0x48, 0xcb, 0xaf, 0x50, 0xc8, 0x2a, 0xcd, 0x2d, 0x28, 0x56, 0xc8, 0x2f,
        0x4b, 0x2d, 0x52, 0x28, 0x01, 0x4a, 0xe7, 0x24, 0x56, 0x55, 0x2a, 0xa4, 0xe4, 0xa7, 0xeb, 0x29, 0x84, 0x8c, 0x2a,
        0x1e, 0x55, 0x3c, 0xaa, 0x98, 0xda, 0x8a, 0x4b, 0xf3, 0x32, 0x0b, 0x4b, 0x53, 0x15, 0x4a, 0x12, 0x33, 0x73, 0x14,
        0x0c, 0x8d, 0x8c, 0x4d, 0x4c, 0x01, 0xc5, 0x4b, 0x2e, 0x53, 0x95, 0x03, 0x00, 0x00,
    ];

    #[test]
    fn gzip_import_and_stored_export() {
        let imp = import("notes.txt.gz", GZ).unwrap().unwrap();
        let expected = format!("{}unique tail 12345", "The quick brown fox jumps over the lazy dog. ".repeat(20));
        assert_eq!(imp.text, expected);
        let packed = gzip_stored(expected.as_bytes());
        assert_eq!(inflate::gunzip(&packed, 1 << 20).unwrap(), expected.as_bytes());
    }

    #[test]
    fn detection_and_sniffing() {
        assert_eq!(kind_from_name("A.DOCX"), Some(Kind::Docx));
        assert_eq!(kind_from_name("a.txt"), None);
        let odt = odf::export_odt("hi\n");
        assert_eq!(sniff(&odt), Some(Kind::Odt));
        assert_eq!(sniff(&ooxml::export_docx("hi\n")), Some(Kind::Docx));
        assert_eq!(sniff(b"{\\rtf1 x}"), Some(Kind::Rtf));
        assert!(import("mystery", &odt).unwrap().is_ok());
        assert!(writable(Path::new("x.odt")) && !writable(Path::new("x.xlsx")) && writable(Path::new("x.py")));
        assert_eq!(suggest_name("budget.xlsx"), "budget.md");
        assert_eq!(suggest_name("deck.pptx"), "deck.odt");
    }
}
