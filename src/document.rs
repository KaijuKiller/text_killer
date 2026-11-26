//! The document model: a line buffer with cursor, selection, undo/redo and
//! cached syntax-highlighting state.

use std::path::PathBuf;

use crate::fileio::{Encoding, LineEnding};
use crate::syntax::{self, Language, State};

/// A position in the document. `col` is measured in chars, not bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub const fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// A primitive, invertible edit.
#[derive(Clone, Debug)]
enum Op {
    Insert { at: Pos, text: String },
    Delete { at: Pos, text: String },
}

/// Classifies edits so consecutive typing can be merged into one undo step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Deleting,
    Other,
}

#[derive(Clone, Debug)]
struct Group {
    id: u64,
    kind: EditKind,
    ops: Vec<Op>,
    cursor_before: Pos,
    cursor_after: Pos,
    sealed: bool,
}

/// Converts a char index into a byte index within `s`, clamped to `s.len()`.
pub fn byte_idx(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map_or(s.len(), |(i, _)| i)
}

pub fn char_len(s: &str) -> usize {
    s.chars().count()
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub struct Document {
    pub lines: Vec<String>,
    pub cursor: Pos,
    /// Selection anchor; the selection spans anchor..cursor when set.
    pub anchor: Option<Pos>,
    /// Remembered display column for vertical movement.
    pub desired_col: Option<usize>,
    pub scroll_line: usize,
    pub scroll_col: usize,
    pub path: Option<PathBuf>,
    pub name: String,
    pub language: &'static Language,
    pub encoding: Encoding,
    pub line_ending: LineEnding,
    pub readonly: bool,
    /// Set when the text was converted from a document format (e.g. "ODT document").
    pub format: Option<&'static str>,
    /// Saving back to the original file would drop formatting the editor can't show.
    pub lossy_import: bool,
    pub use_tabs: bool,
    pub tab_width: usize,
    pub overwrite: bool,
    undo: Vec<Group>,
    redo: Vec<Group>,
    next_id: u64,
    saved_id: u64,
    /// `hl_states[i]` is the highlighter state at the *start* of line i.
    hl_states: Vec<State>,
    /// Soft-wrap rows and page layout cache (see `wrap.rs`).
    pub(crate) wrap: crate::wrap::WrapCache,
    /// Incremented on every text change.
    pub edit_version: u64,
    /// Visual row within `scroll_line` at the top of the wrapped view.
    pub scroll_row: usize,
    /// First page shown in page view, and its vertical scroll offset.
    pub page_first: usize,
    pub page_scroll: usize,
}

impl Document {
    pub fn new_empty(name: impl Into<String>) -> Self {
        Self::from_text(name.into(), "", None)
    }

    pub fn from_text(name: String, text: &str, path: Option<PathBuf>) -> Self {
        let lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
        let language = syntax::detect(&name, lines.first().map(String::as_str).unwrap_or(""));
        let use_tabs = lines.iter().take(2000).any(|l| l.starts_with('\t'));
        let mut doc = Self {
            lines,
            cursor: Pos::default(),
            anchor: None,
            desired_col: None,
            scroll_line: 0,
            scroll_col: 0,
            path,
            name,
            language,
            encoding: Encoding::Utf8,
            line_ending: LineEnding::Lf,
            readonly: false,
            format: None,
            lossy_import: false,
            use_tabs,
            tab_width: 4,
            overwrite: false,
            undo: Vec::new(),
            redo: Vec::new(),
            next_id: 1,
            saved_id: 0,
            hl_states: Vec::new(),
            wrap: Default::default(),
            edit_version: 0,
            scroll_row: 0,
            page_first: 0,
            page_scroll: 0,
        };
        doc.tab_width = doc.language.tab_width;
        doc
    }

    // ------------------------------------------------------------------
    // Basic queries
    // ------------------------------------------------------------------

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line_len(&self, line: usize) -> usize {
        self.lines.get(line).map_or(0, |l| char_len(l))
    }

    pub fn is_dirty(&self) -> bool {
        self.current_id() != self.saved_id
    }

    fn current_id(&self) -> u64 {
        self.undo.last().map_or(0, |g| g.id)
    }

    pub fn mark_saved(&mut self) {
        self.saved_id = self.current_id();
        self.seal();
    }

    pub fn clamp(&self, p: Pos) -> Pos {
        let line = p.line.min(self.lines.len().saturating_sub(1));
        Pos::new(line, p.col.min(self.line_len(line)))
    }

    pub fn end_pos(&self) -> Pos {
        let last = self.lines.len() - 1;
        Pos::new(last, self.line_len(last))
    }

    pub fn char_at(&self, p: Pos) -> Option<char> {
        self.lines.get(p.line)?.chars().nth(p.col)
    }

    /// Ordered selection range, if a non-empty selection exists.
    pub fn selection(&self) -> Option<(Pos, Pos)> {
        let a = self.anchor?;
        if a == self.cursor {
            return None;
        }
        Some(if a < self.cursor { (a, self.cursor) } else { (self.cursor, a) })
    }

    pub fn text_range(&self, start: Pos, end: Pos) -> String {
        if start.line == end.line {
            let l = &self.lines[start.line];
            return l[byte_idx(l, start.col)..byte_idx(l, end.col)].to_owned();
        }
        let mut out = String::new();
        let first = &self.lines[start.line];
        out.push_str(&first[byte_idx(first, start.col)..]);
        for l in &self.lines[start.line + 1..end.line] {
            out.push('\n');
            out.push_str(l);
        }
        out.push('\n');
        let last = &self.lines[end.line];
        out.push_str(&last[..byte_idx(last, end.col)]);
        out
    }

    pub fn selected_text(&self) -> Option<String> {
        self.selection().map(|(s, e)| self.text_range(s, e))
    }

    pub fn full_text(&self) -> String {
        self.lines.join("\n")
    }

    /// Lines touched by the selection, or the cursor line.
    pub fn selected_line_range(&self) -> (usize, usize) {
        match self.selection() {
            Some((s, e)) => {
                let end = if e.col == 0 && e.line > s.line { e.line - 1 } else { e.line };
                (s.line, end)
            }
            None => (self.cursor.line, self.cursor.line),
        }
    }

    // ------------------------------------------------------------------
    // Undo machinery
    // ------------------------------------------------------------------

    /// Prevents the next edit from merging with the previous undo group.
    pub fn seal(&mut self) {
        if let Some(g) = self.undo.last_mut() {
            g.sealed = true;
        }
    }

    fn begin(&mut self, kind: EditKind) {
        self.redo.clear();
        let mergeable = matches!(self.undo.last(),
            Some(g) if !g.sealed && g.kind == kind && kind != EditKind::Other);
        if !mergeable {
            self.seal();
            let id = self.next_id;
            self.next_id += 1;
            self.undo.push(Group {
                id,
                kind,
                ops: Vec::new(),
                cursor_before: self.cursor,
                cursor_after: self.cursor,
                sealed: false,
            });
        }
    }

    fn finish(&mut self) {
        let cursor = self.cursor;
        if let Some(g) = self.undo.last_mut() {
            g.cursor_after = cursor;
            if g.kind == EditKind::Other {
                g.sealed = true;
            }
        }
        // Drop empty groups so they don't affect dirty state.
        if self.undo.last().is_some_and(|g| g.ops.is_empty()) {
            self.undo.pop();
        }
    }

    /// Runs `f` as a single undoable transaction.
    pub fn transact<R>(&mut self, kind: EditKind, f: impl FnOnce(&mut Self) -> R) -> R {
        self.begin(kind);
        let r = f(self);
        self.finish();
        r
    }

    pub fn undo(&mut self) -> bool {
        let Some(mut g) = self.undo.pop() else { return false };
        for op in g.ops.iter().rev() {
            match op {
                Op::Insert { at, text } => {
                    let end = advance(*at, text);
                    self.raw_delete(*at, end);
                }
                Op::Delete { at, text } => {
                    self.raw_insert(*at, text);
                }
            }
        }
        self.cursor = self.clamp(g.cursor_before);
        self.anchor = None;
        self.desired_col = None;
        g.sealed = true;
        self.redo.push(g);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(g) = self.redo.pop() else { return false };
        for op in &g.ops {
            match op {
                Op::Insert { at, text } => {
                    self.raw_insert(*at, text);
                }
                Op::Delete { at, text } => {
                    let end = advance(*at, text);
                    self.raw_delete(*at, end);
                }
            }
        }
        self.cursor = self.clamp(g.cursor_after);
        self.anchor = None;
        self.desired_col = None;
        self.undo.push(g);
        true
    }

    // ------------------------------------------------------------------
    // Primitive edits (all mutation flows through these)
    // ------------------------------------------------------------------

    fn raw_insert(&mut self, at: Pos, text: &str) -> Pos {
        let at = self.clamp(at);
        self.invalidate_hl(at.line);
        self.edit_version += 1;
        self.wrap.edit(at.line, 0, text.matches('\n').count());
        let line = &mut self.lines[at.line];
        let bi = byte_idx(line, at.col);
        let tail = line.split_off(bi);
        let mut parts = text.split('\n');
        line.push_str(parts.next().unwrap_or(""));
        let mut new_lines: Vec<String> = parts.map(str::to_owned).collect();
        let end;
        if new_lines.is_empty() {
            end = Pos::new(at.line, char_len(line));
            line.push_str(&tail);
        } else {
            let count = new_lines.len();
            let last = new_lines.last_mut().unwrap();
            end = Pos::new(at.line + count, char_len(last));
            last.push_str(&tail);
            let idx = at.line + 1;
            self.lines.splice(idx..idx, new_lines.drain(..));
        }
        end
    }

    fn raw_delete(&mut self, start: Pos, end: Pos) -> String {
        let (start, end) = (self.clamp(start), self.clamp(end));
        if start >= end {
            return String::new();
        }
        self.invalidate_hl(start.line);
        self.edit_version += 1;
        self.wrap.edit(start.line, end.line - start.line, 0);
        let removed = self.text_range(start, end);
        let last_tail = {
            let l = &self.lines[end.line];
            l[byte_idx(l, end.col)..].to_owned()
        };
        let first = &mut self.lines[start.line];
        first.truncate(byte_idx(first, start.col));
        first.push_str(&last_tail);
        if end.line > start.line {
            self.lines.drain(start.line + 1..=end.line);
        }
        removed
    }

    /// Undoable insert. Returns the position after the inserted text.
    pub fn insert(&mut self, at: Pos, text: &str) -> Pos {
        if text.is_empty() {
            return at;
        }
        let at = self.clamp(at);
        let end = self.raw_insert(at, text);
        if let Some(g) = self.undo.last_mut() {
            // Merge adjacent inserts for compact undo history.
            if let Some(Op::Insert { at: pa, text: pt }) = g.ops.last_mut() {
                if advance(*pa, pt) == at {
                    pt.push_str(text);
                    return end;
                }
            }
            g.ops.push(Op::Insert { at, text: text.to_owned() });
        }
        end
    }

    /// Undoable delete of `start..end`. Returns the removed text.
    pub fn delete(&mut self, start: Pos, end: Pos) -> String {
        let removed = self.raw_delete(start, end);
        if removed.is_empty() {
            return removed;
        }
        let start = self.clamp(start);
        if let Some(g) = self.undo.last_mut() {
            g.ops.push(Op::Delete { at: start, text: removed.clone() });
        }
        removed
    }

    /// Replaces a whole line's content (undoable within a transaction).
    pub fn replace_line(&mut self, line: usize, new: &str) {
        if self.lines[line] == new {
            return;
        }
        let len = self.line_len(line);
        self.delete(Pos::new(line, 0), Pos::new(line, len));
        self.insert(Pos::new(line, 0), new);
    }

    // ------------------------------------------------------------------
    // High-level editing commands
    // ------------------------------------------------------------------

    /// Deletes the selection if any. Must be called inside a transaction.
    pub fn delete_selection(&mut self) -> bool {
        if let Some((s, e)) = self.selection() {
            self.delete(s, e);
            self.cursor = s;
            self.anchor = None;
            true
        } else {
            self.anchor = None;
            false
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if self.readonly {
            return;
        }
        let kind = if c.is_whitespace() { EditKind::Other } else { EditKind::Typing };
        self.transact(kind, |d| {
            let had_sel = d.delete_selection();
            if d.overwrite && !had_sel && d.cursor.col < d.line_len(d.cursor.line) {
                let next = Pos::new(d.cursor.line, d.cursor.col + 1);
                d.delete(d.cursor, next);
            }
            let mut buf = [0u8; 4];
            d.cursor = d.insert(d.cursor, c.encode_utf8(&mut buf));
            if let Some(close) = d.language.auto_dedent_char(c) {
                d.auto_dedent(close);
            }
        });
        self.desired_col = None;
    }

    pub fn insert_text(&mut self, text: &str) {
        if self.readonly {
            return;
        }
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        self.transact(EditKind::Other, |d| {
            d.delete_selection();
            d.cursor = d.insert(d.cursor, &text);
        });
        self.desired_col = None;
    }

    pub fn indent_unit(&self) -> String {
        if self.use_tabs { "\t".into() } else { " ".repeat(self.tab_width) }
    }

    fn leading_ws(&self, line: usize) -> String {
        self.lines[line].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
    }

    pub fn newline(&mut self) {
        if self.readonly {
            return;
        }
        self.transact(EditKind::Other, |d| {
            d.delete_selection();
            let line = d.cursor.line;
            let mut indent = d.leading_ws(line);
            let indent_len = char_len(&indent);
            let before: String = d.lines[line].chars().take(d.cursor.col).collect();
            let trimmed = before.trim_end();
            if d.language.indent_after(trimmed) {
                indent.push_str(&d.indent_unit());
            }
            // Cursor inside leading whitespace: don't duplicate it.
            if d.cursor.col < indent_len {
                indent = indent.chars().take(d.cursor.col).collect();
            }
            let after = d.char_at(d.cursor);
            let between_pair = matches!(
                (trimmed.chars().last(), after),
                (Some('{'), Some('}')) | (Some('['), Some(']')) | (Some('('), Some(')'))
            ) || (trimmed.ends_with('>') && d.language.name == "HTML" && d.lines[line]
                .get(byte_idx(&d.lines[line], d.cursor.col)..)
                .is_some_and(|r| r.starts_with("</")));
            let text = format!("\n{indent}");
            d.cursor = d.insert(d.cursor, &text);
            if between_pair {
                let base = d.leading_ws(line);
                let save = d.cursor;
                d.insert(d.cursor, &format!("\n{base}"));
                d.cursor = save;
            }
        });
        self.desired_col = None;
    }

    /// When a closing bracket is typed on an otherwise blank line, remove one indent level.
    fn auto_dedent(&mut self, _close: char) {
        let line = self.cursor.line;
        let content = &self.lines[line];
        let trimmed = content.trim_start();
        if char_len(trimmed) != 1 || line == 0 {
            return;
        }
        let ws = self.leading_ws(line);
        if ws.is_empty() {
            return;
        }
        let unit = self.indent_unit();
        let new_ws = if let Some(stripped) = ws.strip_suffix(&unit) {
            stripped.to_owned()
        } else if ws.ends_with('\t') {
            ws[..ws.len() - 1].to_owned()
        } else {
            let n = char_len(&ws).saturating_sub(self.tab_width);
            ws.chars().take(n).collect()
        };
        let removed = char_len(&ws) - char_len(&new_ws);
        if removed > 0 {
            self.delete(Pos::new(line, char_len(&new_ws)), Pos::new(line, char_len(&ws)));
            self.cursor.col -= removed;
        }
    }

    pub fn backspace(&mut self) {
        if self.readonly {
            return;
        }
        self.transact(EditKind::Deleting, |d| {
            if d.delete_selection() {
                return;
            }
            let c = d.cursor;
            if c.col == 0 {
                if c.line == 0 {
                    return;
                }
                let prev = Pos::new(c.line - 1, d.line_len(c.line - 1));
                d.delete(prev, c);
                d.cursor = prev;
                return;
            }
            // Smart backspace through soft-tab indentation.
            let before: String = d.lines[c.line].chars().take(c.col).collect();
            let start_col = if !d.use_tabs && !before.is_empty() && before.chars().all(|ch| ch == ' ') {
                let rem = c.col % d.tab_width;
                c.col - if rem == 0 { d.tab_width.min(c.col) } else { rem }
            } else {
                c.col - 1
            };
            let start = Pos::new(c.line, start_col);
            d.delete(start, c);
            d.cursor = start;
        });
        self.desired_col = None;
    }

    pub fn delete_forward(&mut self) {
        if self.readonly {
            return;
        }
        self.transact(EditKind::Deleting, |d| {
            if d.delete_selection() {
                return;
            }
            let c = d.cursor;
            let end = if c.col < d.line_len(c.line) {
                Pos::new(c.line, c.col + 1)
            } else if c.line + 1 < d.line_count() {
                Pos::new(c.line + 1, 0)
            } else {
                return;
            };
            d.delete(c, end);
        });
    }

    pub fn delete_word_back(&mut self) {
        if self.readonly {
            return;
        }
        self.transact(EditKind::Other, |d| {
            if d.delete_selection() {
                return;
            }
            let start = d.word_left(d.cursor);
            d.delete(start, d.cursor);
            d.cursor = start;
        });
    }

    pub fn delete_word_forward(&mut self) {
        if self.readonly {
            return;
        }
        self.transact(EditKind::Other, |d| {
            if d.delete_selection() {
                return;
            }
            let end = d.word_right(d.cursor);
            d.delete(d.cursor, end);
        });
    }

    /// Cuts selection, or the whole current line if nothing is selected.
    pub fn cut(&mut self) -> Option<String> {
        if self.readonly {
            return self.copy();
        }
        let text = self.copy()?;
        self.transact(EditKind::Other, |d| {
            if !d.delete_selection() {
                let l = d.cursor.line;
                if d.line_count() == 1 {
                    let len = d.line_len(0);
                    d.delete(Pos::new(0, 0), Pos::new(0, len));
                } else if l + 1 < d.line_count() {
                    d.delete(Pos::new(l, 0), Pos::new(l + 1, 0));
                } else {
                    let prev_len = d.line_len(l - 1);
                    let len = d.line_len(l);
                    d.delete(Pos::new(l - 1, prev_len), Pos::new(l, len));
                }
                d.cursor = d.clamp(Pos::new(l, 0));
            }
        });
        Some(text)
    }

    pub fn copy(&self) -> Option<String> {
        match self.selected_text() {
            Some(t) => Some(t),
            None => Some(format!("{}\n", self.lines[self.cursor.line])),
        }
    }

    // ------------------------------------------------------------------
    // Movement
    // ------------------------------------------------------------------

    /// Prepares for a movement: starts/extends or clears the selection.
    pub fn prepare_move(&mut self, select: bool) {
        self.seal();
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
    }

    pub fn move_to(&mut self, p: Pos, select: bool) {
        self.prepare_move(select);
        self.cursor = self.clamp(p);
        self.desired_col = None;
    }

    pub fn move_left(&mut self, select: bool) {
        if !select {
            if let Some((s, _)) = self.selection() {
                self.move_to(s, false);
                return;
            }
        }
        let c = self.cursor;
        let p = if c.col > 0 {
            Pos::new(c.line, c.col - 1)
        } else if c.line > 0 {
            Pos::new(c.line - 1, self.line_len(c.line - 1))
        } else {
            c
        };
        self.move_to(p, select);
    }

    pub fn move_right(&mut self, select: bool) {
        if !select {
            if let Some((_, e)) = self.selection() {
                self.move_to(e, false);
                return;
            }
        }
        let c = self.cursor;
        let p = if c.col < self.line_len(c.line) {
            Pos::new(c.line, c.col + 1)
        } else if c.line + 1 < self.line_count() {
            Pos::new(c.line + 1, 0)
        } else {
            c
        };
        self.move_to(p, select);
    }

    pub fn move_vertical(&mut self, delta: isize, select: bool) {
        self.prepare_move(select);
        let want = self
            .desired_col
            .unwrap_or_else(|| crate::ui::display_col(&self.lines[self.cursor.line], self.cursor.col, self.tab_width));
        let target = (self.cursor.line as isize + delta).clamp(0, self.line_count() as isize - 1) as usize;
        if target == self.cursor.line && delta != 0 {
            // At top/bottom edge: jump to line start/end like most editors.
            let col = if delta < 0 { 0 } else { self.line_len(target) };
            self.cursor = Pos::new(target, col);
            self.desired_col = None;
            return;
        }
        let col = crate::ui::char_col_at_display(&self.lines[target], want, self.tab_width);
        self.cursor = Pos::new(target, col);
        self.desired_col = Some(want);
    }

    pub fn home(&mut self, select: bool) {
        let first_non_ws = self.lines[self.cursor.line]
            .chars()
            .take_while(|c| c.is_whitespace())
            .count();
        let col = if self.cursor.col == first_non_ws { 0 } else { first_non_ws };
        self.move_to(Pos::new(self.cursor.line, col), select);
    }

    pub fn end(&mut self, select: bool) {
        let len = self.line_len(self.cursor.line);
        self.move_to(Pos::new(self.cursor.line, len), select);
    }

    pub fn word_left(&self, p: Pos) -> Pos {
        if p.col == 0 {
            return if p.line > 0 { Pos::new(p.line - 1, self.line_len(p.line - 1)) } else { p };
        }
        let chars: Vec<char> = self.lines[p.line].chars().collect();
        let mut i = p.col;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i > 0 && is_word_char(chars[i - 1]) {
            while i > 0 && is_word_char(chars[i - 1]) {
                i -= 1;
            }
        } else if i > 0 {
            while i > 0 && !is_word_char(chars[i - 1]) && !chars[i - 1].is_whitespace() {
                i -= 1;
            }
        }
        Pos::new(p.line, i)
    }

    pub fn word_right(&self, p: Pos) -> Pos {
        let chars: Vec<char> = self.lines[p.line].chars().collect();
        if p.col >= chars.len() {
            return if p.line + 1 < self.line_count() { Pos::new(p.line + 1, 0) } else { p };
        }
        let mut i = p.col;
        if is_word_char(chars[i]) {
            while i < chars.len() && is_word_char(chars[i]) {
                i += 1;
            }
        } else if !chars[i].is_whitespace() {
            while i < chars.len() && !is_word_char(chars[i]) && !chars[i].is_whitespace() {
                i += 1;
            }
        }
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        Pos::new(p.line, i)
    }

    pub fn select_all(&mut self) {
        self.seal();
        self.anchor = Some(Pos::new(0, 0));
        self.cursor = self.end_pos();
    }

    /// Selects the word under the cursor.
    pub fn select_word(&mut self) {
        let chars: Vec<char> = self.lines[self.cursor.line].chars().collect();
        let mut s = self.cursor.col.min(chars.len());
        let mut e = s;
        while s > 0 && is_word_char(chars[s - 1]) {
            s -= 1;
        }
        while e < chars.len() && is_word_char(chars[e]) {
            e += 1;
        }
        self.anchor = Some(Pos::new(self.cursor.line, s));
        self.cursor = Pos::new(self.cursor.line, e);
    }

    // ------------------------------------------------------------------
    // Search
    // ------------------------------------------------------------------

    /// Finds `needle` starting after `from`, wrapping around. Smart-case.
    pub fn find(&self, needle: &str, from: Pos, backwards: bool) -> Option<(Pos, Pos)> {
        if needle.is_empty() || needle.contains('\n') {
            return None;
        }
        let ci = !needle.chars().any(char::is_uppercase);
        let needle_cmp = if ci { needle.to_lowercase() } else { needle.to_owned() };
        let nlen = char_len(needle);
        let n = self.line_count();
        let matches_in = |line: usize| -> Vec<usize> {
            let hay = &self.lines[line];
            let hay_cmp = if ci { hay.to_lowercase() } else { hay.clone() };
            // Lowercasing may change byte lengths; fall back to char scanning.
            if hay_cmp.len() != hay.len() {
                let hc: Vec<char> = hay_cmp.chars().collect();
                let nc: Vec<char> = needle_cmp.chars().collect();
                return (0..hc.len().saturating_sub(nc.len() - 1))
                    .filter(|&i| hc[i..].starts_with(&nc))
                    .collect();
            }
            hay_cmp
                .match_indices(&needle_cmp)
                .map(|(b, _)| char_len(&hay[..b]))
                .collect()
        };
        for step in 0..=n {
            let line = if backwards { (from.line + n * 2 - step) % n } else { (from.line + step) % n };
            let ms = matches_in(line);
            let found = if backwards {
                ms.into_iter().rev().find(|&c| step > 0 || c < from.col)
            } else {
                ms.into_iter().find(|&c| step > 0 || c >= from.col)
            };
            if let Some(c) = found {
                return Some((Pos::new(line, c), Pos::new(line, c + nlen)));
            }
            if step == n {
                break;
            }
        }
        None
    }

    /// Replaces all occurrences (case-sensitive). Returns the count.
    pub fn replace_all(&mut self, from: &str, to: &str) -> usize {
        if from.is_empty() || self.readonly {
            return 0;
        }
        let mut count = 0;
        self.transact(EditKind::Other, |d| {
            // Bottom-up, so replacements containing newlines don't shift
            // lines that are still to be processed.
            for i in (0..d.line_count()).rev() {
                let n = d.lines[i].matches(from).count();
                if n > 0 {
                    count += n;
                    let new = d.lines[i].replace(from, to);
                    d.replace_line(i, &new);
                }
            }
            d.cursor = d.clamp(d.cursor);
            d.anchor = None;
        });
        count
    }

    // ------------------------------------------------------------------
    // Highlight cache
    // ------------------------------------------------------------------

    fn invalidate_hl(&mut self, line: usize) {
        self.hl_states.truncate(line + 1);
    }

    pub fn reset_highlight(&mut self) {
        self.hl_states.clear();
    }

    /// Returns the highlighter state at the start of `line`, computing and
    /// caching states for preceding lines as necessary.
    pub fn hl_state_at(&mut self, line: usize) -> State {
        if self.hl_states.is_empty() {
            self.hl_states.push(State::default());
        }
        while self.hl_states.len() <= line {
            let i = self.hl_states.len() - 1;
            let mut st = self.hl_states[i];
            syntax::highlight_line(self.language, &self.lines[i], &mut st);
            self.hl_states.push(st);
        }
        self.hl_states[line]
    }

    pub fn set_language(&mut self, lang: &'static Language) {
        self.language = lang;
        self.reset_highlight();
    }
}

/// Position after inserting `text` at `at`.
fn advance(at: Pos, text: &str) -> Pos {
    let nl = text.matches('\n').count();
    if nl == 0 {
        Pos::new(at.line, at.col + char_len(text))
    } else {
        let last = text.rsplit('\n').next().unwrap_or("");
        Pos::new(at.line + nl, char_len(last))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(t: &str) -> Document {
        Document::from_text("t.txt".into(), t, None)
    }

    #[test]
    fn insert_delete_roundtrip() {
        let mut d = doc("hello\nworld");
        d.transact(EditKind::Other, |d| {
            d.insert(Pos::new(0, 5), " there\nbig");
        });
        assert_eq!(d.full_text(), "hello there\nbig\nworld");
        d.undo();
        assert_eq!(d.full_text(), "hello\nworld");
        d.redo();
        assert_eq!(d.full_text(), "hello there\nbig\nworld");
        assert!(d.is_dirty());
        d.undo();
        assert!(!d.is_dirty());
    }

    #[test]
    fn typing_merges_into_one_undo() {
        let mut d = doc("");
        for c in "abc".chars() {
            d.insert_char(c);
        }
        assert_eq!(d.full_text(), "abc");
        d.undo();
        assert_eq!(d.full_text(), "");
    }

    #[test]
    fn backspace_joins_lines() {
        let mut d = doc("ab\ncd");
        d.cursor = Pos::new(1, 0);
        d.backspace();
        assert_eq!(d.full_text(), "abcd");
        assert_eq!(d.cursor, Pos::new(0, 2));
    }

    #[test]
    fn unicode_columns() {
        let mut d = doc("héllo wörld");
        d.cursor = Pos::new(0, 7);
        d.insert_char('X');
        assert_eq!(d.full_text(), "héllo wXörld");
    }

    #[test]
    fn find_wraps_and_smartcase() {
        let d = doc("Foo bar\nbaz foo");
        let (s, _) = d.find("foo", Pos::new(0, 1), false).unwrap();
        assert_eq!(s, Pos::new(1, 4));
        let (s, _) = d.find("foo", Pos::new(1, 5), false).unwrap();
        assert_eq!(s, Pos::new(0, 0));
        assert!(d.find("Foo", Pos::new(0, 1), false).unwrap().0 == Pos::new(0, 0));
        let (s, _) = d.find("foo", Pos::new(1, 4), true).unwrap();
        assert_eq!(s, Pos::new(0, 0));
    }

    #[test]
    fn python_newline_indents() {
        let mut d = Document::from_text("a.py".into(), "def f():", None);
        d.cursor = Pos::new(0, 8);
        d.newline();
        assert_eq!(d.full_text(), "def f():\n    ");
    }

    #[test]
    fn brace_newline_splits_pair() {
        let mut d = Document::from_text("a.css".into(), "a {}", None);
        d.cursor = Pos::new(0, 3);
        d.newline();
        assert_eq!(d.full_text(), "a {\n    \n}");
        assert_eq!(d.cursor, Pos::new(1, 4));
    }

    #[test]
    fn replace_all_counts() {
        let mut d = doc("a a\na");
        assert_eq!(d.replace_all("a", "bb"), 3);
        assert_eq!(d.full_text(), "bb bb\nbb");
        d.undo();
        assert_eq!(d.full_text(), "a a\na");
        assert_eq!(d.replace_all("a", "x\ny"), 3);
        assert_eq!(d.full_text(), "x\ny x\ny\nx\ny");
    }
}
