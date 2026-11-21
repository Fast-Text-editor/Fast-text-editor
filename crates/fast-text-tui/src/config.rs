//! User configuration, read from `$XDG_CONFIG_HOME/fast-text/config.toml`.
//!
//! The file is a deliberately small TOML subset — sections, `key = value`
//! pairs and `#` comments — parsed by hand so Fast Text keeps its tiny
//! dependency footprint. Every setting is optional: a missing file simply
//! means "all defaults", and an unknown setting is reported but never fatal.

use std::collections::HashMap;
use std::path::PathBuf;

use fast_text_plugin::{CursorStyle, KeyName, KeySpec};

/// The commented file written by *Open config file* when none exists yet.
pub const TEMPLATE: &str = r#"# Fast Text configuration
#
# Every setting is optional; delete a line to fall back to the default.
# Restart, or run "Reload config" from the Ctrl+Shift+P palette, to apply changes.

[editor]
# bar | block | underline
cursor_style = "bar"
# dark | light | one-dark | monokai | dracula | nord | gruvbox
# | tokyo-night | solarized-dark | solarized-light | gojo
# (a plugin can define more names with define_theme)
theme        = "dark"
# start with the folder explorer visible
sidebar      = false
# talk to language servers (the same knob as FAST_TEXT_LSP)
lsp          = true
# show LSP messages on a floating row under the line
inline_diagnostics = true
# Vim-style Normal/Insert/Visual keys (Alt+V toggles at runtime)
vim          = false
# check GitHub for a newer release at startup (the same knob as
# FAST_TEXT_UPDATE_CHECK=0); Alt+U or "Check for updates" always check
check_updates = true

[sidebar]
# list dot-files
hidden = false

[console]
# Shell used by the terminal panel (Alt+C). "default" uses the system shell.
# windows: "default", "cmd", "powershell", "pwsh", or a program path
# linux/macos: "default" or a program path/name (e.g. "/bin/bash", "zsh")
windows = "default"
linux   = "default"
macos   = "default"

# Custom keybindings: "key" = "command".
#
# The command is either a built-in id (listed below) or any command from the
# Ctrl+Shift+P palette, including plugin commands. A bare letter with no modifier is
# refused, because it would stop you typing that letter.
#
# Built-in ids:
#   save, save_as, open, new, close, quit, undo, redo, select_all,
#   clear_selection, copy, cut, paste, find, find_next, find_prev, replace,
#   goto_line, language, command_palette, complete, hover, definition,
#   eval_textwrite, run_automation, toggle_theme, toggle_sidebar, open_folder,
#   manage_plugins, install_plugin, cursor_bar, cursor_block, cursor_underline,
#   help, next_tab, prev_tab, reload_config, open_config,
#   find_in_files, document_symbols, rename_symbol, format_document,
#   code_actions, toggle_comment, duplicate_line, delete_line, move_line_up,
#   move_line_down, select_next_occurrence, record_macro, replay_macro,
#   save_macro, check_updates, console_shell_default,
#   console_shell_cmd, console_shell_powershell, console_shell_pwsh
#
# A named macro can also be bound with the id "macro_<name>".
[keys]
# "alt+m"  = "manage_plugins"
# "f1"     = "help"
# "ctrl+k" = "cursor_block"
# "alt+k"  = "macro_tidy imports"
"#;

/// One `[keys]` entry: a key and the command it triggers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
    /// The parsed key.
    pub key: KeySpec,
    /// The command name exactly as written in the file.
    pub command: String,
    /// The key spelling from the file, e.g. `alt+m` (for messages).
    pub spelling: String,
}

/// Everything `config.toml` can set.
#[derive(Debug, Clone)]
pub struct Config {
    /// Terminal cursor shape.
    pub cursor_style: CursorStyle,
    /// The name of the color theme, e.g. `dark`, `light` or `monokai`.
    pub theme: String,
    /// Start with the folder explorer visible.
    pub sidebar: bool,
    /// List dot-files in the explorer.
    pub show_hidden: bool,
    /// Talk to language servers.
    pub lsp: bool,
    /// Draw inline diagnostics under the affected line.
    pub inline_diagnostics: bool,
    /// Start in Vim Normal mode.
    pub vim: bool,
    /// Check GitHub for a newer release at startup.
    pub check_updates: bool,
    /// Console shell preference for Windows (`default`, `cmd`, `powershell`, `pwsh`, or a path).
    pub console_windows: String,
    /// Console shell preference for Linux.
    pub console_linux: String,
    /// Console shell preference for macOS.
    pub console_macos: String,
    /// `[keys]` bindings, in file order.
    pub keys: Vec<KeyBinding>,
    /// Problems found while loading; surfaced as a status message.
    pub warnings: Vec<String>,
    /// The file the settings came from (or would have).
    pub path: Option<PathBuf>,
    /// True when a config file was actually read.
    pub loaded: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cursor_style: CursorStyle::Bar,
            theme: "dark".to_string(),
            sidebar: false,
            show_hidden: false,
            lsp: true,
            inline_diagnostics: true,
            vim: false,
            check_updates: true,
            console_windows: "default".to_string(),
            console_linux: "default".to_string(),
            console_macos: "default".to_string(),
            keys: Vec::new(),
            warnings: Vec::new(),
            path: None,
            loaded: false,
        }
    }
}

impl Config {
    /// Where the config lives: `$FAST_TEXT_CONFIG`, else
    /// `$XDG_CONFIG_HOME/fast-text/config.toml`, else `~/.config/...`.
    pub fn path() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("FAST_TEXT_CONFIG") {
            if !p.is_empty() {
                return Some(PathBuf::from(p));
            }
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("fast-text").join("config.toml"))
    }

    /// Where user themes live: `$XDG_CONFIG_HOME/fast-text/themes`, else
    /// `~/.config/fast-text/themes`.
    pub fn theme_dir() -> Option<PathBuf> {
        Config::path().and_then(|p| p.parent().map(|d| d.join("themes")))
    }

    /// Loads the config from its usual location.
    pub fn load() -> Self {
        Self::load_from(Self::path())
    }

    /// Loads from an explicit path. A missing file is not an error; any other
    /// read failure becomes a warning.
    pub fn load_from(path: Option<PathBuf>) -> Self {
        let mut config = Self {
            path: path.clone(),
            ..Self::default()
        };
        let Some(path) = path else {
            return config;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                config.loaded = true;
                config.apply(&text);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => config.warnings.push(format!("{}: {e}", path.display())),
        }
        config
    }

    /// Parses `text` on top of the defaults (no file involved).
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        config.apply(text);
        config
    }

    /// The key → command map handed to the editor (later entries win).
    pub fn key_map(&self) -> HashMap<KeySpec, String> {
        let mut map = HashMap::new();
        for binding in &self.keys {
            map.insert(binding.key, binding.command.clone());
        }
        map
    }

    /// Writes the commented template, creating parent directories as needed.
    pub fn write_template(path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        std::fs::write(path, TEMPLATE)
    }

    fn apply(&mut self, text: &str) {
        let mut section = String::from("editor");
        for (index, raw) in text.lines().enumerate() {
            let line_no = index + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                match rest.strip_suffix(']') {
                    Some(name) => section = name.trim().to_ascii_lowercase(),
                    None => self
                        .warnings
                        .push(format!("line {line_no}: unterminated [section]")),
                }
                continue;
            }
            let Some((key, value)) = split_assignment(line) else {
                self.warnings
                    .push(format!("line {line_no}: expected `key = value`"));
                continue;
            };
            let key = unquote(key.trim()).to_ascii_lowercase();
            let Some(value) = unquote_value(value) else {
                self.warnings
                    .push(format!("line {line_no}: unterminated string"));
                continue;
            };
            self.apply_setting(&section, &key, &value, line_no);
        }
    }

    fn apply_setting(&mut self, section: &str, key: &str, value: &str, line_no: usize) {
        match section {
            "editor" => match key {
                "cursor_style" => match CursorStyle::parse(value) {
                    Some(style) => self.cursor_style = style,
                    None => self.warnings.push(format!(
                        "line {line_no}: cursor_style must be bar, block or underline"
                    )),
                },
                "theme" => {
                    if value.trim().is_empty() {
                        self.warnings.push(format!(
                            "line {line_no}: theme needs a name, e.g. dark or monokai"
                        ));
                    } else {
                        // The name is kept as written: plugin-defined themes are
                        // only known once the plugins have loaded, so the editor
                        // resolves (and validates) it at startup.
                        self.theme = value.trim().to_string();
                    }
                }
                // (`visible` is accepted as a synonym for `sidebar`.)
                "sidebar" | "visible" => {
                    set_bool(&mut self.sidebar, value, line_no, &mut self.warnings)
                }
                "lsp" => set_bool(&mut self.lsp, value, line_no, &mut self.warnings),
                "inline_diagnostics" => set_bool(
                    &mut self.inline_diagnostics,
                    value,
                    line_no,
                    &mut self.warnings,
                ),
                "vim" => set_bool(&mut self.vim, value, line_no, &mut self.warnings),
                "check_updates" => {
                    set_bool(&mut self.check_updates, value, line_no, &mut self.warnings)
                }
                _ => self
                    .warnings
                    .push(format!("line {line_no}: unknown setting `editor.{key}`")),
            },
            "sidebar" => match key {
                "visible" => set_bool(&mut self.sidebar, value, line_no, &mut self.warnings),
                "hidden" => set_bool(&mut self.show_hidden, value, line_no, &mut self.warnings),
                _ => self
                    .warnings
                    .push(format!("line {line_no}: unknown setting `sidebar.{key}`")),
            },
            "console" => match key {
                "windows" | "shell_windows" => {
                    self.console_windows = normalize_console_shell(value);
                }
                "linux" | "shell_linux" => {
                    self.console_linux = normalize_console_shell(value);
                }
                "macos" | "shell_macos" => {
                    self.console_macos = normalize_console_shell(value);
                }
                "shell" => {
                    let normalized = normalize_console_shell(value);
                    self.console_windows = normalized.clone();
                    self.console_linux = normalized.clone();
                    self.console_macos = normalized;
                }
                _ => self
                    .warnings
                    .push(format!("line {line_no}: unknown setting `console.{key}`")),
            },
            "keys" => match KeySpec::parse(key) {
                None => self
                    .warnings
                    .push(format!("line {line_no}: cannot parse key `{key}`")),
                Some(key_spec) if is_typing_key(key_spec) => self.warnings.push(format!(
                    "line {line_no}: refusing to bind plain `{key}` — it would stop you typing it"
                )),
                Some(key_spec) => {
                    if self.keys.iter().any(|b| b.key == key_spec) {
                        self.warnings.push(format!(
                            "line {line_no}: `{key}` is bound twice; the later one wins"
                        ));
                        self.keys.retain(|b| b.key != key_spec);
                    }
                    self.keys.push(KeyBinding {
                        key: key_spec,
                        command: value.to_string(),
                        spelling: key.to_string(),
                    });
                }
            },
            other => self
                .warnings
                .push(format!("line {line_no}: unknown section `[{other}]`")),
        }
    }
}

/// Assigns a parsed boolean, leaving `target` alone when the value is unusable.
fn set_bool(target: &mut bool, value: &str, line_no: usize, warnings: &mut Vec<String>) {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => *target = true,
        "false" | "no" | "off" | "0" => *target = false,
        _ => warnings.push(format!(
            "line {line_no}: expected true or false, got `{value}`"
        )),
    }
}

/// Splits `key = value`, ignoring any `=` that sits inside a quoted key.
fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '=' if !quoted => return Some((&line[..i], &line[i + 1..])),
            _ => {}
        }
    }
    None
}

/// Strips surrounding quotes and any trailing comment from a bare value.
fn unquote_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(out),
                '\\' => {
                    if let Some(next) = chars.next() {
                        out.push(next);
                    }
                }
                _ => out.push(c),
            }
        }
        return None; // unterminated
    }
    let end = trimmed.find(['#', ';']).unwrap_or(trimmed.len());
    Some(trimmed[..end].trim().to_string())
}

/// Strips a single layer of surrounding quotes.
fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

/// True for a key that would swallow ordinary typing (a bare letter or space).
fn is_typing_key(spec: KeySpec) -> bool {
    !spec.ctrl && !spec.alt && !spec.shift && matches!(spec.name, KeyName::Char(_) | KeyName::Space)
}

/// Normalizes a console shell setting to a non-empty value (`"default"` when blank).
pub fn normalize_console_shell(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
}

impl Config {
    /// The configured console shell for the current platform.
    pub fn console_shell_for_current_os(&self) -> &str {
        if cfg!(target_os = "windows") {
            &self.console_windows
        } else if cfg!(target_os = "macos") {
            &self.console_macos
        } else {
            &self.console_linux
        }
    }

    /// Sets the console shell for the current platform in-memory.
    pub fn set_console_shell_for_current_os(&mut self, shell: &str) {
        let shell = normalize_console_shell(shell);
        if cfg!(target_os = "windows") {
            self.console_windows = shell;
        } else if cfg!(target_os = "macos") {
            self.console_macos = shell;
        } else {
            self.console_linux = shell;
        }
    }

    /// Updates or inserts `key = "value"` in `section` within a config file.
    ///
    /// The file is created from the template when missing. Returns the path
    /// that was written to, or an error.
    pub fn set_setting_in_file(
        path: Option<PathBuf>,
        section: &str,
        key: &str,
        value: &str,
    ) -> std::io::Result<PathBuf> {
        let path = path.or_else(Config::path).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot locate a config path (HOME is unset)",
            )
        })?;
        let mut text = if path.exists() {
            std::fs::read_to_string(&path)?
        } else {
            TEMPLATE.to_string()
        };
        text = upsert_toml_setting(&text, section, key, value);
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        std::fs::write(&path, text)?;
        Ok(path)
    }
}

/// Rewrites `text` so that `[section]` contains `key = "value"`.
fn upsert_toml_setting(text: &str, section: &str, key: &str, value: &str) -> String {
    let line = format!(
        "{key} = \"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let section_header = format!("[{section}]");
    let mut in_section = false;
    let mut section_start = None;
    let mut section_end = lines.len();
    let mut key_index = None;

    for (i, raw) in lines.iter().enumerate() {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_section {
                section_end = i;
                break;
            }
            if trimmed.eq_ignore_ascii_case(&section_header) {
                in_section = true;
                section_start = Some(i);
            }
            continue;
        }
        if in_section {
            if let Some((k, _)) = split_assignment(trimmed) {
                if unquote(k.trim()).eq_ignore_ascii_case(key) {
                    key_index = Some(i);
                }
            }
        }
    }
    if in_section && key_index.is_none() && section_start.is_some() {
        // section ran to EOF
        section_end = lines.len();
    }

    if let Some(idx) = key_index {
        lines[idx] = line;
    } else if let Some(start) = section_start {
        let insert_at = if section_end > start + 1 {
            start + 1
        } else {
            section_end
        };
        // insert after header (and after any immediate comment block is fine; keep simple)
        let mut at = insert_at;
        // skip leading blank/comment lines right after header? keep near top of section
        while at < section_end {
            let t = lines[at].trim();
            if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
                at += 1;
                continue;
            }
            break;
        }
        lines.insert(at, line);
    } else {
        if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            lines.push(String::new());
        }
        lines.push(section_header);
        lines.push(line);
    }

    let mut out = lines.join("\n");
    if text.ends_with('\n') || out.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.cursor_style, CursorStyle::Bar);
        assert_eq!(c.theme, "dark");
        assert!(!c.sidebar);
        assert!(!c.show_hidden);
        assert!(c.lsp);
        assert!(c.keys.is_empty());
        assert!(c.warnings.is_empty());
    }

    #[test]
    fn the_template_parses_without_warnings() {
        let c = Config::parse(TEMPLATE);
        assert!(c.warnings.is_empty(), "template warnings: {:?}", c.warnings);
        assert_eq!(c.cursor_style, CursorStyle::Bar);
        assert_eq!(c.theme, "dark");
        assert!(c.lsp);
        assert!(c.check_updates);
        assert!(c.keys.is_empty());
    }

    #[test]
    fn check_updates_can_be_turned_off() {
        let c = Config::parse(
            r#"
            [editor]
            check_updates = false
            "#,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert!(!c.check_updates);
    }

    #[test]
    fn reads_editor_and_sidebar_settings() {
        let c = Config::parse(
            r#"
            # a comment
            [editor]
            cursor_style = block   # trailing comment
            theme        = "light"
            sidebar      = yes
            lsp          = off

            [sidebar]
            hidden = true
            "#,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.cursor_style, CursorStyle::Block);
        assert_eq!(c.theme, "light");
        assert!(c.sidebar);
        assert!(!c.lsp);
        assert!(c.show_hidden);
    }

    #[test]
    fn reads_keybindings_including_quoted_keys() {
        let c = Config::parse(
            r#"
            [keys]
            "alt+m" = "manage_plugins"
            f1      = "help"
            ctrl+k  = cursor_block
            "#,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.keys.len(), 3);
        let map = c.key_map();
        assert_eq!(
            map.get(&KeySpec::parse("alt+m").unwrap()),
            Some(&"manage_plugins".to_string())
        );
        assert_eq!(
            map.get(&KeySpec::parse("f1").unwrap()),
            Some(&"help".to_string())
        );
        assert_eq!(
            map.get(&KeySpec::parse("ctrl+k").unwrap()),
            Some(&"cursor_block".to_string())
        );
    }

    #[test]
    fn refuses_bare_letters_but_allows_modified_ones() {
        let c = Config::parse(
            r#"
            [keys]
            f = "help"
            space = "help"
            ctrl+s = "save"
            alt+space = "command_palette"
            "#,
        );
        assert_eq!(c.keys.len(), 2);
        assert_eq!(c.warnings.len(), 2);
        assert!(c.warnings.iter().all(|w| w.contains("refusing")));
    }

    #[test]
    fn unknown_settings_and_bad_values_warn_but_do_not_fail() {
        let c = Config::parse(
            r#"
            [editor]
            cursor_style = squiggly
            theme = neon
            wobble = 3
            lsp = maybe

            [mystery]
            a = 1

            [keys]
            ctrl+ = "help"
            ctrl+nothing = "help"
            "#,
        );
        assert_eq!(c.cursor_style, CursorStyle::Bar);
        // An unknown theme name is stored; the editor reports it at startup so
        // a plugin-defined theme can still be selected from the config.
        assert_eq!(c.theme, "neon");
        assert!(c.lsp);
        assert_eq!(c.warnings.len(), 6, "{:?}", c.warnings);
    }

    #[test]
    fn last_binding_for_a_key_wins() {
        let c = Config::parse(
            r#"
            [keys]
            "alt+z" = "help"
            "alt+z" = "quit"
            "#,
        );
        assert_eq!(c.keys.len(), 1);
        assert_eq!(c.keys[0].command, "quit");
        assert!(c.warnings.iter().any(|w| w.contains("bound twice")));
    }

    #[test]
    fn reads_console_shell_settings() {
        let c = Config::parse(
            r#"
            [console]
            windows = "powershell"
            linux = "/bin/bash"
            macos = "zsh"
            "#,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.console_windows, "powershell");
        assert_eq!(c.console_linux, "/bin/bash");
        assert_eq!(c.console_macos, "zsh");
    }

    #[test]
    fn upsert_inserts_and_updates_console_setting() {
        let text = "[editor]\ntheme = \"dark\"\n\n[console]\nwindows = \"default\"\n";
        let updated = upsert_toml_setting(text, "console", "windows", "cmd");
        assert!(updated.contains("windows = \"cmd\""));
        assert!(!updated.contains("windows = \"default\""));
        let inserted =
            upsert_toml_setting("[editor]\ntheme = \"dark\"\n", "console", "windows", "cmd");
        assert!(inserted.contains("[console]"));
        assert!(inserted.contains("windows = \"cmd\""));
    }
}
