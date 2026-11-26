//! Dependency-free syntax highlighting.
//!
//! Highlighting is line-oriented: each line is tokenized given the [`State`]
//! at its start, and produces the state for the next line. The document
//! caches per-line start states so only edited regions are re-scanned.

mod css;
mod generic;
mod html;
pub mod languages;
mod markdown;

pub use languages::{by_name, detect, Language, Mode, LANGUAGES};

/// Token classification for a single character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Normal,
    Keyword,
    Type,
    Builtin,
    Constant,
    String,
    Escape,
    Number,
    Comment,
    Function,
    Operator,
    Punct,
    Tag,
    Attribute,
    Property,
    Selector,
    Preproc,
    Heading,
    Emphasis,
    Strong,
    Code,
    Link,
    Added,
    Removed,
}

/// Lexer context carried across lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Ctx {
    #[default]
    Normal,
    BlockComment,
    /// Inside a multi-line string; index into `Language::multiline_strings`.
    MultiString(u8),
}

/// HTML sub-mode carried across lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HtmlMode {
    #[default]
    Text,
    Tag,
    Comment,
    Script,
    Style,
}

/// Complete highlighter state at a line boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct State {
    pub ctx: Ctx,
    pub html: HtmlMode,
    /// Embedded language to enter when the current tag closes.
    pub pending: HtmlMode,
    /// CSS brace depth.
    pub depth: u16,
}

/// Tokenizes one line, returning one [`Kind`] per char and advancing `state`.
pub fn highlight_line(lang: &Language, line: &str, state: &mut State) -> Vec<Kind> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = vec![Kind::Normal; chars.len()];
    match lang.mode {
        Mode::Plain => {}
        Mode::Generic => generic::tokenize(lang, &chars, 0, chars.len(), &mut state.ctx, &mut out),
        Mode::Css => css::tokenize(lang, &chars, 0, chars.len(), &mut state.ctx, &mut state.depth, &mut out),
        Mode::Html => html::tokenize(&chars, state, &mut out),
        Mode::Markdown => markdown::tokenize(&chars, &mut state.ctx, &mut out),
        Mode::Diff => diff(&chars, &mut out),
        Mode::Hex => hex(&chars, &mut out),
    }
    out
}

fn diff(chars: &[char], out: &mut [Kind]) {
    let s: String = chars.iter().take(4).collect();
    let kind = if s.starts_with("+++") || s.starts_with("---") || s.starts_with("diff") || s.starts_with("index") {
        Kind::Strong
    } else if s.starts_with("@@") {
        Kind::Preproc
    } else if s.starts_with('+') {
        Kind::Added
    } else if s.starts_with('-') {
        Kind::Removed
    } else {
        Kind::Normal
    };
    out.fill(kind);
}

fn hex(chars: &[char], out: &mut [Kind]) {
    let bar = chars.iter().position(|&c| c == '|');
    for (i, k) in out.iter_mut().enumerate() {
        *k = if i < 8 {
            Kind::Number
        } else if bar.is_some_and(|b| i >= b) {
            Kind::String
        } else {
            Kind::Normal
        };
    }
    if chars.first() == Some(&'.') {
        out.fill(Kind::Comment);
    }
}

// ----------------------------------------------------------------------
// Shared lexing helpers
// ----------------------------------------------------------------------

pub(crate) fn starts_with_at(chars: &[char], i: usize, pat: &str) -> bool {
    (i..).zip(pat.chars()).all(|(j, p)| chars.get(j) == Some(&p))
}

pub(crate) fn starts_with_at_ci(chars: &[char], i: usize, pat: &str) -> bool {
    (i..).zip(pat.chars()).all(|(j, p)| chars.get(j).is_some_and(|c| c.eq_ignore_ascii_case(&p)))
}

/// Finds `pat` at or after `from`, returning its start index.
pub(crate) fn find_at(chars: &[char], from: usize, end: usize, pat: &str, ci: bool) -> Option<usize> {
    let n = pat.chars().count();
    (from..end.saturating_sub(n.saturating_sub(1)).max(from)).find(|&i| {
        i + n <= end && if ci { starts_with_at_ci(chars, i, pat) } else { starts_with_at(chars, i, pat) }
    })
}

pub(crate) fn fill(out: &mut [Kind], from: usize, to: usize, k: Kind) {
    let to = to.min(out.len());
    if from < to {
        out[from..to].fill(k);
    }
}

/// Scans a quoted string starting at `i` (the opening quote). Returns the
/// index just past the closing quote, or `end` if unterminated.
pub(crate) fn scan_string(chars: &[char], i: usize, end: usize, quote: char, raw: bool, out: &mut [Kind]) -> usize {
    out[i] = Kind::String;
    let mut j = i + 1;
    while j < end {
        let c = chars[j];
        if c == '\\' && !raw && j + 1 < end {
            out[j] = Kind::Escape;
            out[j + 1] = Kind::Escape;
            j += 2;
            continue;
        }
        out[j] = Kind::String;
        j += 1;
        if c == quote {
            return j;
        }
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lang: &str, src: &str) -> Vec<Vec<Kind>> {
        let lang = by_name(lang).unwrap();
        let mut st = State::default();
        src.lines().map(|l| highlight_line(lang, l, &mut st)).collect()
    }

    #[test]
    fn python_basics() {
        let k = kinds("python", "def foo(x): # hi\n    return 'a\\n' + 42");
        assert_eq!(k[0][0], Kind::Keyword);
        assert_eq!(k[0][4], Kind::Function);
        assert_eq!(k[0][12], Kind::Comment);
        assert_eq!(k[1][4], Kind::Keyword);
        assert_eq!(k[1][11], Kind::String);
        assert_eq!(k[1][13], Kind::Escape);
        assert_eq!(k[1][19], Kind::Number);
    }

    #[test]
    fn python_triple_quotes_span_lines() {
        let k = kinds("python", "x = \"\"\"start\nmiddle\nend\"\"\" + y");
        assert!(k[1].iter().all(|&c| c == Kind::String));
        assert_eq!(k[2][0], Kind::String);
        assert_eq!(*k[2].last().unwrap(), Kind::Normal);
    }

    #[test]
    fn html_with_embedded_css_and_js() {
        let k = kinds(
            "html",
            "<div class=\"a\">&amp;</div>\n<style>\nbody { color: red; }\n</style>\n<script>var x = 1;</script>",
        );
        assert_eq!(k[0][1], Kind::Tag);
        assert_eq!(k[0][5], Kind::Attribute);
        assert_eq!(k[0][11], Kind::String);
        assert_eq!(k[0][15], Kind::Constant);
        assert_eq!(k[2][0], Kind::Tag);
        assert_eq!(k[2][7], Kind::Property);
        assert_eq!(k[4][8], Kind::Keyword);
        assert_eq!(k[4][16], Kind::Number);
        assert_eq!(k[4][20], Kind::Tag);
    }

    #[test]
    fn html_comment_multiline() {
        let k = kinds("html", "<!-- a\nb -->\n<p>");
        assert!(k[1].iter().all(|&c| c == Kind::Comment));
        assert_eq!(k[2][1], Kind::Tag);
    }

    #[test]
    fn css_selectors_and_values() {
        let k = kinds("css", ".btn:hover, #id > a {\n  margin: 10px auto !important; /* c */\n  color: #fff;\n}");
        assert_eq!(k[0][0], Kind::Selector);
        assert_eq!(k[0][5], Kind::Keyword);
        assert_eq!(k[0][12], Kind::Selector);
        assert_eq!(k[0][18], Kind::Tag);
        assert_eq!(k[1][2], Kind::Property);
        assert_eq!(k[1][10], Kind::Number);
        assert_eq!(k[1][15], Kind::Constant);
        assert_eq!(k[1][20], Kind::Keyword);
        assert_eq!(k[1][33], Kind::Comment);
        assert_eq!(k[2][9], Kind::Number);
    }

    #[test]
    fn rust_lifetimes_are_not_strings() {
        let k = kinds("rust", "fn f<'a>(x: &'a str) -> char { 'x' }");
        assert_eq!(k[0][5], Kind::Type);
        assert_eq!(k[0][13], Kind::Type);
        assert_eq!(k[0][16], Kind::Type);
        assert_eq!(k[0][31], Kind::String);
    }

    #[test]
    fn markdown_inline() {
        let k = kinds("markdown", "# Title\nsome **bold** and `code` [l](u)");
        assert!(k[0].iter().all(|&c| c == Kind::Heading));
        assert_eq!(k[1][7], Kind::Strong);
        assert_eq!(k[1][19], Kind::Code);
        assert_eq!(k[1][26], Kind::Link);
    }

    #[test]
    fn every_language_survives_garbage() {
        let junk = "\"'`/*<!--{[(#@$\\ \t\u{1F600}é<script><style>*/-->}])\"\"\"'''";
        for lang in LANGUAGES {
            let mut st = State::default();
            for _ in 0..3 {
                let k = highlight_line(lang, junk, &mut st);
                assert_eq!(k.len(), junk.chars().count());
                highlight_line(lang, "", &mut st);
            }
        }
    }
}
