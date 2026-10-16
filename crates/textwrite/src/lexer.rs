//! The TextWrite lexer: source text in, tokens out.

use crate::error::{Error, Pos, Result, Span};

/// A lexical token kind.
#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Int(i64),
    Float(f64),
    /// A double-quoted string, stored raw (escapes and `{...}` untouched).
    DStr(String),
    /// A single-quoted string (no interpolation), stored raw.
    SStr(String),
    Ident(String),

    // Keywords.
    Let,
    Set,
    Fn,
    Return,
    If,
    Else,
    While,
    For,
    In,
    Repeat,
    Times,
    Break,
    Continue,
    Import,
    And,
    Or,
    Not,
    Is,
    True,
    False,
    None,

    // Symbols.
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar,
    Assign,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semicolon,
    Dot,
    Newline,
    Eof,
}

impl Tok {
    /// A human-readable description used in parser error messages.
    pub fn describe(&self) -> String {
        match self {
            Tok::Int(n) => format!("number `{n}`"),
            Tok::Float(n) => format!("number `{n}`"),
            Tok::DStr(_) => "string".to_string(),
            Tok::SStr(_) => "string".to_string(),
            Tok::Ident(n) => format!("identifier `{n}`"),
            Tok::Eof => "end of input".to_string(),
            Tok::Newline => "end of line".to_string(),
            other => format!("`{}`", other.symbol()),
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            Tok::Let => "let",
            Tok::Fn => "fn",
            Tok::Return => "return",
            Tok::If => "if",
            Tok::Else => "else",
            Tok::While => "while",
            Tok::For => "for",
            Tok::In => "in",
            Tok::Repeat => "repeat",
            Tok::Times => "times",
            Tok::Break => "break",
            Tok::Continue => "continue",
            Tok::Import => "import",
            Tok::And => "and",
            Tok::Or => "or",
            Tok::Not => "not",
            Tok::Is => "is",
            Tok::True => "true",
            Tok::False => "false",
            Tok::None => "none",
            Tok::Set => "set",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::StarStar => "**",
            Tok::Assign => "=",
            Tok::Eq => "==",
            Tok::Ne => "!=",
            Tok::Lt => "<",
            Tok::Le => "<=",
            Tok::Gt => ">",
            Tok::Ge => ">=",
            Tok::PlusAssign => "+=",
            Tok::MinusAssign => "-=",
            Tok::StarAssign => "*=",
            Tok::SlashAssign => "/=",
            Tok::PercentAssign => "%=",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            Tok::Comma => ",",
            Tok::Colon => ":",
            Tok::Semicolon => ";",
            Tok::Dot => ".",
            Tok::Newline => "newline",
            _ => "?",
        }
    }
}

/// A token together with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Tokenizes `src`, always ending with a [`Tok::Eof`] token.
pub fn tokenize(src: &str) -> Result<Vec<Token>> {
    Lexer::new(src).run()
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    line: u32,
    col: u32,
}

impl Lexer {
    fn new(src: &str) -> Self {
        Self {
            chars: src.chars().collect(),
            i: 0,
            line: 1,
            col: 1,
        }
    }

    fn pos(&self) -> Pos {
        Pos::new(self.line, self.col, self.i)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.i + ahead).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.i).copied()?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn run(mut self) -> Result<Vec<Token>> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia()?;
            let start = self.pos();
            let Some(c) = self.peek() else {
                out.push(Token {
                    tok: Tok::Eof,
                    span: Span::point(start),
                });
                return Ok(out);
            };

            let tok = match c {
                '\n' => {
                    self.bump();
                    Tok::Newline
                }
                '0'..='9' => self.number(start)?,
                '"' => self.string('"', start)?,
                '\'' => self.string('\'', start)?,
                c if is_ident_start(c) => self.ident(),
                _ => self.symbol(start)?,
            };
            out.push(Token {
                tok,
                span: Span::new(start, self.pos()),
            });
        }
    }

    fn skip_trivia(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\r') => {
                    self.bump();
                }
                Some('#') => self.skip_line_comment(),
                Some('/') if self.peek_at(1) == Some('/') => self.skip_line_comment(),
                Some('/') if self.peek_at(1) == Some('*') => {
                    let start = self.pos();
                    self.bump();
                    self.bump();
                    loop {
                        match self.peek() {
                            None => {
                                return Err(Error::lex(
                                    "unterminated block comment",
                                    Span::point(start),
                                ))
                            }
                            Some('*') if self.peek_at(1) == Some('/') => {
                                self.bump();
                                self.bump();
                                break;
                            }
                            _ => {
                                self.bump();
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
    }

    fn number(&mut self, start: Pos) -> Result<Tok> {
        // Hexadecimal.
        if self.peek() == Some('0') && matches!(self.peek_at(1), Some('x' | 'X')) {
            self.bump();
            self.bump();
            let mut digits = String::new();
            while let Some(c) = self.peek() {
                if c == '_' {
                    self.bump();
                } else if c.is_ascii_hexdigit() {
                    digits.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
            if digits.is_empty() {
                return Err(Error::lex(
                    "malformed hexadecimal literal",
                    Span::point(start),
                ));
            }
            return match i64::from_str_radix(&digits, 16) {
                Ok(n) => Ok(Tok::Int(n)),
                Err(_) => Err(Error::lex(
                    "hexadecimal literal out of range",
                    Span::new(start, self.pos()),
                )),
            };
        }

        let mut text = String::new();
        let mut is_float = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                text.push(c);
                self.bump();
            } else if c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        // Fractional part.
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            text.push('.');
            self.bump();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    text.push(c);
                    self.bump();
                } else if c == '_' {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        // Exponent.
        if matches!(self.peek(), Some('e' | 'E')) {
            let sign = self.peek_at(1);
            let digit_at = if matches!(sign, Some('+' | '-')) {
                2
            } else {
                1
            };
            if self.peek_at(digit_at).is_some_and(|c| c.is_ascii_digit()) {
                is_float = true;
                text.push('e');
                self.bump();
                if let Some(s) = sign {
                    if s == '+' || s == '-' {
                        text.push(s);
                        self.bump();
                    }
                }
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        text.push(c);
                        self.bump();
                    } else if c == '_' {
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
        }

        if is_float {
            text.parse::<f64>()
                .map(Tok::Float)
                .map_err(|_| Error::lex("malformed number literal", Span::new(start, self.pos())))
        } else {
            text.parse::<i64>().map(Tok::Int).map_err(|_| {
                Error::lex("integer literal out of range", Span::new(start, self.pos()))
            })
        }
    }

    /// Reads a quoted string, returning its *raw* contents (escapes intact).
    fn string(&mut self, quote: char, start: Pos) -> Result<Tok> {
        self.bump(); // opening quote
        let mut raw = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(Error::lex("unterminated string", Span::point(start)));
                }
                Some('\n') => {
                    return Err(Error::lex(
                        "unterminated string (strings may not span lines)",
                        Span::point(start),
                    ));
                }
                Some(c) if c == quote => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    raw.push('\\');
                    self.bump();
                    if let Some(n) = self.peek() {
                        raw.push(n);
                        self.bump();
                    }
                }
                Some(c) => {
                    raw.push(c);
                    self.bump();
                }
            }
        }
        Ok(if quote == '"' {
            Tok::DStr(raw)
        } else {
            Tok::SStr(raw)
        })
    }

    fn ident(&mut self) -> Tok {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        match text.as_str() {
            "let" => Tok::Let,
            "set" => Tok::Set,
            "fn" => Tok::Fn,
            "return" => Tok::Return,
            "if" => Tok::If,
            "else" => Tok::Else,
            "while" => Tok::While,
            "for" => Tok::For,
            "in" => Tok::In,
            "repeat" => Tok::Repeat,
            "times" => Tok::Times,
            "break" => Tok::Break,
            "continue" => Tok::Continue,
            "import" => Tok::Import,
            "and" => Tok::And,
            "or" => Tok::Or,
            "not" => Tok::Not,
            "is" => Tok::Is,
            "true" => Tok::True,
            "false" => Tok::False,
            "none" => Tok::None,
            _ => Tok::Ident(text),
        }
    }

    fn symbol(&mut self, start: Pos) -> Result<Tok> {
        let c = self.bump().expect("caller checked a char exists");
        let tok = match c {
            '+' => self.two_char('=', Tok::PlusAssign, Tok::Plus),
            '-' => self.two_char('=', Tok::MinusAssign, Tok::Minus),
            '*' => {
                if self.peek() == Some('*') {
                    self.bump();
                    Tok::StarStar
                } else {
                    self.two_char('=', Tok::StarAssign, Tok::Star)
                }
            }
            '/' => self.two_char('=', Tok::SlashAssign, Tok::Slash),
            '%' => self.two_char('=', Tok::PercentAssign, Tok::Percent),
            '=' => self.two_char('=', Tok::Eq, Tok::Assign),
            '!' => {
                if self.peek() == Some('=') {
                    self.bump();
                    Tok::Ne
                } else {
                    return Err(Error::lex("unexpected `!`", Span::new(start, self.pos())));
                }
            }
            '<' => self.two_char('=', Tok::Le, Tok::Lt),
            '>' => self.two_char('=', Tok::Ge, Tok::Gt),
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            ',' => Tok::Comma,
            ':' => Tok::Colon,
            ';' => Tok::Semicolon,
            '.' => Tok::Dot,
            other => {
                return Err(Error::lex(
                    format!("unexpected character `{other}`"),
                    Span::new(start, self.pos()),
                ));
            }
        };
        Ok(tok)
    }

    /// Consumes `next` if present, returning `yes`, otherwise `no`.
    fn two_char(&mut self, next: char, yes: Tok, no: Tok) -> Tok {
        if self.peek() == Some(next) {
            self.bump();
            yes
        } else {
            no
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}
