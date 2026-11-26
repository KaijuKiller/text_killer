//! Modal overlays: options menu, file browser, confirmations, help and log.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::{centered, str_width, truncate, truncate_left};
use crate::app::{App, Level, Overlay};
use crate::browser::{human_size, BrowserMode};
use crate::theme::Theme;

pub const HELP: &str = "\
text_killer — a terminal text editor with an IRC-style interface

FILES & WINDOWS
  ^N new          ^O open (file browser)   ^S save      F12 / ^Alt-S save as
  ^W close tab    ^Q quit                  F2 / ^P options menu
  Alt-1..9 jump to window   Alt-←/→ or ^PgUp/^PgDn cycle windows
  F6 focus window list (↑↓ Enter, Del closes, n new, o open)

EDITING
  ^Z undo   ^Y / ^Shift-Z redo   ^X cut (line if no selection)   ^C copy   ^V paste
  ^A select all   Shift+move select   ^Space select word   double-click word
  ^← ^→ word   Home smart-home   ^Home ^End document   PgUp PgDn page
  ^Backspace / ^Del delete word   Ins overwrite   Tab / Shift-Tab indent
  ^D duplicate   ^K delete line   ^J join lines   Alt-↑ Alt-↓ move lines
  ^Up ^Down scroll view

FORMATTING
  ^B bold   ^T italic   ^U underline   ^E inline code   Alt-H heading
  ^/ toggle comment   Alt-U upper   Alt-L lower   Alt-T title case
  Alt-Q reflow paragraph   (HTML uses tags, Markdown/text uses ** * ` ~~)
  Alt-Enter insert page break (kept as real page breaks in .odt/.docx/.rtf)
  More in Options › Format and Options › Lines (sort, trim, JSON, tabs…)

SEARCH
  ^F find (incremental)   F3 / Shift-F3 next / previous   ^R replace all
  ^G go to line           Esc clears selection, then search highlight

VIEW
  F4 line numbers  F5 window list  F7 whitespace  F8 next theme  F9 mouse
  Options › View › Theme picks a theme (Crimson is the default)

PAGE VIEW (on by default: Options › View › Page view)
  Documents are shown as US Letter pages (65 x 54 characters) side by side,
  as many as fit (Auto, up to 12) or a fixed 1-8. Page scaling \"Fill window\"
  stretches the pages to use the whole window. Off restores the original
  continuous view; \"Wrap long lines\" wraps text in that view.
  ↑↓ move by screen row  PgUp/PgDn one page  Alt-PgUp/PgDn a whole spread
  Home/End go to the row start/end first, then the line start/end
  Mouse wheel scrolls the spread, then turns pages. Page size: A4, Legal...

SETTINGS (Options › Settings)
  Remember theme / Remember page view & layout across sessions; Reset.

COMMAND LINE (^L)  — Tab completes commands and paths, ↑↓ history
  /open <file…>  /join   /save [file]  /saveas <file>  /saveall  /wq
  /new [file]    /close  /part  /quit  /q!  /reload  /cd <dir>  /pwd
  /find <text>   /replace <find> <with>  (use \"quotes\" for spaces)
  /goto <line[:col]>  (+N / -N relative)   bare numbers jump, bare text finds
  /lang <name>   /tabsize <n>  /tabs  /spaces  /eol lf|crlf|cr
  /encoding utf-8|utf-8-bom|utf-16le|utf-16be|latin1
  /theme [crimson|midnight|matrix|amber|ocean|frost|solar|irssi|weechat|
          mirc|paper]   /win <n|name>  /next  /prev
  /tag <name> [attrs]  wrap selection in an HTML tag
  /upper /lower /title /sort [desc] /uniq /trim /comment /bold /italic
  /json [min]  /reflow [width]  /log  /clear  /menu  /help
  /pages off|auto|1-12  /pagescale actual|fill
  /pagesize letter|a4|legal|a5|wide|<cols>x<lines>
  /wrap [on|off]  /pagebreak  /remember theme|layout|all|none  /settings

FILE BROWSER
  type to filter (open) or edit the name (save)   ↑↓ PgUp PgDn select
  Enter open/enter dir   Backspace/← parent   Tab complete   ^H hidden files
  typing a path (with / or ~) and Enter jumps there

FORMATS
  Opens any text file: UTF-8 (± BOM), UTF-16 LE/BE, Latin-1; LF/CRLF/CR are
  detected and preserved on save. Binary files open read-only as a hex dump.

  Documents are converted to editable text (# headings, - bullets, **bold**,
  *italic*):
    Open and save:  .odt (OpenDocument)  .docx (Word)  .rtf  .gz
    Open only:      .ods .odp .fodt/.fods/.fodp  .xlsx .pptx  .epub
  Save As picks the format from the extension, e.g. notes.md -> notes.odt.
  Overwriting an original rich document asks first, because images, tables,
  fonts and layout are not kept.

  Highlighting for 93 languages, including Python, HTML, CSS, JavaScript,
  TypeScript, JSON, Markdown, XML, Rust, C/C++, C#, Java, Kotlin, Go, Swift,
  PHP, Ruby, Shell, PowerShell, SQL, YAML, TOML, Zig, Dart, Scala, Elixir,
  Erlang, Haskell, OCaml, F#, Julia, Nim, Lua, R, Fortran, Pascal, Ada, COBOL,
  Assembly, Verilog, VHDL, GLSL/HLSL, Terraform, Nix, GraphQL, Protobuf,
  CMake, LaTeX, man pages and more (Options › Document › Language).

LICENSES
  text_killer is MIT-licensed. Run  text_killer --licenses  to see its license
  and the notices of the open-source components it includes.

Press any other key to close.";

fn popup_block<'a>(t: &Theme, title: &str) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(t.popup_border)
        .title(Line::from(Span::styled(format!(" {title} "), t.popup_title)))
        .style(t.popup)
}

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme();
    let mut list_rect = Rect::default();
    match &mut app.overlay {
        None => {}
        Some(Overlay::Menu(m)) => {
            let label_w = m.items.iter().map(|i| str_width(&i.label) + str_width(i.hint) + 4).max().unwrap_or(10);
            let crumb = m.breadcrumb();
            const FOOTER: &str = " ⏎ select · ← back · Esc close ";
            let w = label_w.max(str_width(&crumb) + 4).max(str_width(FOOTER) + 2) as u16 + 2;
            let h = m.items.len() as u16 + 2;
            let rect = centered(area, w, h);
            f.render_widget(Clear, rect);
            let block = popup_block(t, &crumb)
                .title_bottom(Line::from(Span::styled(FOOTER, t.popup_dim)));
            let inner = block.inner(rect);
            f.render_widget(block, rect);
            let rows = inner.height as usize;
            let sel = m.selected();
            if sel < m.scroll {
                m.scroll = sel;
            } else if sel >= m.scroll + rows {
                m.scroll = sel + 1 - rows;
            }
            let iw = inner.width as usize;
            let lines: Vec<Line> = m
                .items
                .iter()
                .enumerate()
                .skip(m.scroll)
                .take(rows)
                .map(|(i, it)| {
                    let style = if i == sel { t.popup_selected } else { t.popup };
                    let hint_style = if i == sel { t.popup_selected } else { t.popup_dim };
                    let label = truncate(&it.label, iw.saturating_sub(str_width(it.hint) + 3));
                    let gap = iw.saturating_sub(str_width(&label) + str_width(it.hint) + 2);
                    Line::from(vec![
                        Span::styled(format!(" {label}"), style),
                        Span::styled(" ".repeat(gap), style),
                        Span::styled(format!("{} ", it.hint), hint_style),
                    ])
                })
                .collect();
            f.render_widget(Paragraph::new(lines), inner);
            list_rect = inner;
        }
        Some(Overlay::Browser(b)) => {
            let w = (area.width * 4 / 5).clamp(30.min(area.width), 110);
            let h = (area.height * 4 / 5).clamp(8.min(area.height), 40);
            let rect = centered(area, w, h);
            f.render_widget(Clear, rect);
            let title = if b.mode == BrowserMode::Open { "Open file" } else { "Save as" };
            let hint = if b.mode == BrowserMode::Open {
                " ⏎ open · ⌫ up · Tab complete · ^H hidden · Esc "
            } else {
                " ⏎ save · ↑↓ pick · ⌫ up (empty name) · ^H hidden · Esc "
            };
            let block = popup_block(t, title).title_bottom(Line::from(Span::styled(hint, t.popup_dim)));
            let inner = block.inner(rect);
            f.render_widget(block, rect);
            if inner.height < 4 || inner.width < 10 {
                return;
            }
            let iw = inner.width as usize;
            // Row 0: directory.
            let dir = b.dir.display().to_string();
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(" in ", t.popup_dim),
                    Span::styled(truncate_left(&dir, iw.saturating_sub(5)), t.popup_title),
                ])),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            // Last rows: input + error/info.
            let list_h = inner.height - 3;
            let list = Rect::new(inner.x, inner.y + 1, inner.width, list_h);
            b.fix_scroll(list_h as usize);
            let size_w = 8;
            let lines: Vec<Line> = b
                .entries()
                .enumerate()
                .skip(b.scroll)
                .take(list_h as usize)
                .map(|(i, e)| {
                    let sel = i == b.selected;
                    let base = if sel { t.popup_selected } else { t.popup };
                    let name_style = if sel {
                        t.popup_selected
                    } else if e.is_dir {
                        t.popup_title
                    } else {
                        t.popup
                    };
                    let suffix = if e.is_dir && e.name != ".." { "/" } else if e.is_link { "@" } else { "" };
                    let size = if e.is_dir { String::new() } else { human_size(e.size) };
                    let name = truncate(&format!("{}{suffix}", e.name), iw.saturating_sub(size_w + 3));
                    let gap = iw.saturating_sub(str_width(&name) + 1 + size.len() + 1);
                    Line::from(vec![
                        Span::styled(" ", base),
                        Span::styled(name, name_style),
                        Span::styled(" ".repeat(gap), base),
                        Span::styled(format!("{size} "), if sel { base } else { t.popup_dim }),
                    ])
                })
                .collect();
            let empty = lines.is_empty();
            f.render_widget(Paragraph::new(lines), list);
            if empty {
                f.render_widget(
                    Paragraph::new(Span::styled("  (no matching entries — Enter creates it)", t.popup_dim)),
                    Rect::new(list.x, list.y + 1.min(list.height.saturating_sub(1)), list.width, 1),
                );
            }
            list_rect = list;

            let label = if b.mode == BrowserMode::Open { " filter: " } else { " name: " };
            let field_y = inner.y + inner.height - 2;
            let lw = str_width(label);
            let (text, cx) = b.input.view(iw.saturating_sub(lw + 1));
            f.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(label, t.popup_title), Span::styled(text, t.popup)])),
                Rect::new(inner.x, field_y, inner.width, 1),
            );
            f.set_cursor_position((inner.x + (lw + cx) as u16, field_y));
            let info = match &b.error {
                Some(e) => Span::styled(truncate(&format!(" {e}"), iw), t.error.bg(t.popup.bg.unwrap_or_default())),
                None => {
                    let hidden = if b.show_hidden { "showing hidden" } else { "hidden files off" };
                    Span::styled(format!(" {} entries · {hidden}", b.visible.len()), t.popup_dim)
                }
            };
            f.render_widget(Paragraph::new(Line::from(info)), Rect::new(inner.x, field_y + 1, inner.width, 1));
        }
        Some(Overlay::Confirm(c)) => {
            let w = (str_width(&c.message) as u16 + 6).clamp(30, 70).min(area.width);
            let text_w = w.saturating_sub(4).max(1) as usize;
            let msg_lines = str_width(&c.message).div_ceil(text_w).max(1) as u16;
            let rect = centered(area, w, msg_lines + 4);
            f.render_widget(Clear, rect);
            let block = popup_block(t, c.title);
            let inner = block.inner(rect);
            f.render_widget(block, rect);
            let choices = if c.three_way { "[Y]es  [N]o  [C]ancel" } else { "[Y]es  [N]o" };
            let lines = vec![
                Line::from(Span::styled(c.message.clone(), t.popup)),
                Line::from(""),
                Line::from(Span::styled(choices, t.popup_title)).centered(),
            ];
            f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner.inner(ratatui::layout::Margin::new(1, 0)));
        }
        Some(Overlay::Help { scroll }) => {
            let lines: Vec<Line> = HELP.lines().map(|l| styled_help_line(t, l)).collect();
            render_scroller(f, t, area, "Help", lines, scroll);
        }
        Some(Overlay::Log { scroll }) => {
            let lines: Vec<Line> = app
                .log
                .iter()
                .map(|(lvl, m)| {
                    let st = match lvl {
                        Level::Info => t.popup,
                        Level::Warn => t.warn.bg(t.popup.bg.unwrap_or_default()),
                        Level::Error => t.error.bg(t.popup.bg.unwrap_or_default()),
                    };
                    Line::from(Span::styled(m.clone(), st))
                })
                .collect();
            render_scroller(f, t, area, "Message log", lines, scroll);
        }
    }
    app.layout.popup_list = list_rect;
}

fn styled_help_line<'a>(t: &Theme, l: &'a str) -> Line<'a> {
    if !l.starts_with(' ') && !l.is_empty() && l.chars().all(|c| c.is_uppercase() || c.is_whitespace() || "&()^L—".contains(c)) {
        Line::from(Span::styled(l, t.popup_title))
    } else {
        Line::from(Span::styled(l, t.popup))
    }
}

fn render_scroller(f: &mut Frame, t: &Theme, area: Rect, title: &str, lines: Vec<Line>, scroll: &mut usize) {
    let w = area.width.saturating_sub(4).min(90);
    let h = area.height.saturating_sub(2).min(lines.len() as u16 + 2).max(5.min(area.height));
    let rect = centered(area, w, h);
    f.render_widget(Clear, rect);
    let block = popup_block(t, title)
        .title_bottom(Line::from(Span::styled(" ↑↓ PgUp PgDn scroll · any key closes ", t.popup_dim)));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let max = lines.len().saturating_sub(inner.height as usize);
    *scroll = (*scroll).min(max);
    let visible: Vec<Line> = lines.into_iter().skip(*scroll).take(inner.height as usize).collect();
    f.render_widget(Paragraph::new(visible), inner);
}
