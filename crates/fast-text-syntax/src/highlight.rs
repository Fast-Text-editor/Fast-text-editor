use crate::language::{classify_word, Language};
use crate::token::TokenKind;

/// A highlighted region within a single line, as `[start, end)` char offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

/// Highlighting state carried from one line to the next.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineState {
    /// `Some(close)` while inside a block comment delimited by `close`.
    block_comment_close: Option<String>,
    /// `Some(delim)` while inside a multi-line string using `delim`.
    string_delim: Option<String>,
}

impl LineState {
    pub fn is_continuation(&self) -> bool {
        self.block_comment_close.is_some() || self.string_delim.is_some()
    }

    fn reset_block(&mut self) {
        self.block_comment_close = None;
    }
}

/// Highlights one line, updating `state` for the next line.
pub fn highlight_line(lang: &Language, line: &str, state: &mut LineState) -> Vec<HighlightSpan> {
    let chars: Vec<char> = line.chars().collect();
    let mut spans = Vec::new();
    let n = chars.len();
    let mut i = 0;

    // Continue a block comment from a previous line.
    if let Some(close) = state.block_comment_close.clone() {
        if let Some(pos) = find_str(&chars, 0, &close) {
            let end = pos + close.chars().count();
            push(&mut spans, 0, end, TokenKind::Comment);
            state.reset_block();
            i = end;
        } else {
            push(&mut spans, 0, n, TokenKind::Comment);
            return spans;
        }
    }

    // Continue a multi-line string from a previous line.
    if let Some(delim) = state.string_delim.clone() {
        if let Some(pos) = find_unescaped(&chars, 0, &delim) {
            let end = pos + delim.chars().count();
            push(&mut spans, 0, end, TokenKind::String);
            state.string_delim = None;
            i = end;
        } else {
            push(&mut spans, 0, n, TokenKind::String);
            return spans;
        }
    }

    while i < n {
        let c = chars[i];

        // Whitespace.
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // Line comment.
        if let Some(marker) = lang.line_comment.iter().find(|m| starts_with(&chars, i, m)) {
            let _ = marker;
            push(&mut spans, i, n, TokenKind::Comment);
            break;
        }

        // Block comment open.
        if let Some((open, close)) = lang.block_comment {
            if starts_with(&chars, i, open) {
                let start = i;
                let open_len = open.chars().count();
                if let Some(pos) = find_str(&chars, i + open_len, close) {
                    let end = pos + close.chars().count();
                    push(&mut spans, start, end, TokenKind::Comment);
                    i = end;
                } else {
                    push(&mut spans, start, n, TokenKind::Comment);
                    state.block_comment_close = Some(close.to_string());
                    return spans;
                }
                continue;
            }
        }

        // Attribute / decorator prefix.
        if let Some(prefix) = lang.attribute_prefix {
            if starts_with(&chars, i, prefix) {
                // Only treat as attribute when starting a token.
                let start = i;
                let mut j = i + prefix.chars().count();
                if j < n
                    && (chars[j] == '['
                        || chars[j].is_alphabetic()
                        || chars[j] == '_'
                        || chars[j] == '{')
                {
                    while j < n && !chars[j].is_whitespace() && chars[j] != '=' && chars[j] != '(' {
                        // Stop at the end of a bracketed attribute.
                        if chars[j] == ']' || chars[j] == ')' {
                            j += 1;
                            break;
                        }
                        j += 1;
                    }
                    push(&mut spans, start, j, TokenKind::Attribute);
                    i = j;
                    continue;
                }
            }
        }

        // Strings (triple first, then single-line).
        if let Some(delim) = lang
            .triple_strings
            .iter()
            .find(|d| starts_with(&chars, i, d))
        {
            let start = i;
            let dlen = delim.chars().count();
            if let Some(pos) = find_unescaped(&chars, i + dlen, delim) {
                let end = pos + dlen;
                push(&mut spans, start, end, TokenKind::String);
                i = end;
            } else {
                push(&mut spans, start, n, TokenKind::String);
                state.string_delim = Some(delim.to_string());
                return spans;
            }
            continue;
        }

        let single = match lang
            .string_delims
            .iter()
            .find(|d| starts_with(&chars, i, d))
        {
            Some(d)
                if !lang
                    .triple_strings
                    .iter()
                    .any(|t| starts_with(&chars, i, t)) =>
            {
                Some(*d)
            }
            _ => None,
        };
        if let Some(delim) = single {
            let start = i;
            let dlen = delim.chars().count();
            let mut j = i + dlen;
            let mut closed = false;
            while j < n {
                if lang.string_escapes && chars[j] == '\\' {
                    j += 2;
                    continue;
                }
                if starts_with(&chars, j, delim) {
                    j += dlen;
                    closed = true;
                    break;
                }
                j += 1;
            }
            if closed {
                push(&mut spans, start, j, TokenKind::String);
            } else {
                push(&mut spans, start, n, TokenKind::String);
            }
            i = j.min(n);
            continue;
        }

        // Numbers.
        if c.is_ascii_digit() {
            let start = i;
            let mut j = i;
            while j < n && (chars[j].is_ascii_alphanumeric() || chars[j] == '.' || chars[j] == '_')
            {
                j += 1;
            }
            push(&mut spans, start, j, TokenKind::Number);
            i = j;
            continue;
        }

        // Identifiers / keywords.
        if is_ident_start(c) {
            let start = i;
            let mut j = i;
            while j < n && is_ident_continue(chars[j]) {
                j += 1;
            }
            let word: String = chars[start..j].iter().collect();
            let mut kind = classify_word(lang, &word);
            if kind == TokenKind::Plain {
                // A following `(` suggests a function call/definition.
                if next_non_space(&chars, j) == Some('(') {
                    kind = TokenKind::Function;
                }
            }
            if kind != TokenKind::Plain {
                push(&mut spans, start, j, kind);
            }
            i = j;
            continue;
        }

        // Operators and punctuation.
        let start = i;
        let mut j = i + 1;
        while j < n && is_operator(chars[j]) {
            j += 1;
        }
        push(&mut spans, start, j, TokenKind::Operator);
        i = j;
    }

    spans
}

fn push(spans: &mut Vec<HighlightSpan>, start: usize, end: usize, kind: TokenKind) {
    if end > start {
        spans.push(HighlightSpan { start, end, kind });
    }
}

fn starts_with(chars: &[char], at: usize, pat: &str) -> bool {
    for (j, pc) in (at..).zip(pat.chars()) {
        if j >= chars.len() || chars[j] != pc {
            return false;
        }
    }
    true
}

fn find_str(chars: &[char], from: usize, pat: &str) -> Option<usize> {
    let pat: Vec<char> = pat.chars().collect();
    if pat.is_empty() {
        return Some(from);
    }
    let mut i = from;
    while i + pat.len() <= chars.len() {
        if chars[i..i + pat.len()] == pat[..] {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_unescaped(chars: &[char], from: usize, pat: &str) -> Option<usize> {
    let mut i = from;
    while let Some(pos) = find_str(chars, i, pat) {
        // Count preceding backslashes to decide if escaped.
        let mut backslashes = 0;
        let mut k = pos;
        while k > 0 && chars[k - 1] == '\\' {
            backslashes += 1;
            k -= 1;
        }
        if backslashes % 2 == 0 {
            return Some(pos);
        }
        i = pos + 1;
    }
    None
}

fn next_non_space(chars: &[char], from: usize) -> Option<char> {
    chars[from..].iter().copied().find(|c| !c.is_whitespace())
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

fn is_operator(c: char) -> bool {
    matches!(
        c,
        '+' | '-'
            | '*'
            | '/'
            | '%'
            | '='
            | '<'
            | '>'
            | '!'
            | '&'
            | '|'
            | '^'
            | '~'
            | '?'
            | ':'
            | '.'
            | ','
            | ';'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::LanguageId;

    fn kinds(lang: LanguageId, line: &str, state: &mut LineState) -> Vec<(TokenKind, String)> {
        let l = lang.language();
        highlight_line(&l, line, state)
            .into_iter()
            .map(|s| {
                let text: String = line.chars().skip(s.start).take(s.end - s.start).collect();
                (s.kind, text)
            })
            .collect()
    }

    #[test]
    fn highlights_rust_keywords_and_strings() {
        let mut st = LineState::default();
        let out = kinds(LanguageId::Rust, "let x = \"hi\";", &mut st);
        assert!(out.contains(&(TokenKind::Keyword, "let".to_string())));
        assert!(out.contains(&(TokenKind::String, "\"hi\"".to_string())));
    }

    #[test]
    fn multiline_block_comment_carries_state() {
        let mut st = LineState::default();
        let out = kinds(LanguageId::Rust, "/* start", &mut st);
        assert_eq!(out, vec![(TokenKind::Comment, "/* start".to_string())]);
        assert!(st.is_continuation());
        let out = kinds(LanguageId::Rust, "still */ let", &mut st);
        assert_eq!(out[0].0, TokenKind::Comment);
        assert!(!st.is_continuation());
    }

    #[test]
    fn python_triple_string_carries_state() {
        let mut st = LineState::default();
        kinds(LanguageId::Python, "x = \"\"\"doc", &mut st);
        assert!(st.is_continuation());
        let out = kinds(LanguageId::Python, "end\"\"\"", &mut st);
        assert_eq!(out[0].0, TokenKind::String);
        assert!(!st.is_continuation());
    }

    #[test]
    fn detects_function_calls() {
        let mut st = LineState::default();
        let out = kinds(LanguageId::Rust, "foo(bar)", &mut st);
        assert!(out.contains(&(TokenKind::Function, "foo".to_string())));
    }

    #[test]
    fn highlights_textwrite_keywords_and_comments() {
        let mut st = LineState::default();
        let out = kinds(LanguageId::TextWrite, "repeat 3 times # nope", &mut st);
        assert!(out.contains(&(TokenKind::Keyword, "repeat".to_string())));
        assert!(out.contains(&(TokenKind::Keyword, "times".to_string())));
        assert!(out.contains(&(TokenKind::Comment, "# nope".to_string())));

        // `is` / `and` are word operators, and `//` also comments.
        let out = kinds(
            LanguageId::TextWrite,
            "if x is 0 and y { /* ok */ }",
            &mut st,
        );
        assert!(out.contains(&(TokenKind::Keyword, "is".to_string())));
        assert!(out.contains(&(TokenKind::Keyword, "and".to_string())));
        assert!(out.contains(&(TokenKind::Comment, "/* ok */".to_string())));

        // `import ... as` is highlighted as keywords too.
        let out = kinds(
            LanguageId::TextWrite,
            "import \"fast-text/automator\" as auto",
            &mut st,
        );
        assert!(out.contains(&(TokenKind::Keyword, "import".to_string())));
        assert!(out.contains(&(TokenKind::Keyword, "as".to_string())));
        assert!(out
            .iter()
            .any(|(k, t)| *k == TokenKind::String && t.contains("fast-text/automator")));
    }
}
