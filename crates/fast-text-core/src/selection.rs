/// A selection between two character indices.
///
/// `head` is the moving end (where the caret is), `anchor` is the fixed end.
/// A collapsed selection (`anchor == head`) is a plain caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    /// A collapsed caret at `at`.
    pub const fn caret(at: usize) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

    /// A selection from `anchor` to `head`.
    pub const fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// True when there is no selected text.
    pub const fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The normalized `[start, end)` range.
    pub fn range(&self) -> (usize, usize) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Number of characters selected.
    pub fn len(&self) -> usize {
        self.anchor.abs_diff(self.head)
    }

    /// The caret (moving end).
    pub const fn cursor(&self) -> usize {
        self.head
    }

    /// Collapses the selection to its head.
    pub fn collapse(&mut self) {
        self.anchor = self.head;
    }

    /// Clamps both ends to `[0, len]`.
    pub fn clamp(&mut self, len: usize) {
        self.anchor = self.anchor.min(len);
        self.head = self.head.min(len);
    }
}
