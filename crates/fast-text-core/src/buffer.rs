use std::fmt;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use ropey::{Rope, RopeSlice};

use crate::{Error, Result};

/// The line ending style of a buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    pub const fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }

    /// Detects the dominant line ending in a string.
    pub fn detect(text: &str) -> Self {
        if text.contains("\r\n") {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        }
    }
}

/// A text buffer backed by a `rope`.
///
/// A `Buffer` knows how to load and save itself, tracks whether it has
/// unsaved changes, and exposes index-based editing primitives. Higher level
/// commands (cursor movement, undo grouping) live in [`crate::Document`].
#[derive(Clone)]
pub struct Buffer {
    rope: Rope,
    path: Option<PathBuf>,
    dirty: bool,
    line_ending: LineEnding,
    read_only: bool,
    /// Bumped on every mutation; lets frontends invalidate caches cheaply.
    revision: u64,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Buffer {
    /// Creates an empty buffer.
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            path: None,
            dirty: false,
            line_ending: LineEnding::Lf,
            read_only: false,
            revision: 0,
        }
    }

    /// Creates a buffer from an in-memory string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            path: None,
            dirty: false,
            line_ending: LineEnding::detect(text),
            read_only: false,
            revision: 0,
        }
    }

    /// Loads a buffer from a file on disk.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|e| Error::io(Some(path.to_path_buf()), e))?;
        let rope = Rope::from_reader(file).map_err(|e| Error::io(Some(path.to_path_buf()), e))?;
        let text_head: String = rope.slice(0..rope.len_chars().min(8192)).into();
        Ok(Self {
            rope,
            path: Some(path.to_path_buf()),
            dirty: false,
            line_ending: LineEnding::detect(&text_head),
            read_only: false,
            revision: 0,
        })
    }

    /// The file backing this buffer, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Sets the backing path (does not touch disk).
    pub fn set_path(&mut self, path: impl Into<PathBuf>) {
        self.path = Some(path.into());
    }

    /// True when there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks the buffer as saved.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// True when the buffer is read-only.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// A counter that changes whenever the buffer content changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn set_line_ending(&mut self, ending: LineEnding) {
        self.line_ending = ending;
    }

    // --- Queries -----------------------------------------------------------

    /// Number of characters in the buffer.
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Number of lines (`\n` count + 1).
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// The last valid character index (for use as a caret position).
    pub fn max_char_idx(&self) -> usize {
        self.rope.len_chars()
    }

    /// Character at `idx`, if in range.
    pub fn char_at(&self, idx: usize) -> Option<char> {
        if idx < self.rope.len_chars() {
            Some(self.rope.char(idx))
        } else {
            None
        }
    }

    /// The 0-based line containing character index `idx`.
    pub fn line_of_char(&self, idx: usize) -> usize {
        let idx = idx.min(self.rope.len_chars());
        self.rope.char_to_line(idx)
    }

    /// The character index where `line` starts.
    pub fn char_of_line(&self, line: usize) -> usize {
        let line = line.min(self.rope.len_lines().saturating_sub(1));
        self.rope.line_to_char(line)
    }

    /// The full text of `line`, without its trailing newline.
    pub fn line(&self, line: usize) -> RopeSlice<'_> {
        self.rope
            .line(line.min(self.rope.len_lines().saturating_sub(1)))
    }

    /// The text of `line` as an owned `String`, newline trimmed.
    pub fn line_str(&self, line: usize) -> String {
        let mut s: String = self.line(line).into();
        while s.ends_with('\n') || s.ends_with('\r') {
            s.pop();
        }
        s
    }

    /// Number of characters in `line`, excluding the trailing newline.
    pub fn line_len_chars(&self, line: usize) -> usize {
        let slice = self.line(line);
        let mut n = slice.len_chars();
        // Trim trailing \n and \r.
        if n > 0 && slice.char(n - 1) == '\n' {
            n -= 1;
            if n > 0 && slice.char(n - 1) == '\r' {
                n -= 1;
            }
        }
        n
    }

    /// A slice of the buffer as an owned `String`.
    pub fn slice(&self, start: usize, end: usize) -> String {
        let start = start.min(self.rope.len_chars());
        let end = end.min(self.rope.len_chars()).max(start);
        self.rope.slice(start..end).into()
    }

    /// The entire buffer as an owned `String`.
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Access to the underlying rope.
    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    // --- Mutation primitives ----------------------------------------------

    /// Inserts `text` at character index `idx`.
    pub fn insert(&mut self, idx: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        let idx = idx.min(self.rope.len_chars());
        if idx == self.rope.len_chars() {
            self.rope.append(Rope::from_str(text));
        } else {
            self.rope.insert(idx, text);
        }
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }

    /// Removes the character range `[start, end)` and returns the removed text.
    pub fn remove(&mut self, start: usize, end: usize) -> String {
        let start = start.min(self.rope.len_chars());
        let end = end.min(self.rope.len_chars()).max(start);
        if start == end {
            return String::new();
        }
        let removed: String = self.rope.slice(start..end).into();
        self.rope.remove(start..end);
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
        removed
    }

    // --- Persistence -------------------------------------------------------

    /// Saves to the buffer's path, if it has one.
    pub fn save(&mut self) -> Result<()> {
        let path = self.path.clone().ok_or_else(|| {
            Error::io(
                None,
                std::io::Error::new(std::io::ErrorKind::NotFound, "buffer has no backing file"),
            )
        })?;
        self.save_as(path)
    }

    /// Saves to a specific path and adopts it.
    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        let path = path.into();
        let file = File::create(&path).map_err(|e| Error::io(Some(path.clone()), e))?;
        let mut writer = BufWriter::new(file);

        // Normalize line endings on write if the buffer prefers CRLF.
        match self.line_ending {
            LineEnding::Lf => {
                self.rope
                    .write_to(&mut writer)
                    .map_err(|e| Error::io(Some(path.clone()), e))?;
            }
            LineEnding::CrLf => {
                let text = self.rope.to_string();
                let normalized = text.replace("\r\n", "\n").replace('\n', "\r\n");
                writer
                    .write_all(normalized.as_bytes())
                    .map_err(|e| Error::io(Some(path.clone()), e))?;
            }
        }
        writer
            .flush()
            .map_err(|e| Error::io(Some(path.clone()), e))?;
        self.path = Some(path);
        self.dirty = false;
        Ok(())
    }

    /// Reloads the buffer from disk, discarding unsaved changes.
    pub fn reload(&mut self) -> Result<()> {
        let path = self.path.clone().ok_or_else(|| {
            Error::io(
                None,
                std::io::Error::new(std::io::ErrorKind::NotFound, "buffer has no backing file"),
            )
        })?;
        *self = Buffer::from_file(path)?;
        Ok(())
    }

    /// True when the backing file exists on disk.
    pub fn file_exists(&self) -> bool {
        self.path.as_ref().is_some_and(|p| p.exists())
    }
}

impl fmt::Debug for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Buffer")
            .field("chars", &self.len_chars())
            .field("lines", &self.len_lines())
            .field("path", &self.path)
            .field("dirty", &self.dirty)
            .finish()
    }
}

impl std::str::FromStr for Buffer {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Buffer::from_str(s))
    }
}
