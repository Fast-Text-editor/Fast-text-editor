//! An editor action requested by a plugin.
//!
//! Plugins never touch the editor directly. They *describe* changes by pushing
//! `EditorAction`s into a queue, and the host applies them in order. This keeps
//! scripting fully decoupled from the editor's internals (and from Rust's
//! borrow checker).

/// The shape of the terminal cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    /// A filled block (the classic terminal cursor).
    Block,
    /// A thin vertical bar (the usual editor default).
    Bar,
    /// An underscore under the character.
    Underline,
}

impl CursorStyle {
    /// Parses a friendly spelling like `bar`, `block`, or `underline`.
    pub fn parse(name: &str) -> Option<CursorStyle> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "block" | "box" => CursorStyle::Block,
            "bar" | "line" | "beam" | "ibeam" => CursorStyle::Bar,
            "underline" | "under" | "underscore" => CursorStyle::Underline,
            _ => return None,
        })
    }

    /// The canonical spelling of this style.
    pub fn name(self) -> &'static str {
        match self {
            CursorStyle::Block => "block",
            CursorStyle::Bar => "bar",
            CursorStyle::Underline => "underline",
        }
    }
}

/// A movement or edit a plugin wants performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAction {
    Insert(String),
    InsertAt {
        at: usize,
        text: String,
    },
    DeleteRange {
        start: usize,
        end: usize,
    },
    DeleteSelection,
    Backspace,
    DeleteForward,
    InsertNewline,
    SetCursor(usize),
    Select {
        anchor: usize,
        head: usize,
    },
    SelectAll,
    MoveLeft {
        extend: bool,
    },
    MoveRight {
        extend: bool,
    },
    MoveUp {
        extend: bool,
    },
    MoveDown {
        extend: bool,
    },
    MoveWordForward {
        extend: bool,
    },
    MoveWordBackward {
        extend: bool,
    },
    MoveLineStart {
        extend: bool,
    },
    MoveLineEnd {
        extend: bool,
    },
    MoveDocStart {
        extend: bool,
    },
    MoveDocEnd {
        extend: bool,
    },
    Undo,
    Redo,
    Save,
    SetLanguage(String),
    SetCursorStyle(CursorStyle),
    /// Switch to the named color theme (built-in or defined by a plugin).
    SetTheme(String),
    /// Pause automated playback for this many milliseconds before running the
    /// next action. Hosts without timed playback may treat it immediately.
    Wait(u64),
    Message(String),
}

impl EditorAction {
    /// Parses a movement name into an action.
    pub fn movement(name: &str, extend: bool) -> Option<EditorAction> {
        Some(match name {
            "left" => EditorAction::MoveLeft { extend },
            "right" => EditorAction::MoveRight { extend },
            "up" => EditorAction::MoveUp { extend },
            "down" => EditorAction::MoveDown { extend },
            "word_left" => EditorAction::MoveWordBackward { extend },
            "word_right" => EditorAction::MoveWordForward { extend },
            "line_start" => EditorAction::MoveLineStart { extend },
            "line_end" => EditorAction::MoveLineEnd { extend },
            "doc_start" => EditorAction::MoveDocStart { extend },
            "doc_end" => EditorAction::MoveDocEnd { extend },
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cursor_styles() {
        assert_eq!(CursorStyle::parse("bar"), Some(CursorStyle::Bar));
        assert_eq!(CursorStyle::parse("Block"), Some(CursorStyle::Block));
        assert_eq!(
            CursorStyle::parse(" underline "),
            Some(CursorStyle::Underline)
        );
        assert_eq!(CursorStyle::parse("ibeam"), Some(CursorStyle::Bar));
        assert_eq!(CursorStyle::parse("nope"), None);
        assert_eq!(CursorStyle::Bar.name(), "bar");
        assert_eq!(CursorStyle::Block.name(), "block");
        assert_eq!(CursorStyle::Underline.name(), "underline");
    }
}
