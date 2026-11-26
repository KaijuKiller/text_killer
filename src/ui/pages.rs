//! Page view: the document laid out as fixed-size pages (65×54 characters
//! for US Letter by default) shown side by side, as many as fit.
//!
//! Text flows from the bottom of one page to the top of the next. When a
//! page is taller than the window, the whole spread scrolls vertically.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::editor::{line_kinds, matching_bracket, Painter};
use crate::app::{App, Focus, PageInfo, RowHit};
use crate::config::{PageMode, PageScale, PageSize, MAX_PAGES};
use crate::document::Pos;
use crate::syntax::Kind;
use crate::wrap;

/// Columns of space between pages.
const GAP: usize = 2;
/// Narrowest / shortest page text area allowed when filling the window.
const MIN_FILL_COLS: usize = 20;
const MIN_FILL_ROWS: usize = 5;

/// Width of a page box: text + 1 cell padding and 1 border on each side.
pub fn box_width(cols: usize) -> usize {
    cols + 4
}

/// How many pages fit in `width` columns (0 if not even one).
pub fn pages_that_fit(width: usize, cols: usize) -> usize {
    (width + GAP) / (box_width(cols) + GAP)
}

/// Decides how many pages to show and their text size (columns, rows) for a
/// `width`×`height` area. None when not even one page fits.
pub fn layout(width: usize, height: usize, mode: PageMode, scale: PageScale, size: PageSize) -> Option<(usize, usize, usize)> {
    let max = MAX_PAGES as usize;
    let (cols, rows) = (size.cols as usize, size.rows as usize);
    let fit = pages_that_fit(width, cols);
    match scale {
        PageScale::Actual => {
            if fit == 0 {
                return None;
            }
            let n = match mode {
                PageMode::Fixed(k) => (k as usize).clamp(1, fit),
                _ => fit.min(max),
            };
            Some((n, cols, rows))
        }
        PageScale::Fill => {
            // Auto keeps the number of true-size pages that fit and stretches
            // them over the leftover space; a fixed count is stretched as is.
            let mut n = match mode {
                PageMode::Fixed(k) => k as usize,
                _ => fit.max(1),
            }
            .clamp(1, max);
            let rows = height.saturating_sub(2).max(MIN_FILL_ROWS);
            loop {
                let cols = ((width + GAP) / n).saturating_sub(GAP + 4);
                if cols >= MIN_FILL_COLS {
                    return Some((n, cols, rows));
                }
                if n == 1 {
                    return None;
                }
                n -= 1;
            }
        }
    }
}

/// Renders the page view. Returns false (drawing nothing) when not even one
/// page fits, so the caller can fall back to the continuous view.
pub fn render(f: &mut Frame, app: &mut App, area: Rect) -> bool {
    let s = &app.settings;
    let Some((n, cols, rows)) = layout(area.width as usize, area.height as usize, s.page_mode, s.page_scale, s.page_size)
        .filter(|_| area.height >= 3)
    else {
        app.layout.page_fallback = true;
        return false;
    };
    let theme = app.theme();
    let search = if app.search_visible { app.search.clone() } else { None };
    let focused = app.focus == Focus::Editor && app.overlay.is_none();
    let follow = app.follow_cursor;
    let show_ws = app.settings.whitespace;
    let d = &mut app.docs[app.active];

    let pages = d.pages(cols, rows);
    let total = pages.count();
    let cv = d.vrow_of(d.cursor, cols);
    let cp = pages.page_of(cv);
    let cr = d.rows_between(pages.starts[cp], cv, rows, cols).unwrap_or(0);

    let box_w = box_width(cols);
    let box_h = rows + 2;
    let view_h = area.height as usize;
    let max_scroll = box_h.saturating_sub(view_h);
    if follow {
        if cp < d.page_first {
            d.page_first = cp;
        } else if cp >= d.page_first + n {
            d.page_first = cp + 1 - n;
        }
        // Keep the cursor row (plus a little context) inside the window.
        let y = cr + 1;
        let margin = if view_h > 12 { 3 } else { 0 };
        if y < d.page_scroll + margin {
            d.page_scroll = y.saturating_sub(margin);
        } else if y + margin >= d.page_scroll + view_h {
            d.page_scroll = y + margin + 1 - view_h;
        }
    }
    d.page_first = d.page_first.min(total.saturating_sub(n)).min(total - 1);
    d.page_scroll = d.page_scroll.min(max_scroll);
    let (first, scroll) = (d.page_first, d.page_scroll);

    let painter = Painter {
        theme,
        sel: d.selection(),
        brackets: matching_bracket(d),
        show_ws,
        search: search.as_deref(),
        tab: d.tab_width,
    };
    let spread_w = n * box_w + (n - 1) * GAP;
    let x0 = area.x as usize + (area.width as usize - spread_w) / 2;
    let paper = theme.base();
    let mut kinds: (usize, Vec<Kind>) = (usize::MAX, Vec::new());
    let mut cursor_xy = None;

    for i in 0..n {
        let page = first + i;
        if page >= total {
            break;
        }
        let px = x0 + i * (box_w + GAP);
        let border = if page == cp { theme.prompt.bg(theme.bg) } else { theme.border.bg(theme.bg) };
        let mut lines: Vec<Line> = Vec::with_capacity(box_h);
        lines.push(Line::from(Span::styled(format!("┌{}┐", "─".repeat(cols + 2)), border)));

        let end = pages.starts.get(page + 1).copied();
        let mut v = Some(pages.starts[page]);
        for r in 0..rows {
            let mut spans = vec![Span::styled("│", border), Span::styled(" ", paper)];
            match v.filter(|cur| Some(*cur) != end) {
                Some(cur) => {
                    let (idx, row) = cur;
                    let segs = d.segs(idx, cols);
                    let seg = segs[row];
                    let last = row + 1 == segs.len();
                    if wrap::is_page_break(&d.lines[idx]) {
                        let p0 = Pos::new(idx, 0);
                        let sel = painter.sel.is_some_and(|(s, e)| p0 >= s && p0 < e);
                        spans.extend(painter.page_break(cols, sel));
                    } else {
                        if kinds.0 != idx {
                            kinds = (idx, line_kinds(d, idx));
                        }
                        let bg = (cur == cv && focused).then_some(theme.current_line_bg);
                        spans.extend(painter.row(idx, &d.lines[idx], &kinds.1, seg, 0, cols, bg, last));
                    }
                    // Screen row of this page row, if visible.
                    let sy = (r + 1).checked_sub(scroll).filter(|&y| y < view_h);
                    if let Some(sy) = sy {
                        let y = area.y + sy as u16;
                        app.layout.rows.push(RowHit { x: (px + 2) as u16, y, width: cols as u16, line: idx, seg, last });
                        if cur == cv {
                            let x = wrap::x_in_seg(&d.lines[idx], seg, d.cursor.col, d.tab_width).min(cols - 1);
                            cursor_xy = Some(((px + 2 + x) as u16, y));
                        }
                    }
                    v = d.next_vrow(cur, cols);
                }
                None => spans.push(Span::styled(" ".repeat(cols), paper)),
            }
            spans.push(Span::styled(" ", paper));
            spans.push(Span::styled("│", border));
            lines.push(Line::from(spans));
        }
        let label = format!(" {} / {} ", page + 1, total);
        let dashes = (cols + 2).saturating_sub(label.chars().count() + 1);
        lines.push(Line::from(vec![
            Span::styled(format!("└{}", "─".repeat(dashes)), border),
            Span::styled(label, if page == cp { theme.prompt.bg(theme.bg) } else { theme.gutter.bg(theme.bg) }),
            Span::styled("─┘", border),
        ]));

        let visible: Vec<Line> = lines.into_iter().skip(scroll).take(view_h).collect();
        let h = visible.len() as u16;
        f.render_widget(Paragraph::new(visible).style(Style::new().bg(theme.bg)), Rect::new(px as u16, area.y, box_w as u16, h));
    }

    if let (true, Some(xy)) = (focused, cursor_xy) {
        f.set_cursor_position(xy);
    }
    app.layout.editor = area;
    app.layout.wrap_width = Some(cols);
    app.layout.page = Some(PageInfo { first, shown: n.min(total - first), total, rows });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting() {
        // Letter pages are 69 cells wide with a 2-cell gap.
        assert_eq!(pages_that_fit(68, 65), 0);
        assert_eq!(pages_that_fit(69, 65), 1);
        assert_eq!(pages_that_fit(140, 65), 2);
        assert_eq!(pages_that_fit(300, 65), 4);
    }

    #[test]
    fn layouts() {
        let letter = PageSize::LETTER;
        let (auto, fixed) = (PageMode::Auto, PageMode::Fixed(3));
        // Actual size: Auto is no longer capped at 4 pages.
        assert_eq!(layout(500, 60, auto, PageScale::Actual, letter), Some((7, 65, 54)));
        assert_eq!(layout(1000, 60, auto, PageScale::Actual, letter), Some((12, 65, 54)));
        assert_eq!(layout(300, 60, fixed, PageScale::Actual, letter), Some((3, 65, 54)));
        assert_eq!(layout(60, 60, auto, PageScale::Actual, letter), None);
        // Fill: the spread uses the whole width and height.
        let (n, cols, rows) = layout(330, 50, auto, PageScale::Fill, letter).unwrap();
        assert_eq!((n, rows), (4, 48));
        assert!(n * box_width(cols) + (n - 1) * GAP <= 330);
        assert!((n * box_width(cols + 1) + (n - 1) * GAP) > 330, "fills the width");
        assert_eq!(layout(300, 40, fixed, PageScale::Fill, letter).map(|l| l.0), Some(3));
        // A fixed count too large for the width shrinks to what is readable.
        assert_eq!(layout(100, 40, PageMode::Fixed(8), PageScale::Fill, letter).map(|l| l.0), Some(3));
        // Fill works even where an actual-size page would not fit.
        assert_eq!(layout(50, 30, auto, PageScale::Fill, letter), Some((1, 46, 28)));
    }
}
