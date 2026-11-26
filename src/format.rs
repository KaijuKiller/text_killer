//! Text formatting commands operating on the active document.

use crate::document::{char_len, Document, EditKind, Pos};
use crate::syntax::Mode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatOp {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    Link,
    Heading,
    ToggleComment,
    Indent,
    Outdent,
    Upper,
    Lower,
    Title,
    SortLines,
    SortLinesDesc,
    UniqueLines,
    ReverseLines,
    TrimTrailing,
    TabsToSpaces,
    SpacesToTabs,
    Reflow,
    JoinLines,
    DuplicateLine,
    DeleteLine,
    MoveLineUp,
    MoveLineDown,
    PrettyJson,
    MinifyJson,
}

impl FormatOp {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::Underline => "Underline",
            Self::Strike => "Strikethrough",
            Self::Code => "Inline code",
            Self::Link => "Link",
            Self::Heading => "Heading (cycle)",
            Self::ToggleComment => "Toggle comment",
            Self::Indent => "Indent",
            Self::Outdent => "Outdent",
            Self::Upper => "UPPERCASE",
            Self::Lower => "lowercase",
            Self::Title => "Title Case",
            Self::SortLines => "Sort lines A-Z",
            Self::SortLinesDesc => "Sort lines Z-A",
            Self::UniqueLines => "Remove duplicate lines",
            Self::ReverseLines => "Reverse lines",
            Self::TrimTrailing => "Trim trailing whitespace",
            Self::TabsToSpaces => "Tabs -> spaces",
            Self::SpacesToTabs => "Spaces -> tabs",
            Self::Reflow => "Reflow paragraph (80 cols)",
            Self::JoinLines => "Join lines",
            Self::DuplicateLine => "Duplicate line",
            Self::DeleteLine => "Delete line",
            Self::MoveLineUp => "Move line up",
            Self::MoveLineDown => "Move line down",
            Self::PrettyJson => "Pretty-print JSON",
            Self::MinifyJson => "Minify JSON",
        }
    }
}

/// Applies `op` to `doc`. Returns a status message on success or an error.
pub fn apply(doc: &mut Document, op: FormatOp) -> Result<Option<String>, String> {
    if doc.readonly {
        return Err("document is read-only".into());
    }
    match op {
        FormatOp::Bold | FormatOp::Italic | FormatOp::Underline | FormatOp::Strike | FormatOp::Code | FormatOp::Link => {
            let (pre, post) = markup_for(doc, op)?;
            wrap_selection(doc, &pre, &post);
        }
        FormatOp::Heading => heading(doc)?,
        FormatOp::ToggleComment => toggle_comment(doc)?,
        FormatOp::Indent => indent(doc),
        FormatOp::Outdent => outdent(doc),
        FormatOp::Upper => change_case(doc, |s| s.to_uppercase()),
        FormatOp::Lower => change_case(doc, |s| s.to_lowercase()),
        FormatOp::Title => change_case(doc, title_case),
        FormatOp::SortLines => map_lines(doc, |mut v| {
            v.sort_by_key(|a| a.to_lowercase());
            v
        }),
        FormatOp::SortLinesDesc => map_lines(doc, |mut v| {
            v.sort_by_key(|a| std::cmp::Reverse(a.to_lowercase()));
            v
        }),
        FormatOp::UniqueLines => map_lines(doc, |v| {
            let mut seen = std::collections::HashSet::new();
            v.into_iter().filter(|l| seen.insert(l.clone())).collect()
        }),
        FormatOp::ReverseLines => map_lines(doc, |mut v| {
            v.reverse();
            v
        }),
        FormatOp::TrimTrailing => {
            let n = trim_trailing(doc);
            return Ok(Some(format!("trimmed {n} line(s)")));
        }
        FormatOp::TabsToSpaces => {
            let tw = doc.tab_width;
            whole_or_selected_lines(doc, |l| expand_tabs(l, tw));
            doc.use_tabs = false;
        }
        FormatOp::SpacesToTabs => {
            let tw = doc.tab_width;
            whole_or_selected_lines(doc, |l| leading_spaces_to_tabs(l, tw));
            doc.use_tabs = true;
        }
        FormatOp::Reflow => reflow(doc, 80),
        FormatOp::JoinLines => join_lines(doc),
        FormatOp::DuplicateLine => duplicate(doc),
        FormatOp::DeleteLine => delete_lines(doc),
        FormatOp::MoveLineUp => move_lines(doc, -1),
        FormatOp::MoveLineDown => move_lines(doc, 1),
        FormatOp::PrettyJson | FormatOp::MinifyJson => {
            let src = doc.selected_text().unwrap_or_else(|| doc.full_text());
            let out = json_format(&src, op == FormatOp::PrettyJson, doc.tab_width.clamp(1, 8))?;
            doc.transact(EditKind::Other, |d| {
                if d.selection().is_none() {
                    d.select_all();
                }
                d.delete_selection();
                d.cursor = d.insert(d.cursor, &out);
            });
            doc.cursor = Pos::default();
            doc.anchor = None;
        }
    }
    Ok(None)
}

fn markup_for(doc: &Document, op: FormatOp) -> Result<(String, String), String> {
    let pair = |a: &str, b: &str| Ok((a.to_owned(), b.to_owned()));
    match doc.language.mode {
        Mode::Html => match op {
            FormatOp::Bold => pair("<strong>", "</strong>"),
            FormatOp::Italic => pair("<em>", "</em>"),
            FormatOp::Underline => pair("<u>", "</u>"),
            FormatOp::Strike => pair("<s>", "</s>"),
            FormatOp::Code => pair("<code>", "</code>"),
            FormatOp::Link => pair("<a href=\"\">", "</a>"),
            _ => unreachable!(),
        },
        Mode::Plain | Mode::Markdown => match op {
            FormatOp::Bold => pair("**", "**"),
            FormatOp::Italic => pair("*", "*"),
            FormatOp::Underline => pair("<u>", "</u>"),
            FormatOp::Strike => pair("~~", "~~"),
            FormatOp::Code => pair("`", "`"),
            FormatOp::Link => pair("[", "]()"),
            _ => unreachable!(),
        },
        _ => match op {
            FormatOp::Code => pair("`", "`"),
            _ => Err(format!("{} has no {} markup", doc.language.name, op.label().to_lowercase())),
        },
    }
}

/// Wraps the selection (or word under cursor) in `pre`/`post`, or unwraps
/// if it is already wrapped.
pub fn wrap_selection(doc: &mut Document, pre: &str, post: &str) {
    if doc.selection().is_none() {
        doc.select_word();
    }
    let (s, e) = doc.selection().unwrap_or((doc.cursor, doc.cursor));
    let text = doc.text_range(s, e);
    doc.transact(EditKind::Other, |d| {
        d.anchor = None;
        // Unwrap if the surrounding text is already the markup.
        let pre_len = char_len(pre);
        let post_len = char_len(post);
        let before = (s.col >= pre_len).then(|| d.text_range(Pos::new(s.line, s.col - pre_len), s));
        let after = (e.col + post_len <= d.line_len(e.line)).then(|| d.text_range(e, Pos::new(e.line, e.col + post_len)));
        if before.as_deref() == Some(pre) && after.as_deref() == Some(post) {
            d.delete(e, Pos::new(e.line, e.col + post_len));
            d.delete(Pos::new(s.line, s.col - pre_len), s);
            let ns = Pos::new(s.line, s.col - pre_len);
            let ne = if e.line == s.line { Pos::new(e.line, e.col - pre_len) } else { e };
            d.anchor = Some(ns);
            d.cursor = ne;
            return;
        }
        if text.starts_with(pre) && text.ends_with(post) && char_len(&text) >= pre_len + post_len {
            let inner: String = text.chars().skip(pre_len).take(char_len(&text) - pre_len - post_len).collect();
            d.delete(s, e);
            let end = d.insert(s, &inner);
            d.anchor = Some(s);
            d.cursor = end;
            return;
        }
        d.insert(e, post);
        let ns = d.insert(s, pre);
        let ne = if e.line == s.line { Pos::new(e.line, e.col + pre_len) } else { e };
        d.anchor = Some(ns);
        d.cursor = ne;
    });
}

fn heading(doc: &mut Document) -> Result<(), String> {
    let line = doc.cursor.line;
    let content = doc.lines[line].clone();
    match doc.language.mode {
        Mode::Markdown | Mode::Plain => {
            let level = content.chars().take_while(|&c| c == '#').count();
            let body = content[level..].trim_start();
            let new = if level >= 6 { body.to_owned() } else { format!("{} {}", "#".repeat(level + 1), body) };
            doc.transact(EditKind::Other, |d| d.replace_line(line, &new));
            doc.cursor = Pos::new(line, doc.line_len(line));
        }
        Mode::Html => {
            let t = content.trim();
            let ws: String = content.chars().take_while(|c| c.is_whitespace()).collect();
            let level = (1..=6).find(|n| t.starts_with(&format!("<h{n}>")) && t.ends_with(&format!("</h{n}>")));
            let new = match level {
                Some(n) => {
                    let inner = &t[4..t.len() - 5];
                    if n >= 6 { format!("{ws}{inner}") } else { format!("{ws}<h{0}>{inner}</h{0}>", n + 1) }
                }
                None => format!("{ws}<h1>{t}</h1>"),
            };
            doc.transact(EditKind::Other, |d| d.replace_line(line, &new));
            doc.cursor = Pos::new(line, doc.line_len(line));
        }
        _ => return Err(format!("headings are not supported for {}", doc.language.name)),
    }
    doc.anchor = None;
    Ok(())
}

fn toggle_comment(doc: &mut Document) -> Result<(), String> {
    let (open, close) = doc
        .language
        .comment_markers()
        .ok_or_else(|| format!("{} has no comment syntax", doc.language.name))?;
    let (first, last) = doc.selected_line_range();
    let lines: Vec<usize> = (first..=last).filter(|&l| !doc.lines[l].trim().is_empty()).collect();
    if lines.is_empty() {
        return Ok(());
    }
    let open_t = open.trim_end();
    let close_t = close.trim_start();
    let all_commented = lines.iter().all(|&l| {
        let t = doc.lines[l].trim();
        t.starts_with(open_t) && (close.is_empty() || t.ends_with(close_t))
    });
    // Insert markers at the minimum indentation so blocks stay aligned.
    let min_indent = lines
        .iter()
        .map(|&l| doc.lines[l].chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);
    let had_sel = doc.anchor.is_some();
    doc.transact(EditKind::Other, |d| {
        for &l in &lines {
            let line = d.lines[l].clone();
            let new = if all_commented {
                let ws_len = line.len() - line.trim_start().len();
                let (ws, rest) = line.split_at(ws_len);
                let r = rest.strip_prefix(open_t).unwrap_or(rest);
                let mut rest = r.strip_prefix(' ').unwrap_or(r).to_owned();
                if !close.is_empty() {
                    let r = rest.trim_end();
                    let r = r.strip_suffix(close_t).unwrap_or(r);
                    rest = r.strip_suffix(' ').unwrap_or(r).to_owned();
                }
                format!("{ws}{rest}")
            } else {
                let split = crate::document::byte_idx(&line, min_indent);
                let (ws, rest) = line.split_at(split);
                let sep = if open.ends_with(' ') { "" } else { " " };
                if close.is_empty() {
                    format!("{ws}{open}{sep}{rest}")
                } else {
                    format!("{ws}{open}{sep}{rest} {close}")
                }
            };
            d.replace_line(l, &new);
        }
    });
    if had_sel {
        doc.anchor = Some(Pos::new(first, 0));
        doc.cursor = Pos::new(last, doc.line_len(last));
    } else {
        doc.cursor = doc.clamp(doc.cursor);
    }
    Ok(())
}

pub fn indent(doc: &mut Document) {
    let (first, last) = doc.selected_line_range();
    let unit = doc.indent_unit();
    let unit_len = char_len(&unit);
    let sel = doc.selection();
    doc.transact(EditKind::Other, |d| {
        for l in first..=last {
            if !d.lines[l].is_empty() || first == last {
                d.insert(Pos::new(l, 0), &unit);
            }
        }
    });
    match sel {
        Some(_) => {
            doc.anchor = Some(Pos::new(first, 0));
            doc.cursor = Pos::new(last, doc.line_len(last));
        }
        None => doc.cursor.col += unit_len,
    }
}

pub fn outdent(doc: &mut Document) {
    let (first, last) = doc.selected_line_range();
    let tw = doc.tab_width;
    let sel = doc.selection();
    let mut cursor_shift = 0;
    doc.transact(EditKind::Other, |d| {
        for l in first..=last {
            let line = &d.lines[l];
            let n = if line.starts_with('\t') {
                1
            } else {
                line.chars().take(tw).take_while(|&c| c == ' ').count()
            };
            if n > 0 {
                d.delete(Pos::new(l, 0), Pos::new(l, n));
                if l == d.cursor.line {
                    cursor_shift = n;
                }
            }
        }
    });
    match sel {
        Some(_) => {
            doc.anchor = Some(Pos::new(first, 0));
            doc.cursor = Pos::new(last, doc.line_len(last));
        }
        None => doc.cursor.col = doc.cursor.col.saturating_sub(cursor_shift),
    }
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = c.is_whitespace() || matches!(c, '-' | '_' | '(' | '"' | '/');
        }
    }
    out
}

fn change_case(doc: &mut Document, f: impl Fn(&str) -> String) {
    if doc.selection().is_none() {
        doc.select_word();
    }
    let Some((s, e)) = doc.selection() else { return };
    let text = doc.text_range(s, e);
    let new = f(&text);
    doc.transact(EditKind::Other, |d| {
        d.delete(s, e);
        let end = d.insert(s, &new);
        d.anchor = Some(s);
        d.cursor = end;
    });
}

/// Replaces the selected lines (or the whole document if none) via `f`.
fn map_lines(doc: &mut Document, f: impl FnOnce(Vec<String>) -> Vec<String>) {
    let (first, last) = if doc.selection().is_some() {
        doc.selected_line_range()
    } else {
        // Ignore a trailing empty line (file's final newline).
        let mut last = doc.line_count() - 1;
        if last > 0 && doc.lines[last].is_empty() {
            last -= 1;
        }
        (0, last)
    };
    let old: Vec<String> = doc.lines[first..=last].to_vec();
    let new = f(old.clone());
    if new == old {
        return;
    }
    let end = Pos::new(last, doc.line_len(last));
    let text = new.join("\n");
    doc.transact(EditKind::Other, |d| {
        d.delete(Pos::new(first, 0), end);
        let e = d.insert(Pos::new(first, 0), &text);
        d.anchor = Some(Pos::new(first, 0));
        d.cursor = e;
    });
}

fn whole_or_selected_lines(doc: &mut Document, f: impl Fn(&str) -> String) {
    let (first, last) = if doc.selection().is_some() { doc.selected_line_range() } else { (0, doc.line_count() - 1) };
    doc.transact(EditKind::Other, |d| {
        for l in first..=last {
            let new = f(&d.lines[l]);
            d.replace_line(l, &new);
        }
    });
    doc.cursor = doc.clamp(doc.cursor);
    if let Some(a) = doc.anchor {
        doc.anchor = Some(doc.clamp(a));
    }
}

fn trim_trailing(doc: &mut Document) -> usize {
    let mut n = 0;
    doc.transact(EditKind::Other, |d| {
        for l in 0..d.line_count() {
            let t = d.lines[l].trim_end().len();
            if t != d.lines[l].len() {
                let keep = char_len(&d.lines[l][..t]);
                let len = d.line_len(l);
                d.delete(Pos::new(l, keep), Pos::new(l, len));
                n += 1;
            }
        }
    });
    doc.cursor = doc.clamp(doc.cursor);
    doc.anchor = None;
    n
}

pub fn expand_tabs(line: &str, tw: usize) -> String {
    let mut out = String::with_capacity(line.len());
    let mut col = 0;
    for c in line.chars() {
        if c == '\t' {
            let n = tw - col % tw;
            out.extend(std::iter::repeat_n(' ', n));
            col += n;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

fn leading_spaces_to_tabs(line: &str, tw: usize) -> String {
    let ws: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    let width = expand_tabs(&ws, tw).len();
    format!("{}{}{}", "\t".repeat(width / tw), " ".repeat(width % tw), &line[ws.len()..])
}

/// Re-wraps the paragraph around the cursor (or the selected lines).
pub fn reflow(doc: &mut Document, width: usize) {
    let (first, last) = if doc.selection().is_some() {
        doc.selected_line_range()
    } else {
        let mut a = doc.cursor.line;
        let mut b = doc.cursor.line;
        if doc.lines[a].trim().is_empty() {
            return;
        }
        while a > 0 && !doc.lines[a - 1].trim().is_empty() {
            a -= 1;
        }
        while b + 1 < doc.line_count() && !doc.lines[b + 1].trim().is_empty() {
            b += 1;
        }
        (a, b)
    };
    let indent: String = doc.lines[first].chars().take_while(|c| c.is_whitespace()).collect();
    let words: Vec<String> = doc.lines[first..=last]
        .iter()
        .flat_map(|l| l.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut cur = indent.clone();
    for w in words {
        let cur_len = char_len(&cur);
        if cur_len > char_len(&indent) && cur_len + 1 + char_len(&w) > width {
            out.push(std::mem::replace(&mut cur, indent.clone()));
        }
        if char_len(&cur) > char_len(&indent) {
            cur.push(' ');
        }
        cur.push_str(&w);
    }
    if char_len(&cur) > char_len(&indent) {
        out.push(cur);
    }
    let text = out.join("\n");
    let end = Pos::new(last, doc.line_len(last));
    doc.transact(EditKind::Other, |d| {
        d.delete(Pos::new(first, 0), end);
        d.cursor = d.insert(Pos::new(first, 0), &text);
        d.anchor = None;
    });
}

fn join_lines(doc: &mut Document) {
    let (first, mut last) = doc.selected_line_range();
    if first == last {
        last = (first + 1).min(doc.line_count() - 1);
    }
    if first == last {
        return;
    }
    doc.transact(EditKind::Other, |d| {
        for _ in first..last {
            let len = d.line_len(first);
            let next = d.lines[first + 1].clone();
            let lead = next.chars().take_while(|c| c.is_whitespace()).count();
            d.delete(Pos::new(first, len), Pos::new(first + 1, lead));
            let sep = if len == 0 || next.trim().is_empty() || d.lines[first].ends_with(' ') { "" } else { " " };
            d.insert(Pos::new(first, len), sep);
            d.cursor = Pos::new(first, len);
        }
        d.anchor = None;
    });
}

fn duplicate(doc: &mut Document) {
    if let Some((s, e)) = doc.selection() {
        let text = doc.text_range(s, e);
        doc.transact(EditKind::Other, |d| {
            let end = d.insert(e, &text);
            d.anchor = Some(e);
            d.cursor = end;
        });
        return;
    }
    let l = doc.cursor.line;
    let text = format!("\n{}", doc.lines[l]);
    let col = doc.cursor.col;
    doc.transact(EditKind::Other, |d| {
        let len = d.line_len(l);
        d.insert(Pos::new(l, len), &text);
        d.cursor = Pos::new(l + 1, col);
    });
}

fn delete_lines(doc: &mut Document) {
    let (first, last) = doc.selected_line_range();
    doc.transact(EditKind::Other, |d| {
        let n = d.line_count();
        if last + 1 < n {
            d.delete(Pos::new(first, 0), Pos::new(last + 1, 0));
        } else if first > 0 {
            let plen = d.line_len(first - 1);
            let llen = d.line_len(last);
            d.delete(Pos::new(first - 1, plen), Pos::new(last, llen));
        } else {
            let llen = d.line_len(last);
            d.delete(Pos::new(0, 0), Pos::new(last, llen));
        }
        d.anchor = None;
        d.cursor = d.clamp(Pos::new(first, 0));
    });
}

fn move_lines(doc: &mut Document, dir: isize) {
    let (first, last) = doc.selected_line_range();
    if (dir < 0 && first == 0) || (dir > 0 && last + 1 >= doc.line_count()) {
        return;
    }
    let anchor = doc.anchor;
    let cursor = doc.cursor;
    let block: Vec<String> = doc.lines[first..=last].to_vec();
    doc.transact(EditKind::Other, |d| {
        if dir < 0 {
            let above = d.lines[first - 1].clone();
            let mut new = block.clone();
            new.push(above);
            let end = Pos::new(last, d.line_len(last));
            d.delete(Pos::new(first - 1, 0), end);
            d.insert(Pos::new(first - 1, 0), &new.join("\n"));
        } else {
            let below = d.lines[last + 1].clone();
            let mut new = vec![below];
            new.extend(block.iter().cloned());
            let end = Pos::new(last + 1, d.line_len(last + 1));
            d.delete(Pos::new(first, 0), end);
            d.insert(Pos::new(first, 0), &new.join("\n"));
        }
        let shift = |p: Pos| Pos::new((p.line as isize + dir) as usize, p.col);
        d.cursor = d.clamp(shift(cursor));
        d.anchor = anchor.map(|a| d.clamp(shift(a)));
    });
}

// ----------------------------------------------------------------------
// Minimal JSON formatter (validates structure, preserves string contents)
// ----------------------------------------------------------------------

pub fn json_format(src: &str, pretty: bool, indent: usize) -> Result<String, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len() + src.len() / 4);
    let mut depth: usize = 0;
    let mut stack: Vec<char> = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let pad = |out: &mut String, depth: usize| {
        out.push('\n');
        out.extend(std::iter::repeat_n(' ', depth * indent));
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => line += 1,
            ' ' | '\t' | '\r' => {}
            '"' => {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    if chars.get(i) == Some(&'\n') {
                        return Err(format!("line {line}: newline inside string"));
                    }
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(format!("line {line}: unterminated string"));
                }
                out.extend(&chars[start..=i]);
            }
            '{' | '[' => {
                stack.push(if c == '{' { '}' } else { ']' });
                out.push(c);
                // Keep empty containers compact.
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if chars.get(j) == Some(stack.last().unwrap()) {
                    out.push(chars[j]);
                    stack.pop();
                    for ch in &chars[i + 1..j] {
                        if *ch == '\n' {
                            line += 1;
                        }
                    }
                    i = j;
                } else {
                    depth += 1;
                    if pretty {
                        pad(&mut out, depth);
                    }
                }
            }
            '}' | ']' => {
                if stack.pop() != Some(c) {
                    return Err(format!("line {line}: unexpected '{c}'"));
                }
                depth = depth.saturating_sub(1);
                if pretty {
                    pad(&mut out, depth);
                }
                out.push(c);
            }
            ',' => {
                out.push(',');
                if pretty {
                    pad(&mut out, depth);
                }
            }
            ':' => out.push_str(if pretty { ": " } else { ":" }),
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 2;
                continue;
            }
            c if c.is_alphanumeric() || matches!(c, '-' | '+' | '.') => {
                let start = i;
                while i + 1 < chars.len() && (chars[i + 1].is_alphanumeric() || matches!(chars[i + 1], '-' | '+' | '.')) {
                    i += 1;
                }
                let tok: String = chars[start..=i].iter().collect();
                let valid = matches!(tok.as_str(), "true" | "false" | "null") || tok.parse::<f64>().is_ok();
                if !valid {
                    return Err(format!("line {line}: invalid token '{tok}'"));
                }
                out.push_str(&tok);
            }
            other => return Err(format!("line {line}: unexpected character '{other}'")),
        }
        i += 1;
    }
    if !stack.is_empty() {
        return Err("unexpected end of input: unclosed bracket".into());
    }
    if pretty {
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(name: &str, t: &str) -> Document {
        Document::from_text(name.into(), t, None)
    }

    #[test]
    fn bold_markdown_and_html() {
        let mut d = doc("a.md", "hello world");
        d.cursor = Pos::new(0, 2);
        apply(&mut d, FormatOp::Bold).unwrap();
        assert_eq!(d.full_text(), "**hello** world");
        apply(&mut d, FormatOp::Bold).unwrap();
        assert_eq!(d.full_text(), "hello world");
        let mut h = doc("a.html", "hi");
        h.select_all();
        apply(&mut h, FormatOp::Italic).unwrap();
        assert_eq!(h.full_text(), "<em>hi</em>");
        let mut p = doc("a.py", "x");
        assert!(apply(&mut p, FormatOp::Bold).is_err());
    }

    #[test]
    fn comment_toggle_python_and_css() {
        let mut d = doc("a.py", "  a = 1\n  b = 2");
        d.select_all();
        apply(&mut d, FormatOp::ToggleComment).unwrap();
        assert_eq!(d.full_text(), "  # a = 1\n  # b = 2");
        d.select_all();
        apply(&mut d, FormatOp::ToggleComment).unwrap();
        assert_eq!(d.full_text(), "  a = 1\n  b = 2");
        let mut c = doc("a.css", "a { }");
        apply(&mut c, FormatOp::ToggleComment).unwrap();
        assert_eq!(c.full_text(), "/* a { } */");
        apply(&mut c, FormatOp::ToggleComment).unwrap();
        assert_eq!(c.full_text(), "a { }");
    }

    #[test]
    fn sort_and_move() {
        let mut d = doc("a.txt", "b\nc\na\n");
        apply(&mut d, FormatOp::SortLines).unwrap();
        assert_eq!(d.full_text(), "a\nb\nc\n");
        d.anchor = None;
        d.cursor = Pos::new(0, 0);
        apply(&mut d, FormatOp::MoveLineDown).unwrap();
        assert_eq!(d.full_text(), "b\na\nc\n");
        assert_eq!(d.cursor.line, 1);
    }

    #[test]
    fn indent_outdent() {
        let mut d = doc("a.py", "x\ny");
        d.select_all();
        apply(&mut d, FormatOp::Indent).unwrap();
        assert_eq!(d.full_text(), "    x\n    y");
        apply(&mut d, FormatOp::Outdent).unwrap();
        assert_eq!(d.full_text(), "x\ny");
    }

    #[test]
    fn reflow_and_join() {
        let mut d = doc("a.txt", &"word ".repeat(30));
        apply(&mut d, FormatOp::Reflow).unwrap();
        assert!(d.lines.iter().all(|l| l.len() <= 80));
        assert_eq!(d.line_count(), 2);
        d.cursor = Pos::new(0, 0);
        apply(&mut d, FormatOp::JoinLines).unwrap();
        assert_eq!(d.line_count(), 1);
    }

    #[test]
    fn json() {
        let out = json_format("{\"a\":[1,2,{}],\"b\":\"x,y\"}", true, 2).unwrap();
        assert_eq!(out, "{\n  \"a\": [\n    1,\n    2,\n    {}\n  ],\n  \"b\": \"x,y\"\n}\n");
        assert_eq!(json_format(&out, false, 2).unwrap(), "{\"a\":[1,2,{}],\"b\":\"x,y\"}");
        assert!(json_format("{\"a\":}", true, 2).is_ok()); // structure-only validation
        assert!(json_format("{\"a\": nope}", true, 2).is_err());
        assert!(json_format("[1,2", true, 2).is_err());
    }

    #[test]
    fn case_changes() {
        let mut d = doc("a.txt", "hello big world");
        d.select_all();
        apply(&mut d, FormatOp::Title).unwrap();
        assert_eq!(d.full_text(), "Hello Big World");
        apply(&mut d, FormatOp::Upper).unwrap();
        assert_eq!(d.full_text(), "HELLO BIG WORLD");
    }
}
