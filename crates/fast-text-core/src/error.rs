use std::fmt;
use std::path::PathBuf;

/// Errors produced by Fast Text core operations.
#[derive(Debug)]
pub enum Error {
    /// An I/O error, with the offending path when known.
    Io {
        path: Option<PathBuf>,
        source: std::io::Error,
    },
    /// A character index was outside the buffer.
    OutOfBounds { index: usize, len: usize },
}

impl Error {
    pub(crate) fn io(path: impl Into<Option<PathBuf>>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io {
                path: Some(p),
                source,
            } => {
                write!(f, "{}: {}", p.display(), source)
            }
            Error::Io { path: None, source } => write!(f, "{}", source),
            Error::OutOfBounds { index, len } => {
                write!(f, "index {index} out of bounds (len {len})")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            Error::OutOfBounds { .. } => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
