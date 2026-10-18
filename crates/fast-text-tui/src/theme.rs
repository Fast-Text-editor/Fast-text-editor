use fast_text_syntax::TokenKind;
use ratatui::style::{Color, Modifier, Style};

/// Colors and styles used throughout the interface.
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
}

/// Every theme that ships with the editor, in palette order.
pub const BUILTIN_THEMES: &[&str] = &[
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

/// The color keys a `define_theme` plugin may set.
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

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// A modern dark theme (the default).
    pub fn dark() -> Self {
        Self::from_palette(Palette {
            background: 0x1E_21_26,
            foreground: 0xD7_DA_E0,
            gutter: 0x4B_52_5E,
            gutter_active: 0xC8_CC_D4,
            cursor_line: 0x2A_2E_38,
            selection: 0x3A_44_55,
            status_bg: 0x28_2C_34,
            status_fg: 0xD7_DA_E0,
            status_accent: 0x61_AF_EF,
            prompt_bg: 0x2C_31_3A,
            tab_active: 0x61_AF_EF,
            tab_inactive: 0x4B_52_5E,
            error: 0xE0_6C_75,
            warning: 0xE5_C0_7B,
            plain: 0xD7_DA_E0,
            keyword: 0xC6_78_DD,
            type_: 0xE5_C0_7B,
            function: 0x61_AF_EF,
            string: 0xCE_91_78,
            number: 0xD1_9A_66,
            comment: 0x6A_73_7D,
            operator: 0x56_B6_C2,
            punctuation: 0x9A_A5_B1,
            constant: 0xD1_9A_66,
            attribute: 0xE5_C0_7B,
            variable: 0xE0_6C_75,
        })
    }

    /// A light theme.
    pub fn light() -> Self {
        let mut theme = Self::from_palette(Palette {
            background: 0xFA_FA_FA,
            foreground: 0x24_29_2E,
            gutter: 0x9D_A3_AB,
            gutter_active: 0x24_29_2E,
            cursor_line: 0xEE_F1_F5,
            selection: 0xC8_DD_F5,
            status_bg: 0xE8_EA_ED,
            status_fg: 0x24_29_2E,
            status_accent: 0x0B_6C_D6,
            prompt_bg: 0xE8_EA_ED,
            tab_active: 0x0B_6C_D6,
            tab_inactive: 0x9D_A3_AB,
            error: 0xC0_39_2B,
            warning: 0xB0_6A_00,
            plain: 0x24_29_2E,
            keyword: 0xA6_26_A6,
            type_: 0x9A_67_00,
            function: 0x1A_5F_B0,
            string: 0x0A_7A_4A,
            number: 0xB0_5A_00,
            comment: 0x7A_82_8C,
            operator: 0x0B_72_8A,
            punctuation: 0x5A_62_6C,
            constant: 0xB0_5A_00,
            attribute: 0x9A_67_00,
            variable: 0xB0_3A_40,
        });
        theme.rebuild_styles();
        theme
    }

    /// The Atom "One Dark" palette.
    pub fn one_dark() -> Self {
        Self::from_palette(Palette {
            background: 0x28_2C_34,
            foreground: 0xAB_B2_BF,
            gutter: 0x4B_52_63,
            gutter_active: 0xD7_DA_E0,
            cursor_line: 0x2C_31_3A,
            selection: 0x3E_44_51,
            status_bg: 0x21_25_2B,
            status_fg: 0xAB_B2_BF,
            status_accent: 0x61_AF_EF,
            prompt_bg: 0x2C_31_3A,
            tab_active: 0x61_AF_EF,
            tab_inactive: 0x4B_52_63,
            error: 0xE0_6C_75,
            warning: 0xE5_C0_7B,
            plain: 0xAB_B2_BF,
            keyword: 0xC6_78_DD,
            type_: 0xE5_C0_7B,
            function: 0x61_AF_EF,
            string: 0x98_C3_79,
            number: 0xD1_9A_66,
            comment: 0x5C_63_70,
            operator: 0x56_B6_C2,
            punctuation: 0xAB_B2_BF,
            constant: 0xD1_9A_66,
            attribute: 0xE5_C0_7B,
            variable: 0xE0_6C_75,
        })
    }

    /// The classic Monokai palette.
    pub fn monokai() -> Self {
        Self::from_palette(Palette {
            background: 0x27_28_22,
            foreground: 0xF8_F8_F2,
            gutter: 0x75_71_5E,
            gutter_active: 0xF8_F8_F2,
            cursor_line: 0x31_32_2A,
            selection: 0x49_48_3E,
            status_bg: 0x1E_1F_1C,
            status_fg: 0xF8_F8_F2,
            status_accent: 0xA6_E2_2E,
            prompt_bg: 0x31_32_2A,
            tab_active: 0xF9_26_72,
            tab_inactive: 0x75_71_5E,
            error: 0xF9_26_72,
            warning: 0xE6_DB_74,
            plain: 0xF8_F8_F2,
            keyword: 0xF9_26_72,
            type_: 0x66_D9_EF,
            function: 0xA6_E2_2E,
            string: 0xE6_DB_74,
            number: 0xAE_81_FF,
            comment: 0x75_71_5E,
            operator: 0xF9_26_72,
            punctuation: 0xF8_F8_F2,
            constant: 0xAE_81_FF,
            attribute: 0xA6_E2_2E,
            variable: 0xF8_F8_F2,
        })
    }

    /// The Dracula palette.
    pub fn dracula() -> Self {
        Self::from_palette(Palette {
            background: 0x28_2A_36,
            foreground: 0xF8_F8_F2,
            gutter: 0x62_72_A4,
            gutter_active: 0xF8_F8_F2,
            cursor_line: 0x30_32_3F,
            selection: 0x44_47_5A,
            status_bg: 0x21_22_2C,
            status_fg: 0xF8_F8_F2,
            status_accent: 0xBD_93_F9,
            prompt_bg: 0x30_32_3F,
            tab_active: 0xFF_79_C6,
            tab_inactive: 0x62_72_A4,
            error: 0xFF_55_55,
            warning: 0xF1_FA_8C,
            plain: 0xF8_F8_F2,
            keyword: 0xFF_79_C6,
            type_: 0x8B_E9_FD,
            function: 0x50_FA_7B,
            string: 0xF1_FA_8C,
            number: 0xBD_93_F9,
            comment: 0x62_72_A4,
            operator: 0xFF_79_C6,
            punctuation: 0xF8_F8_F2,
            constant: 0xBD_93_F9,
            attribute: 0x50_FA_7B,
            variable: 0xF8_F8_F2,
        })
    }

    /// The Nord palette.
    pub fn nord() -> Self {
        Self::from_palette(Palette {
            background: 0x2E_34_40,
            foreground: 0xD8_DE_E9,
            gutter: 0x4C_56_6A,
            gutter_active: 0xEC_EF_F4,
            cursor_line: 0x3B_42_52,
            selection: 0x43_4C_5E,
            status_bg: 0x29_2E_39,
            status_fg: 0xD8_DE_E9,
            status_accent: 0x88_C0_D0,
            prompt_bg: 0x3B_42_52,
            tab_active: 0x88_C0_D0,
            tab_inactive: 0x4C_56_6A,
            error: 0xBF_61_6A,
            warning: 0xEB_CB_8B,
            plain: 0xD8_DE_E9,
            keyword: 0x81_A1_C1,
            type_: 0x8F_BC_BB,
            function: 0x88_C0_D0,
            string: 0xA3_BE_8C,
            number: 0xB4_8E_AD,
            comment: 0x61_6E_88,
            operator: 0x81_A1_C1,
            punctuation: 0xD8_DE_E9,
            constant: 0xB4_8E_AD,
            attribute: 0x88_C0_D0,
            variable: 0xD8_DE_E9,
        })
    }

    /// The Gruvbox dark palette.
    pub fn gruvbox() -> Self {
        Self::from_palette(Palette {
            background: 0x28_28_28,
            foreground: 0xEB_DB_B2,
            gutter: 0x7C6F64,
            gutter_active: 0xEB_DB_B2,
            cursor_line: 0x32_30_2F,
            selection: 0x50_49_45,
            status_bg: 0x3C_38_36,
            status_fg: 0xEB_DB_B2,
            status_accent: 0x83_A5_98,
            prompt_bg: 0x32_30_2F,
            tab_active: 0xFA_BD_2F,
            tab_inactive: 0x7C6F64,
            error: 0xFB_49_34,
            warning: 0xFA_BD_2F,
            plain: 0xEB_DB_B2,
            keyword: 0xFB_49_34,
            type_: 0xFA_BD_2F,
            function: 0xB8_BB_26,
            string: 0xB8_BB_26,
            number: 0xD3_86_9B,
            comment: 0x92_83_74,
            operator: 0xFE_80_19,
            punctuation: 0xEB_DB_B2,
            constant: 0xD3_86_9B,
            attribute: 0x8E_C0_7C,
            variable: 0xEB_DB_B2,
        })
    }
    /// The Tokyo Night palette.
    pub fn tokyo_night() -> Self {
        Self::from_palette(Palette {
            background: 0x1A_1B_26,
            foreground: 0xC0_CA_F5,
            gutter: 0x3B_42_61,
            gutter_active: 0xC0_CA_F5,
            cursor_line: 0x1F_23_35,
            selection: 0x33_46_7C,
            status_bg: 0x16_16_1E,
            status_fg: 0xC0_CA_F5,
            status_accent: 0x7A_A2_F7,
            prompt_bg: 0x1F_23_35,
            tab_active: 0x7A_A2_F7,
            tab_inactive: 0x3B_42_61,
            error: 0xF7_76_8E,
            warning: 0xE0_AF_68,
            plain: 0xC0_CA_F5,
            keyword: 0xBB_9A_F7,
            type_: 0x2A_C3_DE,
            function: 0x7A_A2_F7,
            string: 0x9E_CE_6A,
            number: 0xFF9E64,
            comment: 0x56_5F_89,
            operator: 0x89_DD_FF,
            punctuation: 0xC0_CA_F5,
            constant: 0xFF9E64,
            attribute: 0x7A_A2_F7,
            variable: 0xC0_CA_F5,
        })
    }

    /// The Solarized dark palette.
    pub fn solarized_dark() -> Self {
        Self::from_palette(Palette {
            background: 0x00_2B_36,
            foreground: 0x83_94_96,
            gutter: 0x58_6E_75,
            gutter_active: 0x93_A1_A1,
            cursor_line: 0x07_36_42,
            selection: 0x07_36_42,
            status_bg: 0x07_36_42,
            status_fg: 0x93_A1_A1,
            status_accent: 0x26_8B_D2,
            prompt_bg: 0x07_36_42,
            tab_active: 0x26_8B_D2,
            tab_inactive: 0x58_6E_75,
            error: 0xDC_32_2F,
            warning: 0xB5_89_00,
            plain: 0x83_94_96,
            keyword: 0x85_99_00,
            type_: 0xB5_89_00,
            function: 0x26_8B_D2,
            string: 0x2A_A1_98,
            number: 0xD3_36_82,
            comment: 0x58_6E_75,
            operator: 0x85_99_00,
            punctuation: 0x83_94_96,
            constant: 0xD3_36_82,
            attribute: 0x26_8B_D2,
            variable: 0x83_94_96,
        })
    }

    /// The Solarized light palette.
    pub fn solarized_light() -> Self {
        Self::from_palette(Palette {
            background: 0xFD_F6_E3,
            foreground: 0x65_7B_83,
            gutter: 0x93_A1_A1,
            gutter_active: 0x58_6E_75,
            cursor_line: 0xEE_E8_D5,
            selection: 0xEE_E8_D5,
            status_bg: 0xEE_E8_D5,
            status_fg: 0x58_6E_75,
            status_accent: 0x26_8B_D2,
            prompt_bg: 0xEE_E8_D5,
            tab_active: 0x26_8B_D2,
            tab_inactive: 0x93_A1_A1,
            error: 0xDC_32_2F,
            warning: 0xB5_89_00,
            plain: 0x65_7B_83,
            keyword: 0x85_99_00,
            type_: 0xB5_89_00,
            function: 0x26_8B_D2,
            string: 0x2A_A1_98,
            number: 0xD3_36_82,
            comment: 0x93_A1_A1,
            operator: 0x85_99_00,
            punctuation: 0x65_7B_83,
            constant: 0xD3_36_82,
            attribute: 0x26_8B_D2,
            variable: 0x65_7B_83,
        })
    }

    /// Every built-in theme name.
    pub fn names() -> &'static [&'static str] {
        BUILTIN_THEMES
    }

    /// A built-in theme by name, ignoring case, `_`, and spaces.
    pub fn by_name(name: &str) -> Option<Theme> {
        Some(match normalize_name(name).as_str() {
            "dark" | "default" => Theme::dark(),
            "light" => Theme::light(),
            "one-dark" | "onedark" | "atom-one-dark" => Theme::one_dark(),
            "monokai" => Theme::monokai(),
            "dracula" => Theme::dracula(),
            "nord" => Theme::nord(),
            "gruvbox" | "gruvbox-dark" => Theme::gruvbox(),
            "tokyo-night" | "tokyonight" => Theme::tokyo_night(),
            "solarized-dark" | "solarized" => Theme::solarized_dark(),
            "solarized-light" => Theme::solarized_light(),
            _ => return None,
        })
    }

    /// Whether a name refers to a dark palette (`false` for light themes).
    pub fn is_dark(name: &str) -> bool {
        !matches!(
            normalize_name(name).as_str(),
            "light" | "solarized-light" | "solarizedlight"
        )
    }

    /// The canonical spelling of `name`, or `None` when it is not built in.
    pub fn canonical_name(name: &str) -> Option<&'static str> {
        let normalized = normalize_name(name);
        BUILTIN_THEMES
            .iter()
            .copied()
            .find(|candidate| normalize_name(candidate) == normalized)
    }

    /// A theme built from a plugin's color map, over the dark palette.
    ///
    /// Returns the theme and any keys that were not recognized.
    pub fn from_colors(colors: &[(String, u32)]) -> (Theme, Vec<String>) {
        let mut theme = Theme::dark();
        let mut unknown = Vec::new();
        for (key, value) in colors {
            let normalized = normalize_name(key).replace('-', "_");
            let color = Color::Rgb(
                ((value >> 16) & 0xFF) as u8,
                ((value >> 8) & 0xFF) as u8,
                (value & 0xFF) as u8,
            );
            if !theme.set_color(&normalized, color) {
                unknown.push(key.clone());
            }
        }
        theme.rebuild_styles();
        (theme, unknown)
    }

    /// Sets one named color, returning false when the key is unknown.
    fn set_color(&mut self, key: &str, color: Color) -> bool {
        match key {
            "background" => self.background = color,
            "foreground" => self.foreground = color,
            "gutter" => self.gutter = color,
            "gutter_active" => self.gutter_active = color,
            "cursor_line" => self.cursor_line = color,
            "selection" => self.selection = color,
            "status_bg" => self.status_bg = color,
            "status_fg" => self.status_fg = color,
            "status_accent" => self.status_accent = color,
            "prompt_bg" => self.prompt_bg = color,
            "tab_active" => self.tab_active = color,
            "tab_inactive" => self.tab_inactive = color,
            "error" => self.error = color,
            "warning" => self.warning = color,
            "plain" => self.plain = Style::default().fg(color),
            "keyword" => self.keyword = Style::default().fg(color).add_modifier(Modifier::BOLD),
            "type" | "type_" => self.type_ = Style::default().fg(color),
            "function" => self.function = Style::default().fg(color),
            "string" => self.string = Style::default().fg(color),
            "number" => self.number = Style::default().fg(color),
            "comment" => self.comment = Style::default().fg(color).add_modifier(Modifier::ITALIC),
            "operator" => self.operator = Style::default().fg(color),
            "punctuation" => self.punctuation = Style::default().fg(color),
            "constant" => self.constant = Style::default().fg(color),
            "attribute" => self.attribute = Style::default().fg(color),
            "variable" => self.variable = Style::default().fg(color),
            _ => return false,
        }
        true
    }

    /// Re-applies the syntax modifiers to the current syntax colors.
    fn rebuild_styles(&mut self) {
        self.plain = Style::default().fg(self.plain.fg.unwrap_or(self.foreground));
        self.keyword = Style::default()
            .fg(self.keyword.fg.unwrap_or(self.foreground))
            .add_modifier(Modifier::BOLD);
        self.comment = Style::default()
            .fg(self.comment.fg.unwrap_or(self.gutter))
            .add_modifier(Modifier::ITALIC);
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

    /// Builds a theme from a flat palette of 0xRRGGBB colors.
    fn from_palette(palette: Palette) -> Self {
        let rgb = |value: u32| {
            Color::Rgb(
                ((value >> 16) & 0xFF) as u8,
                ((value >> 8) & 0xFF) as u8,
                (value & 0xFF) as u8,
            )
        };
        Self {
            background: rgb(palette.background),
            foreground: rgb(palette.foreground),
            gutter: rgb(palette.gutter),
            gutter_active: rgb(palette.gutter_active),
            cursor_line: rgb(palette.cursor_line),
            selection: rgb(palette.selection),
            status_bg: rgb(palette.status_bg),
            status_fg: rgb(palette.status_fg),
            status_accent: rgb(palette.status_accent),
            prompt_bg: rgb(palette.prompt_bg),
            tab_active: rgb(palette.tab_active),
            tab_inactive: rgb(palette.tab_inactive),
            error: rgb(palette.error),
            warning: rgb(palette.warning),
            plain: Style::default().fg(rgb(palette.plain)),
            keyword: Style::default()
                .fg(rgb(palette.keyword))
                .add_modifier(Modifier::BOLD),
            type_: Style::default().fg(rgb(palette.type_)),
            function: Style::default().fg(rgb(palette.function)),
            string: Style::default().fg(rgb(palette.string)),
            number: Style::default().fg(rgb(palette.number)),
            comment: Style::default()
                .fg(rgb(palette.comment))
                .add_modifier(Modifier::ITALIC),
            operator: Style::default().fg(rgb(palette.operator)),
            punctuation: Style::default().fg(rgb(palette.punctuation)),
            constant: Style::default().fg(rgb(palette.constant)),
            attribute: Style::default().fg(rgb(palette.attribute)),
            variable: Style::default().fg(rgb(palette.variable)),
        }
    }
}

/// A flat list of the colors a built-in theme is made of.
struct Palette {
    background: u32,
    foreground: u32,
    gutter: u32,
    gutter_active: u32,
    cursor_line: u32,
    selection: u32,
    status_bg: u32,
    status_fg: u32,
    status_accent: u32,
    prompt_bg: u32,
    tab_active: u32,
    tab_inactive: u32,
    error: u32,
    warning: u32,
    plain: u32,
    keyword: u32,
    type_: u32,
    function: u32,
    string: u32,
    number: u32,
    comment: u32,
    operator: u32,
    punctuation: u32,
    constant: u32,
    attribute: u32,
    variable: u32,
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
    fn every_builtin_name_resolves() {
        for name in Theme::names() {
            let theme = Theme::by_name(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(Theme::canonical_name(name), Some(*name));
            assert!(matches!(theme.background, Color::Rgb(..)));
        }
    }

    #[test]
    fn names_ignore_case_and_separators() {
        assert!(Theme::by_name("Monokai").is_some());
        assert!(Theme::by_name("SOLARIZED_DARK").is_some());
        assert!(Theme::by_name("tokyo night").is_some());
        assert_eq!(Theme::canonical_name("One_Dark"), Some("one-dark"));
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
    fn unknown_name_has_no_theme() {
        assert!(Theme::by_name("neon").is_none());
    }
}
