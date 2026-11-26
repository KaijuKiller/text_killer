//! Rich Text Format (.rtf): read (text, paragraphs, bold, italic, Unicode)
//! and write (headings, bullets, bold, italic).

use super::markup::{parse_blocks, BlockKind, TextBuilder};

/// Windows-1252 code points for bytes 0x80..=0x9F.
const CP1252: [u32; 32] = [
    0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D,
    0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D,
    0x017E, 0x0178,
];

fn cp1252(b: u8) -> char {
    let cp = if (0x80..0xA0).contains(&b) { CP1252[(b - 0x80) as usize] } else { u32::from(b) };
    char::from_u32(cp).unwrap_or('?')
}

/// Destinations whose content is not document text.
const SKIP_DEST: &[&str] = &[
    "fonttbl", "colortbl", "stylesheet", "info", "pict", "object", "header", "footer", "headerl", "headerr", "headerf",
    "footerl", "footerr", "footerf", "footnote", "fldinst", "themedata", "colorschememapping", "latentstyles",
    "datastore", "xmlnstbl", "listtable", "listoverridetable", "rsidtbl", "generator", "pgdsctbl", "annotation",
    "atnid", "atnauthor", "bkmkstart", "bkmkend", "pntxtb", "pntxta", "falt", "panose",
    "filetbl", "revtbl", "userprops", "nonshppict", "shp", "mmathPr", "wgrffmtfilter", "xmlopen", "docvar",
];

#[derive(Clone, Copy)]
struct State {
    bold: bool,
    italic: bool,
    skip: bool,
    uc: usize,
}

/// Paragraph-level parser state that is not scoped to `{ }` groups.
struct Out {
    tb: TextBuilder,
    /// A table cell ended; insert " | " before the next text in the row.
    pending_sep: bool,
    in_table: bool,
    /// `\outlinelevelN` of the current paragraph (0 = Heading 1).
    outline: Option<u8>,
}

impl Out {
    fn emit(&mut self, st: &State, s: &str) {
        if st.skip || s.is_empty() {
            return;
        }
        if self.pending_sep && self.in_table {
            self.pending_sep = false;
            self.tb.push(" | ", false, false);
        }
        self.tb.push(s, st.bold, st.italic);
        if let Some(l) = self.outline {
            self.tb.set_kind(BlockKind::Heading(l + 1));
        }
    }

    fn par(&mut self) {
        if !self.tb.is_open() {
            self.tb.start(BlockKind::Para);
        }
        self.tb.end();
        self.outline = None;
    }
}

/// Bullet characters used for list levels 1-3 (RTF has no list markup that
/// survives conversion, so writers put the bullet in the text).
const BULLETS: [char; 3] = ['•', '◦', '▪'];

pub fn import_rtf(data: &[u8]) -> Result<String, String> {
    if !data.starts_with(b"{\\rtf") {
        return Err("not an RTF document".into());
    }
    let mut o = Out { tb: TextBuilder::default(), pending_sep: false, in_table: false, outline: None };
    let mut stack: Vec<State> = Vec::new();
    let mut st = State { bold: false, italic: false, skip: false, uc: 1 };
    let mut skip_chars = 0usize;
    let mut i = 0;
    // `ignorable` is set by `\*` and applies to the next control word.
    let mut ignorable = false;
    let mut group_start = false;
    while i < data.len() {
        let c = data[i];
        match c {
            b'{' => {
                stack.push(st);
                group_start = true;
                i += 1;
                continue;
            }
            b'}' => {
                st = stack.pop().unwrap_or(st);
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&n) = data.get(i) else { break };
                if n.is_ascii_alphabetic() {
                    let ws = i;
                    while i < data.len() && data[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = std::str::from_utf8(&data[ws..i]).unwrap_or("");
                    let ps = i;
                    if i < data.len() && (data[i] == b'-' || data[i].is_ascii_digit()) {
                        i += 1;
                        while i < data.len() && data[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let param: Option<i32> = std::str::from_utf8(&data[ps..i]).ok().and_then(|p| p.parse().ok());
                    if i < data.len() && data[i] == b' ' {
                        i += 1;
                    }
                    // List numbering text ("•\tab", "◦\tab") is kept so the
                    // bullet character reveals the level; for headings it is
                    // outline numbering and is dropped.
                    let list_text = group_start && matches!(word, "listtext" | "pntext");
                    if list_text && o.outline.is_some() {
                        st.skip = true;
                    }
                    if (group_start && SKIP_DEST.contains(&word) && !list_text) || ignorable {
                        st.skip = true;
                    }
                    ignorable = false;
                    group_start = false;
                    if skip_chars > 0 && !matches!(word, "par" | "line" | "tab" | "cell" | "row") {
                        skip_chars -= 1;
                        continue;
                    }
                    match word {
                        "page" if !st.skip => {
                            if o.tb.is_open() {
                                o.par();
                            }
                            o.tb.page_break();
                        }
                        "par" | "sect" if !st.skip => {
                            if o.in_table {
                                // Paragraphs inside a cell are joined with spaces.
                                o.emit(&st, " ");
                            } else {
                                o.par();
                            }
                        }
                        "pard" if !st.skip => o.outline = None,
                        "outlinelevel" if !st.skip => {
                            let l = param.unwrap_or(0);
                            o.outline = (0..9).contains(&l).then_some(l as u8);
                        }
                        "intbl" | "trowd" if !st.skip => o.in_table = true,
                        "cell" if !st.skip => o.pending_sep = true,
                        "row" if !st.skip => {
                            o.pending_sep = false;
                            o.in_table = false;
                            o.par();
                        }
                        "line" => o.emit(&st, "\n"),
                        "tab" => o.emit(&st, "\t"),
                        "b" => st.bold = param != Some(0),
                        "i" => st.italic = param != Some(0),
                        "plain" => {
                            st.bold = false;
                            st.italic = false;
                        }
                        "uc" => st.uc = param.unwrap_or(1).max(0) as usize,
                        "u" => {
                            let v = param.unwrap_or(0);
                            let cp = if v < 0 { v + 65536 } else { v } as u32;
                            if let Some(ch) = char::from_u32(cp) {
                                o.emit(&st, &ch.to_string());
                            }
                            skip_chars = st.uc;
                        }
                        "bin" => i += param.unwrap_or(0).max(0) as usize,
                        "emdash" => o.emit(&st, "—"),
                        "endash" => o.emit(&st, "–"),
                        "bullet" => o.emit(&st, "•"),
                        "lquote" => o.emit(&st, "‘"),
                        "rquote" => o.emit(&st, "’"),
                        "ldblquote" => o.emit(&st, "“"),
                        "rdblquote" => o.emit(&st, "”"),
                        "emspace" | "enspace" | "qmspace" => o.emit(&st, " "),
                        _ => {}
                    }
                    continue;
                }
                // Control symbols.
                i += 1;
                group_start = false;
                match n {
                    b'*' => ignorable = true,
                    b'\'' => {
                        let hex = data.get(i..i + 2).and_then(|h| std::str::from_utf8(h).ok());
                        if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                            i += 2;
                            if skip_chars > 0 {
                                skip_chars -= 1;
                            } else {
                                o.emit(&st, &cp1252(b).to_string());
                            }
                        }
                    }
                    b'~' => o.emit(&st, "\u{a0}"),
                    b'_' => o.emit(&st, "-"),
                    b'-' => {}
                    b'\n' | b'\r' if !st.skip => o.par(),
                    b'\n' | b'\r' => {}
                    other => {
                        if skip_chars > 0 {
                            skip_chars -= 1;
                        } else {
                            o.emit(&st, &(other as char).to_string());
                        }
                    }
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                group_start = false;
                // Plain text run up to the next special byte.
                let s = i;
                while i < data.len() && !matches!(data[i], b'\\' | b'{' | b'}' | b'\r' | b'\n') {
                    i += 1;
                }
                let mut text: String = data[s..i].iter().map(|&b| cp1252(b)).collect();
                if skip_chars > 0 {
                    let n = skip_chars.min(text.chars().count());
                    text = text.chars().skip(n).collect();
                    skip_chars -= n;
                }
                o.emit(&st, &text);
            }
        }
    }
    // Turn leading bullet characters back into "- " list markup.
    let text = o.tb.finish();
    let lines: Vec<String> = text
        .split('\n')
        .map(|l| {
            let t = l.trim_start();
            let Some(first) = t.chars().next() else { return l.to_owned() };
            let level = match first {
                '•' | '·' | '-' => 1,
                '◦' | '–' | 'o' => 2,
                '▪' | '§' => 3,
                _ => return l.to_owned(),
            };
            let rest = &t[first.len_utf8()..];
            if !rest.starts_with([' ', '\t']) || (first == '-' && !rest.starts_with('\t')) || (first == 'o' && !rest.starts_with('\t')) {
                return l.to_owned();
            }
            format!("{}- {}", "  ".repeat(level - 1), rest.trim_start())
        })
        .collect();
    // Cell text often ends with a space before the separator.
    Ok(lines.join("\n").replace("  | ", " | "))
}

fn escape_rtf(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\t' => out.push_str("\\tab "),
            '\n' => out.push_str("\\line "),
            c if c.is_ascii() => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{}?", *unit as i16));
                }
            }
        }
    }
    out
}

pub fn export_rtf(text: &str) -> Vec<u8> {
    let mut out = String::from(
        "{\\rtf1\\ansi\\ansicpg1252\\deff0\\uc1{\\fonttbl{\\f0\\fswiss\\fcharset0 Arial;}}\n\
         {\\*\\generator text_killer;}\\viewkind4\\f0\\fs24\n",
    );
    for b in parse_blocks(text) {
        if b.kind == BlockKind::PageBreak {
            out.push_str("\\page\n");
            continue;
        }
        let (open, prefix) = match b.kind {
            BlockKind::Heading(n) => {
                let size = [36, 32, 28, 26, 24, 24][(n as usize - 1).min(5)];
                (format!("\\pard\\outlinelevel{}\\sb240\\sa120\\keepn\\b\\fs{size} ", n - 1), String::new())
            }
            BlockKind::Bullet(d) => {
                let li = 360 * u32::from(d);
                let ch = BULLETS[(d as usize - 1) % 3] as u32;
                (format!("\\pard\\sa60\\li{li}\\fi-360 "), format!("\\u{ch}?\\tab "))
            }
            BlockKind::Para | BlockKind::PageBreak => ("\\pard\\sa120 ".to_owned(), String::new()),
        };
        out.push('{');
        out.push_str(&open);
        out.push_str(&prefix);
        for r in &b.runs {
            let fmt = match (r.bold, r.italic) {
                (true, true) => "\\b\\i ",
                (true, false) => "\\b ",
                (false, true) => "\\i ",
                _ => "",
            };
            if fmt.is_empty() {
                out.push_str(&escape_rtf(&r.text));
            } else {
                out.push('{');
                out.push_str(fmt);
                out.push_str(&escape_rtf(&r.text));
                out.push('}');
            }
        }
        out.push_str("\\par}\n");
    }
    out.push('}');
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtf_roundtrip() {
        let src = "# Title\nSome **bold**, *italic* and ünïcödé 日本 text.\n- item\n  - sub\n\tTabbed\n";
        let back = import_rtf(&export_rtf(src)).unwrap();
        assert_eq!(back, "# Title\nSome **bold**, *italic* and ünïcödé 日本 text.\n- item\n  - sub\n\tTabbed\n");
    }

    #[test]
    fn rtf_page_breaks() {
        let src = "Before\n\u{c}\nAfter\n";
        assert_eq!(import_rtf(&export_rtf(src)).unwrap(), src);
    }

    #[test]
    fn rtf_from_wordpad_style() {
        let rtf = br"{\rtf1\ansi\deff0{\fonttbl{\f0 Arial;}}{\colortbl ;\red255\green0\blue0;}{\*\generator Riched20;}
\pard\f0\fs22 Hello \b world\b0 , caf\'e9 \u8364? done.\par
Second\tab line\par
}";
        assert_eq!(import_rtf(rtf).unwrap(), "Hello **world**, café € done.\nSecond\tline\n");
    }
}
