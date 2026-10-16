//! Source positions and the single error type used across TextWrite.

use std::fmt;

/// A UTF-8 source position. `offset` counts characters from the start of input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
    pub offset: usize,
}

impl Pos {
    pub const fn new(line: u32, col: u32, offset: usize) -> Self {
        Self { line, col, offset }
    }
}

/// A half-open span `[start, end)` in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    pub const fn new(start: Pos, end: Pos) -> Self {
        Self { start, end }
    }

    /// A span at the very start of a source, for host-level errors that have no
    /// corresponding source text.
    pub const fn zero() -> Self {
        Self {
            start: Pos::new(0, 0, 0),
            end: Pos::new(0, 0, 0),
        }
    }

    /// A zero-width span at the given position.
    pub const fn point(pos: Pos) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    /// The smallest span covering both `self` and `other`.
    pub fn merge(self, other: Span) -> Span {
        let start = if self.start.offset <= other.start.offset {
            self.start
        } else {
            other.start
        };
        let end = if self.end.offset >= other.end.offset {
            self.end
        } else {
            other.end
        };
        Span { start, end }
    }
}

/// Which stage produced an [`Error`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The lexer could not tokenize the source.
    Lex,
    /// The parser could not build an AST.
    Parse,
    /// The interpreter failed at runtime.
    Runtime,
}

impl ErrorKind {
    fn label(self) -> &'static str {
        match self {
            ErrorKind::Lex => "lex error",
            ErrorKind::Parse => "parse error",
            ErrorKind::Runtime => "runtime error",
        }
    }
}

/// A TextWrite error, optionally carrying the source span that caused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    pub span: Option<Span>,
    /// True when a parse error was caused by running out of input, which the
    /// REPL uses to request another line instead of reporting a failure.
    pub incomplete: bool,
}

impl Error {
    pub fn lex(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ErrorKind::Lex,
            message: message.into(),
            span: Some(span),
            incomplete: false,
        }
    }

    pub fn parse(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ErrorKind::Parse,
            message: message.into(),
            span: Some(span),
            incomplete: false,
        }
    }

    /// A parse error raised at end-of-input.
    pub fn incomplete(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ErrorKind::Parse,
            message: message.into(),
            span: Some(span),
            incomplete: true,
        }
    }

    pub fn runtime(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ErrorKind::Runtime,
            message: message.into(),
            span: Some(span),
            incomplete: false,
        }
    }

    /// A runtime error with no known span.
    pub fn runtime_bare(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Runtime,
            message: message.into(),
            span: None,
            incomplete: false,
        }
    }

    /// True when the error came from running out of input mid-parse.
    pub fn is_incomplete(&self) -> bool {
        self.incomplete
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.span {
            Some(s) => write!(
                f,
                "{} (line {}, column {}): {}",
                self.kind.label(),
                s.start.line,
                s.start.col,
                self.message
            ),
            None => write!(f, "{}: {}", self.kind.label(), self.message),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
