//! The bridge between rich documents and editable text.
//!
//! Imported documents are shown as lightweight Markdown:
//!   `# Heading` (levels 1-6), `- bullet` (indented two spaces per level),
//!   `**bold**`, `*italic*`, `***bold italic***`.
//! The same markup is parsed back into paragraphs and styled runs on export.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Para,
    Heading(u8),
    /// Bullet list item at depth 1..=6.
    Bullet(u8),
    /// A hard page break (a line holding only a form feed, `\f`).
    PageBreak,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub runs: Vec<Run>,
}

// ----------------------------------------------------------------------
// Import: collecting paragraphs into text
// ----------------------------------------------------------------------

/// Accumulates paragraphs of styled runs and renders them as text lines.
#[derive(Default)]
pub struct TextBuilder {
    pub lines: Vec<String>,
    runs: Vec<Run>,
    prefix: String,
    open: bool,
}

impl TextBuilder {
    fn prefix_for(kind: BlockKind) -> String {
        match kind {
            BlockKind::Para => String::new(),
            BlockKind::Heading(n) => format!("{} ", "#".repeat(n.clamp(1, 6) as usize)),
            BlockKind::Bullet(d) => format!("{}- ", "  ".repeat(d.saturating_sub(1) as usize)),
            BlockKind::PageBreak => String::new(),
        }
    }

    /// Emits a page break line (`\f`), ending any open paragraph.
    pub fn page_break(&mut self) {
        self.end();
        if self.lines.last().is_some_and(|l| l.is_empty()) {
            self.lines.pop();
        }
        self.lines.push("\u{c}".into());
    }

    pub fn start(&mut self, kind: BlockKind) {
        if self.open {
            self.end();
        }
        self.prefix = Self::prefix_for(kind);
        self.open = true;
    }

    /// Changes the kind of the paragraph currently being built.
    pub fn set_kind(&mut self, kind: BlockKind) {
        if !self.open {
            self.start(kind);
        } else {
            self.prefix = Self::prefix_for(kind);
        }
    }

    /// True when the open paragraph has no visible text.
    pub fn is_blank(&self) -> bool {
        self.runs.iter().all(|r| r.text.trim().is_empty())
    }

    /// Drops the open paragraph without emitting it.
    pub fn discard(&mut self) {
        self.runs.clear();
        self.prefix.clear();
        self.open = false;
    }

    pub fn push(&mut self, text: &str, bold: bool, italic: bool) {
        if text.is_empty() {
            return;
        }
        if !self.open {
            self.start(BlockKind::Para);
        }
        match self.runs.last_mut() {
            Some(r) if r.bold == bold && r.italic == italic => r.text.push_str(text),
            _ => self.runs.push(Run { text: text.to_owned(), bold, italic }),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Finishes the current paragraph (if any) and returns its rendered text
    /// without adding it to `lines` when `take` is used by table cells.
    pub fn take(&mut self) -> String {
        // Headings are rendered without bold: "#" already implies emphasis.
        if self.prefix.starts_with('#') {
            for r in &mut self.runs {
                r.bold = false;
            }
        }
        let body = render_runs(&self.runs);
        let body = body.trim_end_matches(' ');
        let s = if body.trim().is_empty() && self.prefix.is_empty() { String::new() } else { format!("{}{}", self.prefix, body) };
        self.runs.clear();
        self.prefix.clear();
        self.open = false;
        s
    }

    pub fn end(&mut self) {
        if self.open {
            let line = self.take();
            self.lines.push(line);
        }
    }

    pub fn blank(&mut self) {
        self.end();
        if self.lines.last().is_some_and(|l| !l.is_empty()) {
            self.lines.push(String::new());
        }
    }

    pub fn raw_line(&mut self, s: String) {
        self.end();
        self.lines.push(s);
    }

    pub fn finish(mut self) -> String {
        self.end();
        while self.lines.last().is_some_and(|l| l.trim().is_empty()) {
            self.lines.pop();
        }
        let mut s = self.lines.join("\n");
        s.push('\n');
        s
    }
}

/// Renders runs as Markdown-style text, keeping whitespace outside markers.
pub fn render_runs(runs: &[Run]) -> String {
    let mut out = String::new();
    for r in runs {
        let marker = match (r.bold, r.italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        };
        let core = r.text.trim();
        if marker.is_empty() || core.is_empty() {
            out.push_str(&r.text);
            continue;
        }
        let lead = &r.text[..r.text.len() - r.text.trim_start().len()];
        let trail = &r.text[r.text.trim_end().len()..];
        // Multi-line runs (line breaks inside) are wrapped per line.
        let wrapped: Vec<String> = core.split('\n').map(|l| {
            if l.trim().is_empty() { l.to_owned() } else { format!("{marker}{l}{marker}") }
        }).collect();
        out.push_str(lead);
        out.push_str(&wrapped.join("\n"));
        out.push_str(trail);
    }
    out
}

// ----------------------------------------------------------------------
// Export: parsing text into blocks
// ----------------------------------------------------------------------

pub fn parse_blocks(text: &str) -> Vec<Block> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    // A trailing newline does not create an extra empty paragraph.
    if lines.len() > 1 && lines.last() == Some(&"") {
        lines.pop();
    }
    lines.into_iter().map(parse_line).collect()
}

fn parse_line(line: &str) -> Block {
    if crate::wrap::is_page_break(line) {
        return Block { kind: BlockKind::PageBreak, runs: Vec::new() };
    }
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
        return Block { kind: BlockKind::Heading(hashes as u8), runs: parse_inline(line[hashes..].trim()) };
    }
    let indent = line.chars().take_while(|&c| c == ' ').count();
    let rest = &line[indent..];
    for marker in ["- ", "* ", "+ ", "• "] {
        if let Some(body) = rest.strip_prefix(marker) {
            let depth = (indent / 2 + 1).min(6) as u8;
            return Block { kind: BlockKind::Bullet(depth), runs: parse_inline(body) };
        }
    }
    Block { kind: BlockKind::Para, runs: parse_inline(line) }
}

/// Parses `**bold**`, `*italic*` and `***both***` into runs. A `*` is only a
/// marker when it opens before non-space text and a matching closer exists,
/// so arithmetic like `2 * 3` is left alone. `\*` is a literal asterisk.
pub fn parse_inline(s: &str) -> Vec<Run> {
    let chars: Vec<char> = s.chars().collect();
    let mut runs: Vec<Run> = Vec::new();
    let (mut bold, mut italic) = (false, false);
    let mut cur = String::new();
    let flush = |cur: &mut String, runs: &mut Vec<Run>, bold: bool, italic: bool| {
        if !cur.is_empty() {
            runs.push(Run { text: std::mem::take(cur), bold, italic });
        }
    };
    let star_run = |i: usize| chars[i..].iter().take_while(|&&c| c == '*').count();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && matches!(chars.get(i + 1), Some('*' | '\\')) {
            cur.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if c != '*' {
            cur.push(c);
            i += 1;
            continue;
        }
        let n = star_run(i).min(3);
        let prev_ws = i == 0 || chars[i - 1].is_whitespace();
        let next_ws = chars.get(i + n).is_none_or(|c| c.is_whitespace());
        let want_bold = n >= 2;
        let want_italic = n == 1 || n == 3;
        let closing = !prev_ws && (!want_bold || bold) && (!want_italic || italic);
        // An opener needs a later closer of the same length.
        let has_closer = || {
            let mut j = i + n;
            while j < chars.len() {
                if chars[j] == '*' {
                    let m = star_run(j);
                    if m >= n && !chars[j - 1].is_whitespace() {
                        return true;
                    }
                    j += m;
                } else {
                    j += 1;
                }
            }
            false
        };
        if closing {
            flush(&mut cur, &mut runs, bold, italic);
            if want_bold {
                bold = false;
            }
            if want_italic {
                italic = false;
            }
        } else if !next_ws && (!want_bold || !bold) && (!want_italic || !italic) && has_closer() {
            flush(&mut cur, &mut runs, bold, italic);
            bold |= want_bold;
            italic |= want_italic;
        } else {
            cur.extend(std::iter::repeat_n('*', n));
        }
        i += n;
    }
    flush(&mut cur, &mut runs, bold, italic);
    runs
}

/// Plain text of a block's runs.
#[cfg(test)]
pub fn plain(runs: &[Run]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(t: &str, b: bool, i: bool) -> Run {
        Run { text: t.into(), bold: b, italic: i }
    }

    #[test]
    fn inline_parsing() {
        assert_eq!(parse_inline("a **b** c"), vec![r("a ", false, false), r("b", true, false), r(" c", false, false)]);
        assert_eq!(parse_inline("*i* and ***bi***"), vec![r("i", false, true), r(" and ", false, false), r("bi", true, true)]);
        assert_eq!(parse_inline("2 * 3 * 4"), vec![r("2 * 3 * 4", false, false)]);
        assert_eq!(parse_inline("\\*lit\\*"), vec![r("*lit*", false, false)]);
        assert_eq!(parse_inline("**unclosed"), vec![r("**unclosed", false, false)]);
    }

    #[test]
    fn blocks() {
        let b = parse_blocks("# Title\n- one\n    - nested **b**\nplain\n");
        assert_eq!(b.len(), 4);
        assert_eq!(b[0].kind, BlockKind::Heading(1));
        assert_eq!(b[1].kind, BlockKind::Bullet(1));
        assert_eq!(b[2].kind, BlockKind::Bullet(3));
        assert_eq!(b[2].runs[1], r("b", true, false));
        assert_eq!(b[3].kind, BlockKind::Para);
    }

    #[test]
    fn render_roundtrip() {
        let runs = vec![r("Hello ", false, false), r(" world ", true, false), r("!", false, true)];
        let s = render_runs(&runs);
        assert_eq!(s, "Hello  **world** *!*");
        assert_eq!(plain(&parse_inline(&s)), "Hello  world !");
        let mut tb = TextBuilder::default();
        tb.start(BlockKind::Heading(2));
        tb.push("Sub", false, false);
        tb.start(BlockKind::Bullet(2));
        tb.push("x", false, false);
        assert_eq!(tb.finish(), "## Sub\n  - x\n");
    }
}
