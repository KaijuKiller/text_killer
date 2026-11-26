//! Application state, key/mouse dispatch and command execution.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::browser::{BrowserAction, BrowserMode, FileBrowser};
use crate::clipboard::Clipboard;
use crate::docformats;
use crate::document::{Document, Pos};
use crate::fileio::{self, Encoding, LineEnding};
use crate::format::{self, FormatOp};
use crate::input::{InputResult, LineInput};
use crate::config::{PageMode, PageScale, PageSize};
use crate::menu::{Action, Menu, MenuCtx, SubMenu};
use crate::syntax::{self, LANGUAGES};
use crate::theme::{Theme, THEMES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Editor,
    Sidebar,
    Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

pub enum ConfirmKind {
    CloseTab(usize),
    Quit,
    Overwrite { path: PathBuf, target: usize, close_after: bool },
    Reload(usize),
    LossySave { idx: usize, close_after: bool },
}

pub struct Confirm {
    pub title: &'static str,
    pub message: String,
    pub kind: ConfirmKind,
    /// Whether the dialog offers a "no" choice distinct from cancel.
    pub three_way: bool,
}

pub enum Overlay {
    Menu(Menu),
    Browser(FileBrowser),
    Confirm(Confirm),
    Help { scroll: usize },
    Log { scroll: usize },
}

pub use crate::config::Settings;

/// One visual row drawn by the wrapped or page view, for mouse hit-testing.
#[derive(Clone, Copy, Debug)]
pub struct RowHit {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub line: usize,
    pub seg: crate::wrap::Seg,
    pub last: bool,
}

/// What the page view showed in the last frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct PageInfo {
    pub first: usize,
    pub shown: usize,
    pub total: usize,
    pub rows: usize,
}

/// Screen regions from the last render, used for mouse hit-testing and
/// page-size calculations.
#[derive(Default, Clone)]
pub struct LayoutCache {
    pub editor: Rect,
    pub text_x: u16,
    pub sidebar: Rect,
    /// (screen row, document index)
    pub sidebar_rows: Vec<(u16, usize)>,
    pub popup_list: Rect,
    pub cmd: Rect,
    /// Wrap width when the last frame used soft wrapping (wrap or page view).
    pub wrap_width: Option<usize>,
    /// Set when the last frame showed the page view.
    pub page: Option<PageInfo>,
    pub rows: Vec<RowHit>,
    /// Shown in the status bar when page view is on but does not fit.
    pub page_fallback: bool,
}

pub struct App {
    pub docs: Vec<Document>,
    pub active: usize,
    pub focus: Focus,
    pub overlay: Option<Overlay>,
    pub cmdline: LineInput,
    pub log: Vec<(Level, String)>,
    pub status: Option<(Level, String, Instant)>,
    pub clipboard: Clipboard,
    pub search: Option<String>,
    pub search_visible: bool,
    search_origin: Option<Pos>,
    pub settings: Settings,
    pub layout: LayoutCache,
    pub sidebar_sel: usize,
    /// When false, the view does not snap back to the cursor (mouse wheel).
    pub follow_cursor: bool,
    pub quit: bool,
    untitled_counter: usize,
    last_click: Option<(Instant, Pos)>,
    dragging: bool,
    /// Set when mouse capture should be (re)applied by the terminal layer.
    pub mouse_changed: bool,
}

pub const COMMANDS: &[&str] = &[
    "/open", "/join", "/save", "/saveas", "/saveall", "/new", "/close", "/part", "/quit", "/q!", "/find",
    "/replace", "/goto", "/lang", "/tabsize", "/theme", "/encoding", "/eol", "/reload", "/cd", "/win", "/next",
    "/prev", "/tag", "/upper", "/lower", "/title", "/sort", "/trim", "/comment", "/bold", "/italic", "/json",
    "/reflow", "/log", "/clear", "/help", "/menu", "/pages", "/pagesize", "/pagescale", "/wrap", "/pagebreak", "/settings",
    "/remember",
];

impl App {
    pub fn new(paths: &[String]) -> Self {
        let mut app = Self {
            docs: Vec::new(),
            active: 0,
            focus: Focus::Editor,
            overlay: None,
            cmdline: LineInput::default(),
            log: Vec::new(),
            status: None,
            clipboard: Clipboard::new(),
            search: None,
            search_visible: false,
            search_origin: None,
            settings: if cfg!(test) { Settings::default() } else { Settings::load() },
            layout: LayoutCache::default(),
            sidebar_sel: 0,
            follow_cursor: true,
            quit: false,
            untitled_counter: 0,
            last_click: None,
            dragging: false,
            mouse_changed: false,
        };
        for p in paths {
            app.open_path(&fileio::expand_tilde(p));
        }
        if app.docs.is_empty() {
            app.new_doc(None);
        }
        app.active = 0;
        app.msg(Level::Info, "Welcome to text_killer. F2/^P: menu · ^O: open · ^S: save · F1: help");
        app
    }

    pub fn theme(&self) -> &'static Theme {
        THEMES[self.settings.theme % THEMES.len()]
    }

    pub fn doc(&self) -> &Document {
        &self.docs[self.active]
    }

    pub fn doc_mut(&mut self) -> &mut Document {
        &mut self.docs[self.active]
    }

    pub fn msg(&mut self, level: Level, text: impl Into<String>) {
        let text = text.into();
        self.log.push((level, text.clone()));
        if self.log.len() > 1000 {
            self.log.drain(..100);
        }
        self.status = Some((level, text, Instant::now()));
    }

    /// Called periodically: expires old status messages. Returns true if a redraw is needed.
    pub fn tick(&mut self) -> bool {
        if let Some((_, _, t)) = &self.status {
            if t.elapsed() > Duration::from_secs(10) {
                self.status = None;
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------------
    // Documents / tabs
    // ------------------------------------------------------------------

    fn new_doc(&mut self, name: Option<String>) {
        self.untitled_counter += 1;
        let name = name.unwrap_or_else(|| {
            if self.untitled_counter == 1 { "untitled".into() } else { format!("untitled-{}", self.untitled_counter) }
        });
        self.docs.push(Document::new_empty(name));
        self.switch_to(self.docs.len() - 1);
    }

    pub fn switch_to(&mut self, idx: usize) {
        if idx < self.docs.len() {
            self.doc_mut().seal();
            self.active = idx;
            self.sidebar_sel = idx;
            self.follow_cursor = true;
        }
    }

    fn is_pristine(d: &Document) -> bool {
        d.path.is_none() && !d.is_dirty() && d.line_count() == 1 && d.lines[0].is_empty()
    }

    pub fn open_path(&mut self, path: &Path) {
        let abs = fileio::absolute(path);
        if let Some(i) = self.docs.iter().position(|d| d.path.as_deref() == Some(abs.as_path())) {
            self.switch_to(i);
            self.msg(Level::Info, format!("-!- already open: {}", abs.display()));
            return;
        }
        let doc = if abs.exists() {
            match fileio::load(&abs) {
                Ok(d) => {
                    let ro = if d.readonly { " [read-only]" } else { "" };
                    self.msg(
                        Level::Info,
                        format!("-!- joined {} ({} lines, {}, {}){ro}", abs.display(), d.line_count(), d.encoding, d.line_ending),
                    );
                    d
                }
                Err(e) => {
                    self.msg(Level::Error, format!("-!- cannot open {}: {e}", abs.display()));
                    return;
                }
            }
        } else {
            self.msg(Level::Info, format!("-!- new file: {}", abs.display()));
            let mut d = Document::new_empty(fileio::file_name(&abs));
            d.path = Some(abs);
            d
        };
        // Replace an untouched scratch buffer instead of adding a tab.
        if !self.docs.is_empty() && Self::is_pristine(self.doc()) {
            let a = self.active;
            self.docs[a] = doc;
            self.switch_to(a);
        } else {
            self.docs.push(doc);
            self.switch_to(self.docs.len() - 1);
        }
    }

    fn close_tab(&mut self, idx: usize, force: bool) {
        if idx >= self.docs.len() {
            return;
        }
        if !force && self.docs[idx].is_dirty() {
            let name = self.docs[idx].name.clone();
            self.overlay = Some(Overlay::Confirm(Confirm {
                title: "Unsaved changes",
                message: format!("Save changes to \"{name}\" before closing?"),
                kind: ConfirmKind::CloseTab(idx),
                three_way: true,
            }));
            return;
        }
        let name = self.docs.remove(idx).name;
        self.msg(Level::Info, format!("-!- parted {name}"));
        if self.docs.is_empty() {
            self.new_doc(None);
        } else {
            let a = if self.active > idx || self.active >= self.docs.len() { self.active.saturating_sub(1) } else { self.active };
            self.switch_to(a.min(self.docs.len() - 1));
        }
    }

    fn request_quit(&mut self) {
        let dirty: Vec<String> = self.docs.iter().filter(|d| d.is_dirty()).map(|d| d.name.clone()).collect();
        if dirty.is_empty() {
            self.quit = true;
            return;
        }
        self.overlay = Some(Overlay::Confirm(Confirm {
            title: "Quit",
            message: format!("{} unsaved document(s): {}. Save all before quitting?", dirty.len(), dirty.join(", ")),
            kind: ConfirmKind::Quit,
            three_way: true,
        }));
    }

    /// Saves document `idx` to its path, or opens a Save As browser.
    /// Returns true if the document was written.
    fn save_doc(&mut self, idx: usize, close_after: bool) -> bool {
        let Some(path) = self.docs[idx].path.clone() else {
            self.open_save_browser(idx, close_after);
            return false;
        };
        // Spreadsheets, slides and e-books are opened as text but can't be written.
        if !docformats::writable(&path) {
            let label = self.docs[idx].format.unwrap_or("this format");
            self.msg(
                Level::Warn,
                format!("-!- {label} files can't be written; pick a new name (.odt, .docx, .rtf, .md or .txt)"),
            );
            let suggested = docformats::suggest_name(&self.docs[idx].name);
            self.open_save_browser_named(idx, close_after, &suggested);
            return false;
        }
        if self.docs[idx].lossy_import {
            let d = &self.docs[idx];
            self.overlay = Some(Overlay::Confirm(Confirm {
                title: "Overwrite document",
                message: format!(
                    "\"{}\" was converted from {}. Saving rewrites it with text, headings, lists, bold and \
                     italic only; images, tables, fonts and page layout will be lost. Overwrite it? \
                     (Choose No, then F12, to save a copy instead.)",
                    d.name,
                    d.format.unwrap_or("another format")
                ),
                kind: ConfirmKind::LossySave { idx, close_after },
                three_way: false,
            }));
            return false;
        }
        self.write_doc(idx, &path, close_after)
    }

    fn write_doc(&mut self, idx: usize, path: &Path, close_after: bool) -> bool {
        if self.docs[idx].encoding == Encoding::Binary {
            self.msg(Level::Error, "-!- binary (hex view) documents cannot be saved");
            return false;
        }
        match fileio::save(&self.docs[idx], path) {
            Ok(bytes) => {
                let d = &mut self.docs[idx];
                let abs = fileio::absolute(path);
                let renamed = d.path.as_deref() != Some(abs.as_path());
                d.name = fileio::file_name(&abs);
                d.path = Some(abs.clone());
                d.readonly = false;
                d.mark_saved();
                // The file on disk now holds exactly what the editor shows.
                d.format = docformats::label_for(&abs);
                d.lossy_import = false;
                if renamed && d.language.name == "Plain Text" {
                    let lang = syntax::detect(&d.name, &d.lines[0]);
                    d.set_language(lang);
                }
                let lines = d.line_count();
                self.msg(Level::Info, format!("-!- saved {} ({lines} lines, {bytes} bytes)", abs.display()));
                if close_after {
                    self.close_tab(idx, true);
                }
                true
            }
            Err(e) => {
                self.msg(Level::Error, format!("-!- save failed: {e}"));
                false
            }
        }
    }

    fn open_save_browser(&mut self, idx: usize, close_after: bool) {
        let name = self.docs[idx].name.clone();
        self.open_save_browser_named(idx, close_after, &name);
    }

    fn open_save_browser_named(&mut self, idx: usize, close_after: bool, name: &str) {
        let d = &self.docs[idx];
        let start = d.path.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let mut b = FileBrowser::new(BrowserMode::Save, &start, name, idx);
        b.close_after = close_after;
        self.overlay = Some(Overlay::Browser(b));
    }

    fn open_browser(&mut self) {
        let start = self
            .doc()
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        self.overlay = Some(Overlay::Browser(FileBrowser::new(BrowserMode::Open, &start, "", self.active)));
    }

    fn save_all(&mut self) {
        let mut n = 0;
        for i in 0..self.docs.len() {
            if self.docs[i].is_dirty() && self.docs[i].path.is_some() && self.save_doc(i, false) {
                n += 1;
            }
        }
        let untitled = self.docs.iter().filter(|d| d.is_dirty() && d.path.is_none()).count();
        if untitled > 0 {
            self.msg(Level::Warn, format!("-!- saved {n}; {untitled} untitled document(s) need Save As"));
        } else {
            self.msg(Level::Info, format!("-!- saved {n} document(s)"));
        }
    }

    fn reload(&mut self, idx: usize, force: bool) {
        let Some(path) = self.docs[idx].path.clone() else {
            self.msg(Level::Warn, "-!- document has no file to reload");
            return;
        };
        if !force && self.docs[idx].is_dirty() {
            self.overlay = Some(Overlay::Confirm(Confirm {
                title: "Reload",
                message: "Discard unsaved changes and reload from disk?".into(),
                kind: ConfirmKind::Reload(idx),
                three_way: false,
            }));
            return;
        }
        match fileio::load(&path) {
            Ok(mut d) => {
                let old = &self.docs[idx];
                d.cursor = d.clamp(old.cursor);
                d.scroll_line = old.scroll_line;
                self.docs[idx] = d;
                self.msg(Level::Info, format!("-!- reloaded {}", path.display()));
            }
            Err(e) => self.msg(Level::Error, format!("-!- reload failed: {e}")),
        }
    }

    // ------------------------------------------------------------------
    // Actions
    // ------------------------------------------------------------------

    pub fn run(&mut self, action: Action) {
        self.run_inner(action);
        // Persist settings the user asked to remember.
        let theme_change = matches!(action, Action::SetTheme(_) | Action::NextTheme);
        let layout_change = matches!(
            action,
            Action::SetPageMode(_)
                | Action::SetPageSize(_)
                | Action::SetPageScale(_)
                | Action::ToggleWrap
                | Action::ToggleLineNumbers
                | Action::ToggleSidebar
                | Action::ToggleWhitespace
                | Action::ToggleMouse
        );
        if (theme_change && self.settings.remember_theme) || (layout_change && self.settings.remember_layout) {
            if let Err(e) = self.settings.save() {
                self.msg(Level::Error, format!("-!- could not save settings: {e}"));
            }
        }
    }

    fn insert_page_break(&mut self) {
        self.follow_cursor = true;
        let d = self.doc_mut();
        if d.readonly {
            return;
        }
        d.transact(crate::document::EditKind::Other, |d| {
            d.delete_selection();
            let c = d.cursor;
            let text = if c.col > 0 { "\n\u{c}\n" } else { "\u{c}\n" };
            d.cursor = d.insert(c, text);
        });
        self.msg(Level::Info, "-!- page break inserted (Alt+Enter)");
    }

    fn settings_saved_msg(&mut self, what: &str) {
        match self.settings.save() {
            Ok(path) => {
                let s = format!("-!- {what} ({})", path.display());
                self.msg(Level::Info, s);
            }
            Err(e) => self.msg(Level::Error, format!("-!- could not save settings: {e}")),
        }
    }

    fn run_inner(&mut self, action: Action) {
        match action {
            Action::New => self.new_doc(None),
            Action::Open => self.open_browser(),
            Action::Save => {
                self.save_doc(self.active, false);
            }
            Action::SaveAs => self.open_save_browser(self.active, false),
            Action::SaveAll => self.save_all(),
            Action::Reload => self.reload(self.active, false),
            Action::Close => self.close_tab(self.active, false),
            Action::Quit => self.request_quit(),
            Action::Undo => {
                if !self.doc_mut().undo() {
                    self.msg(Level::Warn, "-!- nothing to undo");
                }
            }
            Action::Redo => {
                if !self.doc_mut().redo() {
                    self.msg(Level::Warn, "-!- nothing to redo");
                }
            }
            Action::Cut => {
                if let Some(t) = self.doc_mut().cut() {
                    self.clipboard.set(t);
                }
            }
            Action::Copy => {
                if let Some(t) = self.doc().copy() {
                    let n = t.lines().count().max(1);
                    self.clipboard.set(t);
                    self.msg(Level::Info, format!("-!- copied {n} line(s)"));
                }
            }
            Action::Paste => {
                let t = self.clipboard.text.clone();
                if t.is_empty() {
                    self.msg(Level::Warn, "-!- clipboard is empty (terminal paste also works)");
                } else {
                    self.doc_mut().insert_text(&t);
                }
            }
            Action::SelectAll => self.doc_mut().select_all(),
            Action::Find => self.prompt("/find ", true),
            Action::FindNext => self.find_next(false),
            Action::FindPrev => self.find_next(true),
            Action::Replace => self.prompt("/replace ", false),
            Action::Goto => self.prompt("/goto ", false),
            Action::Format(op) => self.format(op),
            Action::ToggleLineNumbers => self.settings.line_numbers = !self.settings.line_numbers,
            Action::ToggleSidebar => {
                self.settings.sidebar = !self.settings.sidebar;
                if !self.settings.sidebar && self.focus == Focus::Sidebar {
                    self.focus = Focus::Editor;
                }
            }
            Action::ToggleWhitespace => self.settings.whitespace = !self.settings.whitespace,
            Action::ToggleMouse => {
                self.settings.mouse = !self.settings.mouse;
                self.mouse_changed = true;
                let s = if self.settings.mouse { "on" } else { "off (terminal selection enabled)" };
                self.msg(Level::Info, format!("-!- mouse capture {s}"));
            }
            Action::ToggleOsc52 => {
                self.clipboard.osc52 = !self.clipboard.osc52;
                let s = if self.clipboard.osc52 { "on" } else { "off" };
                self.msg(Level::Info, format!("-!- system clipboard (OSC 52) {s}"));
            }
            Action::ToggleIndentStyle => {
                let d = self.doc_mut();
                d.use_tabs = !d.use_tabs;
                let s = if d.use_tabs { "tabs" } else { "spaces" };
                self.msg(Level::Info, format!("-!- indenting with {s}"));
            }
            Action::ToggleOverwrite => {
                let d = self.doc_mut();
                d.overwrite = !d.overwrite;
            }
            Action::NextTheme => self.run_inner(Action::SetTheme((self.settings.theme + 1) % THEMES.len())),
            Action::SetPageMode(m) => {
                self.settings.page_mode = m;
                self.follow_cursor = true;
                let what = match m {
                    PageMode::Off => "off (continuous view)".to_owned(),
                    PageMode::Auto => "auto (as many pages as fit)".to_owned(),
                    PageMode::Fixed(n) => format!("{n} page(s)"),
                };
                self.msg(Level::Info, format!("-!- page view: {what}"));
            }
            Action::SetPageSize(size) => {
                self.settings.page_size = size;
                self.follow_cursor = true;
                self.msg(Level::Info, format!("-!- page size: {} columns × {} lines", size.cols, size.rows));
            }
            Action::SetPageScale(sc) => {
                self.settings.page_scale = sc;
                self.follow_cursor = true;
                let what = match sc {
                    PageScale::Actual => "actual size (true pages)",
                    PageScale::Fill => "fill window (pages stretch to fit; page breaks depend on window size)",
                };
                self.msg(Level::Info, format!("-!- page scaling: {what}"));
            }
            Action::ToggleWrap => {
                self.settings.wrap = !self.settings.wrap;
                let s = if self.settings.wrap { "on" } else { "off" };
                let note = if self.settings.page_mode != PageMode::Off { " (applies when page view is off)" } else { "" };
                self.msg(Level::Info, format!("-!- wrap long lines: {s}{note}"));
            }
            Action::InsertPageBreak => self.insert_page_break(),
            Action::ToggleRememberTheme => {
                self.settings.remember_theme = !self.settings.remember_theme;
                let what = if self.settings.remember_theme { "theme will be remembered" } else { "theme will not be remembered" };
                self.settings_saved_msg(what);
            }
            Action::ToggleRememberLayout => {
                self.settings.remember_layout = !self.settings.remember_layout;
                let what = if self.settings.remember_layout {
                    "page view & layout will be remembered"
                } else {
                    "page view & layout will not be remembered"
                };
                self.settings_saved_msg(what);
            }
            Action::ResetSettings => {
                self.settings = Settings::default();
                self.mouse_changed = true;
                self.follow_cursor = true;
                self.settings_saved_msg("settings reset to defaults; saved settings removed");
            }
            Action::ShowSettingsPath => {
                let m = match crate::config::config_path() {
                    Some(p) => {
                        let state = if p.exists() { "exists" } else { "not created yet" };
                        format!("-!- settings file: {} ({state})", p.display())
                    }
                    None => "-!- no settings location available".to_owned(),
                };
                self.msg(Level::Info, m);
            }
            Action::SetTheme(i) => {
                self.settings.theme = i % THEMES.len();
                let t = self.theme();
                self.msg(Level::Info, format!("-!- theme: {} ({})", t.name, t.blurb));
            }
            Action::TabWidth(n) => {
                self.doc_mut().tab_width = n as usize;
                self.msg(Level::Info, format!("-!- tab width {n}"));
            }
            Action::Language(i) => {
                let lang = LANGUAGES[i];
                self.doc_mut().set_language(lang);
                self.msg(Level::Info, format!("-!- language: {}", lang.name));
            }
            Action::LineEnding(le) => {
                let d = self.doc_mut();
                if d.line_ending != le {
                    d.line_ending = le;
                    d.seal();
                    // Force dirty so the change is saved.
                    d.transact(crate::document::EditKind::Other, |d| {
                        d.insert(Pos::new(0, 0), " ");
                        d.delete(Pos::new(0, 0), Pos::new(0, 1));
                    });
                }
                self.msg(Level::Info, format!("-!- line endings: {le}"));
            }
            Action::Encoding(enc) => {
                let d = self.doc_mut();
                if d.encoding == Encoding::Binary {
                    self.msg(Level::Error, "-!- cannot change encoding of a binary view");
                    return;
                }
                if d.encoding != enc {
                    d.encoding = enc;
                    d.transact(crate::document::EditKind::Other, |d| {
                        d.insert(Pos::new(0, 0), " ");
                        d.delete(Pos::new(0, 0), Pos::new(0, 1));
                    });
                }
                self.msg(Level::Info, format!("-!- encoding: {enc}"));
            }
            Action::NextTab => self.switch_to((self.active + 1) % self.docs.len()),
            Action::PrevTab => self.switch_to((self.active + self.docs.len() - 1) % self.docs.len()),
            Action::FocusSidebar => {
                if self.focus == Focus::Sidebar {
                    self.focus = Focus::Editor;
                } else {
                    self.settings.sidebar = true;
                    self.focus = Focus::Sidebar;
                    self.sidebar_sel = self.active;
                }
            }
            Action::FocusCommand => self.prompt("", false),
            Action::MessageLog => self.overlay = Some(Overlay::Log { scroll: usize::MAX }),
            Action::Help => self.overlay = Some(Overlay::Help { scroll: 0 }),
            Action::Submenu(s) => {
                let mut m = Menu::new(self.menu_ctx());
                if s != SubMenu::Root {
                    m.enter(s);
                }
                self.overlay = Some(Overlay::Menu(m));
            }
        }
    }

    pub fn menu_ctx(&self) -> MenuCtx {
        let d = self.doc();
        MenuCtx {
            theme: self.settings.theme,
            language: LANGUAGES.iter().position(|l| std::ptr::eq(*l, d.language)).unwrap_or(usize::MAX),
            line_ending: d.line_ending,
            encoding: d.encoding,
            tab_width: d.tab_width,
            page_mode: self.settings.page_mode,
            page_size: self.settings.page_size,
            page_scale: self.settings.page_scale,
            wrap: self.settings.wrap,
            remember_theme: self.settings.remember_theme,
            remember_layout: self.settings.remember_layout,
        }
    }

    fn format(&mut self, op: FormatOp) {
        self.follow_cursor = true;
        match format::apply(self.doc_mut(), op) {
            Ok(Some(m)) => self.msg(Level::Info, format!("-!- {m}")),
            Ok(None) => {}
            Err(e) => self.msg(Level::Warn, format!("-!- {e}")),
        }
    }

    fn prompt(&mut self, prefill: &str, incremental: bool) {
        let mut text = prefill.to_owned();
        if incremental {
            if let Some(sel) = self.doc().selected_text().filter(|s| !s.contains('\n')) {
                text.push_str(&sel);
            } else if let Some(s) = &self.search {
                text.push_str(s);
            }
        }
        self.cmdline.set(&text);
        self.search_origin = Some(self.doc().cursor);
        self.focus = Focus::Command;
    }

    fn find_next(&mut self, backwards: bool) {
        let Some(needle) = self.search.clone() else {
            self.prompt("/find ", true);
            return;
        };
        self.search_visible = true;
        let d = self.doc();
        let from = if backwards {
            d.selection().map_or(d.cursor, |(s, _)| s)
        } else {
            d.selection().map_or(d.cursor, |(_, e)| e)
        };
        match d.find(&needle, from, backwards) {
            Some((s, e)) => {
                let wrapped = if backwards { s >= from } else { s < from };
                let d = self.doc_mut();
                d.seal();
                d.anchor = Some(s);
                d.cursor = e;
                d.desired_col = None;
                self.follow_cursor = true;
                if wrapped {
                    self.msg(Level::Info, "-!- search wrapped");
                }
            }
            None => self.msg(Level::Warn, format!("-!- not found: {needle}")),
        }
    }

    // ------------------------------------------------------------------
    // Event dispatch
    // ------------------------------------------------------------------

    pub fn handle_event(&mut self, ev: Event) {
        match ev {
            Event::Key(k) if k.kind != KeyEventKind::Release => self.handle_key(k),
            Event::Paste(text) => self.handle_paste(&text),
            Event::Mouse(m) => self.handle_mouse(m),
            _ => {}
        }
    }

    fn handle_paste(&mut self, text: &str) {
        match (&mut self.overlay, self.focus) {
            (Some(Overlay::Browser(b)), _) => b.input.insert_str(text.lines().next().unwrap_or("")),
            (Some(_), _) => {}
            (None, Focus::Command) => self.cmdline.insert_str(text.lines().next().unwrap_or("")),
            (None, _) => {
                self.focus = Focus::Editor;
                self.follow_cursor = true;
                self.doc_mut().insert_text(text);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if self.overlay.is_some() {
            self.handle_overlay_key(key);
            return;
        }
        if self.handle_global_key(key) {
            return;
        }
        match self.focus {
            Focus::Editor => self.handle_editor_key(key),
            Focus::Sidebar => self.handle_sidebar_key(key),
            Focus::Command => self.handle_command_key(key),
        }
    }

    fn handle_global_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let action = match key.code {
            KeyCode::Char('q') if ctrl => Action::Quit,
            KeyCode::Char('s') if ctrl && (alt || shift) => Action::SaveAs,
            KeyCode::Char('s') if ctrl => Action::Save,
            KeyCode::Char('o') if ctrl => Action::Open,
            KeyCode::Char('n') if ctrl => Action::New,
            KeyCode::Char('w') if ctrl && self.focus != Focus::Command => Action::Close,
            KeyCode::Char('p') if ctrl => Action::Submenu(SubMenu::Root),
            KeyCode::Char('l') if ctrl => Action::FocusCommand,
            KeyCode::F(1) => Action::Help,
            KeyCode::F(2) | KeyCode::F(10) => Action::Submenu(SubMenu::Root),
            KeyCode::F(3) => {
                if shift { Action::FindPrev } else { Action::FindNext }
            }
            KeyCode::F(4) => Action::ToggleLineNumbers,
            KeyCode::F(5) => Action::ToggleSidebar,
            KeyCode::F(6) => Action::FocusSidebar,
            KeyCode::F(7) => Action::ToggleWhitespace,
            KeyCode::F(8) => Action::NextTheme,
            KeyCode::F(9) => Action::ToggleMouse,
            KeyCode::F(12) => Action::SaveAs,
            KeyCode::PageDown if ctrl => Action::NextTab,
            KeyCode::PageUp if ctrl => Action::PrevTab,
            KeyCode::Right if alt && !shift => Action::NextTab,
            KeyCode::Left if alt && !shift => Action::PrevTab,
            KeyCode::Char(c @ '1'..='9') if alt => {
                let idx = c as usize - '1' as usize;
                if idx < self.docs.len() {
                    self.switch_to(idx);
                }
                return true;
            }
            KeyCode::Char('0') if alt => {
                self.switch_to(self.docs.len() - 1);
                return true;
            }
            _ => return false,
        };
        self.run(action);
        true
    }

    fn handle_sidebar_key(&mut self, key: KeyEvent) {
        let n = self.docs.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.sidebar_sel = self.sidebar_sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.sidebar_sel = (self.sidebar_sel + 1).min(n - 1),
            KeyCode::Home => self.sidebar_sel = 0,
            KeyCode::End => self.sidebar_sel = n - 1,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => {
                let s = self.sidebar_sel;
                self.switch_to(s);
                if key.code != KeyCode::Char(' ') {
                    self.focus = Focus::Editor;
                }
            }
            KeyCode::Delete | KeyCode::Char('d' | 'x') => {
                let s = self.sidebar_sel;
                self.close_tab(s, false);
                self.sidebar_sel = self.sidebar_sel.min(self.docs.len() - 1);
            }
            KeyCode::Char('n') => self.run(Action::New),
            KeyCode::Char('o') => self.run(Action::Open),
            KeyCode::Esc | KeyCode::Tab | KeyCode::Left => self.focus = Focus::Editor,
            _ => {}
        }
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Tab {
            self.complete_command();
            return;
        }
        match self.cmdline.handle_key(key) {
            InputResult::Submit => {
                let text = self.cmdline.text.clone();
                self.cmdline.push_history(&text);
                self.cmdline.clear();
                self.focus = Focus::Editor;
                self.follow_cursor = true;
                self.execute(&text);
            }
            InputResult::Cancel => {
                self.cmdline.clear();
                self.focus = Focus::Editor;
                if let Some(o) = self.search_origin.take() {
                    let d = self.doc_mut();
                    d.anchor = None;
                    d.cursor = d.clamp(o);
                }
                self.follow_cursor = true;
            }
            InputResult::Consumed => self.incremental_search(),
            InputResult::Ignored => {}
        }
    }

    /// Live preview while typing `/find text`.
    fn incremental_search(&mut self) {
        let Some(q) = self.cmdline.text.strip_prefix("/find ").or_else(|| self.cmdline.text.strip_prefix("/f ")) else {
            return;
        };
        let q = q.to_owned();
        let origin = self.search_origin.unwrap_or(self.doc().cursor);
        if q.is_empty() {
            self.search_visible = false;
            return;
        }
        self.search = Some(q.clone());
        self.search_visible = true;
        if let Some((s, e)) = self.doc().find(&q, origin, false) {
            let d = self.doc_mut();
            d.anchor = Some(s);
            d.cursor = e;
            self.follow_cursor = true;
        }
    }

    fn complete_command(&mut self) {
        let text = self.cmdline.text.clone();
        if !text.contains(' ') {
            let matches: Vec<&str> = COMMANDS.iter().copied().filter(|c| c.starts_with(text.as_str())).collect();
            match matches.as_slice() {
                [] => {}
                [one] => self.cmdline.set(&format!("{one} ")),
                many => {
                    let prefix = common_prefix(many);
                    if prefix.len() > text.len() {
                        self.cmdline.set(&prefix);
                    } else {
                        self.msg(Level::Info, many.join(" "));
                    }
                }
            }
            return;
        }
        // Path completion for file commands.
        let (cmd, arg) = text.split_once(' ').unwrap_or((&text, ""));
        if !matches!(cmd, "/open" | "/o" | "/join" | "/j" | "/saveas" | "/save" | "/w" | "/cd" | "/e") {
            if cmd == "/lang" {
                let a = arg.to_lowercase();
                let names: Vec<&str> = LANGUAGES
                    .iter()
                    .flat_map(|l| l.aliases.iter().copied())
                    .filter(|n| n.starts_with(&a))
                    .collect();
                if let [one] = names.as_slice() {
                    self.cmdline.set(&format!("{cmd} {one}"));
                } else {
                    self.msg(Level::Info, names.join(" "));
                }
            }
            return;
        }
        let base_dir = self
            .doc()
            .path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let (dir_part, file_part) = match arg.rfind('/') {
            Some(i) => (&arg[..=i], &arg[i + 1..]),
            None => ("", arg),
        };
        let dir = if dir_part.is_empty() {
            base_dir
        } else {
            let p = fileio::expand_tilde(dir_part);
            if p.is_absolute() { p } else { base_dir.join(p) }
        };
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let mut names: Vec<(String, bool)> = rd
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path().is_dir()))
            .filter(|(n, _)| n.starts_with(file_part) && (file_part.starts_with('.') || !n.starts_with('.')))
            .collect();
        names.sort();
        match names.as_slice() {
            [] => {}
            [(n, is_dir)] => {
                let suffix = if *is_dir { "/" } else { "" };
                self.cmdline.set(&format!("{cmd} {dir_part}{n}{suffix}"));
            }
            many => {
                let strs: Vec<&str> = many.iter().map(|(n, _)| n.as_str()).collect();
                let prefix = common_prefix(&strs);
                if prefix.len() > file_part.len() {
                    self.cmdline.set(&format!("{cmd} {dir_part}{prefix}"));
                } else {
                    let shown: Vec<String> = many.iter().take(30).map(|(n, d)| if *d { format!("{n}/") } else { n.clone() }).collect();
                    self.msg(Level::Info, shown.join("  "));
                }
            }
        }
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) {
        let Some(overlay) = self.overlay.take() else { return };
        match overlay {
            Overlay::Menu(mut m) => {
                let mut keep = true;
                match key.code {
                    KeyCode::Esc | KeyCode::F(2) | KeyCode::F(10) => keep = false,
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => keep = false,
                    KeyCode::Left | KeyCode::Backspace => keep = m.back(),
                    KeyCode::Up | KeyCode::Char('k') => m.move_sel(-1),
                    KeyCode::Down | KeyCode::Char('j') => m.move_sel(1),
                    KeyCode::Home => m.set_selected(0),
                    KeyCode::End => m.set_selected(usize::MAX),
                    KeyCode::PageUp => m.move_sel(-8),
                    KeyCode::PageDown => m.move_sel(8),
                    KeyCode::Enter | KeyCode::Right | KeyCode::Char(' ') => {
                        if let Some(item) = m.items.get(m.selected()) {
                            match item.action {
                                Action::Submenu(s) => m.enter(s),
                                a => {
                                    self.run(a);
                                    // Keep the menu open for toggles in View.
                                    keep = keeps_menu_open(a) && self.overlay.is_none();
                                }
                            }
                        }
                    }
                    KeyCode::Char(c) if c.is_alphanumeric() => {
                        m.jump_to_letter(c);
                    }
                    _ => {}
                }
                if keep && self.overlay.is_none() {
                    m.refresh(self.menu_ctx());
                    self.overlay = Some(Overlay::Menu(m));
                }
            }
            Overlay::Browser(mut b) => match b.handle_key(key) {
                BrowserAction::None => self.overlay = Some(Overlay::Browser(b)),
                BrowserAction::Close => {}
                BrowserAction::Open(p) => self.open_path(&p),
                BrowserAction::Save(p) => self.browser_save(b, p),
            },
            Overlay::Confirm(c) => self.handle_confirm(c, key),
            Overlay::Help { scroll } => {
                if let Some(scroll) = scroll_key(key, scroll) {
                    self.overlay = Some(Overlay::Help { scroll });
                }
            }
            Overlay::Log { scroll } => {
                if let Some(scroll) = scroll_key(key, scroll) {
                    self.overlay = Some(Overlay::Log { scroll });
                }
            }
        }
    }

    fn browser_save(&mut self, b: FileBrowser, path: PathBuf) {
        let target = b.target.min(self.docs.len() - 1);
        let same_file = self.docs[target].path.as_deref() == Some(fileio::absolute(&path).as_path());
        if path.exists() && !same_file {
            self.overlay = Some(Overlay::Confirm(Confirm {
                title: "Overwrite",
                message: format!("{} already exists. Overwrite it?", path.display()),
                kind: ConfirmKind::Overwrite { path, target, close_after: b.close_after },
                three_way: false,
            }));
            return;
        }
        self.write_doc(target, &path, b.close_after);
    }

    fn handle_confirm(&mut self, c: Confirm, key: KeyEvent) {
        // Some(true) = yes, Some(false) = no, None = cancel.
        let answer = match key.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => Some(true),
            KeyCode::Char('n' | 'N') if c.three_way => Some(false),
            KeyCode::Char('n' | 'N') | KeyCode::Esc | KeyCode::Char('c' | 'C') => None,
            _ => {
                self.overlay = Some(Overlay::Confirm(c));
                return;
            }
        };
        match (c.kind, answer) {
            (_, None) => self.msg(Level::Info, "-!- cancelled"),
            (ConfirmKind::CloseTab(i), Some(true)) => {
                self.save_doc(i, true);
            }
            (ConfirmKind::CloseTab(i), Some(false)) => self.close_tab(i, true),
            (ConfirmKind::Quit, Some(true)) => {
                for i in 0..self.docs.len() {
                    if self.docs[i].is_dirty() && self.docs[i].path.is_some() {
                        self.save_doc(i, false);
                    }
                }
                if let Some(i) = self.docs.iter().position(Document::is_dirty) {
                    self.switch_to(i);
                    self.msg(Level::Warn, "-!- choose a file name for this document, then quit again");
                    self.open_save_browser(i, false);
                } else {
                    self.quit = true;
                }
            }
            (ConfirmKind::Quit, Some(false)) => self.quit = true,
            (ConfirmKind::Overwrite { path, target, close_after }, Some(_)) => {
                self.write_doc(target, &path, close_after);
            }
            (ConfirmKind::Reload(i), Some(_)) => self.reload(i, true),
            (ConfirmKind::LossySave { idx, close_after }, Some(_)) => {
                if let Some(path) = self.docs.get(idx).and_then(|d| d.path.clone()) {
                    self.docs[idx].lossy_import = false;
                    self.write_doc(idx, &path, close_after);
                }
            }
        }
    }

    fn handle_editor_key(&mut self, key: KeyEvent) {
        self.follow_cursor = true;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let page = (self.layout.editor.height.max(2) - 1) as isize;
        match key.code {
            KeyCode::Esc => {
                let d = self.doc_mut();
                if d.anchor.is_some() {
                    d.anchor = None;
                } else {
                    self.search_visible = false;
                }
            }
            // AltGr on some platforms reports Ctrl+Alt: treat as text.
            KeyCode::Char(c) if ctrl && alt && !c.is_ascii_alphabetic() => self.doc_mut().insert_char(c),
            KeyCode::Char(c) if ctrl => match c.to_ascii_lowercase() {
                'z' if shift => self.run(Action::Redo),
                'z' => self.run(Action::Undo),
                'y' => self.run(Action::Redo),
                'x' => self.run(Action::Cut),
                'c' => self.run(Action::Copy),
                'v' => self.run(Action::Paste),
                'a' => self.run(Action::SelectAll),
                'f' => self.run(Action::Find),
                'r' => self.run(Action::Replace),
                'g' => self.run(Action::Goto),
                'b' => self.format(FormatOp::Bold),
                't' => self.format(FormatOp::Italic),
                'u' => self.format(FormatOp::Underline),
                'e' => self.format(FormatOp::Code),
                '/' | '7' | '_' => self.format(FormatOp::ToggleComment),
                'd' => self.format(FormatOp::DuplicateLine),
                'k' => self.format(FormatOp::DeleteLine),
                'j' => self.format(FormatOp::JoinLines),
                'h' => self.doc_mut().backspace(),
                ' ' => {
                    let d = self.doc_mut();
                    d.select_word();
                }
                _ => {}
            },
            KeyCode::Char(c) if alt => match c {
                'h' => self.format(FormatOp::Heading),
                'u' => self.format(FormatOp::Upper),
                'l' => self.format(FormatOp::Lower),
                't' => self.format(FormatOp::Title),
                'q' => self.format(FormatOp::Reflow),
                'b' => {
                    let d = self.doc_mut();
                    let p = d.word_left(d.cursor);
                    d.move_to(p, shift);
                }
                'f' => {
                    let d = self.doc_mut();
                    let p = d.word_right(d.cursor);
                    d.move_to(p, shift);
                }
                _ => {}
            },
            KeyCode::Char(c) => self.doc_mut().insert_char(c),
            KeyCode::Enter if alt => self.insert_page_break(),
            KeyCode::Enter => self.doc_mut().newline(),
            KeyCode::Tab => {
                let d = self.doc_mut();
                let multi = d.selection().is_some_and(|(s, e)| s.line != e.line);
                if multi {
                    format::indent(d);
                } else if !d.readonly {
                    let text = if d.use_tabs {
                        "\t".to_owned()
                    } else {
                        let col = crate::ui::display_col(&d.lines[d.cursor.line], d.cursor.col, d.tab_width);
                        " ".repeat(d.tab_width - col % d.tab_width)
                    };
                    d.insert_text(&text);
                }
            }
            KeyCode::BackTab => self.format(FormatOp::Outdent),
            KeyCode::Backspace if ctrl || alt => self.doc_mut().delete_word_back(),
            KeyCode::Backspace => self.doc_mut().backspace(),
            KeyCode::Delete if ctrl => self.doc_mut().delete_word_forward(),
            KeyCode::Delete if shift => self.run(Action::Cut),
            KeyCode::Delete => self.doc_mut().delete_forward(),
            KeyCode::Insert if shift => self.run(Action::Paste),
            KeyCode::Insert if ctrl => self.run(Action::Copy),
            KeyCode::Insert => self.run(Action::ToggleOverwrite),
            KeyCode::Left if ctrl => {
                let d = self.doc_mut();
                let p = d.word_left(d.cursor);
                d.move_to(p, shift);
            }
            KeyCode::Right if ctrl => {
                let d = self.doc_mut();
                let p = d.word_right(d.cursor);
                d.move_to(p, shift);
            }
            KeyCode::Left => self.doc_mut().move_left(shift),
            KeyCode::Right => self.doc_mut().move_right(shift),
            KeyCode::Up if alt => self.format(FormatOp::MoveLineUp),
            KeyCode::Down if alt => self.format(FormatOp::MoveLineDown),
            KeyCode::Up if ctrl => self.scroll_view(-1),
            KeyCode::Down if ctrl => self.scroll_view(1),
            KeyCode::Up => self.move_rows(-1, shift),
            KeyCode::Down => self.move_rows(1, shift),
            // Alt+PageUp/PageDown: a whole spread of pages at a time.
            KeyCode::PageUp | KeyCode::PageDown if alt && self.layout.page.is_some() => {
                let pi = self.layout.page.unwrap_or_default();
                let dir = if key.code == KeyCode::PageUp { -1 } else { 1 };
                self.move_rows(dir * (pi.rows * pi.shown.max(1)) as isize, shift);
            }
            KeyCode::PageUp | KeyCode::PageDown if self.layout.wrap_width.is_some() => {
                let rows = self.layout.page.map_or(page as usize, |pi| pi.rows) as isize;
                let dir = if key.code == KeyCode::PageUp { -1 } else { 1 };
                self.move_rows(dir * rows, shift);
            }
            KeyCode::PageUp => {
                self.doc_mut().move_vertical(-page, shift);
                let d = self.doc_mut();
                d.scroll_line = d.scroll_line.saturating_sub(page as usize);
            }
            KeyCode::PageDown => {
                self.doc_mut().move_vertical(page, shift);
                let d = self.doc_mut();
                d.scroll_line = (d.scroll_line + page as usize).min(d.line_count().saturating_sub(1));
            }
            KeyCode::Home if ctrl => self.doc_mut().move_to(Pos::new(0, 0), shift),
            KeyCode::End if ctrl => {
                let d = self.doc_mut();
                let e = d.end_pos();
                d.move_to(e, shift);
            }
            KeyCode::Home => match self.layout.wrap_width {
                // On a wrapped row: row start first, then the usual smart home.
                Some(w) => {
                    let d = self.doc_mut();
                    let (start, _, _) = d.vrow_bounds(w);
                    if start > 0 && d.cursor.col != start {
                        d.move_to(Pos::new(d.cursor.line, start), shift);
                    } else {
                        d.home(shift);
                    }
                }
                None => self.doc_mut().home(shift),
            },
            KeyCode::End => match self.layout.wrap_width {
                // Row end first (before the hanging space), then line end.
                Some(w) => {
                    let d = self.doc_mut();
                    let (start, end, last) = d.vrow_bounds(w);
                    let line = d.cursor.line;
                    let row_end = if last { end } else { end.saturating_sub(1).max(start) };
                    let target = if d.cursor.col < row_end { row_end } else { d.line_len(line) };
                    d.move_to(Pos::new(line, target), shift);
                }
                None => self.doc_mut().end(shift),
            },
            _ => {}
        }
    }

    /// Moves the cursor by visual rows when wrapping, else by lines.
    fn move_rows(&mut self, delta: isize, shift: bool) {
        match self.layout.wrap_width {
            Some(w) => self.doc_mut().move_visual(delta, shift, w),
            None => self.doc_mut().move_vertical(delta, shift),
        }
    }

    fn scroll_view(&mut self, delta: isize) {
        self.follow_cursor = false;
        let editor_h = self.layout.editor.height as usize;
        if let Some(pi) = self.layout.page {
            // Page view: scroll the spread vertically, then turn pages.
            let max = (pi.rows + 2).saturating_sub(editor_h) as isize;
            let d = self.doc_mut();
            let next = d.page_scroll as isize + delta;
            if next < 0 {
                if d.page_first > 0 {
                    d.page_first -= 1;
                    d.page_scroll = max as usize;
                } else {
                    d.page_scroll = 0;
                }
            } else if next > max {
                if d.page_first + 1 < pi.total {
                    d.page_first += 1;
                    d.page_scroll = 0;
                } else {
                    d.page_scroll = max as usize;
                }
            } else {
                d.page_scroll = next as usize;
            }
            return;
        }
        if let Some(w) = self.layout.wrap_width {
            let d = self.doc_mut();
            let v = d.step_vrow((d.scroll_line, d.scroll_row), delta, w);
            (d.scroll_line, d.scroll_row) = v;
            return;
        }
        let d = self.doc_mut();
        let max = d.line_count().saturating_sub(1) as isize;
        d.scroll_line = (d.scroll_line as isize + delta).clamp(0, max) as usize;
    }

    /// Maps a click on a wrapped/page row to a document position.
    fn hit_to_pos(&self, x: u16, y: u16) -> Option<Pos> {
        let rows = &self.layout.rows;
        if rows.is_empty() {
            return None;
        }
        // Prefer the row under the pointer; when dragging past the text,
        // fall back to the nearest row in the same column of pages.
        let hit = rows
            .iter()
            .find(|h| h.y == y && x >= h.x.saturating_sub(2) && x < h.x + h.width + 2)
            .or_else(|| {
                let col: Vec<&RowHit> =
                    rows.iter().filter(|h| x >= h.x.saturating_sub(2) && x < h.x + h.width + 2).collect();
                let pool = if col.is_empty() { rows.iter().collect() } else { col };
                pool.into_iter().min_by_key(|h| (h.y as i32 - y as i32).abs())
            })?;
        let d = self.doc();
        let rel = x.saturating_sub(hit.x) as usize;
        let col = crate::wrap::col_at_x(&d.lines[hit.line], hit.seg, rel, d.tab_width, hit.last);
        Some(Pos::new(hit.line, col.min(d.line_len(hit.line))))
    }

    fn screen_to_pos(&self, x: u16, y: u16) -> Pos {
        if let Some(p) = self.hit_to_pos(x, y) {
            return p;
        }
        let ed = self.layout.editor;
        let d = self.doc();
        let line = if y < ed.y {
            d.scroll_line.saturating_sub(1)
        } else {
            d.scroll_line + (y - ed.y) as usize
        }
        .min(d.line_count() - 1);
        let dcol = d.scroll_col + x.saturating_sub(self.layout.text_x) as usize;
        Pos::new(line, crate::ui::char_col_at_display(&d.lines[line], dcol, d.tab_width))
    }

    fn handle_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column, m.row);
        let inside = |r: Rect| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
        if let Some(overlay) = self.overlay.take() {
            let list = self.layout.popup_list;
            let row = y.saturating_sub(list.y) as usize;
            match overlay {
                Overlay::Browser(mut b) => {
                    let action = match m.kind {
                        MouseEventKind::ScrollUp => {
                            b.move_sel(-3);
                            BrowserAction::None
                        }
                        MouseEventKind::ScrollDown => {
                            b.move_sel(3);
                            BrowserAction::None
                        }
                        MouseEventKind::Down(MouseButton::Left) if inside(list) => b.click(row),
                        _ => BrowserAction::None,
                    };
                    match action {
                        BrowserAction::None => self.overlay = Some(Overlay::Browser(b)),
                        BrowserAction::Close => {}
                        BrowserAction::Open(p) => self.open_path(&p),
                        BrowserAction::Save(p) => self.browser_save(b, p),
                    }
                }
                Overlay::Menu(mut menu) => {
                    let mut keep = true;
                    match m.kind {
                        MouseEventKind::ScrollUp => menu.move_sel(-1),
                        MouseEventKind::ScrollDown => menu.move_sel(1),
                        MouseEventKind::Down(MouseButton::Left) if inside(list) => {
                            let idx = menu.scroll + row;
                            if idx < menu.items.len() {
                                menu.set_selected(idx);
                                match menu.items[idx].action {
                                    Action::Submenu(s) => menu.enter(s),
                                    a => {
                                        self.run(a);
                                        keep = keeps_menu_open(a);
                                    }
                                }
                            }
                        }
                        MouseEventKind::Down(MouseButton::Right) => keep = menu.back(),
                        MouseEventKind::Down(_) => keep = false,
                        _ => {}
                    }
                    if keep && self.overlay.is_none() {
                        menu.refresh(self.menu_ctx());
                        self.overlay = Some(Overlay::Menu(menu));
                    }
                }
                // A click anywhere closes the help / log viewers.
                Overlay::Help { .. } | Overlay::Log { .. } if matches!(m.kind, MouseEventKind::Down(_)) => {}
                Overlay::Help { scroll } => {
                    self.overlay = Some(Overlay::Help { scroll: wheel(scroll, m.kind) });
                }
                Overlay::Log { scroll } => {
                    self.overlay = Some(Overlay::Log { scroll: wheel(scroll, m.kind) });
                }
                other => self.overlay = Some(other),
            }
            return;
        }

        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if inside(self.layout.sidebar) {
                    if let Some(&(_, idx)) = self.layout.sidebar_rows.iter().find(|(r, _)| *r == y) {
                        self.switch_to(idx);
                        self.focus = Focus::Editor;
                    }
                } else if inside(self.layout.editor) {
                    self.focus = Focus::Editor;
                    self.follow_cursor = true;
                    let p = self.screen_to_pos(x, y);
                    let now = Instant::now();
                    let double = self
                        .last_click
                        .is_some_and(|(t, lp)| lp == p && now.duration_since(t) < Duration::from_millis(400));
                    let shift = m.modifiers.contains(KeyModifiers::SHIFT);
                    let d = self.doc_mut();
                    d.move_to(p, shift);
                    if double {
                        d.select_word();
                        self.last_click = None;
                    } else {
                        self.last_click = Some((now, p));
                        self.dragging = true;
                    }
                } else if inside(self.layout.cmd) {
                    self.focus = Focus::Command;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => {
                let p = self.screen_to_pos(x, y);
                self.follow_cursor = true;
                let d = self.doc_mut();
                if d.anchor.is_none() {
                    d.anchor = Some(d.cursor);
                }
                d.cursor = d.clamp(p);
                d.desired_col = None;
            }
            MouseEventKind::Up(_) => self.dragging = false,
            MouseEventKind::ScrollUp if !inside(self.layout.sidebar) => self.scroll_view(-3),
            MouseEventKind::ScrollDown if !inside(self.layout.sidebar) => self.scroll_view(3),
            MouseEventKind::ScrollUp => self.run(Action::PrevTab),
            MouseEventKind::ScrollDown => self.run(Action::NextTab),
            MouseEventKind::ScrollLeft => {
                let d = self.doc_mut();
                d.scroll_col = d.scroll_col.saturating_sub(4);
                self.follow_cursor = false;
            }
            MouseEventKind::ScrollRight => {
                let d = self.doc_mut();
                d.scroll_col += 4;
                self.follow_cursor = false;
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Command line (IRC-style slash commands)
    // ------------------------------------------------------------------

    pub fn execute(&mut self, input: &str) {
        let input = input.trim();
        if input.is_empty() {
            return;
        }
        if !input.starts_with('/') {
            // Bare input: a number jumps to a line, anything else searches.
            if input.trim_start_matches(['+', '-']).split(':').all(|p| p.parse::<usize>().is_ok()) {
                self.execute(&format!("/goto {input}"));
            } else {
                self.execute(&format!("/find {input}"));
            }
            return;
        }
        let (cmd, rest) = input.split_once(' ').unwrap_or((input, ""));
        let rest = rest.trim();
        let args = parse_args(rest);
        let arg0 = args.first().map(String::as_str).unwrap_or("");
        match cmd {
            "/open" | "/o" | "/join" | "/j" | "/e" | "/edit" => {
                if args.is_empty() {
                    self.run(Action::Open);
                }
                for a in &args {
                    self.open_path(&self.resolve_path(a));
                }
            }
            "/save" | "/w" | "/write" => {
                if args.is_empty() {
                    self.run(Action::Save);
                } else {
                    let p = self.resolve_path(arg0);
                    self.write_doc(self.active, &p, false);
                }
            }
            "/saveas" => {
                if args.is_empty() {
                    self.run(Action::SaveAs);
                } else {
                    let p = self.resolve_path(arg0);
                    let b = FileBrowser::new(BrowserMode::Save, &p, "", self.active);
                    self.browser_save(b, p);
                }
            }
            "/saveall" | "/wa" => self.save_all(),
            "/wq" | "/x" => {
                if self.save_doc(self.active, false) {
                    self.request_quit();
                }
            }
            "/new" | "/n" => {
                self.new_doc(None);
                if !args.is_empty() {
                    let p = self.resolve_path(arg0);
                    let d = self.doc_mut();
                    d.name = fileio::file_name(&p);
                    d.set_language(syntax::detect(&d.name, ""));
                    d.path = Some(fileio::absolute(&p));
                }
            }
            "/close" | "/part" | "/c" => self.close_tab(self.active, false),
            "/close!" | "/part!" => self.close_tab(self.active, true),
            "/quit" | "/q" | "/exit" => self.request_quit(),
            "/quit!" | "/q!" => self.quit = true,
            "/find" | "/f" | "/search" => {
                if rest.is_empty() {
                    self.search_visible = false;
                    return;
                }
                self.search = Some(rest.to_owned());
                // Incremental search already moved the cursor to a match.
                let at_match = self.doc().selected_text().is_some_and(|s| s.eq_ignore_ascii_case(rest));
                if !at_match {
                    self.find_next(false);
                } else {
                    self.search_visible = true;
                }
            }
            "/replace" | "/s" | "/r" => {
                if args.len() < 2 {
                    self.msg(Level::Warn, "usage: /replace <find> <replacement>  (quote text with spaces)");
                    return;
                }
                let n = self.doc_mut().replace_all(&args[0], &args[1]);
                self.msg(Level::Info, format!("-!- replaced {n} occurrence(s)"));
            }
            "/goto" | "/g" | "/line" => {
                let mut parts = arg0.split(':');
                let line_s = parts.next().unwrap_or("");
                let col: usize = parts.next().and_then(|c| c.parse().ok()).unwrap_or(1);
                let cur = self.doc().cursor.line as isize + 1;
                let target = if let Some(n) = line_s.strip_prefix('+') {
                    n.parse::<isize>().ok().map(|n| cur + n)
                } else if let Some(n) = line_s.strip_prefix('-') {
                    n.parse::<isize>().ok().map(|n| cur - n)
                } else {
                    line_s.parse::<isize>().ok()
                };
                match target {
                    Some(n) => {
                        let d = self.doc_mut();
                        let line = (n.max(1) as usize - 1).min(d.line_count() - 1);
                        d.move_to(Pos::new(line, col.saturating_sub(1)), false);
                        // Centre the target line.
                        let h = self.layout.editor.height as usize;
                        let d = self.doc_mut();
                        d.scroll_line = line.saturating_sub(h / 2);
                    }
                    None => self.msg(Level::Warn, "usage: /goto <line>[:col]"),
                }
            }
            "/lang" | "/syntax" => match syntax::by_name(rest) {
                Some(l) => {
                    self.doc_mut().set_language(l);
                    self.msg(Level::Info, format!("-!- language: {}", l.name));
                }
                None => {
                    let names: Vec<&str> = LANGUAGES.iter().map(|l| l.aliases.first().copied().unwrap_or(l.name)).collect();
                    self.msg(Level::Warn, format!("languages: {}", names.join(" ")));
                }
            },
            "/tabsize" | "/ts" => match arg0.parse::<u8>() {
                Ok(n) if (1..=16).contains(&n) => self.run(Action::TabWidth(n)),
                _ => self.msg(Level::Warn, "usage: /tabsize <1-16>"),
            },
            "/tabs" => {
                self.doc_mut().use_tabs = true;
                self.msg(Level::Info, "-!- indenting with tabs");
            }
            "/spaces" => {
                self.doc_mut().use_tabs = false;
                self.msg(Level::Info, "-!- indenting with spaces");
            }
            "/theme" => {
                if rest.is_empty() {
                    self.run(Action::NextTheme);
                } else if let Some(i) = crate::theme::find(rest) {
                    self.run(Action::SetTheme(i));
                } else {
                    let ids: Vec<&str> = THEMES.iter().map(|t| t.id).collect();
                    self.msg(Level::Warn, format!("-!- themes: {}", ids.join(" ")));
                }
            }
            "/encoding" | "/enc" => {
                let enc = match arg0.to_ascii_lowercase().replace(['-', '_'], "").as_str() {
                    "utf8" => Some(Encoding::Utf8),
                    "utf8bom" => Some(Encoding::Utf8Bom),
                    "utf16" | "utf16le" => Some(Encoding::Utf16Le),
                    "utf16be" => Some(Encoding::Utf16Be),
                    "latin1" | "iso88591" => Some(Encoding::Latin1),
                    _ => None,
                };
                match enc {
                    Some(e) => self.run(Action::Encoding(e)),
                    None => self.msg(Level::Warn, "usage: /encoding utf-8|utf-8-bom|utf-16le|utf-16be|latin1"),
                }
            }
            "/eol" => {
                let le = match arg0.to_ascii_lowercase().as_str() {
                    "lf" | "unix" => Some(LineEnding::Lf),
                    "crlf" | "dos" | "windows" => Some(LineEnding::CrLf),
                    "cr" | "mac" => Some(LineEnding::Cr),
                    _ => None,
                };
                match le {
                    Some(le) => self.run(Action::LineEnding(le)),
                    None => self.msg(Level::Warn, "usage: /eol lf|crlf|cr"),
                }
            }
            "/reload" => self.reload(self.active, false),
            "/reload!" => self.reload(self.active, true),
            "/cd" => {
                let p = if rest.is_empty() { fileio::expand_tilde("~") } else { self.resolve_path(rest) };
                match std::env::set_current_dir(&p) {
                    Ok(()) => self.msg(Level::Info, format!("-!- cwd: {}", p.display())),
                    Err(e) => self.msg(Level::Error, format!("-!- cd: {e}")),
                }
            }
            "/pwd" => {
                let cwd = std::env::current_dir().unwrap_or_default();
                self.msg(Level::Info, format!("-!- cwd: {}", cwd.display()));
            }
            "/win" | "/window" | "/w#" | "/buffer" | "/b" => match arg0.parse::<usize>() {
                Ok(n) if n >= 1 && n <= self.docs.len() => self.switch_to(n - 1),
                _ => {
                    // Switch by (partial) name.
                    let a = arg0.to_lowercase();
                    match self.docs.iter().position(|d| d.name.to_lowercase().contains(&a)) {
                        Some(i) if !a.is_empty() => self.switch_to(i),
                        _ => self.msg(Level::Warn, "usage: /win <number|name>"),
                    }
                }
            },
            "/next" => self.run(Action::NextTab),
            "/prev" => self.run(Action::PrevTab),
            "/tag" => {
                if arg0.is_empty() {
                    self.msg(Level::Warn, "usage: /tag <name> [attributes]");
                    return;
                }
                let name = arg0.trim_matches(['<', '>']).to_owned();
                let attrs = rest[arg0.len()..].trim();
                let open = if attrs.is_empty() { format!("<{name}>") } else { format!("<{name} {attrs}>") };
                format::wrap_selection(self.doc_mut(), &open, &format!("</{name}>"));
            }
            "/upper" => self.format(FormatOp::Upper),
            "/lower" => self.format(FormatOp::Lower),
            "/title" => self.format(FormatOp::Title),
            "/sort" => self.format(if arg0 == "desc" { FormatOp::SortLinesDesc } else { FormatOp::SortLines }),
            "/uniq" => self.format(FormatOp::UniqueLines),
            "/trim" => self.format(FormatOp::TrimTrailing),
            "/comment" => self.format(FormatOp::ToggleComment),
            "/bold" => self.format(FormatOp::Bold),
            "/italic" => self.format(FormatOp::Italic),
            "/json" => self.format(if arg0 == "min" { FormatOp::MinifyJson } else { FormatOp::PrettyJson }),
            "/reflow" => {
                let width = arg0.parse::<usize>().unwrap_or(80).max(10);
                if self.doc().readonly {
                    self.msg(Level::Warn, "-!- document is read-only");
                } else {
                    format::reflow(self.doc_mut(), width);
                }
            }
            "/pages" | "/pageview" => match PageMode::parse(rest) {
                Some(m) => self.run(Action::SetPageMode(m)),
                None => self.msg(Level::Warn, "usage: /pages off|auto|1-12"),
            },
            "/pagescale" | "/scale" => match PageScale::parse(rest) {
                Some(sc) => self.run(Action::SetPageScale(sc)),
                None => self.msg(Level::Warn, "usage: /pagescale actual|fill"),
            },
            "/pagesize" => match PageSize::parse(rest) {
                Some(sz) => self.run(Action::SetPageSize(sz)),
                None => self.msg(Level::Warn, "usage: /pagesize letter|a4|legal|a5|wide|<cols>x<lines>"),
            },
            "/wrap" => {
                let want = match arg0 {
                    "on" => Some(true),
                    "off" => Some(false),
                    _ => None,
                };
                if want != Some(self.settings.wrap) {
                    self.run(Action::ToggleWrap);
                }
            }
            "/pagebreak" | "/pb" => self.run(Action::InsertPageBreak),
            "/settings" => self.run(Action::ShowSettingsPath),
            "/remember" => {
                let (t, l) = match arg0 {
                    "theme" => (true, self.settings.remember_layout),
                    "layout" | "pages" => (self.settings.remember_theme, true),
                    "all" | "on" => (true, true),
                    "none" | "off" => (false, false),
                    _ => {
                        self.msg(Level::Warn, "usage: /remember theme|layout|all|none");
                        return;
                    }
                };
                self.settings.remember_theme = t;
                self.settings.remember_layout = l;
                self.settings_saved_msg("remembered settings updated");
            }
            "/log" => self.run(Action::MessageLog),
            "/clear" => {
                self.log.clear();
                self.status = None;
            }
            "/help" | "/?" => self.run(Action::Help),
            "/menu" => self.run(Action::Submenu(SubMenu::Root)),
            "/me" => {
                let t = format!("* {rest}");
                self.doc_mut().insert_text(&t);
            }
            _ => self.msg(Level::Warn, format!("-!- unknown command: {cmd} (try /help)")),
        }
    }

    fn resolve_path(&self, s: &str) -> PathBuf {
        let p = fileio::expand_tilde(s);
        if p.is_absolute() {
            p
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        }
    }
}

/// Menu items that leave the menu open so the effect can be previewed.
fn keeps_menu_open(a: Action) -> bool {
    matches!(
        a,
        Action::ToggleLineNumbers
            | Action::ToggleWhitespace
            | Action::NextTheme
            | Action::SetTheme(_)
            | Action::ToggleSidebar
            | Action::SetPageMode(_)
            | Action::SetPageSize(_)
            | Action::SetPageScale(_)
            | Action::ToggleWrap
            | Action::ToggleRememberTheme
            | Action::ToggleRememberLayout
    )
}

fn wheel(scroll: usize, kind: MouseEventKind) -> usize {
    match kind {
        MouseEventKind::ScrollUp => scroll.saturating_sub(3),
        MouseEventKind::ScrollDown => scroll.saturating_add(3),
        _ => scroll,
    }
}

/// Handles scrolling keys for help/log viewers; `None` closes the viewer.
fn scroll_key(key: KeyEvent, scroll: usize) -> Option<usize> {
    Some(match key.code {
        KeyCode::Up | KeyCode::Char('k') => scroll.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => scroll.saturating_add(1),
        KeyCode::PageUp => scroll.saturating_sub(10),
        KeyCode::PageDown | KeyCode::Char(' ') => scroll.saturating_add(10),
        KeyCode::Home | KeyCode::Char('g') => 0,
        KeyCode::End | KeyCode::Char('G') => usize::MAX,
        _ => return None,
    })
}

fn common_prefix(items: &[&str]) -> String {
    let Some(first) = items.first() else { return String::new() };
    let mut len = first.len();
    for s in &items[1..] {
        len = first
            .char_indices()
            .zip(s.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, a), _)| i + a.len_utf8())
            .min(len);
    }
    first[..len].to_owned()
}

/// Splits arguments on whitespace, honouring "double" and 'single' quotes.
pub fn parse_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) if c == '\\' => {
                if let Some(n) = chars.next() {
                    cur.push(match n {
                        'n' => '\n',
                        't' => '\t',
                        o => o,
                    });
                }
            }
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                has = true;
            }
            None if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            None => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args() {
        assert_eq!(parse_args("a b"), vec!["a", "b"]);
        assert_eq!(parse_args("\"a b\" ''"), vec!["a b", ""]);
        assert_eq!(parse_args("'x\\ty'"), vec!["x\ty"]);
        assert_eq!(common_prefix(&["/save", "/saveas", "/saveall"]), "/save");
    }

    #[test]
    fn clicks_map_to_wrapped_rows() {
        let mut app = App::new(&[]);
        app.doc_mut().insert_text("hello world foo");
        app.layout.rows = vec![
            RowHit { x: 10, y: 5, width: 6, line: 0, seg: (0, 6), last: false },
            RowHit { x: 10, y: 6, width: 6, line: 0, seg: (6, 12), last: false },
            RowHit { x: 10, y: 7, width: 6, line: 0, seg: (12, 15), last: true },
        ];
        assert_eq!(app.hit_to_pos(12, 6), Some(Pos::new(0, 8)));
        // Past the end of a middle row: stays on that row.
        assert_eq!(app.hit_to_pos(15, 5), Some(Pos::new(0, 5)));
        // Past the end of the last row: end of line.
        assert_eq!(app.hit_to_pos(15, 7), Some(Pos::new(0, 15)));
        // Below all rows (dragging): nearest row.
        assert_eq!(app.hit_to_pos(10, 30), Some(Pos::new(0, 12)));
    }

    #[test]
    fn page_break_command() {
        let mut app = App::new(&[]);
        app.doc_mut().insert_text("abcdef");
        app.doc_mut().cursor = Pos::new(0, 3);
        app.execute("/pagebreak");
        assert_eq!(app.doc().full_text(), "abc\n\u{c}\ndef");
        assert_eq!(app.doc().cursor, Pos::new(2, 0));
        app.execute("/pages 2");
        assert_eq!(app.settings.page_mode, PageMode::Fixed(2));
        app.execute("/pagescale fill");
        assert_eq!(app.settings.page_scale, PageScale::Fill);
        app.execute("/pages 10");
        assert_eq!(app.settings.page_mode, PageMode::Fixed(10));
        app.execute("/pagesize 70x50");
        assert_eq!(app.settings.page_size, PageSize { cols: 70, rows: 50 });
    }

    #[test]
    fn commands_drive_document() {
        let mut app = App::new(&[]);
        app.doc_mut().insert_text("foo bar foo\nline2\nline3");
        app.execute("/replace foo baz");
        assert_eq!(app.doc().lines[0], "baz bar baz");
        app.execute("/goto 3");
        assert_eq!(app.doc().cursor.line, 2);
        app.execute("bar");
        assert_eq!(app.doc().cursor, Pos::new(0, 7));
        app.execute("/new");
        assert_eq!(app.docs.len(), 2);
        app.execute("/win 1");
        assert_eq!(app.active, 0);
        app.execute("/close");
        assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
        app.handle_event(Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)));
        assert_eq!(app.docs.len(), 1);
    }
}
