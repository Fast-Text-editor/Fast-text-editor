//! Syntax highlighting for Fast Text.
//!
//! A small, dependency-free, stateful highlighter. It carries per-line state
//! (open block comments and multi-line strings) so it can colorize a file
//! line-by-line as it scrolls, and it compiles anywhere Rust does — including
//! WebAssembly for the browser frontend.

mod generated;
mod highlight;
mod language;
mod token;

pub use generated::language_server;
pub use highlight::{highlight_line, HighlightSpan, LineState};
pub use language::{all_names, Language, LanguageId, CURATED_LANGUAGES};
pub use token::TokenKind;

/// Every language imported from Helix (the hand-curated ones live in
/// [`LanguageId`] and are always available).
pub fn imported_languages() -> &'static [&'static Language] {
    generated::LANGUAGES
}

/// Highlight every line of `text`, returning spans per line.
///
/// Spans are `(start, end)` character offsets within each line.
pub fn highlight_text(language: LanguageId, text: &str) -> Vec<Vec<HighlightSpan>> {
    let lang = language.language();
    let mut state = LineState::default();
    text.split('\n')
        .map(|line| highlight_line(&lang, line, &mut state))
        .collect()
}
