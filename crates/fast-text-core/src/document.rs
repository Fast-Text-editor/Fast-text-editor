use crate::buffer::{Buffer, LineEnding};
use crate::edit::Edit;
use crate::history::{History, Transaction, TxnKind};
use crate::position::Position;
use crate::search::{self, Match, SearchDirection};
use crate::selection::Selection;
use crate::Result;

/// A full editing document: a [`Buffer`] plus a [`Selection`] and undo history.
///
/// This is the primary API for frontends. Every mutating command records a
/// transaction so undo/redo works uniformly, and cursor movement preserves a
/// "goal column" so vertical motion feels natural.
pub struct Document {
    buffer: Buffer,
    history: History,
    selection: Selection,
    /// Additional carets/selections for multi-cursor editing. The primary
    /// selection stays in `selection`; these are always kept sorted and
    /// non-overlapping with it.
    extra: Vec<Selection>,
    /// Sticky column for vertical movement.
    goal_col: Option<usize>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

// How a multi-cursor deletion treats a collapsed caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MultiMode {
    /// Delete the character before each collapsed caret.
    Backward,
    /// Delete the character after each collapsed caret.
    Forward,
    /// Only delete ranged selections; leave collapsed carets in place.
    SelectionOnly,
}

impl Document {
    // --- Construction ------------------------------------------------------

    /// Creates an empty, unnamed document.
    pub fn new() -> Self {
        Self {
            buffer: Buffer::new(),
            history: History::default(),
            selection: Selection::caret(0),
            extra: Vec::new(),
            goal_col: None,
        }
    }

    /// Creates a document from an in-memory string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Self {
        Self {
            buffer: Buffer::from_str(text),
            history: History::default(),
            selection: Selection::caret(0),
            extra: Vec::new(),
            goal_col: None,
        }
    }

    /// Opens a file as a document.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Ok(Self {
            buffer: Buffer::from_file(path)?,
            history: History::default(),
            selection: Selection::caret(0),
            extra: Vec::new(),
            goal_col: None,
        })
    }

    // --- Accessors ---------------------------------------------------------

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Mutable access to the buffer.
    ///
    /// Bypassing the document mutates text without recording undo history;
    /// prefer the command methods below for anything user-facing.
    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffer
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn set_selection(&mut self, selection: Selection) {
        let mut s = selection;
        s.clamp(self.buffer.max_char_idx());
        self.selection = s;
        self.extra.clear();
        self.goal_col = None;
    }

    /// The caret (moving end of the selection).
    pub fn cursor(&self) -> usize {
        self.selection.head
    }

    /// Moves the caret, collapsing any selection.
    pub fn set_cursor(&mut self, at: usize) {
        self.set_selection(Selection::caret(at));
    }

    /// The caret as a human-facing line/column.
    pub fn cursor_position(&self) -> Position {
        self.position_of(self.selection.head)
    }

    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// The selected `[start, end)` range.
    pub fn selection_range(&self) -> (usize, usize) {
        self.selection.range()
    }

    /// The selected text.
    pub fn selected_text(&self) -> String {
        let (s, e) = self.selection.range();
        self.buffer.slice(s, e)
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer.is_dirty()
    }

    pub fn text(&self) -> String {
        self.buffer.text()
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        self.buffer.path()
    }

    /// Binds the document to a backing path without touching disk.
    ///
    /// Used when naming a brand-new, not-yet-saved file (e.g. `ft new.py`).
    pub fn set_path(&mut self, path: impl Into<std::path::PathBuf>) {
        self.buffer.set_path(path);
    }

    // --- Position helpers --------------------------------------------------

    /// Converts a character index to a `(line, col)` position.
    pub fn position_of(&self, idx: usize) -> Position {
        let idx = idx.min(self.buffer.max_char_idx());
        let line = self.buffer.line_of_char(idx);
        let line_start = self.buffer.char_of_line(line);
        Position::new(line, idx - line_start)
    }

    /// Converts a `(line, col)` position to a character index, clamped.
    pub fn index_of(&self, pos: Position) -> usize {
        let line = pos.line.min(self.buffer.len_lines().saturating_sub(1));
        let line_start = self.buffer.char_of_line(line);
        let line_len = self.buffer.line_len_chars(line);
        line_start + pos.col.min(line_len)
    }

    // --- Selection ---------------------------------------------------------

    pub fn select_all(&mut self) {
        self.selection = Selection::new(0, self.buffer.max_char_idx());
        self.extra.clear();
        self.goal_col = None;
    }

    pub fn select(&mut self, anchor: usize, head: usize) {
        self.set_selection(Selection::new(anchor, head));
    }

    pub fn collapse_selection(&mut self) {
        let head = self.selection.head;
        self.selection = Selection::caret(head);
        self.extra.clear();
    }

    // --- Multi-cursor ------------------------------------------------------

    /// The secondary selections, sorted by position. Empty for a single caret.
    pub fn extra_selections(&self) -> &[Selection] {
        &self.extra
    }

    /// True when more than one caret/selection is active.
    pub fn has_multi_cursor(&self) -> bool {
        !self.extra.is_empty()
    }

    /// Every selection, primary first.
    pub fn selections(&self) -> Vec<Selection> {
        std::iter::once(self.selection)
            .chain(self.extra.iter().copied())
            .collect()
    }

    /// Drops every secondary selection, keeping the primary caret.
    pub fn clear_extra_selections(&mut self) {
        self.extra.clear();
    }

    /// Adds a collapsed caret at `at` (clamped, de-duplicated, sorted).
    pub fn add_caret(&mut self, at: usize) {
        let at = at.min(self.buffer.max_char_idx());
        if at == self.selection.head || self.extra.iter().any(|s| s.head == at) {
            return;
        }
        self.extra.push(Selection::caret(at));
        self.extra.sort_by_key(|s| s.range().0);
    }

    /// Adds a caret one line above the topmost caret (same column).
    pub fn add_caret_above(&mut self) {
        let top = self
            .selections()
            .into_iter()
            .min_by_key(|s| s.range().0)
            .unwrap_or(self.selection);
        self.add_caret_relative(top, -1);
    }

    /// Adds a caret one line below the bottommost caret (same column).
    pub fn add_caret_below(&mut self) {
        let bottom = self
            .selections()
            .into_iter()
            .max_by_key(|s| s.range().1)
            .unwrap_or(self.selection);
        self.add_caret_relative(bottom, 1);
    }

    fn add_caret_relative(&mut self, from: Selection, delta: i64) {
        let pos = self.position_of(from.head);
        let line = pos.line as i64 + delta;
        if line < 0 || line as usize >= self.buffer.len_lines() {
            return;
        }
        let col = pos.col.min(self.buffer.line_len_chars(line as usize));
        let at = self.index_of(Position::new(line as usize, col));
        self.add_caret(at);
    }

    /// `Ctrl+D`: selects the word under the caret, then each next occurrence.
    ///
    /// The first call selects the word at the caret; later calls extend the
    /// multi-selection to the following occurrence of the same text. Returns
    /// false when there is nothing more to select.
    pub fn select_next_occurrence(&mut self, case_sensitive: bool) -> bool {
        let (start, end) = self.selection.range();
        if start == end {
            let Some((ws, we)) = self.word_at_index(self.selection.head) else {
                return false;
            };
            self.selection = Selection::new(ws, we);
            self.goal_col = None;
            return true;
        }
        let needle = self.buffer.slice(start, end);
        let from = self
            .selections()
            .into_iter()
            .map(|s| s.range().1)
            .max()
            .unwrap_or(end);
        match search::find_literal(
            &self.buffer,
            &needle,
            from,
            SearchDirection::Forward,
            case_sensitive,
            false,
        ) {
            Some(m) => {
                self.extra.push(Selection::new(m.start, m.end));
                self.extra.sort_by_key(|s| s.range().0);
                true
            }
            None => false,
        }
    }

    /// Character bounds of the word at `at`, or `None` when it is not on a word.
    fn word_at_index(&self, at: usize) -> Option<(usize, usize)> {
        let n = self.buffer.max_char_idx();
        if n == 0 {
            return None;
        }
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let probe = if self.buffer.char_at(at).is_some_and(is_word) {
            at
        } else if at > 0 && self.buffer.char_at(at - 1).is_some_and(is_word) {
            at - 1
        } else {
            return None;
        };
        let mut s = probe;
        while s > 0 && self.buffer.char_at(s - 1).is_some_and(is_word) {
            s -= 1;
        }
        let mut e = probe + 1;
        while e < n && self.buffer.char_at(e).is_some_and(is_word) {
            e += 1;
        }
        Some((s, e))
    }

    /// The text of the word under the caret, if any (used for rename).
    pub fn word_under_cursor(&self) -> Option<String> {
        let (s, e) = self.word_at_index(self.selection.head)?;
        Some(self.buffer.slice(s, e))
    }

    /// Applies `(start, end, replacement)` character-range edits as one
    /// undoable transaction. Edits may be given in any order but must not
    /// overlap.
    pub fn apply_edits(&mut self, edits: &[(usize, usize, String)]) {
        if self.buffer.is_read_only() || edits.is_empty() {
            return;
        }
        let mut ordered: Vec<(usize, usize, &String)> =
            edits.iter().map(|(s, e, t)| (*s, *e, t)).collect();
        ordered.sort_by_key(|e| std::cmp::Reverse(e.0));
        let before = self.selection;
        let mut txn = Transaction::new(TxnKind::Other, before);
        let mut caret = None;
        for (start, end, text) in ordered {
            let start = start.min(self.buffer.max_char_idx());
            let end = end.min(self.buffer.max_char_idx()).max(start);
            if start < end {
                let removed = self.buffer.remove(start, end);
                txn.edits.push(Edit::Delete {
                    at: start,
                    text: removed,
                });
            }
            if !text.is_empty() {
                self.buffer.insert(start, text);
                txn.edits.push(Edit::Insert {
                    at: start,
                    text: text.clone(),
                });
            }
            caret = Some(start + text.chars().count());
        }
        self.selection = Selection::caret(caret.unwrap_or(before.head));
        self.extra.clear();
        self.goal_col = None;
        self.commit(txn, false);
    }

    // --- Editing commands --------------------------------------------------

    fn multi_insert(&mut self, text: &str) {
        let sels = self.selections();
        let mut order: Vec<usize> = (0..sels.len()).collect();
        // Edit from the end backwards so earlier indices stay valid.
        order.sort_by_key(|&i| std::cmp::Reverse(sels[i].range().0));

        let before = self.selection;
        let mut txn = Transaction::new(TxnKind::Insert, before);
        if sels.iter().any(|s| !s.is_empty()) {
            txn.kind = TxnKind::Other;
        }
        let inserted = text.chars().count();
        let mut positions: Vec<Option<usize>> = vec![None; sels.len()];
        let mut limit = usize::MAX;
        for &i in &order {
            let (s, e) = sels[i].range();
            if e > limit {
                continue; // overlaps a selection already edited
            }
            if s != e {
                let removed = self.buffer.remove(s, e);
                txn.edits.push(Edit::Delete {
                    at: s,
                    text: removed,
                });
            }
            self.buffer.insert(s, text);
            txn.edits.push(Edit::Insert {
                at: s,
                text: text.to_string(),
            });
            positions[i] = Some(s + inserted);
            limit = limit.min(s);
        }
        self.apply_multi_positions(positions);
        self.goal_col = None;
        self.commit(txn, false);
    }

    /// Deletes at every selection in one transaction.
    fn multi_delete(&mut self, mode: MultiMode) {
        let sels = self.selections();
        let mut order: Vec<usize> = (0..sels.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(sels[i].range().0));

        let before = self.selection;
        let mut txn = Transaction::new(TxnKind::Delete, before);
        let mut positions: Vec<Option<usize>> = vec![None; sels.len()];
        let mut limit = usize::MAX;
        for &i in &order {
            let (s, e) = sels[i].range();
            if e > limit {
                continue;
            }
            if s != e {
                let removed = self.buffer.remove(s, e);
                txn.edits.push(Edit::Delete {
                    at: s,
                    text: removed,
                });
                positions[i] = Some(s);
            } else if mode == MultiMode::Backward && s > 0 {
                let removed = self.buffer.remove(s - 1, s);
                txn.edits.push(Edit::Delete {
                    at: s - 1,
                    text: removed,
                });
                positions[i] = Some(s - 1);
            } else if mode == MultiMode::Forward && s < self.buffer.max_char_idx() {
                let removed = self.buffer.remove(s, s + 1);
                txn.edits.push(Edit::Delete {
                    at: s,
                    text: removed,
                });
                positions[i] = Some(s);
            } else {
                positions[i] = Some(s);
            }
            limit = limit.min(s);
        }
        if txn.edits.is_empty() {
            return;
        }
        self.apply_multi_positions(positions);
        self.goal_col = None;
        self.commit(txn, false);
    }

    /// Sets the primary caret and the secondary carets from per-selection
    /// positions, dropping overlaps and duplicates.
    fn apply_multi_positions(&mut self, positions: Vec<Option<usize>>) {
        let max = self.buffer.max_char_idx();
        let primary = positions
            .first()
            .copied()
            .flatten()
            .unwrap_or(self.selection.head)
            .min(max);
        self.selection = Selection::caret(primary);
        let mut extras: Vec<usize> = positions
            .into_iter()
            .skip(1)
            .flatten()
            .map(|p| p.min(max))
            .filter(|&p| p != primary)
            .collect();
        extras.sort_unstable();
        extras.dedup();
        self.extra = extras.into_iter().map(Selection::caret).collect();
    }

    /// Inserts `text` at the caret, replacing any selection.
    pub fn insert_str(&mut self, text: &str) {
        if self.buffer.is_read_only() || text.is_empty() {
            return;
        }
        if self.has_multi_cursor() {
            self.multi_insert(text);
            return;
        }
        let mut txn = Transaction::new(TxnKind::Insert, self.selection);
        let mut coalesce = true;

        if !self.selection.is_empty() {
            let (s, e) = self.selection.range();
            let removed = self.buffer.remove(s, e);
            txn.kind = TxnKind::Other;
            txn.edits.push(Edit::Delete {
                at: s,
                text: removed,
            });
            self.selection = Selection::caret(s);
            coalesce = false;
        }

        let at = self.selection.head;
        self.buffer.insert(at, text);
        txn.edits.push(Edit::Insert {
            at,
            text: text.to_string(),
        });
        self.selection = Selection::caret(at + text.chars().count());
        self.goal_col = None;

        // Only coalesce single-character typing bursts.
        self.commit(txn, coalesce && text.chars().count() == 1);
    }

    /// Inserts a single character.
    pub fn insert_char(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.insert_str(c.encode_utf8(&mut buf));
    }

    /// Inserts `text` at an arbitrary character index as one undoable step,
    /// leaving the caret at the end of the inserted text.
    pub fn insert_at(&mut self, at: usize, text: &str) {
        if self.buffer.is_read_only() || text.is_empty() {
            return;
        }
        let at = at.min(self.buffer.max_char_idx());
        let before = self.selection;
        self.buffer.insert(at, text);
        let mut txn = Transaction::new(TxnKind::Insert, before);
        txn.edits.push(Edit::Insert {
            at,
            text: text.to_string(),
        });
        self.selection = Selection::caret(at + text.chars().count());
        self.goal_col = None;
        self.commit(txn, false);
    }

    /// Inserts a newline using the buffer's line-ending style.
    pub fn insert_newline(&mut self) {
        let ending = self.buffer.line_ending().as_str();
        if self.buffer.is_read_only() {
            return;
        }
        if self.has_multi_cursor() {
            self.multi_insert(ending);
            return;
        }
        let mut txn = Transaction::new(TxnKind::Insert, self.selection);
        if !self.selection.is_empty() {
            let (s, e) = self.selection.range();
            let removed = self.buffer.remove(s, e);
            txn.kind = TxnKind::Other;
            txn.edits.push(Edit::Delete {
                at: s,
                text: removed,
            });
            self.selection = Selection::caret(s);
        }
        let at = self.selection.head;
        self.buffer.insert(at, ending);
        txn.edits.push(Edit::Insert {
            at,
            text: ending.to_string(),
        });
        self.selection = Selection::caret(at + ending.chars().count());
        self.goal_col = None;
        self.commit(txn, false);
    }

    /// Inserts a tab character.
    pub fn insert_tab(&mut self) {
        self.insert_char('\t');
    }

    /// Deletes the selection, or the character before the caret.
    pub fn backspace(&mut self) {
        if self.buffer.is_read_only() {
            return;
        }
        if self.has_multi_cursor() {
            self.multi_delete(MultiMode::Backward);
            return;
        }
        if !self.selection.is_empty() {
            self.delete_selection();
            return;
        }
        let at = self.selection.head;
        if at == 0 {
            return;
        }
        let removed = self.buffer.remove(at - 1, at);
        let mut txn = Transaction::new(TxnKind::Delete, self.selection);
        txn.edits.push(Edit::Delete {
            at: at - 1,
            text: removed,
        });
        self.selection = Selection::caret(at - 1);
        self.goal_col = None;
        self.commit(txn, true);
    }

    /// Deletes the selection, or the character after the caret.
    pub fn delete_forward(&mut self) {
        if self.buffer.is_read_only() {
            return;
        }
        if self.has_multi_cursor() {
            self.multi_delete(MultiMode::Forward);
            return;
        }
        if !self.selection.is_empty() {
            self.delete_selection();
            return;
        }
        let at = self.selection.head;
        if at >= self.buffer.max_char_idx() {
            return;
        }
        let removed = self.buffer.remove(at, at + 1);
        let mut txn = Transaction::new(TxnKind::Delete, self.selection);
        txn.edits.push(Edit::Delete { at, text: removed });
        self.selection = Selection::caret(at);
        self.goal_col = None;
        self.commit(txn, true);
    }

    /// Deletes the current selection.
    pub fn delete_selection(&mut self) {
        if self.buffer.is_read_only() {
            return;
        }
        if self.has_multi_cursor() {
            // Only drop the ranges; a collapsed extra caret stays put.
            self.multi_delete(MultiMode::SelectionOnly);
            return;
        }
        if self.selection.is_empty() {
            return;
        }
        let (s, e) = self.selection.range();
        let removed = self.buffer.remove(s, e);
        let mut txn = Transaction::new(TxnKind::Delete, self.selection);
        txn.edits.push(Edit::Delete {
            at: s,
            text: removed,
        });
        self.selection = Selection::caret(s);
        self.goal_col = None;
        self.commit(txn, false);
    }

    /// Deletes an arbitrary `[start, end)` range as one undoable step.
    pub fn delete_range(&mut self, start: usize, end: usize) {
        if self.buffer.is_read_only() {
            return;
        }
        let (s, e) = (start.min(end), start.max(end));
        if s == e {
            return;
        }
        let removed = self.buffer.remove(s, e);
        let mut txn = Transaction::new(TxnKind::Delete, self.selection);
        txn.edits.push(Edit::Delete {
            at: s,
            text: removed,
        });
        self.selection = Selection::caret(s);
        self.goal_col = None;
        self.commit(txn, false);
    }

    /// Replaces the current selection with `text`.
    pub fn replace_selection(&mut self, text: &str) {
        self.insert_str(text);
    }

    fn commit(&mut self, mut txn: Transaction, coalesce: bool) {
        txn.after = self.selection;
        self.history.record(txn, coalesce);
    }

    // --- Undo / redo -------------------------------------------------------

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) {
        if let Some(txn) = self.history.undo() {
            txn.revert(&mut self.buffer);
            self.selection = txn.before;
            self.selection.clamp(self.buffer.max_char_idx());
            self.extra.clear();
            self.goal_col = None;
        }
    }

    pub fn redo(&mut self) {
        if let Some(txn) = self.history.redo() {
            txn.apply(&mut self.buffer);
            self.selection = txn.after;
            self.selection.clamp(self.buffer.max_char_idx());
            self.extra.clear();
            self.goal_col = None;
        }
    }

    // --- Movement ----------------------------------------------------------

    fn move_caret_to(&mut self, target: usize, extend: bool) {
        let target = target.min(self.buffer.max_char_idx());
        if extend {
            self.selection.head = target;
        } else {
            self.selection = Selection::caret(target);
            self.extra.clear();
        }
        self.goal_col = None;
    }

    fn move_caret_keep_goal(&mut self, target: usize, extend: bool) {
        let target = target.min(self.buffer.max_char_idx());
        if extend {
            self.selection.head = target;
        } else {
            self.selection = Selection::caret(target);
            self.extra.clear();
        }
    }

    /// Left arrow. Collapses a selection to its start when not extending.
    pub fn move_left(&mut self, extend: bool) {
        let (start, _) = self.selection.range();
        let head = self.selection.head;
        let target = if !extend && !self.selection.is_empty() {
            start
        } else {
            head.saturating_sub(1)
        };
        self.move_caret_to(target, extend);
    }

    /// Right arrow. Collapses a selection to its end when not extending.
    pub fn move_right(&mut self, extend: bool) {
        let (_, end) = self.selection.range();
        let head = self.selection.head;
        let target = if !extend && !self.selection.is_empty() {
            end
        } else {
            head.saturating_add(1)
        };
        self.move_caret_to(target, extend);
    }

    pub fn move_up(&mut self, extend: bool) {
        if !extend {
            self.extra.clear();
        }
        let pos = self.position_of(self.selection.head);
        let goal = *self.goal_col.get_or_insert(pos.col);
        if pos.line > 0 {
            let line = pos.line - 1;
            let col = goal.min(self.buffer.line_len_chars(line));
            let target = self.index_of(Position::new(line, col));
            self.move_caret_keep_goal(target, extend);
        }
    }

    pub fn move_down(&mut self, extend: bool) {
        if !extend {
            self.extra.clear();
        }
        let pos = self.position_of(self.selection.head);
        let goal = *self.goal_col.get_or_insert(pos.col);
        if pos.line + 1 < self.buffer.len_lines() {
            let line = pos.line + 1;
            let col = goal.min(self.buffer.line_len_chars(line));
            let target = self.index_of(Position::new(line, col));
            self.move_caret_keep_goal(target, extend);
        }
    }

    pub fn move_line_start(&mut self, extend: bool) {
        let pos = self.position_of(self.selection.head);
        let target = self.index_of(Position::new(pos.line, 0));
        self.move_caret_to(target, extend);
    }

    pub fn move_line_end(&mut self, extend: bool) {
        let pos = self.position_of(self.selection.head);
        let target = self.index_of(Position::new(pos.line, usize::MAX));
        self.move_caret_to(target, extend);
    }

    pub fn move_doc_start(&mut self, extend: bool) {
        self.move_caret_to(0, extend);
    }

    pub fn move_doc_end(&mut self, extend: bool) {
        self.move_caret_to(self.buffer.max_char_idx(), extend);
    }

    pub fn move_word_forward(&mut self, extend: bool) {
        let mut i = self.selection.head;
        let n = self.buffer.max_char_idx();
        // Skip the rest of the current word, then any separators, landing on
        // the start of the next word.
        while i < n && is_word_char(self.buffer.char_at(i).unwrap_or(' ')) {
            i += 1;
        }
        while i < n && !is_word_char(self.buffer.char_at(i).unwrap_or(' ')) {
            i += 1;
        }
        self.move_caret_to(i, extend);
    }

    pub fn move_word_backward(&mut self, extend: bool) {
        let mut i = self.selection.head;
        while i > 0 && !is_word_char(self.buffer.char_at(i - 1).unwrap_or(' ')) {
            i -= 1;
        }
        while i > 0 && is_word_char(self.buffer.char_at(i - 1).unwrap_or(' ')) {
            i -= 1;
        }
        self.move_caret_to(i, extend);
    }

    // --- Find & replace ----------------------------------------------------

    /// Moves the selection to the next/previous literal match.
    pub fn find(
        &mut self,
        needle: &str,
        direction: SearchDirection,
        case_sensitive: bool,
        wrap: bool,
    ) -> Option<Match> {
        let from = match direction {
            SearchDirection::Forward => self.selection.head,
            SearchDirection::Backward => self.selection.head,
        };
        let m = search::find_literal(&self.buffer, needle, from, direction, case_sensitive, wrap)?;
        self.selection = Selection::new(m.start, m.end);
        self.goal_col = None;
        Some(m)
    }

    /// Replaces every literal occurrence of `needle` with `replacement`.
    pub fn replace_all(&mut self, needle: &str, replacement: &str, case_sensitive: bool) -> usize {
        if needle.is_empty() || self.buffer.is_read_only() {
            return 0;
        }
        let matches = search::find_all(&self.buffer, needle, case_sensitive);
        if matches.is_empty() {
            return 0;
        }
        let before = self.selection;
        let mut txn = Transaction::new(TxnKind::Other, before);
        // Replace from the end so earlier indices stay valid.
        for m in matches.iter().rev() {
            let removed = self.buffer.remove(m.start, m.end);
            txn.edits.push(Edit::Delete {
                at: m.start,
                text: removed,
            });
            self.buffer.insert(m.start, replacement);
            txn.edits.push(Edit::Insert {
                at: m.start,
                text: replacement.to_string(),
            });
        }
        let count = matches.len();
        self.selection = Selection::caret(matches[0].start);
        self.goal_col = None;
        self.commit(txn, false);
        count
    }

    // --- Persistence -------------------------------------------------------

    pub fn save(&mut self) -> Result<()> {
        self.buffer.save()
    }

    pub fn save_as(&mut self, path: impl Into<std::path::PathBuf>) -> Result<()> {
        self.buffer.save_as(path)
    }

    pub fn reload(&mut self) -> Result<()> {
        self.buffer.reload()?;
        self.history.clear();
        self.selection.clamp(self.buffer.max_char_idx());
        self.goal_col = None;
        Ok(())
    }

    pub fn set_line_ending(&mut self, ending: LineEnding) {
        self.buffer.set_line_ending(ending);
    }
}

/// True for characters considered part of a word for navigation.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
