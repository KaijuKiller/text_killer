//! A single-line text input with history and horizontal scrolling, used by
//! the command line and the file browser.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthChar;

#[derive(Default)]
pub struct LineInput {
    pub text: String,
    /// Cursor position in chars.
    pub cursor: usize,
    history: Vec<String>,
    hist_idx: Option<usize>,
    stash: String,
}

/// Result of feeding a key to the input.
#[derive(Debug, PartialEq, Eq)]
pub enum InputResult {
    Consumed,
    Submit,
    Cancel,
    Ignored,
}

impl LineInput {
    pub fn set(&mut self, s: &str) {
        self.text = s.to_owned();
        self.cursor = s.chars().count();
        self.hist_idx = None;
    }

    pub fn clear(&mut self) {
        self.set("");
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, col: usize) -> usize {
        crate::document::byte_idx(&self.text, col)
    }

    pub fn push_history(&mut self, s: &str) {
        if !s.trim().is_empty() && self.history.last().map(String::as_str) != Some(s) {
            self.history.push(s.to_owned());
            if self.history.len() > 500 {
                self.history.remove(0);
            }
        }
        self.hist_idx = None;
    }

    pub fn insert_str(&mut self, s: &str) {
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        let b = self.byte(self.cursor);
        self.text.insert_str(b, &s);
        self.cursor += s.chars().count();
    }

    fn word_left(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i - 1].is_whitespace() && chars[i - 1] != '/' {
            i -= 1;
        }
        if i == self.cursor && i > 0 {
            i -= 1;
        }
        i
    }

    fn word_right(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        i
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> InputResult {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Enter => return InputResult::Submit,
            KeyCode::Esc => return InputResult::Cancel,
            KeyCode::Char('c' | 'g') if ctrl => return InputResult::Cancel,
            KeyCode::Left if ctrl || alt => self.cursor = self.word_left(),
            KeyCode::Right if ctrl || alt => self.cursor = self.word_right(),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len(),
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.len(),
            KeyCode::Char('u') if ctrl => {
                let b = self.byte(self.cursor);
                self.text.drain(..b);
                self.cursor = 0;
            }
            KeyCode::Char('k') if ctrl => {
                let b = self.byte(self.cursor);
                self.text.truncate(b);
            }
            KeyCode::Char('w') if ctrl => self.delete_word_back(),
            KeyCode::Backspace if ctrl || alt => self.delete_word_back(),
            KeyCode::Backspace | KeyCode::Char('h') if key.code == KeyCode::Backspace || ctrl => {
                if self.cursor > 0 {
                    let b0 = self.byte(self.cursor - 1);
                    let b1 = self.byte(self.cursor);
                    self.text.drain(b0..b1);
                    self.cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.len() {
                    let b0 = self.byte(self.cursor);
                    let b1 = self.byte(self.cursor + 1);
                    self.text.drain(b0..b1);
                }
            }
            KeyCode::Up => self.history_prev(),
            KeyCode::Down => self.history_next(),
            KeyCode::Char(c) if !ctrl => {
                let mut buf = [0u8; 4];
                self.insert_str(c.encode_utf8(&mut buf));
            }
            _ => return InputResult::Ignored,
        }
        InputResult::Consumed
    }

    fn delete_word_back(&mut self) {
        let start = self.word_left();
        let b0 = self.byte(start);
        let b1 = self.byte(self.cursor);
        self.text.drain(b0..b1);
        self.cursor = start;
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.hist_idx {
            None => {
                self.stash = self.text.clone();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.hist_idx = Some(idx);
        self.text = self.history[idx].clone();
        self.cursor = self.len();
    }

    fn history_next(&mut self) {
        let Some(i) = self.hist_idx else { return };
        if i + 1 < self.history.len() {
            self.hist_idx = Some(i + 1);
            self.text = self.history[i + 1].clone();
        } else {
            self.hist_idx = None;
            self.text = std::mem::take(&mut self.stash);
        }
        self.cursor = self.len();
    }

    /// Returns the visible slice for a field `width` cells wide and the
    /// cursor's x offset within it.
    pub fn view(&self, width: usize) -> (String, usize) {
        if width == 0 {
            return (String::new(), 0);
        }
        let chars: Vec<char> = self.text.chars().collect();
        let w = |c: &char| c.width().unwrap_or(0);
        let cursor_x: usize = chars[..self.cursor].iter().map(w).sum();
        // Scroll so the cursor stays within the field.
        let mut start = 0;
        let mut skipped = 0;
        while cursor_x - skipped >= width && start < chars.len() {
            skipped += w(&chars[start]);
            start += 1;
        }
        let mut out = String::new();
        let mut used = 0;
        for c in &chars[start..] {
            let cw = w(c);
            if used + cw > width {
                break;
            }
            out.push(*c);
            used += cw;
        }
        (out, cursor_x - skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn edit_and_history() {
        let mut li = LineInput::default();
        for c in "héllo".chars() {
            li.handle_key(key(KeyCode::Char(c)));
        }
        li.handle_key(key(KeyCode::Backspace));
        assert_eq!(li.text, "héll");
        li.push_history("héll");
        li.clear();
        li.handle_key(key(KeyCode::Up));
        assert_eq!(li.text, "héll");
        li.handle_key(key(KeyCode::Down));
        assert_eq!(li.text, "");
    }

    #[test]
    fn view_scrolls() {
        let mut li = LineInput::default();
        li.set("abcdefghij");
        let (s, x) = li.view(5);
        assert_eq!(s, "ghij");
        assert_eq!(x, 4);
    }
}
