//! IRC-style topic bar, status bar and input line.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::{str_width, truncate, truncate_left};
use crate::app::{App, Focus, Level};

/// Pads `spans` with `style` to exactly `width` cells.
fn fill_line(mut spans: Vec<Span<'static>>, width: usize, style: Style) -> Line<'static> {
    let used: usize = spans.iter().map(|s| str_width(&s.content)).sum();
    if used < width {
        spans.push(Span::styled(" ".repeat(width - used), style));
    }
    Line::from(spans)
}

/// Top line: like an IRC channel topic.
pub fn topic(f: &mut Frame, app: &App, area: Rect) {
    let t = app.theme();
    let d = app.doc();
    let width = area.width as usize;
    let right = " F1 help · F2 menu · ^O open · ^S save ";
    let right = if width > 70 { right } else if width > 40 { " F2 menu " } else { "" };
    let where_ = d.path.as_ref().map_or_else(|| format!("{} (unsaved)", d.name), |p| p.display().to_string());
    let left_budget = width.saturating_sub(str_width(right) + 12);
    let spans = vec![
        Span::styled(" text_killer ", t.bar_accent),
        Span::styled("│ ", t.bar_bracket),
        Span::styled(truncate_left(&where_, left_budget), t.bar),
    ];
    let used: usize = spans.iter().map(|s| str_width(&s.content)).sum();
    let mut spans = spans;
    if used + str_width(right) <= width {
        spans.push(Span::styled(" ".repeat(width - used - str_width(right)), t.bar));
        spans.push(Span::styled(right.to_owned(), t.bar_bracket));
    }
    f.render_widget(Paragraph::new(fill_line(spans, width, t.bar)), area);
}

/// Status bar: `[name(+)] [Python] [Ln 1/20 Col 3] [UTF-8 LF sp4] [INS]`.
pub fn status(f: &mut Frame, app: &App, area: Rect) {
    let t = app.theme();
    let d = app.doc();
    let width = area.width as usize;

    // (priority, text, accent) — lower priority segments are dropped first.
    let mut segs: Vec<(u8, String, bool)> = Vec::new();
    let flags = format!(
        "{}{}",
        if d.is_dirty() { "+" } else { "" },
        if d.readonly { "ro" } else { "" }
    );
    let name = if flags.is_empty() {
        format!("{}:{}", app.active + 1, d.name)
    } else {
        format!("{}:{}({flags})", app.active + 1, d.name)
    };
    segs.push((9, truncate(&name, width / 2), true));
    segs.push((8, format!("Ln {}/{} Col {}", d.cursor.line + 1, d.line_count(), d.cursor.col + 1), false));
    segs.push((5, d.language.name.to_owned(), false));
    if let Some(pi) = app.layout.page {
        let last = pi.first + pi.shown;
        let pages = if pi.shown > 1 { format!("Pages {}-{last}", pi.first + 1) } else { format!("Page {}", pi.first + 1) };
        let fill = if app.settings.page_scale == crate::config::PageScale::Fill { ", fill" } else { "" };
        segs.push((8, format!("{pages} of {}{fill}", pi.total), true));
    } else if app.layout.page_fallback {
        segs.push((1, "pages: window too narrow".into(), false));
    }
    if let Some((s, e)) = d.selection() {
        let chars = d.text_range(s, e).chars().count();
        segs.push((6, format!("sel {chars}"), false));
    }
    let indent = if d.use_tabs { format!("tab{}", d.tab_width) } else { format!("sp{}", d.tab_width) };
    match d.format {
        // Converted documents show their format instead of encoding details.
        Some(f) => segs.push((3, f.to_owned(), false)),
        None => segs.push((3, format!("{} {} {indent}", d.encoding, d.line_ending), false)),
    }
    segs.push((7, if d.overwrite { "OVR".into() } else { "INS".into() }, d.overwrite));
    let mode = match app.focus {
        Focus::Editor => "edit",
        Focus::Sidebar => "windows",
        Focus::Command => "cmd",
    };
    segs.push((4, mode.into(), false));
    let dirty_others = app.docs.iter().enumerate().filter(|(i, x)| *i != app.active && x.is_dirty()).count();
    if dirty_others > 0 {
        // Like IRC's "Act:" indicator for channels with activity.
        let acts: Vec<String> = app
            .docs
            .iter()
            .enumerate()
            .filter(|(i, x)| *i != app.active && x.is_dirty())
            .map(|(i, _)| (i + 1).to_string())
            .collect();
        segs.push((2, format!("Act: {}", acts.join(",")), true));
    }

    let seg_w = |s: &(u8, String, bool)| str_width(&s.1) + 3;
    while segs.iter().map(seg_w).sum::<usize>() + 1 > width && segs.len() > 1 {
        let (i, _) = segs.iter().enumerate().min_by_key(|(_, s)| s.0).expect("non-empty");
        segs.remove(i);
    }

    let mut spans = vec![Span::styled(" ", t.bar)];
    for (_, text, accent) in segs {
        spans.push(Span::styled("[", t.bar_bracket));
        spans.push(Span::styled(text, if accent { t.bar_accent } else { t.bar }));
        spans.push(Span::styled("] ", t.bar_bracket));
    }
    f.render_widget(Paragraph::new(fill_line(spans, width, t.bar)), area);
}

/// Bottom line: `[#name] ` prompt followed by the command input or the
/// latest status message.
pub fn input(f: &mut Frame, app: &App, area: Rect) {
    let t = app.theme();
    let width = area.width as usize;
    let prompt = format!("[#{}] ", truncate(&app.doc().name, (width / 3).max(4)));
    let pw = str_width(&prompt);
    let mut spans = vec![Span::styled(prompt, t.prompt)];
    let field = width.saturating_sub(pw + 1);
    if app.focus == Focus::Command && app.overlay.is_none() {
        let (text, cx) = app.cmdline.view(field);
        spans.push(Span::styled(text, t.base()));
        f.set_cursor_position((area.x + (pw + cx) as u16, area.y));
    } else if let Some((level, msg, _)) = &app.status {
        let style = match level {
            Level::Info => t.info,
            Level::Warn => t.warn,
            Level::Error => t.error,
        };
        spans.push(Span::styled(truncate(msg, field), style));
    } else {
        spans.push(Span::styled(truncate("^L or type /help for commands", field), t.gutter));
    }
    f.render_widget(Paragraph::new(fill_line(spans, width, t.base())), area);
}
