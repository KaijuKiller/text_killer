//! User settings and their optional persistence across sessions.
//!
//! Settings live in a small `key = value` text file:
//!   Linux/BSD: $XDG_CONFIG_HOME/text_killer/settings.conf (~/.config/...)
//!   macOS:     ~/Library/Application Support/text_killer/settings.conf
//!   Windows:   %APPDATA%\text_killer\settings.conf
//! The TEXT_KILLER_CONFIG environment variable overrides the path.
//!
//! Nothing is written unless the user turns on "Remember theme" and/or
//! "Remember page view & layout" in Options › Settings.

use std::fs;
use std::path::PathBuf;

use crate::theme::THEMES;

/// How documents are laid out in the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageMode {
    /// The original continuous view (one long column).
    Off,
    /// As many pages side by side as fit (up to [`MAX_PAGES`]).
    Auto,
    /// A fixed number of pages (1 to [`MAX_PAGES`]), if they fit.
    Fixed(u8),
}

/// Most pages shown side by side.
pub const MAX_PAGES: u8 = 12;

/// How pages are sized on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageScale {
    /// True page size (e.g. 65×54 for Letter); unused space stays empty.
    Actual,
    /// Pages stretch to fill the window's width and height. Each page then
    /// holds more text, so page boundaries depend on the window size.
    Fill,
}

impl PageScale {
    pub fn id(self) -> &'static str {
        match self {
            Self::Actual => "actual",
            Self::Fill => "fill",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "actual" | "true" | "real" | "100" => Some(Self::Actual),
            "fill" | "fit" | "stretch" => Some(Self::Fill),
            _ => None,
        }
    }
}

impl PageMode {
    pub fn id(self) -> String {
        match self {
            Self::Off => "off".into(),
            Self::Auto => "auto".into(),
            Self::Fixed(n) => n.to_string(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "0" | "original" => Some(Self::Off),
            "auto" | "on" => Some(Self::Auto),
            n => n.parse::<u8>().ok().filter(|n| (1..=MAX_PAGES).contains(n)).map(Self::Fixed),
        }
    }
}

/// Page size in character cells (columns × lines of text).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSize {
    pub cols: u16,
    pub rows: u16,
}

/// Named page sizes: (id, menu label, size). Sizes assume 1 in margins,
/// 10 characters per inch and 6 lines per inch (typewriter metrics).
pub const PAGE_SIZES: &[(&str, &str, PageSize)] = &[
    ("letter", "Letter 8.5×11 in (65×54)", PageSize { cols: 65, rows: 54 }),
    ("a4", "A4 210×297 mm (63×58)", PageSize { cols: 63, rows: 58 }),
    ("legal", "Legal 8.5×14 in (65×72)", PageSize { cols: 65, rows: 72 }),
    ("a5", "A5 148×210 mm (42×40)", PageSize { cols: 42, rows: 40 }),
    ("wide", "Wide 80 columns (80×54)", PageSize { cols: 80, rows: 54 }),
];

impl PageSize {
    pub const LETTER: Self = Self { cols: 65, rows: 54 };

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        if let Some((_, _, size)) = PAGE_SIZES.iter().find(|(id, _, _)| *id == s) {
            return Some(*size);
        }
        let (w, h) = s.split_once(['x', '×', '*'])?;
        let size = Self { cols: w.trim().parse().ok()?, rows: h.trim().parse().ok()? };
        ((20..=200).contains(&size.cols) && (10..=200).contains(&size.rows)).then_some(size)
    }

    pub fn id(self) -> String {
        match PAGE_SIZES.iter().find(|(_, _, s)| *s == self) {
            Some((id, _, _)) => (*id).to_owned(),
            None => format!("{}x{}", self.cols, self.rows),
        }
    }
}

pub struct Settings {
    pub line_numbers: bool,
    pub sidebar: bool,
    pub whitespace: bool,
    pub mouse: bool,
    pub theme: usize,
    pub page_mode: PageMode,
    pub page_size: PageSize,
    pub page_scale: PageScale,
    /// Soft-wrap long lines in the continuous (page view off) mode.
    pub wrap: bool,
    pub remember_theme: bool,
    pub remember_layout: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            line_numbers: true,
            sidebar: true,
            whitespace: false,
            mouse: true,
            theme: 0,
            page_mode: PageMode::Off,
            page_size: PageSize::LETTER,
            page_scale: PageScale::Actual,
            wrap: false,
            remember_theme: false,
            remember_layout: false,
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("TEXT_KILLER_CONFIG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(windows) {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library").join("Application Support"))
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }?;
    Some(base.join("text_killer").join("settings.conf"))
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

impl Settings {
    /// Applies `key = value` lines on top of the current values. Unknown
    /// keys and invalid values are ignored so old/new files stay compatible.
    pub fn apply(&mut self, text: &str) {
        let mut theme = None;
        let mut layout: Vec<(String, String)> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_owned());
            match k.as_str() {
                "remember_theme" => self.remember_theme = parse_bool(&v).unwrap_or(false),
                "remember_layout" => self.remember_layout = parse_bool(&v).unwrap_or(false),
                "theme" => theme = crate::theme::find(&v),
                _ => layout.push((k, v)),
            }
        }
        if self.remember_theme {
            if let Some(t) = theme {
                self.theme = t;
            }
        }
        if !self.remember_layout {
            return;
        }
        for (k, v) in layout {
            match k.as_str() {
                "page_view" => self.page_mode = PageMode::parse(&v).unwrap_or(self.page_mode),
                "page_size" => self.page_size = PageSize::parse(&v).unwrap_or(self.page_size),
                "page_scale" => self.page_scale = PageScale::parse(&v).unwrap_or(self.page_scale),
                "wrap" => self.wrap = parse_bool(&v).unwrap_or(self.wrap),
                "line_numbers" => self.line_numbers = parse_bool(&v).unwrap_or(self.line_numbers),
                "sidebar" => self.sidebar = parse_bool(&v).unwrap_or(self.sidebar),
                "whitespace" => self.whitespace = parse_bool(&v).unwrap_or(self.whitespace),
                "mouse" => self.mouse = parse_bool(&v).unwrap_or(self.mouse),
                _ => {}
            }
        }
    }

    pub fn serialize(&self) -> String {
        let mut s = String::from(
            "# text_killer settings. Edited by Options › Settings; safe to edit by hand.\n\
             # Delete this file to return to the defaults.\n\n",
        );
        s.push_str(&format!("remember_theme = {}\n", self.remember_theme));
        if self.remember_theme {
            s.push_str(&format!("theme = {}\n", THEMES[self.theme % THEMES.len()].id));
        }
        s.push_str(&format!("\nremember_layout = {}\n", self.remember_layout));
        if self.remember_layout {
            s.push_str(&format!("page_view = {}\n", self.page_mode.id()));
            s.push_str(&format!("page_size = {}\n", self.page_size.id()));
            s.push_str(&format!("page_scale = {}\n", self.page_scale.id()));
            s.push_str(&format!("wrap = {}\n", self.wrap));
            s.push_str(&format!("line_numbers = {}\n", self.line_numbers));
            s.push_str(&format!("sidebar = {}\n", self.sidebar));
            s.push_str(&format!("whitespace = {}\n", self.whitespace));
            s.push_str(&format!("mouse = {}\n", self.mouse));
        }
        s
    }

    /// Defaults overlaid with the saved file, if any.
    pub fn load() -> Self {
        let mut s = Self::default();
        if let Some(text) = config_path().and_then(|p| fs::read_to_string(p).ok()) {
            s.apply(&text);
        }
        s
    }

    /// Writes the settings file (when anything is remembered) or removes it
    /// (when nothing is). Returns the path touched.
    pub fn save(&self) -> Result<PathBuf, String> {
        let path = config_path().ok_or("no home/config directory found")?;
        if !self.remember_theme && !self.remember_layout {
            if path.exists() {
                fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            }
            return Ok(path);
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let tmp = path.with_extension("conf.tmp");
        fs::write(&tmp, self.serialize()).map_err(|e| format!("{}: {e}", tmp.display()))?;
        fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_crimson_and_continuous_view() {
        let s = Settings::default();
        assert_eq!(s.theme, 0);
        assert_eq!(s.page_mode, PageMode::Off);
        assert_eq!(s.page_size, PageSize { cols: 65, rows: 54 });
        assert!(!s.remember_theme && !s.remember_layout);
    }

    #[test]
    fn roundtrip_and_selective_memory() {
        let mut s = Settings {
            theme: 3,
            page_mode: PageMode::Fixed(2),
            page_size: PageSize { cols: 70, rows: 60 },
            wrap: true,
            page_scale: PageScale::Fill,
            remember_theme: true,
            ..Settings::default()
        };
        let text = s.serialize();
        assert!(text.contains("theme = amber"));
        assert!(!text.contains("page_view"));

        // Layout values in the file are ignored unless remembered.
        let mut t = Settings::default();
        t.apply(&format!("{text}\npage_view = 3\n"));
        assert_eq!(t.theme, 3);
        assert_eq!(t.page_mode, PageMode::Off);

        s.remember_layout = true;
        let mut u = Settings::default();
        u.apply(&s.serialize());
        assert_eq!((u.page_mode, u.page_size, u.wrap), (PageMode::Fixed(2), PageSize { cols: 70, rows: 60 }, true));
        assert_eq!(u.page_scale, PageScale::Fill);
    }

    #[test]
    fn parsing() {
        assert_eq!(PageMode::parse("OFF"), Some(PageMode::Off));
        assert_eq!(PageMode::parse("4"), Some(PageMode::Fixed(4)));
        assert_eq!(PageMode::parse("9"), Some(PageMode::Fixed(9)));
        assert_eq!(PageMode::parse("13"), None);
        assert_eq!(PageScale::parse("Fill"), Some(PageScale::Fill));
        assert_eq!(PageSize::parse("a4"), Some(PageSize { cols: 63, rows: 58 }));
        assert_eq!(PageSize::parse("72x60"), Some(PageSize { cols: 72, rows: 60 }));
        assert_eq!(PageSize::parse("5x5"), None);
        assert_eq!(PageSize { cols: 65, rows: 54 }.id(), "letter");
        let mut s = Settings { remember_layout: true, ..Settings::default() };
        s.apply("garbage line\nwrap = maybe\npage_size = 1x1\nunknown = 3\n");
        assert!(!s.wrap);
        assert_eq!(s.page_size, PageSize::LETTER);
    }

    #[test]
    fn save_and_remove() {
        let dir = std::env::temp_dir().join(format!("tk-cfg-{}", std::process::id()));
        let path = dir.join("sub").join("settings.conf");
        // SAFETY: this test is the only code that sets TEXT_KILLER_CONFIG.
        unsafe { std::env::set_var("TEXT_KILLER_CONFIG", &path) };
        let mut s = Settings { remember_theme: true, theme: 2, ..Settings::default() };
        s.save().unwrap();
        assert_eq!(Settings::load().theme, 2);
        s.remember_theme = false;
        s.save().unwrap();
        assert!(!path.exists());
        unsafe { std::env::remove_var("TEXT_KILLER_CONFIG") };
        let _ = fs::remove_dir_all(&dir);
    }
}
