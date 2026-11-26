//! Rendering. The layout is recomputed from the terminal size every frame,
//! so the interface adapts automatically when the window is resized.

mod bars;
mod editor;
mod pages;
mod popups;
mod sidebar;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

use crate::app::App;

/// Display width of `c` when it starts at display column `col`.
pub fn char_width(c: char, col: usize, tab_width: usize) -> usize {
    if c == '\t' {
        tab_width.max(1) - col % tab_width.max(1)
    } else if c.is_control() {
        2 // rendered as ^X
    } else {
        c.width().unwrap_or(0)
    }
}

/// Display column of char index `col` in `line`.
pub fn display_col(line: &str, col: usize, tab_width: usize) -> usize {
    let mut x = 0;
    for c in line.chars().take(col) {
        x += char_width(c, x, tab_width);
    }
    x
}

/// Char index whose cell covers display column `target` (or line end).
pub fn char_col_at_display(line: &str, target: usize, tab_width: usize) -> usize {
    let mut x = 0;
    for (i, c) in line.chars().enumerate() {
        let w = char_width(c, x, tab_width);
        if x + w > target {
            // Clicking the right half of a wide cell lands after it.
            return if w > 1 && target >= x + w.div_ceil(2) && c != '\t' { i + 1 } else { i };
        }
        x += w;
    }
    line.chars().count()
}

/// Truncates `s` to at most `width` display cells, adding an ellipsis.
pub fn truncate(s: &str, width: usize) -> String {
    let total: usize = s.chars().map(|c| c.width().unwrap_or(0)).sum();
    if total <= width {
        return s.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw + 1 > width {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

/// Truncates from the left (for paths), keeping the end visible.
pub fn truncate_left(s: &str, width: usize) -> String {
    let total: usize = s.chars().map(|c| c.width().unwrap_or(0)).sum();
    if total <= width {
        return s.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut rev = Vec::new();
    let mut w = 0;
    for c in s.chars().rev() {
        let cw = c.width().unwrap_or(0);
        if w + cw + 1 > width {
            break;
        }
        rev.push(c);
        w += cw;
    }
    std::iter::once('…').chain(rev.into_iter().rev()).collect()
}

pub fn str_width(s: &str) -> usize {
    s.chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// A rectangle of at most `w`×`h` centred in `area`.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let theme = app.theme();
    f.render_widget(Block::new().style(theme.base()), area);

    if area.width < 20 || area.height < 3 {
        f.render_widget(Paragraph::new(Line::from("window too small")).style(theme.warn), area);
        return;
    }

    // Drop the topic bar on very short terminals.
    let topic_h = u16::from(area.height >= 8);
    let [topic, body, status, input] = Layout::vertical([
        Constraint::Length(topic_h),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // The sidebar scales with width and hides itself on narrow terminals.
    let sidebar_w = if app.settings.sidebar && area.width >= 50 { (area.width / 6).clamp(14, 28) } else { 0 };
    let [side, main] = Layout::horizontal([Constraint::Length(sidebar_w), Constraint::Min(1)]).areas(body);

    app.layout.sidebar = side;
    app.layout.cmd = input;

    if topic_h > 0 {
        bars::topic(f, app, topic);
    }
    if sidebar_w > 0 {
        sidebar::render(f, app, side);
    } else {
        app.layout.sidebar_rows.clear();
    }
    // Page view when enabled and at least one page fits; otherwise the
    // original continuous view (optionally soft-wrapped).
    app.layout.rows.clear();
    app.layout.wrap_width = None;
    app.layout.page = None;
    app.layout.page_fallback = false;
    let paged = app.settings.page_mode != crate::config::PageMode::Off && pages::render(f, app, main);
    if !paged {
        editor::render(f, app, main);
    }
    bars::status(f, app, status);
    bars::input(f, app, input);
    popups::render(f, app, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_with_tabs_and_wide_chars() {
        assert_eq!(display_col("\tab", 1, 4), 4);
        assert_eq!(display_col("a\tb", 2, 4), 4);
        assert_eq!(display_col("日本", 2, 4), 4);
        assert_eq!(char_col_at_display("日本語", 2, 4), 1);
        assert_eq!(char_col_at_display("\tx", 2, 4), 0);
        assert_eq!(char_col_at_display("\tx", 4, 4), 1);
        assert_eq!(char_col_at_display("abc", 99, 4), 3);
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 6), "hello…");
        assert_eq!(truncate_left("/a/b/c/file.txt", 9), "…file.txt");
    }
}
