//! Syntax-highlighted text layout for the egui editor.
//!
//! This bridges `fast-text-syntax` (which produces per-line `HighlightSpan`s
//! with absolute character offsets) to an egui `LayoutJob` with colored
//! sections. It is deliberately free of any windowing types so it can be unit
//! tested without a display.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};
use fast_text_syntax::{highlight_line, LanguageId, LineState, TokenKind};

/// A dark/light color scheme for syntax tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub dark: bool,
}

impl Palette {
    pub const DARK: Palette = Palette { dark: true };
    pub const LIGHT: Palette = Palette { dark: false };

    /// The color for a token kind.
    pub fn color(self, kind: TokenKind) -> Color32 {
        if self.dark {
            match kind {
                TokenKind::Plain => rgb(0xD7, 0xDA, 0xE0),
                TokenKind::Keyword => rgb(0xC6, 0x78, 0xDD),
                TokenKind::Type => rgb(0xE5, 0xC0, 0x7B),
                TokenKind::Function => rgb(0x61, 0xAF, 0xEF),
                TokenKind::String => rgb(0xCE, 0x91, 0x78),
                TokenKind::Number => rgb(0xD1, 0x9A, 0x66),
                TokenKind::Comment => rgb(0x6A, 0x73, 0x7D),
                TokenKind::Operator => rgb(0x56, 0xB6, 0xC2),
                TokenKind::Punctuation => rgb(0x9A, 0xA5, 0xB1),
                TokenKind::Constant => rgb(0xD1, 0x9A, 0x66),
                TokenKind::Attribute => rgb(0xE5, 0xC0, 0x7B),
                TokenKind::Variable => rgb(0xE0, 0x6C, 0x75),
            }
        } else {
            match kind {
                TokenKind::Plain => rgb(0x24, 0x29, 0x2E),
                TokenKind::Keyword => rgb(0xA6, 0x26, 0xA6),
                TokenKind::Type => rgb(0x9A, 0x67, 0x00),
                TokenKind::Function => rgb(0x1F, 0x6F, 0xB2),
                TokenKind::String => rgb(0xA3, 0x15, 0x15),
                TokenKind::Number => rgb(0x8A, 0x5A, 0x00),
                TokenKind::Comment => rgb(0x7A, 0x82, 0x8C),
                TokenKind::Operator => rgb(0x00, 0x7A, 0x8A),
                TokenKind::Punctuation => rgb(0x5A, 0x63, 0x6E),
                TokenKind::Constant => rgb(0x8A, 0x5A, 0x00),
                TokenKind::Attribute => rgb(0x9A, 0x67, 0x00),
                TokenKind::Variable => rgb(0xC0, 0x39, 0x3F),
            }
        }
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Builds a `LayoutJob` that paints `text` with syntax colors.
///
/// Runs of identical token kind are merged into single sections to keep the
/// galley compact.
pub fn highlight_job(
    text: &str,
    language: LanguageId,
    font: FontId,
    palette: Palette,
) -> LayoutJob {
    let lang = language.language();
    let mut state = LineState::default();
    let mut job = LayoutJob::default();

    let lines: Vec<&str> = text.split('\n').collect();
    for (idx, line) in lines.iter().enumerate() {
        let spans = highlight_line(&lang, line, &mut state);
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let kind = kind_at(&spans, i);
            let start = i;
            while i < chars.len() && kind_at(&spans, i) == kind {
                i += 1;
            }
            let run: String = chars[start..i].iter().collect();
            job.append(&run, 0.0, format_for(kind, font.clone(), palette));
        }
        if idx + 1 < lines.len() {
            job.append(
                "\n",
                0.0,
                format_for(TokenKind::Plain, font.clone(), palette),
            );
        }
    }

    job
}

fn kind_at(spans: &[fast_text_syntax::HighlightSpan], index: usize) -> TokenKind {
    spans
        .iter()
        .find(|s| index >= s.start && index < s.end)
        .map(|s| s.kind)
        .unwrap_or(TokenKind::Plain)
}

fn format_for(kind: TokenKind, font_id: FontId, palette: Palette) -> TextFormat {
    TextFormat {
        font_id,
        color: palette.color(kind),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job_text(job: &LayoutJob) -> String {
        job.text.clone()
    }

    /// The color applied to the first occurrence of `needle` in the job.
    fn color_of(job: &LayoutJob, needle: &str) -> Option<Color32> {
        let byte = job.text.find(needle)?;
        job.sections
            .iter()
            .find(|s| s.byte_range.contains(&byte))
            .map(|s| s.format.color)
    }

    #[test]
    fn colors_tokens_on_lines_after_the_first() {
        // Regression: spans are line-relative, so every line must be colored
        // using its own offsets, not a running document offset.
        let src = "fn first() {}\n\n// a comment on line three\nfn third() { let s = \"x\"; }\n";
        let job = highlight_job(
            src,
            LanguageId::Rust,
            FontId::monospace(14.0),
            Palette::DARK,
        );
        assert_eq!(
            color_of(&job, "// a comment"),
            Some(Palette::DARK.color(TokenKind::Comment)),
            "comment on line 3 should be comment-colored"
        );
        assert_eq!(
            color_of(&job, "\"x\""),
            Some(Palette::DARK.color(TokenKind::String)),
            "string on line 4 should be string-colored"
        );
        assert_eq!(
            color_of(&job, "third"),
            Some(Palette::DARK.color(TokenKind::Function)),
            "function on line 4 should be function-colored"
        );
    }

    #[test]
    fn preserves_text_exactly() {
        let src = "fn main() {\n    let s = \"hi\"; // c\n}\n";
        let job = highlight_job(
            src,
            LanguageId::Rust,
            FontId::monospace(14.0),
            Palette::DARK,
        );
        assert_eq!(job_text(&job), src);
    }

    #[test]
    fn colors_keywords_and_comments_differently() {
        let src = "fn main() {} // comment";
        let job = highlight_job(
            src,
            LanguageId::Rust,
            FontId::monospace(14.0),
            Palette::DARK,
        );
        // There must be at least one keyword-colored and one comment-colored section.
        let has_keyword = job
            .sections
            .iter()
            .any(|s| s.format.color == Palette::DARK.color(TokenKind::Keyword));
        let has_comment = job
            .sections
            .iter()
            .any(|s| s.format.color == Palette::DARK.color(TokenKind::Comment));
        assert!(has_keyword, "expected a keyword section");
        assert!(has_comment, "expected a comment section");
    }

    #[test]
    fn light_and_dark_palettes_differ() {
        assert_ne!(
            Palette::DARK.color(TokenKind::Keyword),
            Palette::LIGHT.color(TokenKind::Keyword)
        );
    }

    #[test]
    fn handles_unicode() {
        let src = "// héllo 😀\nlet x = 1;";
        let job = highlight_job(
            src,
            LanguageId::Rust,
            FontId::monospace(14.0),
            Palette::DARK,
        );
        assert_eq!(job_text(&job), src);
    }

    #[test]
    fn empty_text_is_empty_job() {
        let job = highlight_job(
            "",
            LanguageId::Plain,
            FontId::monospace(14.0),
            Palette::DARK,
        );
        assert!(job.sections.is_empty());
    }
}
