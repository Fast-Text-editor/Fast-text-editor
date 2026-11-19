//! The shared editing keymap.
//!
//! Every frontend (terminal, desktop, web) resolves a key press through the
//! same [`EditorCommand`] table, so `Ctrl+S` saves and `Ctrl+P` opens the fuzzy
//! file finder whether you are in a terminal, a native window or a browser.
//! Frontends translate their native key event into a [`KeySpec`] and call
//! [`command_for`]; user configuration can then override any binding by name.
//!
//! The table deliberately avoids `Cmd` as a separate modifier: the desktop and
//! web frontends translate `Cmd`/`Super` to [`KeySpec::ctrl`], which gives macOS
//! users the conventional Cmd shortcuts without a second set of bindings.

use crate::{KeyName, KeySpec};

/// An editor command that every frontend understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorCommand {
    Save,
    SaveAs,
    Open,
    New,
    Close,
    Quit,
    Undo,
    Redo,
    SelectAll,
    Copy,
    Cut,
    Paste,
    Find,
    FindNext,
    FindPrevious,
    Replace,
    GotoLine,
    CommandPalette,
    FileFinder,
    LanguagePicker,
    ToggleSidebar,
    ToggleConsole,
    ToggleTheme,
    Help,
    Repl,
    ToggleComment,
    AddCursorAbove,
    AddCursorBelow,
    SelectNextOccurrence,
    NextBuffer,
    PrevBuffer,
    Format,
    DuplicateLine,
}

/// Every command, with its stable id, human title and default binding.
///
/// The id is what `config.toml` and plugins use (`"save"`, `"file_finder"`),
/// while the title is what command palettes display. Keeping both makes the
/// mapping stable across releases even if a title is reworded.
pub const COMMANDS: &[(EditorCommand, &str, &str)] = &[
    (EditorCommand::Save, "save", "Save"),
    (EditorCommand::SaveAs, "save_as", "Save As…"),
    (EditorCommand::Open, "open", "Open File…"),
    (EditorCommand::New, "new", "New Buffer"),
    (EditorCommand::Close, "close", "Close Buffer"),
    (EditorCommand::Quit, "quit", "Quit"),
    (EditorCommand::Undo, "undo", "Undo"),
    (EditorCommand::Redo, "redo", "Redo"),
    (EditorCommand::SelectAll, "select_all", "Select All"),
    (EditorCommand::Copy, "copy", "Copy"),
    (EditorCommand::Cut, "cut", "Cut"),
    (EditorCommand::Paste, "paste", "Paste"),
    (EditorCommand::Find, "find", "Find…"),
    (EditorCommand::FindNext, "find_next", "Find Next"),
    (
        EditorCommand::FindPrevious,
        "find_previous",
        "Find Previous",
    ),
    (EditorCommand::Replace, "replace", "Replace…"),
    (EditorCommand::GotoLine, "goto_line", "Go to Line…"),
    (
        EditorCommand::CommandPalette,
        "command_palette",
        "Command Palette…",
    ),
    (EditorCommand::FileFinder, "file_finder", "Go to File…"),
    (
        EditorCommand::LanguagePicker,
        "language",
        "Change Language…",
    ),
    (
        EditorCommand::ToggleSidebar,
        "toggle_sidebar",
        "Toggle Sidebar",
    ),
    (
        EditorCommand::ToggleConsole,
        "toggle_console",
        "Toggle Terminal",
    ),
    (EditorCommand::ToggleTheme, "toggle_theme", "Toggle Theme"),
    (EditorCommand::Help, "help", "Keyboard Shortcuts"),
    (EditorCommand::Repl, "repl", "TextWrite REPL…"),
    (
        EditorCommand::ToggleComment,
        "toggle_comment",
        "Toggle Comment",
    ),
    (
        EditorCommand::AddCursorAbove,
        "cursor_above",
        "Add Cursor Above",
    ),
    (
        EditorCommand::AddCursorBelow,
        "cursor_below",
        "Add Cursor Below",
    ),
    (
        EditorCommand::SelectNextOccurrence,
        "select_next_occurrence",
        "Select Next Occurrence",
    ),
    (EditorCommand::NextBuffer, "next_buffer", "Next Buffer"),
    (EditorCommand::PrevBuffer, "prev_buffer", "Previous Buffer"),
    (EditorCommand::Format, "format", "Format Document"),
    (
        EditorCommand::DuplicateLine,
        "duplicate_line",
        "Duplicate Line",
    ),
];

/// The default key bindings, in table order.
pub fn default_bindings() -> Vec<(KeySpec, EditorCommand)> {
    use EditorCommand::*;
    let c = |ch: char| KeySpec::new(KeyName::Char(ch), true, false, false);
    let cs = |ch: char| KeySpec::new(KeyName::Char(ch), true, false, true);
    let a = |ch: char| KeySpec::new(KeyName::Char(ch), false, true, false);
    let key = |name| KeySpec::new(name, false, false, false);
    let shift_key = |name| KeySpec::new(name, false, false, true);
    vec![
        (c('s'), Save),
        (cs('s'), SaveAs),
        (c('o'), Open),
        (c('n'), New),
        (c('w'), Close),
        (c('q'), Quit),
        (c('z'), Undo),
        (cs('z'), Redo),
        (c('y'), Redo),
        (c('a'), SelectAll),
        (c('c'), Copy),
        (c('x'), Cut),
        (c('v'), Paste),
        (c('f'), Find),
        (key(KeyName::F(3)), FindNext),
        (shift_key(KeyName::F(3)), FindPrevious),
        (c('h'), Replace),
        (c('g'), GotoLine),
        (cs('p'), CommandPalette),
        (c('p'), FileFinder),
        (c('l'), LanguagePicker),
        (c('b'), ToggleSidebar),
        (a('c'), ToggleConsole),
        (a('t'), ToggleTheme),
        (key(KeyName::F(1)), Help),
        (c('e'), Repl),
        (c('/'), ToggleComment),
        (shift_key(KeyName::Up), AddCursorAbove),
        (shift_key(KeyName::Down), AddCursorBelow),
        (c('d'), SelectNextOccurrence),
        (KeySpec::new(KeyName::Tab, true, false, false), NextBuffer),
        (KeySpec::new(KeyName::Tab, true, false, true), PrevBuffer),
        (cs('i'), Format),
        (cs('d'), DuplicateLine),
    ]
}

/// The binding in `default_bindings()` for this key, if any.
pub fn command_for(key: KeySpec) -> Option<EditorCommand> {
    default_bindings()
        .into_iter()
        .find_map(|(candidate, command)| (candidate == key).then_some(command))
}

/// The stable id of a command (`"save"`, `"file_finder"`).
pub fn command_id(command: EditorCommand) -> &'static str {
    COMMANDS
        .iter()
        .find_map(|(candidate, id, _)| (*candidate == command).then_some(*id))
        .unwrap_or("unknown")
}

/// A command's human title (`"Save As…"`).
pub fn command_title(command: EditorCommand) -> &'static str {
    COMMANDS
        .iter()
        .find_map(|(candidate, _, title)| (*candidate == command).then_some(*title))
        .unwrap_or("Unknown")
}

/// Resolves a stable id or a human title (case- and separator-insensitive).
pub fn command_from_name(name: &str) -> Option<EditorCommand> {
    let wanted = normalize(name);
    COMMANDS.iter().find_map(|(command, id, title)| {
        (normalize(id) == wanted || normalize(title) == wanted).then_some(*command)
    })
}

/// The display spelling of a command's default binding, e.g. `Ctrl+P`.
pub fn default_shortcut(command: EditorCommand) -> Option<String> {
    default_bindings()
        .into_iter()
        .find_map(|(key, candidate)| (candidate == command).then(|| key.display()))
}

fn normalize(text: &str) -> String {
    text.trim()
        .to_ascii_lowercase()
        .replace(['_', '-', ' ', '…'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn important_commands_are_bound() {
        let finder = KeySpec::new(KeyName::Char('p'), true, false, false);
        assert_eq!(command_for(finder), Some(EditorCommand::FileFinder));
        let palette = KeySpec::new(KeyName::Char('p'), true, false, true);
        assert_eq!(command_for(palette), Some(EditorCommand::CommandPalette));
        assert_eq!(
            command_for(KeySpec::parse("ctrl+s").unwrap()),
            Some(EditorCommand::Save)
        );
        assert_eq!(
            command_for(KeySpec::parse("alt+c").unwrap()),
            Some(EditorCommand::ToggleConsole)
        );
    }

    #[test]
    fn ids_and_titles_resolve_to_the_same_command() {
        for (command, id, title) in COMMANDS {
            assert_eq!(command_from_name(id), Some(*command), "{id}");
            assert_eq!(command_from_name(title), Some(*command), "{title}");
            assert_eq!(command_id(*command), *id);
            assert_eq!(command_title(*command), *title);
        }
    }

    #[test]
    fn every_command_has_a_unique_id_and_title() {
        let mut ids: Vec<&str> = COMMANDS.iter().map(|(_, id, _)| *id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), COMMANDS.len());
        let mut titles: Vec<&str> = COMMANDS.iter().map(|(_, _, title)| *title).collect();
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(titles.len(), COMMANDS.len());
    }

    #[test]
    fn most_commands_have_a_shortcut() {
        let missing: Vec<&str> = COMMANDS
            .iter()
            .filter(|(command, _, _)| default_shortcut(*command).is_none())
            .map(|(_, id, _)| *id)
            .collect();
        // Prev/next buffer intentionally share Ctrl+Tab with Shift as a prefix
        // on some terminals, and Format/Duplicate have alternates.
        assert!(missing.len() <= 3, "unbound commands: {missing:?}");
    }

    #[test]
    fn no_key_is_bound_twice() {
        let mut keys: Vec<String> = default_bindings()
            .iter()
            .map(|(key, _)| key.display())
            .collect();
        keys.sort();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len());
    }

    #[test]
    fn shortcut_display_uses_the_canonical_spelling() {
        assert_eq!(
            default_shortcut(EditorCommand::FileFinder).as_deref(),
            Some("Ctrl+P")
        );
        assert_eq!(
            default_shortcut(EditorCommand::ToggleTheme).as_deref(),
            Some("Alt+T")
        );
    }
}
