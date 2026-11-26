//! Modal file browser used for opening and saving documents.

use std::fs;
use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::fileio::expand_tilde;
use crate::input::{InputResult, LineInput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserMode {
    Open,
    Save,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub is_link: bool,
}

pub enum BrowserAction {
    None,
    Close,
    Open(PathBuf),
    Save(PathBuf),
}

pub struct FileBrowser {
    pub mode: BrowserMode,
    pub dir: PathBuf,
    all: Vec<Entry>,
    /// Indices into `all` matching the current filter.
    pub visible: Vec<usize>,
    pub selected: usize,
    pub scroll: usize,
    pub show_hidden: bool,
    /// Filter text (open mode) or file name (save mode).
    pub input: LineInput,
    pub error: Option<String>,
    /// Document index this browser saves (save mode).
    pub target: usize,
    /// Close the target tab after a successful save.
    pub close_after: bool,
    /// Rows visible in the list, updated on render.
    pub page: usize,
}

impl FileBrowser {
    pub fn new(mode: BrowserMode, start: &Path, name: &str, target: usize) -> Self {
        let dir = if start.is_dir() { start.to_path_buf() } else { start.parent().map(Path::to_path_buf).unwrap_or_default() };
        let dir = crate::fileio::absolute(if dir.as_os_str().is_empty() { Path::new(".") } else { &dir });
        let mut b = Self {
            mode,
            dir,
            all: Vec::new(),
            visible: Vec::new(),
            selected: 0,
            scroll: 0,
            show_hidden: false,
            input: LineInput::default(),
            error: None,
            target,
            close_after: false,
            page: 10,
        };
        if mode == BrowserMode::Save {
            b.input.set(name);
        }
        b.refresh();
        b
    }

    pub fn refresh(&mut self) {
        self.error = None;
        self.all.clear();
        match fs::read_dir(&self.dir) {
            Ok(rd) => {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let ft = e.file_type().ok();
                    let is_link = ft.is_some_and(|t| t.is_symlink());
                    // Follow symlinks to decide whether they're directories.
                    let meta = fs::metadata(e.path()).ok();
                    let is_dir = meta.as_ref().is_some_and(fs::Metadata::is_dir);
                    let size = meta.map_or(0, |m| m.len());
                    self.all.push(Entry { name, is_dir, size, is_link });
                }
            }
            Err(e) => self.error = Some(format!("{}: {e}", self.dir.display())),
        }
        self.all.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        if self.dir.parent().is_some() {
            self.all.insert(0, Entry { name: "..".into(), is_dir: true, size: 0, is_link: false });
        }
        self.apply_filter();
    }

    fn filter_text(&self) -> String {
        if self.mode == BrowserMode::Open { self.input.text.to_lowercase() } else { String::new() }
    }

    fn apply_filter(&mut self) {
        let f = self.filter_text();
        let path_like = f.contains('/') || f.starts_with('~');
        self.visible = self
            .all
            .iter()
            .enumerate()
            .filter(|(_, e)| e.name == ".." || self.show_hidden || !e.name.starts_with('.'))
            .filter(|(_, e)| path_like || f.is_empty() || e.name.to_lowercase().contains(&f))
            .map(|(i, _)| i)
            .collect();
        // Prefer the first real match over "..".
        self.selected = if !f.is_empty() && self.visible.len() > 1 && !path_like {
            self.visible
                .iter()
                .position(|&i| self.all[i].name.to_lowercase().starts_with(&f))
                .unwrap_or(1.min(self.visible.len() - 1))
        } else {
            0
        };
        self.scroll = 0;
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.visible.iter().map(|&i| &self.all[i])
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.visible.get(self.selected).map(|&i| &self.all[i])
    }

    fn cd(&mut self, path: PathBuf) {
        let prev = self.dir.file_name().map(|n| n.to_string_lossy().into_owned());
        let going_up = path == self.dir.parent().map(Path::to_path_buf).unwrap_or_default();
        self.dir = crate::fileio::absolute(&path);
        if self.mode == BrowserMode::Open {
            self.input.clear();
        }
        self.refresh();
        // When going up, select the directory we came from.
        if going_up {
            if let Some(prev) = prev {
                let found = self.entries().position(|e| e.name == prev);
                if let Some(p) = found {
                    self.selected = p;
                }
            }
        }
    }

    pub fn up_dir(&mut self) {
        if let Some(p) = self.dir.parent() {
            self.cd(p.to_path_buf());
        }
    }

    fn resolve(&self, s: &str) -> PathBuf {
        let p = expand_tilde(s);
        if p.is_absolute() { p } else { self.dir.join(p) }
    }

    pub fn move_sel(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let n = self.visible.len() as isize;
        self.selected = (self.selected as isize + delta).clamp(0, n - 1) as usize;
    }

    /// Activates the selected entry (directory: enter; file: open/pick).
    pub fn activate(&mut self) -> BrowserAction {
        let Some(e) = self.selected_entry().cloned() else { return BrowserAction::None };
        if e.name == ".." {
            self.up_dir();
            return BrowserAction::None;
        }
        let path = self.dir.join(&e.name);
        if e.is_dir {
            self.cd(path);
            BrowserAction::None
        } else if self.mode == BrowserMode::Open {
            BrowserAction::Open(path)
        } else {
            self.input.set(&e.name);
            BrowserAction::None
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> BrowserAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => return BrowserAction::Close,
            KeyCode::Up => self.move_sel(-1),
            KeyCode::Down => self.move_sel(1),
            KeyCode::PageUp => self.move_sel(-(self.page as isize)),
            KeyCode::PageDown => self.move_sel(self.page as isize),
            KeyCode::Home if ctrl || self.input.text.is_empty() => self.selected = 0,
            KeyCode::End if ctrl || self.input.text.is_empty() => self.selected = self.visible.len().saturating_sub(1),
            KeyCode::Char('h') if ctrl || alt => {
                self.show_hidden = !self.show_hidden;
                self.apply_filter();
            }
            KeyCode::Char('r') if ctrl => self.refresh(),
            KeyCode::Tab => {
                // Tab: complete the selected entry's name into the input.
                if let Some(e) = self.selected_entry().cloned() {
                    if e.is_dir {
                        return self.activate();
                    }
                    self.input.set(&e.name);
                    if self.mode == BrowserMode::Open {
                        self.apply_filter();
                    }
                }
            }
            KeyCode::Backspace if self.input.text.is_empty() => self.up_dir(),
            KeyCode::Left if self.input.text.is_empty() => self.up_dir(),
            KeyCode::Right if self.input.text.is_empty() => {
                if self.selected_entry().is_some_and(|e| e.is_dir) {
                    return self.activate();
                }
            }
            KeyCode::Enter => return self.submit(),
            _ => {
                if self.input.handle_key(key) == InputResult::Consumed && self.mode == BrowserMode::Open {
                    self.apply_filter();
                }
            }
        }
        self.error = None;
        BrowserAction::None
    }

    fn submit(&mut self) -> BrowserAction {
        let text = self.input.text.trim().to_owned();
        match self.mode {
            BrowserMode::Open => {
                if text.contains('/') || text.starts_with('~') {
                    let p = self.resolve(&text);
                    if p.is_dir() {
                        self.cd(p);
                        return BrowserAction::None;
                    }
                    return BrowserAction::Open(p);
                }
                if self.visible.is_empty() && !text.is_empty() {
                    // No match: open (create) a new file with that name.
                    return BrowserAction::Open(self.dir.join(text));
                }
                self.activate()
            }
            BrowserMode::Save => {
                if text.is_empty() {
                    return self.activate();
                }
                let p = self.resolve(&text);
                if p.is_dir() {
                    self.cd(p);
                    self.input.clear();
                    return BrowserAction::None;
                }
                if p.parent().is_some_and(|d| !d.as_os_str().is_empty() && !d.is_dir()) {
                    self.error = Some("directory does not exist".into());
                    return BrowserAction::None;
                }
                BrowserAction::Save(p)
            }
        }
    }

    /// Handles a mouse click on list row `row` (0-based). Returns an action
    /// on double-activation (clicking an already-selected row).
    pub fn click(&mut self, row: usize) -> BrowserAction {
        let idx = self.scroll + row;
        if idx >= self.visible.len() {
            return BrowserAction::None;
        }
        if idx == self.selected {
            return self.activate();
        }
        self.selected = idx;
        BrowserAction::None
    }

    /// Keeps the selection within the visible window of `height` rows.
    pub fn fix_scroll(&mut self, height: usize) {
        self.page = height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
        let max_scroll = self.visible.len().saturating_sub(height);
        self.scroll = self.scroll.min(max_scroll);
    }
}

pub fn human_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{n}B") } else { format!("{v:.1}{}", UNITS[u]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browse_and_filter() {
        let dir = std::env::temp_dir().join(format!("text_killer-br-{}", std::process::id()));
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("alpha.txt"), "a").unwrap();
        fs::write(dir.join("beta.py"), "b").unwrap();
        fs::write(dir.join(".hidden"), "h").unwrap();
        let mut b = FileBrowser::new(BrowserMode::Open, &dir, "", 0);
        let names: Vec<_> = b.entries().map(|e| e.name.clone()).collect();
        assert_eq!(names, vec!["..", "sub", "alpha.txt", "beta.py"]);
        b.input.set("bet");
        b.apply_filter();
        assert_eq!(b.selected_entry().unwrap().name, "beta.py");
        match b.submit() {
            BrowserAction::Open(p) => assert!(p.ends_with("beta.py")),
            _ => panic!("expected open"),
        }
        b.input.clear();
        b.apply_filter();
        b.selected = 1;
        b.activate();
        assert!(b.dir.ends_with("sub"));
        b.up_dir();
        assert_eq!(b.selected_entry().unwrap().name, "sub");
        assert_eq!(human_size(2048), "2.0K");
        fs::remove_dir_all(&dir).unwrap();
    }
}
