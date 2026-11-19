//! The shared Fast Text theme schema.
//!
//! A theme is a small TOML file with a `[ui]` table for interface colors and a
//! `[syntax]` table for token colors. The terminal, desktop and web frontends
//! all read this one schema, so a theme looks the same everywhere:
//!
//! ```toml
//! name = "midnight"
//! dark = true
//!
//! [ui]
//! background = "#10141c"
//! foreground = "#d7dae0"
//! ...
//!
//! [syntax]
//! keyword = "#c678dd"
//! ...
//! ```
//!
//! Colors are `#RRGGBB` (or `0xRRGGBB`); `#RGB` shorthand is accepted too.
//! Missing keys fall back to the built-in dark palette, so a theme only needs
//! the colors it wants to change. Unknown keys are reported as warnings rather
//! than errors, which keeps old themes loading after the schema grows.
//!
//! The ten built-in themes are themselves TOML files compiled into the crate
//! (`themes/*.toml`), so the frontends share exactly one definition of each
//! palette. User themes live in `~/.config/fast-text/themes/*.toml` (see
//! [`Theme::load_dir`]).

use std::fmt;
use std::path::{Path, PathBuf};

/// Every theme that ships with the editor, in palette order.
pub const BUILTIN_NAMES: &[&str] = &[
    "dark",
    "light",
    "one-dark",
    "monokai",
    "dracula",
    "nord",
    "gruvbox",
    "tokyo-night",
    "solarized-dark",
    "solarized-light",
];

/// Every color key the schema understands, in documentation order.
pub const THEME_KEYS: &[&str] = &[
    "background",
    "foreground",
    "gutter",
    "gutter_active",
    "cursor_line",
    "selection",
    "status_bg",
    "status_fg",
    "status_accent",
    "prompt_bg",
    "tab_active",
    "tab_inactive",
    "error",
    "warning",
    "plain",
    "keyword",
    "type",
    "function",
    "string",
    "number",
    "comment",
    "operator",
    "punctuation",
    "constant",
    "attribute",
    "variable",
];

/// Every built-in theme, embedded at compile time.
const BUILTINS: &[(&str, &str)] = &[
    ("dark", include_str!("../themes/dark.toml")),
    ("light", include_str!("../themes/light.toml")),
    ("one-dark", include_str!("../themes/one-dark.toml")),
    ("monokai", include_str!("../themes/monokai.toml")),
    ("dracula", include_str!("../themes/dracula.toml")),
    ("nord", include_str!("../themes/nord.toml")),
    ("gruvbox", include_str!("../themes/gruvbox.toml")),
    ("tokyo-night", include_str!("../themes/tokyo-night.toml")),
    (
        "solarized-dark",
        include_str!("../themes/solarized-dark.toml"),
    ),
    (
        "solarized-light",
        include_str!("../themes/solarized-light.toml"),
    ),
];

/// An 8-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Builds a color from `0xRRGGBB`.
    pub const fn from_u32(value: u32) -> Self {
        Self {
            r: ((value >> 16) & 0xFF) as u8,
            g: ((value >> 8) & 0xFF) as u8,
            b: (value & 0xFF) as u8,
        }
    }

    /// The packed `0xRRGGBB` form.
    pub const fn to_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    /// Parses `#RRGGBB`, `0xRRGGBB` or the `#RGB` shorthand.
    pub fn parse(text: &str) -> Option<Self> {
        let raw = text.trim().trim_matches('"').trim_matches('\'');
        let hex = raw
            .strip_prefix('#')
            .or_else(|| raw.strip_prefix("0x"))
            .or_else(|| raw.strip_prefix("0X"))
            .unwrap_or(raw);
        match hex.len() {
            3 => {
                let mut digits = hex.chars().map(|c| c.to_digit(16));
                let mut next = || digits.next().flatten().map(|d| (d as u8) * 17);
                Some(Self::new(next()?, next()?, next()?))
            }
            6 => {
                let value = u32::from_str_radix(hex, 16).ok()?;
                Some(Self::from_u32(value))
            }
            _ => None,
        }
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Interface colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiColors {
    pub background: Rgb,
    pub foreground: Rgb,
    pub gutter: Rgb,
    pub gutter_active: Rgb,
    pub cursor_line: Rgb,
    pub selection: Rgb,
    pub status_bg: Rgb,
    pub status_fg: Rgb,
    pub status_accent: Rgb,
    pub prompt_bg: Rgb,
    pub tab_active: Rgb,
    pub tab_inactive: Rgb,
    pub error: Rgb,
    pub warning: Rgb,
}

impl UiColors {
    /// The dark defaults, which double as the fallback for missing keys.
    pub const DARK: UiColors = UiColors {
        background: Rgb::new(0x1E, 0x21, 0x26),
        foreground: Rgb::new(0xD7, 0xDA, 0xE0),
        gutter: Rgb::new(0x4B, 0x52, 0x5E),
        gutter_active: Rgb::new(0xC8, 0xCC, 0xD4),
        cursor_line: Rgb::new(0x2A, 0x2E, 0x38),
        selection: Rgb::new(0x3A, 0x44, 0x55),
        status_bg: Rgb::new(0x28, 0x2C, 0x34),
        status_fg: Rgb::new(0xD7, 0xDA, 0xE0),
        status_accent: Rgb::new(0x61, 0xAF, 0xEF),
        prompt_bg: Rgb::new(0x2C, 0x31, 0x3A),
        tab_active: Rgb::new(0x61, 0xAF, 0xEF),
        tab_inactive: Rgb::new(0x4B, 0x52, 0x5E),
        error: Rgb::new(0xE0, 0x6C, 0x75),
        warning: Rgb::new(0xE5, 0xC0, 0x7B),
    };
}

impl Default for UiColors {
    fn default() -> Self {
        Self::DARK
    }
}

/// Syntax-highlighting colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxColors {
    pub plain: Rgb,
    pub keyword: Rgb,
    pub type_: Rgb,
    pub function: Rgb,
    pub string: Rgb,
    pub number: Rgb,
    pub comment: Rgb,
    pub operator: Rgb,
    pub punctuation: Rgb,
    pub constant: Rgb,
    pub attribute: Rgb,
    pub variable: Rgb,
}

impl SyntaxColors {
    /// The dark defaults, which double as the fallback for missing keys.
    pub const DARK: SyntaxColors = SyntaxColors {
        plain: Rgb::new(0xD7, 0xDA, 0xE0),
        keyword: Rgb::new(0xC6, 0x78, 0xDD),
        type_: Rgb::new(0xE5, 0xC0, 0x7B),
        function: Rgb::new(0x61, 0xAF, 0xEF),
        string: Rgb::new(0xCE, 0x91, 0x78),
        number: Rgb::new(0xD1, 0x9A, 0x66),
        comment: Rgb::new(0x6A, 0x73, 0x7D),
        operator: Rgb::new(0x56, 0xB6, 0xC2),
        punctuation: Rgb::new(0x9A, 0xA5, 0xB1),
        constant: Rgb::new(0xD1, 0x9A, 0x66),
        attribute: Rgb::new(0xE5, 0xC0, 0x7B),
        variable: Rgb::new(0xE0, 0x6C, 0x75),
    };
}

impl Default for SyntaxColors {
    fn default() -> Self {
        Self::DARK
    }
}

/// A parsed theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// The display name, e.g. `Monokai`.
    pub name: String,
    /// Whether this is a dark palette (frontends use it for native widgets).
    pub dark: bool,
    /// Window, gutter, selection and status-bar colors.
    pub ui: UiColors,
    /// Token colors.
    pub syntax: SyntaxColors,
}

impl Default for Theme {
    fn default() -> Self {
        Self::fallback()
    }
}

impl Theme {
    /// The dark defaults, with no name.
    pub fn fallback() -> Self {
        Self {
            name: "untitled".to_string(),
            dark: true,
            ui: UiColors::DARK,
            syntax: SyntaxColors::DARK,
        }
    }

    /// Every built-in theme name.
    pub fn names() -> &'static [&'static str] {
        BUILTIN_NAMES
    }

    /// A built-in theme by name, ignoring case, `_` and spaces.
    pub fn builtin(name: &str) -> Option<Theme> {
        let wanted = normalize_name(name);
        BUILTINS.iter().find_map(|(candidate, source)| {
            if normalize_name(candidate) == wanted {
                Theme::parse(source).ok()
            } else {
                None
            }
        })
    }

    /// The canonical spelling of a built-in name, or `None`.
    pub fn canonical_name(name: &str) -> Option<&'static str> {
        let wanted = normalize_name(name);
        BUILTIN_NAMES
            .iter()
            .copied()
            .find(|candidate| normalize_name(candidate) == wanted)
    }

    /// Whether a built-in name is a dark palette.
    pub fn is_dark(name: &str) -> bool {
        Theme::builtin(name).map(|t| t.dark).unwrap_or(true)
    }

    /// The default dark theme.
    pub fn dark() -> Theme {
        Theme::builtin("dark").unwrap_or_default()
    }

    /// The default light theme.
    pub fn light() -> Theme {
        Theme::builtin("light").unwrap_or_default()
    }

    /// Parses a theme, ignoring (but reporting) unknown keys and tables.
    pub fn parse(source: &str) -> Result<Theme, ThemeError> {
        Self::parse_with_warnings(source).map(|(theme, _)| theme)
    }

    /// Parses a theme and returns any unknown keys it mentioned.
    pub fn parse_with_warnings(source: &str) -> Result<(Theme, Vec<String>), ThemeError> {
        let mut theme = Theme::fallback();
        theme.name = "untitled".to_string();
        let mut warnings = Vec::new();
        let mut section = Section::Top;
        let mut saw_name = false;

        for (index, raw) in source.lines().enumerate() {
            let line_no = index + 1;
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(header) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                section = match normalize_name(header).as_str() {
                    "ui" | "interface" => Section::Ui,
                    "syntax" | "tokens" => Section::Syntax,
                    other => {
                        warnings.push(format!("line {line_no}: unknown table [{other}]"));
                        Section::Unknown
                    }
                };
                continue;
            }
            if section == Section::Unknown {
                continue;
            }
            let Some((key, value)) = split_assignment(line) else {
                return Err(ThemeError::new(
                    line_no,
                    "expected `key = value` or a `[table]` header",
                ));
            };
            let key = key.trim().to_ascii_lowercase();
            match key.as_str() {
                "name" => {
                    theme.name = unquote(value).trim().to_string();
                    saw_name = true;
                }
                "dark" => {
                    theme.dark = match unquote(value).trim().to_ascii_lowercase().as_str() {
                        "true" | "yes" | "on" | "1" => true,
                        "false" | "no" | "off" | "0" => false,
                        other => {
                            return Err(ThemeError::new(
                                line_no,
                                format!("`dark` must be true or false, found `{other}`"),
                            ))
                        }
                    };
                }
                _ => {
                    let color = Rgb::parse(value).ok_or_else(|| {
                        ThemeError::new(
                            line_no,
                            format!("`{key}` is not a color (expected #RRGGBB)"),
                        )
                    })?;
                    let known = match section {
                        Section::Syntax | Section::Top => {
                            set_syntax(&mut theme.syntax, &key, color)
                                || set_ui(&mut theme.ui, &key, color)
                        }
                        Section::Ui => set_ui(&mut theme.ui, &key, color),
                        Section::Unknown => false,
                    };
                    if !known {
                        warnings.push(format!("line {line_no}: unknown color `{key}`"));
                    }
                }
            }
        }

        if !saw_name {
            warnings.push("no `name` key; using `untitled`".to_string());
        }
        Ok((theme, warnings))
    }

    /// Parses a theme file.
    pub fn load_file(path: impl AsRef<Path>) -> Result<Theme, ThemeError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .map_err(|e| ThemeError::new(0, format!("cannot read {}: {e}", path.display())))?;
        Theme::parse(&source).map_err(|e| e.with_file(path))
    }

    /// The per-user theme directory, usually `~/.config/fast-text/themes`.
    ///
    /// `XDG_CONFIG_HOME` wins on Unix; Windows uses `%APPDATA%`; macOS uses
    /// `~/Library/Application Support`. Returns `None` only when the platform
    /// gives us no home directory at all.
    pub fn user_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(dir).join("fast-text").join("themes"));
        }
        if cfg!(windows) {
            if let Some(dir) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
                return Some(PathBuf::from(dir).join("fast-text").join("themes"));
            }
        }
        let home = std::env::var_os("HOME").filter(|v| !v.is_empty())?;
        let base = if cfg!(target_os = "macos") {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
        } else {
            PathBuf::from(home).join(".config")
        };
        Some(base.join("fast-text").join("themes"))
    }

    /// Loads every `*.toml` theme in a directory, sorted by file name.
    ///
    /// Unreadable or invalid files are returned as warnings, never fatal: one
    /// broken user theme must not stop the editor from starting.
    pub fn load_dir(dir: impl AsRef<Path>) -> (Vec<Theme>, Vec<String>) {
        let dir = dir.as_ref();
        let mut entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
                .collect(),
            Err(_) => return (Vec::new(), Vec::new()),
        };
        entries.sort();
        let mut themes = Vec::new();
        let mut warnings = Vec::new();
        for path in entries {
            match Theme::load_file(&path) {
                Ok(theme) => themes.push(theme),
                Err(e) => warnings.push(e.to_string()),
            }
        }
        (themes, warnings)
    }

    /// Overrides colors from a TextWrite `define_theme(name, {...})` map.
    ///
    /// `colors` are `(key, 0xRRGGBB)` pairs; keys use the same spelling as the
    /// TOML schema (`type` and `type_` both work). Returns the keys that were
    /// not recognized.
    pub fn apply_colors(&mut self, colors: &[(String, u32)]) -> Vec<String> {
        let mut unknown = Vec::new();
        for (key, value) in colors {
            let normalized = key.trim().to_ascii_lowercase().replace(['-', ' '], "_");
            let color = Rgb::from_u32(*value);
            if !set_ui(&mut self.ui, &normalized, color)
                && !set_syntax(&mut self.syntax, &normalized, color)
            {
                unknown.push(key.clone());
            }
        }
        unknown
    }

    /// Renders the theme back to the canonical TOML schema.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("name = \"{}\"\n", escape(&self.name)));
        out.push_str(&format!("dark = {}\n\n[ui]\n", self.dark));
        for (key, color) in ui_entries(&self.ui) {
            out.push_str(&format!("{key} = \"{color}\"\n"));
        }
        out.push_str("\n[syntax]\n");
        for (key, color) in syntax_entries(&self.syntax) {
            out.push_str(&format!("{key} = \"{color}\"\n"));
        }
        out
    }

    /// The color under a schema key, if it is recognized.
    pub fn color(&self, key: &str) -> Option<Rgb> {
        let key = key.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        ui_entries(&self.ui)
            .into_iter()
            .chain(syntax_entries(&self.syntax))
            .find(|(name, _)| *name == key || (*name == "type" && key == "type_"))
            .map(|(_, color)| color)
    }
}

/// A theme parse error, with a line number where possible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeError {
    pub line: usize,
    pub message: String,
    pub file: Option<PathBuf>,
}

impl ThemeError {
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
            file: None,
        }
    }

    fn with_file(mut self, path: &Path) -> Self {
        if self.line == 0 {
            self.message = format!("{}: {}", path.display(), self.message);
        } else {
            self.file = Some(path.to_path_buf());
        }
        self
    }
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(path) if self.line > 0 => {
                write!(f, "{}:{}: {}", path.display(), self.line, self.message)
            }
            Some(path) => write!(f, "{}: {}", path.display(), self.message),
            None if self.line > 0 => write!(f, "line {}: {}", self.line, self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ThemeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Top,
    Ui,
    Syntax,
    Unknown,
}

fn set_ui(ui: &mut UiColors, key: &str, color: Rgb) -> bool {
    match key {
        "background" => ui.background = color,
        "foreground" => ui.foreground = color,
        "gutter" => ui.gutter = color,
        "gutter_active" | "gutteractive" => ui.gutter_active = color,
        "cursor_line" | "cursorline" => ui.cursor_line = color,
        "selection" => ui.selection = color,
        "status_bg" | "statusbar_background" | "statusbar" => ui.status_bg = color,
        "status_fg" | "statusbar_foreground" => ui.status_fg = color,
        "status_accent" | "accent" => ui.status_accent = color,
        "prompt_bg" | "prompt" => ui.prompt_bg = color,
        "tab_active" => ui.tab_active = color,
        "tab_inactive" => ui.tab_inactive = color,
        "error" => ui.error = color,
        "warning" => ui.warning = color,
        _ => return false,
    }
    true
}

fn set_syntax(syntax: &mut SyntaxColors, key: &str, color: Rgb) -> bool {
    match key {
        "plain" | "text" => syntax.plain = color,
        "keyword" => syntax.keyword = color,
        "type" | "type_" => syntax.type_ = color,
        "function" => syntax.function = color,
        "string" => syntax.string = color,
        "number" => syntax.number = color,
        "comment" => syntax.comment = color,
        "operator" => syntax.operator = color,
        "punctuation" => syntax.punctuation = color,
        "constant" => syntax.constant = color,
        "attribute" => syntax.attribute = color,
        "variable" => syntax.variable = color,
        _ => return false,
    }
    true
}

fn ui_entries(ui: &UiColors) -> Vec<(&'static str, Rgb)> {
    vec![
        ("background", ui.background),
        ("foreground", ui.foreground),
        ("gutter", ui.gutter),
        ("gutter_active", ui.gutter_active),
        ("cursor_line", ui.cursor_line),
        ("selection", ui.selection),
        ("status_bg", ui.status_bg),
        ("status_fg", ui.status_fg),
        ("status_accent", ui.status_accent),
        ("prompt_bg", ui.prompt_bg),
        ("tab_active", ui.tab_active),
        ("tab_inactive", ui.tab_inactive),
        ("error", ui.error),
        ("warning", ui.warning),
    ]
}

fn syntax_entries(syntax: &SyntaxColors) -> Vec<(&'static str, Rgb)> {
    vec![
        ("plain", syntax.plain),
        ("keyword", syntax.keyword),
        ("type", syntax.type_),
        ("function", syntax.function),
        ("string", syntax.string),
        ("number", syntax.number),
        ("comment", syntax.comment),
        ("operator", syntax.operator),
        ("punctuation", syntax.punctuation),
        ("constant", syntax.constant),
        ("attribute", syntax.attribute),
        ("variable", syntax.variable),
    ]
}

/// Removes a trailing `#` comment that is outside quotes.
fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some(q) => {
                if ch == '\\' {
                    escaped = true;
                } else if ch == q {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => quote = Some(ch),
                '#' => return &line[..index],
                _ => {}
            },
        }
    }
    line
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let mut quote = None;
    for (index, ch) in line.char_indices() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == '=' => {
                return Some((&line[..index], &line[index + 1..]));
            }
            None => {}
        }
    }
    None
}

fn unquote(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Lower-cases a theme name and turns `_`/spaces into `-`.
pub fn normalize_name(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .replace(['_', ' '], "-")
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_parses() {
        assert_eq!(Theme::names().len(), 10);
        for name in Theme::names() {
            let theme = Theme::builtin(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(theme.name, *name);
            assert_eq!(Theme::canonical_name(name), Some(*name));
            assert_ne!(theme.ui.background, theme.ui.foreground, "{name}");
        }
    }

    #[test]
    fn builtin_palettes_are_distinct() {
        let mut backgrounds = Vec::new();
        for name in Theme::names() {
            backgrounds.push(Theme::builtin(name).unwrap().ui.background);
        }
        backgrounds.sort_by_key(|c| c.to_u32());
        backgrounds.dedup();
        assert_eq!(backgrounds.len(), 10);
    }

    #[test]
    fn names_ignore_case_and_separators() {
        assert!(Theme::builtin("Monokai").is_some());
        assert!(Theme::builtin("SOLARIZED_DARK").is_some());
        assert!(Theme::builtin("tokyo night").is_some());
        assert_eq!(Theme::canonical_name("One_Dark"), Some("one-dark"));
    }

    #[test]
    fn light_themes_report_light() {
        assert!(Theme::is_dark("dark"));
        assert!(Theme::is_dark("monokai"));
        assert!(!Theme::is_dark("light"));
        assert!(!Theme::is_dark("solarized-light"));
        assert!(!Theme::builtin("light").unwrap().dark);
    }

    #[test]
    fn parses_a_minimal_custom_theme() {
        let source = r##"
            # My palette
            name = "Midnight"
            dark = true

            [ui]
            background = "#101820"
            status_accent = "#00ffaa"   # trailing comment

            [syntax]
            keyword = 'ff79c6'
            type = "0x8be9fd"
        "##;
        let theme = Theme::parse(source).unwrap();
        assert_eq!(theme.name, "Midnight");
        assert_eq!(theme.ui.background, Rgb::new(0x10, 0x18, 0x20));
        assert_eq!(theme.ui.status_accent, Rgb::new(0x00, 0xFF, 0xAA));
        assert_eq!(theme.syntax.keyword, Rgb::new(0xFF, 0x79, 0xC6));
        assert_eq!(theme.syntax.type_, Rgb::new(0x8B, 0xE9, 0xFD));
        // Missing keys keep the dark defaults.
        assert_eq!(theme.ui.foreground, UiColors::DARK.foreground);
    }

    #[test]
    fn accepts_short_hex_and_0x() {
        assert_eq!(Rgb::parse("#abc"), Some(Rgb::new(0xAA, 0xBB, 0xCC)));
        assert_eq!(Rgb::parse("0xAABBCC"), Some(Rgb::new(0xAA, 0xBB, 0xCC)));
        assert_eq!(Rgb::parse("#AaBbCc"), Some(Rgb::new(0xAA, 0xBB, 0xCC)));
        assert_eq!(Rgb::parse("nope"), None);
    }

    #[test]
    fn reports_bad_colors_with_a_line_number() {
        let err = Theme::parse("name = \"x\"\n[ui]\nbackground = \"nope\"\n").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.to_string().contains("not a color"), "{err}");
    }

    #[test]
    fn unknown_keys_are_warnings_not_errors() {
        let source =
            "name = \"x\"\n[ui]\nbackground = \"#000000\"\nmystery = \"#ffffff\"\n[nope]\na = 1\n";
        let (theme, warnings) = Theme::parse_with_warnings(source).unwrap();
        assert_eq!(theme.ui.background, Rgb::new(0, 0, 0));
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings[0].contains("mystery"));
        assert!(warnings[1].contains("nope"));
    }

    #[test]
    fn plugin_overrides_use_the_same_keys() {
        let mut theme = Theme::dark();
        let unknown = theme.apply_colors(&[
            ("background".to_string(), 0x10_20_30),
            ("type".to_string(), 0x01_02_03),
            ("bogus".to_string(), 0x04_05_06),
        ]);
        assert_eq!(theme.ui.background, Rgb::new(0x10, 0x20, 0x30));
        assert_eq!(theme.syntax.type_, Rgb::new(0x01, 0x02, 0x03));
        assert_eq!(unknown, vec!["bogus".to_string()]);
    }

    #[test]
    fn to_toml_round_trips() {
        for name in Theme::names() {
            let theme = Theme::builtin(name).unwrap();
            let again = Theme::parse(&theme.to_toml()).unwrap();
            assert_eq!(theme, again, "{name}");
        }
    }

    #[test]
    fn load_dir_reports_broken_files_and_keeps_good_ones() {
        let dir = std::env::temp_dir().join(format!(
            "fast-text-theme-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.toml"),
            "name = \"A\"\n[ui]\nbackground = \"#010203\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("b.toml"),
            "name = \"B\"\n[ui]\nbackground = oops\n",
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let (themes, warnings) = Theme::load_dir(&dir);
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "A");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("b.toml"), "{warnings:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn color_lookup_handles_type_spelling() {
        let theme = Theme::builtin("monokai").unwrap();
        assert_eq!(theme.color("type"), Some(theme.syntax.type_));
        assert_eq!(theme.color("type_"), Some(theme.syntax.type_));
        assert_eq!(theme.color("cursor-line"), Some(theme.ui.cursor_line));
        assert_eq!(theme.color("nope"), None);
    }

    #[test]
    fn missing_name_is_reported() {
        let (theme, warnings) =
            Theme::parse_with_warnings("[ui]\nbackground = \"#000000\"\n").unwrap();
        assert_eq!(theme.name, "untitled");
        assert!(warnings.iter().any(|w| w.contains("name")));
    }
}
