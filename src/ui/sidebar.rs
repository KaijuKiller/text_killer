//! Left-hand document list, styled like an IRC client's window/channel list.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::{str_width, truncate};
use crate::app::{App, Focus};

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme();
    let focused = app.focus == Focus::Sidebar && app.overlay.is_none();
    let title_style = if focused { t.sidebar_focus } else { t.prompt };
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(t.border)
        .title(Line::from(Span::styled(" windows ", title_style)))
        .style(t.sidebar.bg(t.bg));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let width = inner.width as usize;
    let rows = inner.height as usize;
    let n = app.docs.len();
    // Reserve the last row for a hint when there is space.
    let list_rows = if rows > n + 2 { rows - 1 } else { rows };
    let focus_idx = if focused { app.sidebar_sel } else { app.active };
    let offset = if focus_idx >= list_rows { focus_idx + 1 - list_rows } else { 0 };

    app.layout.sidebar_rows.clear();
    let mut lines = Vec::new();
    for (i, d) in app.docs.iter().enumerate().skip(offset).take(list_rows) {
        let num = format!("{:>2}.", i + 1);
        let marker = if d.is_dirty() { "+" } else if d.readonly { "%" } else { "#" };
        let name_w = width.saturating_sub(str_width(&num) + 1);
        let name = truncate(&d.name, name_w);
        let row_style = if focused && i == app.sidebar_sel {
            t.sidebar_focus
        } else if i == app.active {
            t.sidebar_active
        } else {
            t.sidebar.bg(t.bg)
        };
        let marker_style = if d.is_dirty() && row_style == t.sidebar.bg(t.bg) { t.sidebar_dirty.bg(t.bg) } else { row_style };
        let num_style = if row_style == t.sidebar.bg(t.bg) { t.sidebar_num.bg(t.bg) } else { row_style };
        let used = str_width(&num) + 1 + str_width(&name);
        lines.push(Line::from(vec![
            Span::styled(num, num_style),
            Span::styled(marker, marker_style),
            Span::styled(name, row_style),
            Span::styled(" ".repeat(width.saturating_sub(used)), row_style),
        ]));
        app.layout.sidebar_rows.push((inner.y + (i - offset) as u16, i));
    }
    while lines.len() < list_rows {
        lines.push(Line::from(""));
    }
    if list_rows < rows {
        let hint = if focused { "↑↓ ⏎ del n o" } else { "F6 · M-1..9" };
        lines.push(Line::from(Span::styled(truncate(hint, width), t.sidebar_num.bg(t.bg))));
    }
    f.render_widget(Paragraph::new(lines).style(t.sidebar.bg(t.bg)), inner);
}
