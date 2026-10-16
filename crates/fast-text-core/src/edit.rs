use crate::Buffer;

/// A single, reversible edit against a buffer.
///
/// Edits record enough information to both apply and revert themselves, which
/// is what makes undo/redo cheap: we just replay the inverse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Text of the given length was removed at `at` (char index).
    Delete { at: usize, text: String },
    /// `text` was inserted at `at` (char index).
    Insert { at: usize, text: String },
}

impl Edit {
    /// Applies this edit to a buffer.
    pub fn apply(&self, buffer: &mut Buffer) {
        match self {
            Edit::Insert { at, text } => buffer.insert(*at, text),
            Edit::Delete { at, text } => {
                buffer.remove(*at, *at + text.chars().count());
            }
        }
    }

    /// Returns the edit that reverses this one.
    pub fn inverse(&self) -> Edit {
        match self {
            Edit::Insert { at, text } => Edit::Delete {
                at: *at,
                text: text.clone(),
            },
            Edit::Delete { at, text } => Edit::Insert {
                at: *at,
                text: text.clone(),
            },
        }
    }

    /// Character index where this edit begins.
    pub fn at(&self) -> usize {
        match self {
            Edit::Insert { at, .. } | Edit::Delete { at, .. } => *at,
        }
    }

    /// Character index where this edit ends (exclusive).
    pub fn end(&self) -> usize {
        match self {
            Edit::Insert { at, text } | Edit::Delete { at, text } => at + text.chars().count(),
        }
    }

    /// The caret position after this edit is applied, assuming the caret was
    /// at the edit point beforehand.
    pub fn caret_after(&self) -> usize {
        match self {
            Edit::Insert { at, text } => at + text.chars().count(),
            Edit::Delete { at, .. } => *at,
        }
    }
}
