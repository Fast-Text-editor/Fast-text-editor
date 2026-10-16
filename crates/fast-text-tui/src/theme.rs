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

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// A modern dark theme.
    pub fn dark() -> Self {
        let string = Color::Rgb(0xCE, 0x91, 0x78);
        let keyword = Color::Rgb(0xC6, 0x78, 0xDD);
        let type_ = Color::Rgb(0xE5, 0xC0, 0x7B);
        let function = Color::Rgb(0x61, 0xAF, 0xEF);
        let number = Color::Rgb(0xD1, 0x9A, 0x66);
        let comment = Color::Rgb(0x6A, 0x73, 0x7D);
        let operator = Color::Rgb(0x56, 0xB6, 0xC2);
        let constant = Color::Rgb(0xD1, 0x9A, 0x66);
        let attribute = Color::Rgb(0xE5, 0xC0, 0x7B);
        let variable = Color::Rgb(0xE0, 0x6C, 0x75);

        Self {
            background: Color::Rgb(0x1E, 0x21, 0x26),
            foreground: Color::Rgb(0xD7, 0xDA, 0xE0),
            gutter: Color::Rgb(0x4B, 0x52, 0x5E),
            gutter_active: Color::Rgb(0xC8, 0xCC, 0xD4),
            cursor_line: Color::Rgb(0x2A, 0x2E, 0x38),
            selection: Color::Rgb(0x3A, 0x44, 0x55),
            status_bg: Color::Rgb(0x28, 0x2C, 0x34),
            status_fg: Color::Rgb(0xD7, 0xDA, 0xE0),
            status_accent: Color::Rgb(0x61, 0xAF, 0xEF),
            prompt_bg: Color::Rgb(0x2C, 0x31, 0x3A),
            tab_active: Color::Rgb(0x61, 0xAF, 0xEF),
            tab_inactive: Color::Rgb(0x4B, 0x52, 0x5E),
            error: Color::Rgb(0xE0, 0x6C, 0x75),
            warning: Color::Rgb(0xE5, 0xC0, 0x7B),

            plain: Style::default().fg(Color::Rgb(0xD7, 0xDA, 0xE0)),
            keyword: Style::default().fg(keyword).add_modifier(Modifier::BOLD),
            type_: Style::default().fg(type_),
            function: Style::default().fg(function),
            string: Style::default().fg(string),
            number: Style::default().fg(number),
            comment: Style::default().fg(comment).add_modifier(Modifier::ITALIC),
            operator: Style::default().fg(operator),
            punctuation: Style::default().fg(Color::Rgb(0x9A, 0xA5, 0xB1)),
            constant: Style::default().fg(constant),
            attribute: Style::default().fg(attribute),
            variable: Style::default().fg(variable),
        }
    }

    /// A light theme.
    pub fn light() -> Self {
        let mut theme = Self::dark();
        theme.background = Color::Rgb(0xFA, 0xFA, 0xFA);
        theme.foreground = Color::Rgb(0x24, 0x29, 0x2E);
        theme.gutter = Color::Rgb(0x9D, 0xA3, 0xAB);
        theme.gutter_active = Color::Rgb(0x24, 0x29, 0x2E);
        theme.cursor_line = Color::Rgb(0xEE, 0xF1, 0xF5);
        theme.selection = Color::Rgb(0xC8, 0xDD, 0xF5);
        theme.status_bg = Color::Rgb(0xE8, 0xEA, 0xED);
        theme.status_fg = Color::Rgb(0x24, 0x29, 0x2E);
        theme.prompt_bg = Color::Rgb(0xE8, 0xEA, 0xED);
        theme.plain = Style::default().fg(theme.foreground);
        theme
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
