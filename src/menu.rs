//! The options menu: a nested list of actions.

use crate::config::{PageMode, PageScale, PageSize, PAGE_SIZES};
use crate::fileio::{Encoding, LineEnding};
use crate::format::FormatOp;
use crate::syntax::LANGUAGES;
use crate::theme::THEMES;

/// Current settings, used to mark the active choice in radio-style submenus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuCtx {
    pub theme: usize,
    pub language: usize,
    pub line_ending: LineEnding,
    pub encoding: Encoding,
    pub tab_width: usize,
    pub page_mode: PageMode,
    pub page_size: PageSize,
    pub page_scale: PageScale,
    pub wrap: bool,
    pub remember_theme: bool,
    pub remember_layout: bool,
}

const ON: &str = "● ";
const OFF: &str = "  ";

fn mark(on: bool) -> &'static str {
    if on { ON } else { OFF }
}

/// Checkbox prefix for on/off toggles.
fn check(on: bool) -> &'static str {
    if on { "[x] " } else { "[ ] " }
}

/// Every user-invokable command in the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
    SaveAll,
    Reload,
    Close,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Find,
    FindNext,
    FindPrev,
    Replace,
    Goto,
    Format(FormatOp),
    ToggleLineNumbers,
    ToggleSidebar,
    ToggleWhitespace,
    ToggleMouse,
    ToggleOsc52,
    ToggleIndentStyle,
    ToggleOverwrite,
    NextTheme,
    SetTheme(usize),
    SetPageMode(PageMode),
    SetPageSize(PageSize),
    SetPageScale(PageScale),
    ToggleWrap,
    InsertPageBreak,
    ToggleRememberTheme,
    ToggleRememberLayout,
    ResetSettings,
    ShowSettingsPath,
    TabWidth(u8),
    Language(usize),
    LineEnding(LineEnding),
    Encoding(Encoding),
    NextTab,
    PrevTab,
    FocusSidebar,
    FocusCommand,
    MessageLog,
    Help,
    Submenu(SubMenu),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubMenu {
    Root,
    File,
    Edit,
    Format,
    Lines,
    View,
    Theme,
    PageView,
    PageSize,
    PageScale,
    Settings,
    Document,
    Language,
    LineEnding,
    Encoding,
    TabWidth,
}

pub struct MenuItem {
    pub label: String,
    pub hint: &'static str,
    pub action: Action,
}

fn item(label: impl Into<String>, hint: &'static str, action: Action) -> MenuItem {
    MenuItem { label: label.into(), hint, action }
}

fn sub(label: &str, s: SubMenu) -> MenuItem {
    item(label, "▸", Action::Submenu(s))
}

pub fn title(s: SubMenu) -> &'static str {
    match s {
        SubMenu::Root => "Options",
        SubMenu::File => "File",
        SubMenu::Edit => "Edit",
        SubMenu::Format => "Format",
        SubMenu::Lines => "Lines",
        SubMenu::View => "View",
        SubMenu::Theme => "Theme",
        SubMenu::PageView => "Page view",
        SubMenu::PageSize => "Page size",
        SubMenu::PageScale => "Page scaling",
        SubMenu::Settings => "Settings",
        SubMenu::Document => "Document",
        SubMenu::Language => "Language",
        SubMenu::LineEnding => "Line endings",
        SubMenu::Encoding => "Encoding",
        SubMenu::TabWidth => "Tab width",
    }
}

pub fn items(s: SubMenu, ctx: MenuCtx) -> Vec<MenuItem> {
    use Action as A;
    use FormatOp as F;
    match s {
        SubMenu::Root => vec![
            sub("File", SubMenu::File),
            sub("Edit", SubMenu::Edit),
            sub("Format", SubMenu::Format),
            sub("Lines", SubMenu::Lines),
            sub("View", SubMenu::View),
            sub("Document", SubMenu::Document),
            sub("Settings", SubMenu::Settings),
            item("Message log", "/log", A::MessageLog),
            item("Help", "F1", A::Help),
            item("Quit", "^Q", A::Quit),
        ],
        SubMenu::File => vec![
            item("New document", "^N", A::New),
            item("Open...", "^O", A::Open),
            item("Save", "^S", A::Save),
            item("Save as...", "F12", A::SaveAs),
            item("Save all", "/saveall", A::SaveAll),
            item("Reload from disk", "/reload", A::Reload),
            item("Close tab", "^W", A::Close),
            item("Quit", "^Q", A::Quit),
        ],
        SubMenu::Edit => vec![
            item("Undo", "^Z", A::Undo),
            item("Redo", "^Y", A::Redo),
            item("Cut", "^X", A::Cut),
            item("Copy", "^C", A::Copy),
            item("Paste", "^V", A::Paste),
            item("Select all", "^A", A::SelectAll),
            item("Find...", "^F", A::Find),
            item("Find next", "F3", A::FindNext),
            item("Find previous", "S-F3", A::FindPrev),
            item("Replace all...", "^R", A::Replace),
            item("Go to line...", "^G", A::Goto),
            item("Toggle overwrite", "Ins", A::ToggleOverwrite),
        ],
        SubMenu::Format => vec![
            item("Bold", "^B", A::Format(F::Bold)),
            item("Italic", "^T", A::Format(F::Italic)),
            item("Underline", "^U", A::Format(F::Underline)),
            item("Strikethrough", "", A::Format(F::Strike)),
            item("Inline code", "^E", A::Format(F::Code)),
            item("Link", "", A::Format(F::Link)),
            item("Heading (cycle)", "M-h", A::Format(F::Heading)),
            item("Toggle comment", "^/", A::Format(F::ToggleComment)),
            item("UPPERCASE", "M-u", A::Format(F::Upper)),
            item("lowercase", "M-l", A::Format(F::Lower)),
            item("Title Case", "M-t", A::Format(F::Title)),
            item("Reflow paragraph", "M-q", A::Format(F::Reflow)),
            item("Insert page break", "M-Enter", A::InsertPageBreak),
            item("Pretty-print JSON", "", A::Format(F::PrettyJson)),
            item("Minify JSON", "", A::Format(F::MinifyJson)),
        ],
        SubMenu::Lines => vec![
            item("Indent", "Tab", A::Format(F::Indent)),
            item("Outdent", "S-Tab", A::Format(F::Outdent)),
            item("Duplicate line", "^D", A::Format(F::DuplicateLine)),
            item("Delete line", "^K", A::Format(F::DeleteLine)),
            item("Move line up", "M-Up", A::Format(F::MoveLineUp)),
            item("Move line down", "M-Down", A::Format(F::MoveLineDown)),
            item("Join lines", "^J", A::Format(F::JoinLines)),
            item("Sort A-Z", "", A::Format(F::SortLines)),
            item("Sort Z-A", "", A::Format(F::SortLinesDesc)),
            item("Reverse", "", A::Format(F::ReverseLines)),
            item("Remove duplicates", "", A::Format(F::UniqueLines)),
            item("Trim trailing whitespace", "", A::Format(F::TrimTrailing)),
            item("Tabs -> spaces", "", A::Format(F::TabsToSpaces)),
            item("Spaces -> tabs", "", A::Format(F::SpacesToTabs)),
        ],
        SubMenu::View => vec![
            sub("Theme", SubMenu::Theme),
            sub("Page view", SubMenu::PageView),
            sub("Page size", SubMenu::PageSize),
            sub("Page scaling", SubMenu::PageScale),
            item(format!("{}Wrap long lines (page view off)", check(ctx.wrap)), "/wrap", A::ToggleWrap),
            item("Toggle line numbers", "F4", A::ToggleLineNumbers),
            item("Toggle sidebar", "F5", A::ToggleSidebar),
            item("Toggle whitespace", "F7", A::ToggleWhitespace),
            item("Next theme", "F8", A::NextTheme),
            item("Toggle mouse capture", "F9", A::ToggleMouse),
            item("Toggle OSC52 clipboard", "", A::ToggleOsc52),
            item("Focus sidebar", "F6", A::FocusSidebar),
            item("Focus command line", "^L", A::FocusCommand),
        ],
        SubMenu::Document => vec![
            sub("Language", SubMenu::Language),
            sub("Line endings", SubMenu::LineEnding),
            sub("Encoding", SubMenu::Encoding),
            sub("Tab width", SubMenu::TabWidth),
            item("Toggle tabs/spaces", "", A::ToggleIndentStyle),
        ],
        SubMenu::PageView => {
            let mut modes = vec![
                ("Off (continuous view, the default)".to_owned(), PageMode::Off),
                ("Auto (as many pages as fit)".to_owned(), PageMode::Auto),
            ];
            for n in 1..=8u8 {
                let label = if n == 1 { "1 page".to_owned() } else { format!("{n} pages") };
                modes.push((label, PageMode::Fixed(n)));
            }
            modes
                .into_iter()
                .map(|(l, m)| item(format!("{}{l}", mark(m == ctx.page_mode)), "", A::SetPageMode(m)))
                .collect()
        }
        SubMenu::PageScale => {
            let scales = [
                ("Actual size (true pages)", PageScale::Actual),
                ("Fill window (pages stretch to fit)", PageScale::Fill),
            ];
            scales
                .into_iter()
                .map(|(l, sc)| item(format!("{}{l}", mark(sc == ctx.page_scale)), "", A::SetPageScale(sc)))
                .collect()
        }
        SubMenu::PageSize => PAGE_SIZES
            .iter()
            .map(|(_, l, size)| item(format!("{}{l}", mark(*size == ctx.page_size)), "", A::SetPageSize(*size)))
            .collect(),
        SubMenu::Settings => vec![
            item(format!("{}Remember theme across sessions", check(ctx.remember_theme)), "", A::ToggleRememberTheme),
            item(
                format!("{}Remember page view & layout across sessions", check(ctx.remember_layout)),
                "",
                A::ToggleRememberLayout,
            ),
            item("Reset settings to defaults", "", A::ResetSettings),
            item("Show settings file location", "", A::ShowSettingsPath),
        ],
        SubMenu::Theme => THEMES
            .iter()
            .enumerate()
            .map(|(i, t)| item(format!("{}{}", mark(i == ctx.theme), t.name), t.blurb, A::SetTheme(i)))
            .collect(),
        SubMenu::Language => LANGUAGES
            .iter()
            .enumerate()
            .map(|(i, l)| item(format!("{}{}", mark(i == ctx.language), l.name), "", A::Language(i)))
            .collect(),
        SubMenu::LineEnding => [
            ("LF (Unix)", LineEnding::Lf),
            ("CRLF (Windows)", LineEnding::CrLf),
            ("CR (classic Mac)", LineEnding::Cr),
        ]
        .into_iter()
        .map(|(l, e)| item(format!("{}{l}", mark(e == ctx.line_ending)), "", A::LineEnding(e)))
        .collect(),
        SubMenu::Encoding => [
            ("UTF-8", Encoding::Utf8),
            ("UTF-8 with BOM", Encoding::Utf8Bom),
            ("UTF-16 LE", Encoding::Utf16Le),
            ("UTF-16 BE", Encoding::Utf16Be),
            ("Latin-1", Encoding::Latin1),
        ]
        .into_iter()
        .map(|(l, e)| item(format!("{}{l}", mark(e == ctx.encoding)), "", A::Encoding(e)))
        .collect(),
        SubMenu::TabWidth => [2u8, 3, 4, 8]
            .iter()
            .map(|&n| item(format!("{}{n} columns", mark(n as usize == ctx.tab_width)), "", A::TabWidth(n)))
            .collect(),
    }
}

/// Navigable menu state with a submenu stack.
pub struct Menu {
    pub stack: Vec<(SubMenu, usize)>,
    pub items: Vec<MenuItem>,
    pub scroll: usize,
    ctx: MenuCtx,
}

impl Menu {
    pub fn new(ctx: MenuCtx) -> Self {
        Self { stack: vec![(SubMenu::Root, 0)], items: items(SubMenu::Root, ctx), scroll: 0, ctx }
    }

    pub fn current(&self) -> SubMenu {
        self.stack.last().map_or(SubMenu::Root, |s| s.0)
    }

    pub fn selected(&self) -> usize {
        self.stack.last().map_or(0, |s| s.1)
    }

    pub fn set_selected(&mut self, i: usize) {
        if let Some(top) = self.stack.last_mut() {
            top.1 = i.min(self.items.len().saturating_sub(1));
        }
    }

    pub fn move_sel(&mut self, delta: isize) {
        let n = self.items.len() as isize;
        if n == 0 {
            return;
        }
        let i = (self.selected() as isize + delta).rem_euclid(n) as usize;
        self.set_selected(i);
    }

    /// Opens a submenu, highlighting the currently active choice if any.
    pub fn enter(&mut self, s: SubMenu) {
        self.items = items(s, self.ctx);
        let active = self.items.iter().position(|i| i.label.starts_with(ON)).unwrap_or(0);
        self.stack.push((s, active));
        self.scroll = 0;
    }

    /// Rebuilds the items after settings changed (keeps the selection).
    pub fn refresh(&mut self, ctx: MenuCtx) {
        self.ctx = ctx;
        self.items = items(self.current(), ctx);
    }

    /// Returns false when already at the root (caller should close).
    pub fn back(&mut self) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        self.stack.pop();
        self.items = items(self.current(), self.ctx);
        self.scroll = 0;
        true
    }

    /// Selects the next item whose label starts with `c`.
    pub fn jump_to_letter(&mut self, c: char) -> bool {
        let c = c.to_ascii_lowercase();
        let n = self.items.len();
        let start = self.selected();
        for k in 1..=n {
            let i = (start + k) % n;
            let label = self.items[i].label.trim_start_matches(ON).trim_start_matches(['[', 'x', ' ', ']']).trim_start();
            if label.to_ascii_lowercase().starts_with(c) {
                self.set_selected(i);
                return true;
            }
        }
        false
    }

    pub fn breadcrumb(&self) -> String {
        self.stack.iter().map(|(s, _)| title(*s)).collect::<Vec<_>>().join(" › ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(theme: usize) -> MenuCtx {
        MenuCtx {
            theme,
            language: 0,
            line_ending: LineEnding::Lf,
            encoding: Encoding::Utf8,
            tab_width: 4,
            page_mode: PageMode::Auto,
            page_size: PageSize::LETTER,
            page_scale: PageScale::Actual,
            wrap: false,
            remember_theme: false,
            remember_layout: false,
        }
    }

    #[test]
    fn theme_submenu_marks_and_selects_current() {
        let mut m = Menu::new(ctx(2));
        m.enter(SubMenu::View);
        assert_eq!(m.items[0].action, Action::Submenu(SubMenu::Theme));
        m.enter(SubMenu::PageView);
        assert_eq!(m.selected(), 1, "the current page mode (Auto here) is highlighted");
        m.back();
        m.enter(SubMenu::Theme);
        assert_eq!(m.selected(), 2);
        assert!(m.items[2].label.starts_with(ON));
        m.refresh(ctx(0));
        assert_eq!(m.items[0].label, "● Crimson");
        m.set_selected(0);
        assert!(m.jump_to_letter('m'));
        assert_eq!(m.items[m.selected()].label.trim(), "Midnight");
    }
}
