//! The main text area: gutter, syntax-highlighted lines, selection,
//! search matches, bracket matching and the cursor.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::{char_width, display_col};
use crate::app::{App, Focus, RowHit};
use crate::document::{Document, Pos};
use crate::syntax::{self, Kind};
use crate::theme::Theme;
use crate::wrap::{self, Seg, VRow};

const BRACKETS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];
const BRACKET_SCAN_LIMIT: usize = 50_000;

/// Finds the bracket at/before the cursor and its partner.
pub(super) fn matching_bracket(d: &Document) -> Option<(Pos, Pos)> {
    let here = d.cursor;
    let candidates = [Some(here), (here.col > 0).then(|| Pos::new(here.line, here.col - 1))];
    for p in candidates.into_iter().flatten() {
        let Some(c) = d.char_at(p) else { continue };
        for (open, close) in BRACKETS {
            if c == open {
                return scan(d, p, open, close, true).map(|m| (p, m));
            }
            if c == close {
                return scan(d, p, close, open, false).map(|m| (p, m));
            }
        }
    }
    None
}

fn scan(d: &Document, start: Pos, me: char, other: char, forward: bool) -> Option<Pos> {
    let mut depth = 0usize;
    let mut budget = BRACKET_SCAN_LIMIT;
    let mut line = start.line;
    loop {
        let chars: Vec<char> = d.lines[line].chars().collect();
        let range: Box<dyn Iterator<Item = usize>> = if line == start.line {
            if forward { Box::new(start.col + 1..chars.len()) } else { Box::new((0..start.col).rev()) }
        } else if forward {
            Box::new(0..chars.len())
        } else {
            Box::new((0..chars.len()).rev())
        };
        for i in range {
            budget = budget.checked_sub(1)?;
            let c = chars[i];
            if c == me {
                depth += 1;
            } else if c == other {
                if depth == 0 {
                    return Some(Pos::new(line, i));
                }
                depth -= 1;
            }
        }
        if forward {
            line += 1;
            if line >= d.line_count() {
                return None;
            }
        } else {
            line = line.checked_sub(1)?;
        }
    }
}

/// Char ranges in `line` matching `needle` (smart-case).
fn search_matches(line: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let ci = !needle.chars().any(char::is_uppercase);
    let hay: Vec<char> = if ci { line.chars().flat_map(char::to_lowercase).collect() } else { line.chars().collect() };
    let nee: Vec<char> = if ci { needle.chars().flat_map(char::to_lowercase).collect() } else { needle.chars().collect() };
    // Only valid when lowercasing preserved char counts (true for nearly all text).
    if hay.len() != line.chars().count() || nee.len() > hay.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + nee.len() <= hay.len() {
        if hay[i..i + nee.len()] == nee[..] {
            out.push((i, i + nee.len()));
            i += nee.len();
        } else {
            i += 1;
        }
    }
    out
}

fn ensure_visible(d: &mut Document, height: usize, width: usize) {
    let so = if height > 12 { 3 } else if height > 4 { 1 } else { 0 };
    let line = d.cursor.line;
    if line < d.scroll_line + so {
        d.scroll_line = line.saturating_sub(so);
    } else if line + so >= d.scroll_line + height {
        d.scroll_line = (line + so + 1).saturating_sub(height);
    }
    let dc = display_col(&d.lines[line], d.cursor.col, d.tab_width);
    let margin = if width > 30 { 8 } else { 1 };
    if dc < d.scroll_col {
        d.scroll_col = dc.saturating_sub(margin);
    } else if dc >= d.scroll_col + width {
        d.scroll_col = (dc + 1 + margin.min(width / 2)).saturating_sub(width);
    }
}

/// Accumulates styled cells for one row, merging runs of equal style and
/// clipping to the horizontal viewport `[start, end)`.
struct RowBuilder {
    spans: Vec<Span<'static>>,
    text: String,
    style: Option<Style>,
    used: usize,
    start: usize,
    end: usize,
}

impl RowBuilder {
    fn new(start: usize, end: usize) -> Self {
        Self { spans: Vec::new(), text: String::new(), style: None, used: 0, start, end }
    }

    fn push(&mut self, s: &str, style: Style, w: usize, x: usize) {
        if x + w <= self.start || x >= self.end {
            return;
        }
        let partial = x < self.start || x + w > self.end;
        let vis = (x + w).min(self.end) - x.max(self.start);
        if self.style != Some(style) {
            if let Some(cs) = self.style.take() {
                self.spans.push(Span::styled(std::mem::take(&mut self.text), cs));
            }
            self.style = Some(style);
        }
        if partial {
            // Partially visible wide char / tab: pad with spaces.
            self.text.extend(std::iter::repeat_n(' ', vis));
        } else {
            self.text.push_str(s);
        }
        self.used += vis;
    }

    /// Zero-width (combining) characters attach to the previous cell.
    fn attach(&mut self, c: char, x: usize) {
        if x > self.start && x <= self.end && !self.text.is_empty() {
            self.text.push(c);
        }
    }

    fn finish(mut self) -> Vec<Span<'static>> {
        if let Some(cs) = self.style.take() {
            self.spans.push(Span::styled(self.text, cs));
        }
        self.spans
    }
}

/// Shared row painter for the continuous, wrapped and page views.
pub(super) struct Painter<'a> {
    pub theme: &'a Theme,
    pub sel: Option<(Pos, Pos)>,
    pub brackets: Option<(Pos, Pos)>,
    pub show_ws: bool,
    pub search: Option<&'a str>,
    pub tab: usize,
}

impl Painter<'_> {
    /// Paints chars `range` of line `idx`, clipped to the display columns
    /// `[clip, clip + width)` measured from the start of the range. Returns
    /// spans padded to exactly `width` cells.
    #[allow(clippy::too_many_arguments)]
    pub fn row(
        &self,
        idx: usize,
        text: &str,
        kinds: &[Kind],
        range: Seg,
        clip: usize,
        width: usize,
        row_bg: Option<Color>,
        line_end: bool,
    ) -> Vec<Span<'static>> {
        let theme = self.theme;
        let matches = self.search.map(|s| search_matches(text, s)).unwrap_or_default();
        let trailing_from = text.trim_end().chars().count();
        let mut rb = RowBuilder::new(clip, clip + width);
        let mut x = 0usize;
        for (ci, ch) in text.chars().enumerate().skip(range.0).take(range.1 - range.0) {
            let p = Pos::new(idx, ci);
            let mut style = theme.kind(kinds.get(ci).copied().unwrap_or_default());
            let in_sel = self.sel.is_some_and(|(s, e)| p >= s && p < e);
            let in_match = matches.iter().any(|&(a, b)| ci >= a && ci < b);
            let bg = if in_sel {
                theme.selection_bg
            } else if in_match {
                theme.search_bg
            } else {
                row_bg.unwrap_or(theme.bg)
            };
            style = style.bg(bg);
            if self.brackets.is_some_and(|(a, b)| a == p || b == p) {
                style = theme.bracket_match;
            }
            let w = char_width(ch, x, self.tab);
            if ch == '\t' {
                if self.show_ws {
                    rb.push(&format!("→{}", " ".repeat(w - 1)), theme.whitespace.bg(bg), w, x);
                } else {
                    rb.push(&" ".repeat(w), style, w, x);
                }
            } else if ch.is_control() {
                let glyph = format!("^{}", char::from_u32((ch as u32 + 64) & 0x7f).unwrap_or('?'));
                rb.push(&glyph, theme.warn.bg(bg), 2, x);
            } else if ch == ' ' && self.show_ws && ci >= trailing_from {
                rb.push("·", theme.whitespace.bg(bg), 1, x);
            } else if w == 0 {
                rb.attach(ch, x);
            } else {
                let mut buf = [0u8; 4];
                rb.push(ch.encode_utf8(&mut buf), style, w, x);
            }
            x += w;
        }
        // A newline inside the selection is shown as one highlighted cell.
        let eol = Pos::new(idx, text.chars().count());
        if line_end && self.sel.is_some_and(|(s, e)| eol >= s && idx < e.line) {
            let sym = if self.show_ws { "¶" } else { " " };
            rb.push(sym, theme.whitespace.bg(theme.selection_bg), 1, x);
        }
        let used = rb.used;
        let mut spans = rb.finish();
        if used < width {
            let bg = row_bg.unwrap_or(theme.bg);
            spans.push(Span::styled(" ".repeat(width - used), Style::new().bg(bg)));
        }
        spans
    }

    /// The marker drawn for a page-break line.
    pub fn page_break(&self, width: usize, selected: bool) -> Vec<Span<'static>> {
        let label = " page break ";
        let dashes = width.saturating_sub(label.len());
        let left = dashes / 2;
        let text = format!("{}{label}{}", "┄".repeat(left), "┄".repeat(dashes - left));
        let text: String = text.chars().take(width).collect();
        let mut style = self.theme.gutter.bg(self.theme.bg);
        if selected {
            style = style.bg(self.theme.selection_bg);
        }
        vec![Span::styled(text, style)]
    }
}

/// Syntax kinds for line `idx` (computed through the cached line states).
pub(super) fn line_kinds(d: &mut Document, idx: usize) -> Vec<Kind> {
    let mut st = d.hl_state_at(idx);
    syntax::highlight_line(d.language, &d.lines[idx], &mut st)
}

/// Keeps the cursor's visual row inside a wrapped viewport of `height` rows.
fn ensure_visible_wrapped(d: &mut Document, height: usize, width: usize) {
    let so = if height > 12 { 3 } else if height > 4 { 1 } else { 0 };
    let cv = d.vrow_of(d.cursor, width);
    let mut top: VRow = (d.scroll_line, d.scroll_row);
    let above = d.step_vrow(cv, -(so as isize), width);
    if above < top {
        top = above;
    }
    let fits = d.rows_between(top, cv, height, width).is_some_and(|k| k + so < height);
    if !fits {
        top = d.step_vrow(cv, -((height.saturating_sub(1 + so)) as isize), width);
    }
    d.scroll_line = top.0;
    d.scroll_row = top.1;
}

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme();
    let line_numbers = app.settings.line_numbers;
    let wrap_mode = app.settings.wrap;
    let search = if app.search_visible { app.search.clone() } else { None };
    let focused = app.focus == Focus::Editor && app.overlay.is_none();
    let follow = app.follow_cursor;
    let show_ws = app.settings.whitespace;
    let d = &mut app.docs[app.active];

    let digits = d.line_count().to_string().len().max(3);
    let gutter_w = if line_numbers && area.width >= 24 { digits + 2 } else { 0 };
    let text_w = (area.width as usize).saturating_sub(gutter_w).max(1);
    let height = area.height as usize;
    app.layout.editor = area;
    app.layout.text_x = area.x + gutter_w as u16;

    let painter = Painter {
        theme,
        sel: d.selection(),
        brackets: matching_bracket(d),
        show_ws,
        search: search.as_deref(),
        tab: d.tab_width,
    };
    d.scroll_line = d.scroll_line.min(d.line_count().saturating_sub(1));
    let gutter = |idx: Option<usize>, cur: bool| -> Vec<Span<'static>> {
        if gutter_w == 0 {
            return Vec::new();
        }
        let g = if cur { theme.gutter_current } else { theme.gutter };
        match idx {
            Some(i) => vec![Span::styled(format!("{:>w$}", i + 1, w = digits), g), Span::styled("│ ", theme.border)],
            None => vec![Span::styled(" ".repeat(digits), g), Span::styled("│ ", theme.border)],
        }
    };
    let mut lines = Vec::with_capacity(height);

    if wrap_mode {
        // Soft-wrapped continuous view.
        app.layout.wrap_width = Some(text_w);
        d.scroll_col = 0;
        d.scroll_row = d.scroll_row.min(d.row_count(d.scroll_line, text_w) - 1);
        if follow {
            ensure_visible_wrapped(d, height, text_w);
        }
        let cv = d.vrow_of(d.cursor, text_w);
        let mut v = Some((d.scroll_line, d.scroll_row));
        let mut kinds: (usize, Vec<Kind>) = (usize::MAX, Vec::new());
        let mut cursor_xy = None;
        for row in 0..height {
            let Some(cur) = v else {
                let mut spans = Vec::new();
                if gutter_w > 0 {
                    spans.push(Span::styled(format!("{:>w$} ", "~", w = digits), theme.gutter));
                }
                lines.push(Line::from(spans));
                continue;
            };
            let (idx, r) = cur;
            let segs = d.segs(idx, text_w);
            let is_cur = cur == cv;
            let mut spans = gutter(if r == 0 { Some(idx) } else { None }, idx == d.cursor.line);
            if wrap::is_page_break(&d.lines[idx]) {
                let sel = painter.sel.is_some_and(|(s, e)| Pos::new(idx, 0) >= s && Pos::new(idx, 0) < e);
                spans.extend(painter.page_break(text_w, sel));
            } else {
                if kinds.0 != idx {
                    kinds = (idx, line_kinds(d, idx));
                }
                let row_bg = if is_cur && focused { Some(theme.current_line_bg) } else { None };
                let last = r + 1 == segs.len();
                spans.extend(painter.row(idx, &d.lines[idx], &kinds.1, segs[r], 0, text_w, row_bg, last));
            }
            app.layout.rows.push(RowHit {
                x: app.layout.text_x,
                y: area.y + row as u16,
                width: text_w as u16,
                line: idx,
                seg: segs[r],
                last: r + 1 == segs.len(),
            });
            if is_cur {
                let x = wrap::x_in_seg(&d.lines[idx], segs[r], d.cursor.col, d.tab_width).min(text_w - 1);
                cursor_xy = Some((app.layout.text_x + x as u16, area.y + row as u16));
            }
            lines.push(Line::from(spans));
            v = d.next_vrow(cur, text_w);
        }
        f.render_widget(Paragraph::new(lines).style(theme.base()), area);
        if let (true, Some(xy)) = (focused, cursor_xy) {
            f.set_cursor_position(xy);
        }
        return;
    }

    if follow {
        ensure_visible(d, height, text_w);
    }
    for row in 0..height {
        let idx = d.scroll_line + row;
        if idx >= d.line_count() {
            let mut spans = Vec::new();
            if gutter_w > 0 {
                spans.push(Span::styled(format!("{:>w$} ", "~", w = digits), theme.gutter));
            }
            lines.push(Line::from(spans));
            continue;
        }
        let is_cur = idx == d.cursor.line;
        let row_bg = if is_cur && focused { Some(theme.current_line_bg) } else { None };
        let mut spans = gutter(Some(idx), is_cur);
        let kinds = line_kinds(d, idx);
        let len = d.line_len(idx);
        spans.extend(painter.row(idx, &d.lines[idx], &kinds, (0, len), d.scroll_col, text_w, row_bg, true));
        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines).style(theme.base()), area);

    if focused {
        let dc = display_col(&d.lines[d.cursor.line], d.cursor.col, d.tab_width);
        let visible_row = d.cursor.line >= d.scroll_line && d.cursor.line < d.scroll_line + height;
        let visible_col = dc >= d.scroll_col && dc < d.scroll_col + text_w;
        if visible_row && visible_col {
            let cx = app.layout.text_x + (dc - d.scroll_col) as u16;
            let cy = area.y + (d.cursor.line - d.scroll_line) as u16;
            f.set_cursor_position((cx, cy));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_match_across_lines() {
        let mut d = Document::from_text("a.c".into(), "f() {\n  (x)\n}", None);
        d.cursor = Pos::new(0, 4);
        assert_eq!(matching_bracket(&d), Some((Pos::new(0, 4), Pos::new(2, 0))));
        d.cursor = Pos::new(2, 1);
        assert_eq!(matching_bracket(&d), Some((Pos::new(2, 0), Pos::new(0, 4))));
    }

    #[test]
    fn search_ranges() {
        assert_eq!(search_matches("Foo foo", "foo"), vec![(0, 3), (4, 7)]);
        assert_eq!(search_matches("Foo foo", "Foo"), vec![(0, 3)]);
    }

    #[test]
    fn scrolling_follows_cursor() {
        let text = (0..100).map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
        let mut d = Document::from_text("a.txt".into(), &text, None);
        d.cursor = Pos::new(50, 0);
        ensure_visible(&mut d, 20, 40);
        assert!(d.scroll_line <= 50 && 50 < d.scroll_line + 20);
        d.lines[0] = "x".repeat(200);
        d.cursor = Pos::new(0, 150);
        ensure_visible(&mut d, 20, 40);
        assert!(d.scroll_col <= 150 && 150 < d.scroll_col + 40);
    }
}
