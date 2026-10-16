use std::fmt;

/// A logical position in a buffer.
///
/// `line` is zero-based; `col` is zero-based and measured in Unicode scalar
/// values (chars) within the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub const fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }

    pub const fn start() -> Self {
        Self { line: 0, col: 0 }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Humans count from 1.
        write!(f, "{}:{}", self.line + 1, self.col + 1)
    }
}
