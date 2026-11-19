use fast_text_syntax::TokenKind;
use fast_text_theme::{Rgb, Theme as SharedTheme};
use ratatui::style::{Color, Modifier, Style};

/// Lower-cases a theme name and turns `_`/spaces into `-`.
pub use fast_text_theme::normalize_name;
/// Every theme that ships with the editor, in palette order.
pub use fast_text_theme::BUILTIN_NAMES as BUILTIN_THEMES;
/// The color keys a `define_theme` plugin may set.
pub use fast_text_theme::THEME_KEYS;

/// Colors and styles used throughout the interface.
///
/// This is the terminal projection of [`fast_text_theme::Theme`]: the same TOML
/// schema drives the TUI, the desktop GUI and the web frontend, so switching a
/// theme in one place gives every frontend the same palette.
#[derive(Debug, Clone)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub gutter: Color,
    pub gutter_active: Color,
    pub cursor_line: Color,
    pub selection: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    pub status_accent: Color,
    pub prompt_bg: Color,
    pub tab_active: Color,
    pub tab_inactive: Color,
    pub error: Color,
    pub warning: Color,

    pub plain: Style,
    pub keyword: Style,
    pub type_: Style,
    pub function: Style,
    pub string: Style,
    pub number: Style,
    pub comment: Style,
    pub operator: Style,
    pub punctuation: Style,
    pub constant: Style,
    pub attribute: Style,
    pub variable: Style,

    /// The display name of the shared theme this was built from.
    pub name: String,
    /// Whether this is a dark palette.
    pub dark: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Converts a schema color to a ratatui color.
pub fn color(rgb: Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

impl Theme {
    /// The default dark theme.
    pub fn dark() -> Self {
        Self::from_shared(SharedTheme::dark())
    }

    /// The default light theme.
    pub fn light() -> Self {
        Self::from_shared(SharedTheme::light())
    }

    /// Every built-in theme name.
    pub fn names() -> &'static [&'static str] {
        SharedTheme::names()
    }

    /// A built-in theme by name, ignoring case, `_`, and spaces.
    pub fn by_name(name: &str) -> Option<Theme> {
        let canonical = match normalize_name(name).as_str() {
            "default" => "dark",
            "onedark" | "atom-one-dark" => "one-dark",
            "gruvbox-dark" => "gruvbox",
            "tokyonight" => "tokyo-night",
            "solarized" => "solarized-dark",
            "solarizedlight" => "solarized-light",
            _ => "",
        };
        let canonical = if canonical.is_empty() {
            SharedTheme::canonical_name(name)?
        } else {
            canonical
        };
        SharedTheme::builtin(canonical).map(Self::from_shared)
    }

    /// Loads a theme from a TOML file (the shared schema).
    pub fn load_file(
        path: impl AsRef<std::path::Path>,
    ) -> Result<Theme, fast_text_theme::ThemeError> {
        SharedTheme::load_file(path).map(Self::from_shared)
    }

    /// Parses a theme from TOML text.
    pub fn parse(source: &str) -> Result<Theme, fast_text_theme::ThemeError> {
        SharedTheme::parse(source).map(Self::from_shared)
    }

    /// Whether a name refers to a dark palette (`false` for light themes).
    pub fn is_dark(name: &str) -> bool {
        if let Some(theme) = Self::by_name(name) {
            return theme.dark;
        }
        true
    }

    /// The canonical spelling of `name`, or `None` when it is not built in.
    pub fn canonical_name(name: &str) -> Option<&'static str> {
        SharedTheme::canonical_name(name)
    }

    /// A theme built from a plugin's color map, over the dark palette.
    ///
    /// Returns the theme and any keys that were not recognized.
    pub fn from_colors(colors: &[(String, u32)]) -> (Theme, Vec<String>) {
        let mut shared = SharedTheme::dark();
        let unknown = shared.apply_colors(colors);
        (Self::from_shared(shared), unknown)
    }

    /// Projects the shared schema into ratatui colors and styles.
    pub fn from_shared(shared: SharedTheme) -> Self {
        let ui = shared.ui;
        let syntax = shared.syntax;
        let c = color;
        Self {
            background: c(ui.background),
            foreground: c(ui.foreground),
            gutter: c(ui.gutter),
            gutter_active: c(ui.gutter_active),
            cursor_line: c(ui.cursor_line),
            selection: c(ui.selection),
            status_bg: c(ui.status_bg),
            status_fg: c(ui.status_fg),
            status_accent: c(ui.status_accent),
            prompt_bg: c(ui.prompt_bg),
            tab_active: c(ui.tab_active),
            tab_inactive: c(ui.tab_inactive),
            error: c(ui.error),
            warning: c(ui.warning),
            plain: Style::default().fg(c(syntax.plain)),
            keyword: Style::default()
                .fg(c(syntax.keyword))
                .add_modifier(Modifier::BOLD),
            type_: Style::default().fg(c(syntax.type_)),
            function: Style::default().fg(c(syntax.function)),
            string: Style::default().fg(c(syntax.string)),
            number: Style::default().fg(c(syntax.number)),
            comment: Style::default()
                .fg(c(syntax.comment))
                .add_modifier(Modifier::ITALIC),
            operator: Style::default().fg(c(syntax.operator)),
            punctuation: Style::default().fg(c(syntax.punctuation)),
            constant: Style::default().fg(c(syntax.constant)),
            attribute: Style::default().fg(c(syntax.attribute)),
            variable: Style::default().fg(c(syntax.variable)),
            name: shared.name,
            dark: shared.dark,
        }
    }

    /// Style for a syntax token.
    pub fn token_style(&self, kind: TokenKind) -> Style {
        match kind {
            TokenKind::Plain => self.plain,
            TokenKind::Keyword => self.keyword,
            TokenKind::Type => self.type_,
            TokenKind::Function => self.function,
            TokenKind::String => self.string,
            TokenKind::Number => self.number,
            TokenKind::Comment => self.comment,
            TokenKind::Operator => self.operator,
            TokenKind::Punctuation => self.punctuation,
            TokenKind::Constant => self.constant,
            TokenKind::Attribute => self.attribute,
            TokenKind::Variable => self.variable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_name_resolves() {
        for name in Theme::names() {
            let theme = Theme::by_name(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(Theme::canonical_name(name), Some(*name));
            assert!(matches!(theme.background, Color::Rgb(..)));
        }
    }

    #[test]
    fn aliases_resolve_to_builtins() {
        assert!(Theme::by_name("Monokai").is_some());
        assert!(Theme::by_name("SOLARIZED_DARK").is_some());
        assert!(Theme::by_name("tokyo night").is_some());
        assert_eq!(Theme::canonical_name("One_Dark"), Some("one-dark"));
        assert_eq!(Theme::by_name("default").unwrap().name, "dark");
        assert_eq!(Theme::by_name("solarized").unwrap().name, "solarized-dark");
    }

    #[test]
    fn only_light_palettes_are_light() {
        assert!(Theme::is_dark("dark"));
        assert!(Theme::is_dark("monokai"));
        assert!(!Theme::is_dark("light"));
        assert!(!Theme::is_dark("solarized-light"));
    }

    #[test]
    fn plugin_colors_override_and_report_unknown() {
        let colors = vec![
            ("background".to_string(), 0x102030u32),
            ("bogus".to_string(), 0x010203u32),
        ];
        let (theme, unknown) = Theme::from_colors(&colors);
        assert_eq!(theme.background, Color::Rgb(0x10, 0x20, 0x30));
        assert_eq!(unknown, vec!["bogus".to_string()]);
        // Untouched colors fall back to the dark palette.
        assert_eq!(theme.function.fg, Theme::dark().function.fg);
    }

    #[test]
    fn custom_toml_round_trips_through_the_tui() {
        let theme = Theme::parse(
            "name = \"Mine\"\ndark = false\n[ui]\nbackground = \"#010203\"\n[syntax]\ntype = \"#aabbcc\"\n",
        )
        .unwrap();
        assert_eq!(theme.background, Color::Rgb(1, 2, 3));
        assert!(!theme.dark);
        assert_eq!(theme.type_.fg, Some(Color::Rgb(0xAA, 0xBB, 0xCC)));
        assert_eq!(theme.name, "Mine");
    }

    #[test]
    fn unknown_name_has_no_theme() {
        assert!(Theme::by_name("neon").is_none());
    }
}
