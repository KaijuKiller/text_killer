//! Soft wrapping and pagination.
//!
//! A document line is split into *rows* (char ranges) that fit a given
//! width, breaking after spaces, tabs or hyphens where possible. Rows are
//! then grouped into fixed-height *pages*. A line containing only a form
//! feed (`\f`) is a page break: it occupies one row and the next line
//! starts a new page.
//!
//! Row layouts are cached per line in the [`Document`] and invalidated
//! line-by-line as text is edited, so large documents stay responsive.

use std::rc::Rc;

use crate::document::{Document, Pos};
use crate::ui::{char_col_at_display, char_width};

/// A visual row: the char range `[start, end)` of one document line.
pub type Seg = (usize, usize);

/// A position in visual rows: (document line, row within that line).
pub type VRow = (usize, usize);

/// True for a line that is a page break (form feed, optionally padded).
pub fn is_page_break(line: &str) -> bool {
    line.contains('\u{c}') && line.chars().all(|c| matches!(c, '\u{c}' | ' ' | '\t'))
}

/// Splits `text` into rows no wider than `width` display cells. Spaces at a
/// break "hang" past the edge (as in word processors) and are not drawn.
pub fn wrap_line(text: &str, width: usize, tab: usize) -> Vec<Seg> {
    let width = width.max(2);
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![(0, 0)];
    }
    let mut segs = Vec::new();
    let (mut start, mut x, mut i) = (0usize, 0usize, 0usize);
    let mut brk: Option<usize> = None;
    while i < chars.len() {
        let c = chars[i];
        let w = char_width(c, x, tab);
        if c == ' ' {
            x += w;
            i += 1;
            brk = Some(i);
            continue;
        }
        if x + w > width && i > start {
            let end = match brk {
                Some(b) if b > start => b,
                _ => i,
            };
            segs.push((start, end));
            start = end;
            brk = None;
            x = 0;
            for &ch in &chars[start..i] {
                x += char_width(ch, x, tab);
            }
            continue;
        }
        x += w;
        i += 1;
        if matches!(c, '\t' | '-' | '/' | '—' | '–') && i < chars.len() {
            brk = Some(i);
        }
    }
    segs.push((start, chars.len()));
    segs
}

/// Index of the row containing char column `col`.
pub fn row_of_col(segs: &[Seg], col: usize) -> usize {
    segs.iter().rposition(|s| s.0 <= col).unwrap_or(0)
}

/// Display x of char column `col` within row `seg` of `text`.
pub fn x_in_seg(text: &str, seg: Seg, col: usize, tab: usize) -> usize {
    let mut x = 0;
    for c in text.chars().skip(seg.0).take(col.saturating_sub(seg.0)) {
        x += char_width(c, x, tab);
    }
    x
}

/// Char column at display x within row `seg`.
pub fn col_at_x(text: &str, seg: Seg, x: usize, tab: usize, last_row: bool) -> usize {
    let row: String = text.chars().skip(seg.0).take(seg.1 - seg.0).collect();
    let c = seg.0 + char_col_at_display(&row, x, tab);
    // Only the last row of a line may place the cursor after its final char;
    // elsewhere that position belongs to the next row.
    if !last_row && c >= seg.1 && seg.1 > seg.0 { seg.1 - 1 } else { c }
}

/// Start rows of each page (always at least one page).
#[derive(Debug, Default)]
pub struct Pages {
    pub starts: Vec<VRow>,
}

impl Pages {
    pub fn count(&self) -> usize {
        self.starts.len().max(1)
    }

    /// Page containing visual row `v`.
    pub fn page_of(&self, v: VRow) -> usize {
        self.starts.partition_point(|s| *s <= v).saturating_sub(1)
    }
}

/// (edit version, width, rows per page, tab width) a page layout was built for.
type PagesKey = (u64, usize, usize, usize);

#[derive(Default)]
pub struct WrapCache {
    key: (usize, usize),
    rows: Vec<Option<Rc<[Seg]>>>,
    pages: Option<(PagesKey, Rc<Pages>)>,
}

impl WrapCache {
    /// Keeps the cache aligned after `removed` lines following `line` were
    /// merged into it and `inserted` new lines were added after it.
    pub fn edit(&mut self, line: usize, removed: usize, inserted: usize) {
        if self.rows.is_empty() {
            return;
        }
        if line >= self.rows.len() || line + removed >= self.rows.len() {
            self.rows.clear();
            return;
        }
        self.rows[line] = None;
        if removed > 0 {
            self.rows.drain(line + 1..=line + removed);
        }
        if inserted > 0 {
            self.rows.splice(line + 1..line + 1, std::iter::repeat_n(None, inserted));
        }
    }
}

impl Document {
    /// Rows of `line` wrapped at `width`.
    pub fn segs(&mut self, line: usize, width: usize) -> Rc<[Seg]> {
        let key = (width, self.tab_width);
        let n = self.line_count();
        let cache = &mut self.wrap;
        if cache.key != key || cache.rows.len() != n {
            cache.key = key;
            cache.rows = vec![None; n];
        }
        if let Some(r) = &cache.rows[line] {
            return r.clone();
        }
        let r: Rc<[Seg]> = if is_page_break(&self.lines[line]) {
            Rc::from(vec![(0, self.line_len(line))])
        } else {
            Rc::from(wrap_line(&self.lines[line], width, self.tab_width))
        };
        self.wrap.rows[line] = Some(r.clone());
        r
    }

    pub fn row_count(&mut self, line: usize, width: usize) -> usize {
        self.segs(line, width).len()
    }

    /// Visual row of a document position.
    pub fn vrow_of(&mut self, p: Pos, width: usize) -> VRow {
        let segs = self.segs(p.line, width);
        (p.line, row_of_col(&segs, p.col))
    }

    pub fn next_vrow(&mut self, v: VRow, width: usize) -> Option<VRow> {
        if v.1 + 1 < self.row_count(v.0, width) {
            Some((v.0, v.1 + 1))
        } else if v.0 + 1 < self.line_count() {
            Some((v.0 + 1, 0))
        } else {
            None
        }
    }

    pub fn prev_vrow(&mut self, v: VRow, width: usize) -> Option<VRow> {
        if v.1 > 0 {
            Some((v.0, v.1 - 1))
        } else if v.0 > 0 {
            let r = self.row_count(v.0 - 1, width);
            Some((v.0 - 1, r - 1))
        } else {
            None
        }
    }

    /// Moves `v` by `delta` rows, stopping at the document edges.
    pub fn step_vrow(&mut self, mut v: VRow, delta: isize, width: usize) -> VRow {
        for _ in 0..delta.unsigned_abs() {
            let n = if delta < 0 { self.prev_vrow(v, width) } else { self.next_vrow(v, width) };
            match n {
                Some(n) => v = n,
                None => break,
            }
        }
        v
    }

    /// Number of rows from `a` forward to `b`, or None if more than `limit`.
    pub fn rows_between(&mut self, mut a: VRow, b: VRow, limit: usize, width: usize) -> Option<usize> {
        for n in 0..=limit {
            if a >= b {
                return Some(n);
            }
            a = self.next_vrow(a, width)?;
        }
        None
    }

    /// Splits the document into pages of `rows` rows at `width` columns.
    pub fn pages(&mut self, width: usize, rows: usize) -> Rc<Pages> {
        let key = (self.edit_version, width, rows.max(1), self.tab_width);
        if let Some((k, p)) = &self.wrap.pages {
            if *k == key {
                return p.clone();
            }
        }
        let rows = rows.max(1);
        let mut starts = vec![(0, 0)];
        let mut used = 0usize;
        let n = self.line_count();
        for l in 0..n {
            if is_page_break(&self.lines[l]) {
                if used == rows {
                    starts.push((l, 0));
                    used = 0;
                }
                used += 1;
                if l + 1 < n {
                    starts.push((l + 1, 0));
                    used = 0;
                }
                continue;
            }
            for r in 0..self.row_count(l, width) {
                if used == rows {
                    starts.push((l, r));
                    used = 0;
                }
                used += 1;
            }
        }
        let p = Rc::new(Pages { starts });
        self.wrap.pages = Some((key, p.clone()));
        p
    }

    /// Moves the cursor `delta` visual rows, keeping the display column.
    pub fn move_visual(&mut self, delta: isize, select: bool, width: usize) {
        self.prepare_move(select);
        let cur = self.vrow_of(self.cursor, width);
        let segs = self.segs(cur.0, width);
        let want = self.desired_col.unwrap_or_else(|| {
            x_in_seg(&self.lines[cur.0], segs[cur.1], self.cursor.col, self.tab_width)
        });
        let target = self.step_vrow(cur, delta, width);
        if target == cur {
            // At the top/bottom edge: go to the start/end of the line.
            let col = if delta < 0 { 0 } else { self.line_len(cur.0) };
            self.cursor = Pos::new(cur.0, col);
            self.desired_col = None;
            return;
        }
        let tsegs = self.segs(target.0, width);
        let last = target.1 + 1 == tsegs.len();
        let col = col_at_x(&self.lines[target.0], tsegs[target.1], want, self.tab_width, last);
        self.cursor = Pos::new(target.0, col);
        self.desired_col = Some(want);
    }

    /// Start and end of the cursor's visual row (for Home/End on wrapped text).
    pub fn vrow_bounds(&mut self, width: usize) -> (usize, usize, bool) {
        let v = self.vrow_of(self.cursor, width);
        let segs = self.segs(v.0, width);
        let (s, e) = segs[v.1];
        (s, e, v.1 + 1 == segs.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(t: &str) -> Document {
        Document::from_text("t.txt".into(), t, None)
    }

    #[test]
    fn word_wrapping() {
        assert_eq!(wrap_line("", 10, 4), vec![(0, 0)]);
        assert_eq!(wrap_line("hello world foo", 11, 4), vec![(0, 12), (12, 15)]);
        // Exactly fitting word followed by a space: the space hangs.
        assert_eq!(wrap_line("abcde fgh", 5, 4), vec![(0, 6), (6, 9)]);
        // Long words are broken hard.
        assert_eq!(wrap_line("abcdefghij", 4, 4), vec![(0, 4), (4, 8), (8, 10)]);
        // Break after hyphen.
        assert_eq!(wrap_line("well-known x", 7, 4), vec![(0, 5), (5, 12)]);
        // Wide characters never split.
        assert_eq!(wrap_line("日本語", 5, 4), vec![(0, 2), (2, 3)]);
        // Every char appears in exactly one row.
        let text = "The quick brown fox jumps over the lazy dog, again and again.";
        for w in 3..30 {
            let segs = wrap_line(text, w, 4);
            assert_eq!(segs.first().unwrap().0, 0);
            assert_eq!(segs.last().unwrap().1, text.chars().count());
            assert!(segs.windows(2).all(|p| p[0].1 == p[1].0 && p[0].0 < p[0].1));
        }
    }

    #[test]
    fn cache_tracks_edits() {
        // After any edit (and undo), cached rows must equal a fresh wrap.
        let check = |d: &mut Document| {
            for l in 0..d.line_count() {
                let fresh = wrap_line(&d.lines[l].clone(), 4, d.tab_width);
                assert_eq!(&*d.segs(l, 4), fresh.as_slice(), "line {l}");
            }
        };
        let mut d = doc("aaa bbb ccc\nddd\neee fff");
        check(&mut d);
        let edits: [(Pos, Option<Pos>, &str); 4] = [
            (Pos::new(0, 3), None, "\nnew line here"),
            (Pos::new(1, 2), Some(Pos::new(2, 1)), ""),
            (Pos::new(0, 0), None, "x\ny\nz "),
            (Pos::new(2, 0), Some(Pos::new(4, 2)), "joined"),
        ];
        for (at, end, text) in edits {
            d.transact(crate::document::EditKind::Other, |d| {
                if let Some(e) = end {
                    d.delete(at, e);
                }
                d.insert(at, text);
            });
            check(&mut d);
        }
        while d.undo() {
            check(&mut d);
        }
        assert_eq!(d.full_text(), "aaa bbb ccc\nddd\neee fff");
    }

    #[test]
    fn pagination_and_breaks() {
        let text = (0..10).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let mut d = doc(&text);
        let p = d.pages(20, 4);
        assert_eq!(p.starts, vec![(0, 0), (4, 0), (8, 0)]);
        assert_eq!(p.page_of((5, 0)), 1);
        let mut d = doc("a\nb\n\u{c}\nc\n\u{c}");
        let p = d.pages(20, 54);
        assert_eq!(p.starts, vec![(0, 0), (3, 0)]);
        assert!(is_page_break("\u{c}") && !is_page_break("x\u{c}") && !is_page_break(""));
    }

    #[test]
    fn visual_movement() {
        let mut d = doc("aaaa bbbb cccc\nxy");
        d.cursor = Pos::new(0, 1);
        d.move_visual(1, false, 5);
        assert_eq!(d.cursor, Pos::new(0, 6));
        d.move_visual(1, false, 5);
        assert_eq!(d.cursor, Pos::new(0, 11));
        d.move_visual(1, false, 5);
        assert_eq!(d.cursor, Pos::new(1, 1));
        d.move_visual(-2, false, 5);
        assert_eq!(d.cursor, Pos::new(0, 6));
        // Short final row clamps to line end, and the remembered column persists.
        let mut d = doc("abcdefgh ij\nz");
        d.cursor = Pos::new(0, 7);
        d.move_visual(1, false, 9);
        assert_eq!(d.cursor, Pos::new(0, 11));
        assert_eq!(d.vrow_of(d.cursor, 9), (0, 1));
    }
}
