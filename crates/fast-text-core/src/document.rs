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
    /// Sticky column for vertical movement.
    goal_col: Option<usize>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    // --- Construction ------------------------------------------------------

    /// Creates an empty, unnamed document.
    pub fn new() -> Self {
        Self {
            buffer: Buffer::new(),
            history: History::default(),
            selection: Selection::caret(0),
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
            goal_col: None,
        }
    }

    /// Opens a file as a document.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        Ok(Self {
            buffer: Buffer::from_file(path)?,
            history: History::default(),
            selection: Selection::caret(0),
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
        self.goal_col = None;
    }

    pub fn select(&mut self, anchor: usize, head: usize) {
        self.set_selection(Selection::new(anchor, head));
    }

    pub fn collapse_selection(&mut self) {
        let head = self.selection.head;
        self.selection = Selection::caret(head);
    }

    // --- Editing commands --------------------------------------------------

    /// Inserts `text` at the caret, replacing any selection.
    pub fn insert_str(&mut self, text: &str) {
        if self.buffer.is_read_only() || text.is_empty() {
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
        if self.selection.is_empty() || self.buffer.is_read_only() {
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
            self.goal_col = None;
        }
    }

    pub fn redo(&mut self) {
        if let Some(txn) = self.history.redo() {
            txn.apply(&mut self.buffer);
            self.selection = txn.after;
            self.selection.clamp(self.buffer.max_char_idx());
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
        }
        self.goal_col = None;
    }

    fn move_caret_keep_goal(&mut self, target: usize, extend: bool) {
        let target = target.min(self.buffer.max_char_idx());
        if extend {
            self.selection.head = target;
        } else {
            self.selection = Selection::caret(target);
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
