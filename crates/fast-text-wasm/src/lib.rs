//! WebAssembly bindings for the Fast Text engine.
//!
//! This exposes the same `fast-text-core` [`Document`] that powers the TUI to
//! JavaScript, plus the `fast-text-syntax` highlighter rendered to HTML. The web
//! frontend in `web/` is a thin shell over this API.
//!
//! Build with `wasm-pack build crates/fast-text-wasm --target web`.

use fast_text_core::{Document, SearchDirection};
use fast_text_syntax::{highlight_line, HighlightSpan, LanguageId, LineState, TokenKind};
use wasm_bindgen::prelude::*;

/// A single Fast Text buffer, usable from JavaScript.
#[wasm_bindgen]
pub struct WasmEditor {
    doc: Document,
    language: LanguageId,
}

#[wasm_bindgen]
impl WasmEditor {
    /// Creates an editor, optionally detecting the language from a file path.
    #[wasm_bindgen(constructor)]
    pub fn new(text: &str, path: Option<String>) -> WasmEditor {
        let language = match &path {
            Some(p) => LanguageId::from_path(p),
            None => LanguageId::Plain,
        };
        WasmEditor {
            doc: Document::from_str(text),
            language,
        }
    }

    // --- Text ---------------------------------------------------------------

    /// The full document text.
    #[wasm_bindgen(js_name = text)]
    pub fn text(&self) -> String {
        self.doc.text()
    }

    /// Replaces the whole document.
    #[wasm_bindgen(js_name = setText)]
    pub fn set_text(&mut self, text: &str) {
        self.doc = Document::from_str(text);
    }

    /// Inserts text at the cursor (replacing any selection).
    pub fn insert(&mut self, text: &str) {
        self.doc.insert_str(text);
    }

    pub fn backspace(&mut self) {
        self.doc.backspace();
    }

    pub fn delete_forward(&mut self) {
        self.doc.delete_forward();
    }

    pub fn insert_newline(&mut self) {
        self.doc.insert_newline();
    }

    // --- Movement -----------------------------------------------------------

    pub fn move_left(&mut self, extend: bool) {
        self.doc.move_left(extend);
    }
    pub fn move_right(&mut self, extend: bool) {
        self.doc.move_right(extend);
    }
    pub fn move_up(&mut self, extend: bool) {
        self.doc.move_up(extend);
    }
    pub fn move_down(&mut self, extend: bool) {
        self.doc.move_down(extend);
    }
    pub fn move_word_forward(&mut self, extend: bool) {
        self.doc.move_word_forward(extend);
    }
    pub fn move_word_backward(&mut self, extend: bool) {
        self.doc.move_word_backward(extend);
    }
    pub fn move_line_start(&mut self, extend: bool) {
        self.doc.move_line_start(extend);
    }
    pub fn move_line_end(&mut self, extend: bool) {
        self.doc.move_line_end(extend);
    }
    pub fn move_doc_start(&mut self, extend: bool) {
        self.doc.move_doc_start(extend);
    }
    pub fn move_doc_end(&mut self, extend: bool) {
        self.doc.move_doc_end(extend);
    }

    pub fn select_all(&mut self) {
        self.doc.select_all();
    }

    // --- State --------------------------------------------------------------

    /// The cursor as a character index.
    pub fn cursor(&self) -> usize {
        self.doc.cursor()
    }

    /// Sets the cursor to a character index.
    #[wasm_bindgen(js_name = setCursor)]
    pub fn set_cursor(&mut self, at: usize) {
        self.doc.set_cursor(at);
    }

    /// The zero-based line of the cursor.
    pub fn line(&self) -> usize {
        self.doc.cursor_position().line
    }

    /// The zero-based column (in characters) of the cursor.
    pub fn col(&self) -> usize {
        self.doc.cursor_position().col
    }

    #[wasm_bindgen(js_name = lineCount)]
    pub fn line_count(&self) -> usize {
        self.doc.buffer().len_lines()
    }

    #[wasm_bindgen(js_name = charCount)]
    pub fn char_count(&self) -> usize {
        self.doc.buffer().max_char_idx()
    }

    #[wasm_bindgen(js_name = hasSelection)]
    pub fn has_selection(&self) -> bool {
        self.doc.has_selection()
    }

    #[wasm_bindgen(js_name = selectedText)]
    pub fn selected_text(&self) -> String {
        self.doc.selected_text()
    }

    #[wasm_bindgen(js_name = isDirty)]
    pub fn is_dirty(&self) -> bool {
        self.doc.is_dirty()
    }

    // --- History ------------------------------------------------------------

    pub fn undo(&mut self) {
        self.doc.undo();
    }

    pub fn redo(&mut self) {
        self.doc.redo();
    }

    #[wasm_bindgen(js_name = canUndo)]
    pub fn can_undo(&self) -> bool {
        self.doc.can_undo()
    }

    #[wasm_bindgen(js_name = canRedo)]
    pub fn can_redo(&self) -> bool {
        self.doc.can_redo()
    }

    // --- Search -------------------------------------------------------------

    /// Finds the next occurrence, returning true if one was selected.
    #[wasm_bindgen(js_name = findNext)]
    pub fn find_next(&mut self, query: &str, case_sensitive: bool) -> bool {
        self.doc
            .find(query, SearchDirection::Forward, case_sensitive, true)
            .is_some()
    }

    /// Finds the previous occurrence, returning true if one was selected.
    #[wasm_bindgen(js_name = findPrev)]
    pub fn find_prev(&mut self, query: &str, case_sensitive: bool) -> bool {
        self.doc
            .find(query, SearchDirection::Backward, case_sensitive, true)
            .is_some()
    }

    /// Replaces every occurrence and returns the number replaced.
    #[wasm_bindgen(js_name = replaceAll)]
    pub fn replace_all(&mut self, query: &str, replacement: &str, case_sensitive: bool) -> usize {
        self.doc.replace_all(query, replacement, case_sensitive)
    }

    // --- Language & highlighting -------------------------------------------

    #[wasm_bindgen(js_name = languageName)]
    pub fn language_name(&self) -> String {
        self.language.name().to_string()
    }

    /// Sets the language by name (e.g. `"rust"`).
    #[wasm_bindgen(js_name = setLanguage)]
    pub fn set_language(&mut self, name: &str) {
        self.language = LanguageId::from_name(name);
    }

    /// Renders the whole document as syntax-highlighted HTML.
    ///
    /// Each line becomes `<div class="line">…</div>`; tokens are
    /// `<span class="tok-KIND">`. The caller supplies the CSS.
    #[wasm_bindgen(js_name = highlightHtml)]
    pub fn highlight_html(&self) -> String {
        let lang = self.language.language();
        let mut state = LineState::default();
        let mut out = String::new();
        let lines = self.doc.buffer().len_lines();
        for i in 0..lines {
            let text = self.doc.buffer().line_str(i);
            let spans = highlight_line(&lang, &text, &mut state);
            out.push_str("<div class=\"line\">");
            push_line_html(&mut out, &text, &spans);
            out.push_str("</div>");
        }
        out
    }

    /// A JSON array of `{line, spans:[{start,end,kind}]}` for custom renderers.
    #[wasm_bindgen(js_name = highlightJson)]
    pub fn highlight_json(&self) -> String {
        let lang = self.language.language();
        let mut state = LineState::default();
        let mut lines = Vec::new();
        for i in 0..self.doc.buffer().len_lines() {
            let text = self.doc.buffer().line_str(i);
            let spans = highlight_line(&lang, &text, &mut state);
            let rendered: Vec<String> = spans
                .iter()
                .map(|s| {
                    format!(
                        "{{\"start\":{},\"end\":{},\"kind\":\"{}\"}}",
                        s.start,
                        s.end,
                        kind_class(s.kind)
                    )
                })
                .collect();
            lines.push(format!(
                "{{\"line\":{},\"spans\":[{}]}}",
                i,
                rendered.join(",")
            ));
        }
        format!("[{}]", lines.join(","))
    }
}

/// Detects a language from a file path/name.
#[wasm_bindgen(js_name = detectLanguage)]
pub fn detect_language(path: &str) -> String {
    LanguageId::from_path(path).name().to_string()
}

/// All selectable language names as a JSON array, for building pickers.
#[wasm_bindgen(js_name = languageNames)]
pub fn language_names() -> String {
    let names: Vec<String> = fast_text_syntax::all_names()
        .into_iter()
        .map(|n| {
            let escaped = n.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        })
        .collect();
    format!("[{}]", names.join(","))
}

/// The Fast Text version (from the workspace).
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn kind_class(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Plain => "plain",
        TokenKind::Keyword => "keyword",
        TokenKind::Type => "type",
        TokenKind::Function => "function",
        TokenKind::String => "string",
        TokenKind::Number => "number",
        TokenKind::Comment => "comment",
        TokenKind::Operator => "operator",
        TokenKind::Punctuation => "punctuation",
        TokenKind::Constant => "constant",
        TokenKind::Attribute => "attribute",
        TokenKind::Variable => "variable",
    }
}

/// Emits one line's HTML, grouping runs of the same token kind.
///
/// `spans` use offsets relative to the start of `text`.
fn push_line_html(out: &mut String, text: &str, spans: &[HighlightSpan]) {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let kind = kind_at(spans, i);
        let mut run = String::new();
        while i < chars.len() && kind_at(spans, i) == kind {
            run.push(chars[i]);
            i += 1;
        }
        let class = kind_class(kind);
        out.push_str("<span class=\"tok-");
        out.push_str(class);
        out.push_str("\">");
        push_escaped(out, &run);
        out.push_str("</span>");
    }
}

/// The token kind covering a line-relative character index.
fn kind_at(spans: &[HighlightSpan], index: usize) -> TokenKind {
    spans
        .iter()
        .find(|s| index >= s.start && index < s.end)
        .map(|s| s.kind)
        .unwrap_or(TokenKind::Plain)
}

fn push_escaped(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_and_history() {
        let mut ed = WasmEditor::new("", None);
        ed.insert("hello");
        assert_eq!(ed.text(), "hello");
        ed.undo();
        assert_eq!(ed.text(), "");
        ed.redo();
        assert_eq!(ed.text(), "hello");
    }

    #[test]
    fn detects_language_from_path() {
        let ed = WasmEditor::new("fn main() {}", Some("src/main.rs".to_string()));
        assert_eq!(ed.language_name(), "Rust");
        assert_eq!(detect_language("a.py"), "Python");
    }

    #[test]
    fn language_names_lists_curated_and_imported() {
        let json = language_names();
        assert!(json.starts_with('[') && json.ends_with(']'));
        assert!(json.contains("\"Rust\""));
        assert!(json.contains("\"C#\""));
        assert!(json.contains("\"Zig\""));
        assert!(json.contains("\"Haskell\""));
    }

    #[test]
    fn sets_imported_language_by_name() {
        let mut ed = WasmEditor::new("const x = 1;", Some("main.zig".to_string()));
        assert_eq!(ed.language_name(), "Zig");
        // Zig's `const` is a keyword from the imported query data.
        assert!(ed.highlight_html().contains("tok-keyword"));
        ed.set_language("kotlin");
        assert_eq!(ed.language_name(), "Kotlin");
    }

    #[test]
    fn find_and_replace() {
        let mut ed = WasmEditor::new("one two one", None);
        assert!(ed.find_next("one", true));
        assert_eq!(ed.selected_text(), "one");
        let n = ed.replace_all("one", "1", true);
        assert_eq!(n, 2);
        assert_eq!(ed.text(), "1 two 1");
    }

    #[test]
    fn highlighting_emits_classes() {
        let ed = WasmEditor::new("fn main() {}", Some("main.rs".to_string()));
        let html = ed.highlight_html();
        assert!(html.contains("tok-keyword"));
        assert!(html.contains("class=\"line\""));
    }

    #[test]
    fn html_escaping() {
        let ed = WasmEditor::new("let x = a < b && c > d;", Some("x.rs".to_string()));
        let html = ed.highlight_html();
        assert!(html.contains("&lt;"));
        assert!(html.contains("&amp;&amp;"));
        assert!(!html.contains("< b"));
    }

    #[test]
    fn cursor_positions() {
        let mut ed = WasmEditor::new("ab\ncd", None);
        ed.move_down(false);
        assert_eq!(ed.line(), 1);
        ed.move_line_end(false);
        assert_eq!(ed.col(), 2);
        assert_eq!(ed.line_count(), 2);
        assert_eq!(ed.char_count(), 5);
    }

    #[test]
    fn highlights_tokens_after_the_first_line() {
        // Regression: highlight spans are line-relative; later lines must still
        // receive string/comment tokens.
        let src = "x = 1\n# a comment\ny = \"hello\"\n";
        let ed = WasmEditor::new(src, Some("t.py".to_string()));
        let html = ed.highlight_html();
        assert!(html.contains("tok-comment"), "comment on line 2 missing");
        assert!(html.contains("tok-string"), "string on line 3 missing");
        assert_eq!(ed.language_name(), "Python");
    }

    #[test]
    fn highlight_json_offsets_are_line_relative() {
        let ed = WasmEditor::new("# c\n", Some("t.py".to_string()));
        let json = ed.highlight_json();
        // The comment starts at offset 0 of its line.
        assert!(
            json.contains("{\"start\":0,\"end\":3,\"kind\":\"comment\"}"),
            "{json}"
        );
    }
}
