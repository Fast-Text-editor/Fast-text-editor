//! Syntax-highlighted text layout for the egui editor.
//!
//! This bridges `fast-text-syntax` (which produces per-line `HighlightSpan`s
//! with absolute character offsets) to an egui `LayoutJob` with colored
//! sections. It is deliberately free of any windowing types so it can be unit
//! tested without a display.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};
use fast_text_syntax::{highlight_line, LanguageId, LineState, TokenKind};
use fast_text_theme::{Rgb, SyntaxColors, Theme};

/// Syntax colors for the editor, projected from the shared theme schema.
///
/// The terminal, desktop and web builds all render the same [`Theme`], so a
/// file looks the same whichever frontend opened it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub dark: bool,
    pub syntax: SyntaxColors,
}

impl Palette {
    /// The shared dark palette, usable in `const` contexts and tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub const DARK: Palette = Palette {
        dark: true,
        syntax: SyntaxColors::DARK,
    };

    /// The shared light palette.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn light() -> Palette {
        Palette::from_theme(&Theme::light())
    }

    /// Projects a theme's syntax colors into egui colors.
    pub fn from_theme(theme: &Theme) -> Palette {
        Palette {
            dark: theme.dark,
            syntax: theme.syntax,
        }
    }

    /// The color for a token kind.
    pub fn color(self, kind: TokenKind) -> Color32 {
        let color = match kind {
            TokenKind::Plain => self.syntax.plain,
            TokenKind::Keyword => self.syntax.keyword,
            TokenKind::Type => self.syntax.type_,
            TokenKind::Function => self.syntax.function,
            TokenKind::String => self.syntax.string,
            TokenKind::Number => self.syntax.number,
            TokenKind::Comment => self.syntax.comment,
            TokenKind::Operator => self.syntax.operator,
            TokenKind::Punctuation => self.syntax.punctuation,
            TokenKind::Constant => self.syntax.constant,
            TokenKind::Attribute => self.syntax.attribute,
            TokenKind::Variable => self.syntax.variable,
        };
        color32(color)
    }
}

fn color32(color: Rgb) -> Color32 {
    Color32::from_rgb(color.r, color.g, color.b)
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
            Palette::light().color(TokenKind::Keyword)
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
