//! WebAssembly bindings for the Fast Text engine.
//!
//! This exposes the same `fast-text-core` [`Document`] that powers the TUI to
//! JavaScript, plus the `fast-text-syntax` highlighter rendered to HTML. The web
//! frontend in `web/` is a thin shell over this API.
//!
//! Build with `wasm-pack build crates/fast-text-wasm --target web`.

use fast_text_core::{Document, SearchDirection};
use fast_text_plugin::{command_for, command_id, command_title, default_bindings, KeySpec};
use fast_text_syntax::{highlight_line, HighlightSpan, LanguageId, LineState, TokenKind};
use fast_text_theme::{Rgb, Theme};
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

    /// Sets the selection from an anchor to a head (both character indices).
    #[wasm_bindgen(js_name = setSelection)]
    pub fn set_selection(&mut self, anchor: usize, head: usize) {
        self.doc.select(anchor, head);
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

    /// Replaces a character range [start, end) with text.
    #[wasm_bindgen(js_name = replaceRange)]
    pub fn replace_range(&mut self, start: usize, end: usize, text: &str) {
        self.doc.replace_range(start, end, text);
    }

    /// Deletes the current selection.
    #[wasm_bindgen(js_name = deleteSelection)]
    pub fn delete_selection(&mut self) {
        self.doc.delete_selection();
    }

    /// Adds a caret above the primary cursor.
    #[wasm_bindgen(js_name = addCaretAbove)]
    pub fn add_caret_above(&mut self) {
        self.doc.add_caret_above();
    }

    /// Adds a caret below the primary cursor.
    #[wasm_bindgen(js_name = addCaretBelow)]
    pub fn add_caret_below(&mut self) {
        self.doc.add_caret_below();
    }

    /// Selects the next occurrence of the word under the cursor (or current selection).
    #[wasm_bindgen(js_name = selectNextOccurrence)]
    pub fn select_next_occurrence(&mut self, case_sensitive: bool) -> bool {
        self.doc.select_next_occurrence(case_sensitive)
    }

    /// The number of active selections (1 + extra cursors).
    #[wasm_bindgen(js_name = selectionCount)]
    pub fn selection_count(&self) -> usize {
        1 + self.doc.extra_selections().len()
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

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn rgb_hex(color: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// Returns the shared keybindings as a JSON array.
///
/// Each entry is `{"key":"Ctrl+S","command":"save","title":"Save"}` so the
/// web frontend can draw tooltips and menus without duplicating the bindings.
#[wasm_bindgen(js_name = keybindings)]
pub fn keybindings() -> String {
    let mut entries: Vec<String> = Vec::new();
    for (spec, command) in default_bindings() {
        entries.push(format!(
            "{{\"key\":{},\"command\":{},\"title\":{}}}",
            json_string(&spec.display()),
            json_string(command_id(command)),
            json_string(command_title(command)),
        ));
    }
    format!("[{}]", entries.join(","))
}

/// Resolves a key string (e.g. `"Ctrl+P"`) to a command id.
#[wasm_bindgen(js_name = commandForKey)]
pub fn command_for_key(key: &str) -> Option<String> {
    let spec = KeySpec::parse(key)?;
    let command = command_for(spec)?;
    Some(command_id(command).to_string())
}

/// Resolves a key string to a human-readable command title.
#[wasm_bindgen(js_name = commandTitleForKey)]
pub fn command_title_for_key(key: &str) -> Option<String> {
    let spec = KeySpec::parse(key)?;
    let command = command_for(spec)?;
    Some(command_title(command).to_string())
}

/// All built-in theme names as a JSON array.
#[wasm_bindgen(js_name = themeNames)]
pub fn theme_names() -> String {
    let names: Vec<String> = Theme::names().iter().map(|n| json_string(n)).collect();
    format!("[{}]", names.join(","))
}

/// The CSS custom properties for a theme.
///
/// The web frontend can apply this to `:root` to make the entire UI (editor,
/// gutters, tabs, status bar) follow the same shared palette as the terminal
/// and desktop builds.
#[wasm_bindgen(js_name = themeCss)]
pub fn theme_css(name: &str) -> String {
    let Some(theme) = Theme::builtin(name) else {
        return String::new();
    };
    let ui = theme.ui;
    let s = theme.syntax;
    let scheme = if theme.dark { "dark" } else { "light" };
    format!(
        ":root {{ color-scheme: {scheme}; --bg: {bg}; --fg: {fg}; --gutter: {gutter}; --panel: {panel}; --line: {line}; --sel: {sel}; --accent: {accent}; --string: {string}; --keyword: {keyword}; --type: {type}; --number: {number}; --comment: {comment}; --operator: {operator}; --punct: {punct}; --constant: {constant}; --attribute: {attribute}; --variable: {variable}; --error: {error}; --warning: {warning}; --tab-active: {tab_active}; --tab-inactive: {tab_inactive}; --prompt: {prompt}; }}",
        scheme = scheme,
        bg = rgb_hex(ui.background),
        fg = rgb_hex(ui.foreground),
        gutter = rgb_hex(ui.gutter),
        panel = rgb_hex(ui.status_bg),
        line = rgb_hex(ui.cursor_line),
        sel = rgb_hex(ui.selection),
        accent = rgb_hex(ui.status_accent),
        string = rgb_hex(s.string),
        keyword = rgb_hex(s.keyword),
        type = rgb_hex(s.type_),
        number = rgb_hex(s.number),
        comment = rgb_hex(s.comment),
        operator = rgb_hex(s.operator),
        punct = rgb_hex(s.punctuation),
        constant = rgb_hex(s.constant),
        attribute = rgb_hex(s.attribute),
        variable = rgb_hex(s.variable),
        error = rgb_hex(ui.error),
        warning = rgb_hex(ui.warning),
        tab_active = rgb_hex(ui.tab_active),
        tab_inactive = rgb_hex(ui.tab_inactive),
        prompt = rgb_hex(ui.prompt_bg),
    )
}

/// The full theme as JSON for frontends that want structured access.
#[wasm_bindgen(js_name = themeColors)]
pub fn theme_colors(name: &str) -> String {
    let Some(theme) = Theme::builtin(name) else {
        return "{}".to_string();
    };
    let ui = theme.ui;
    let s = theme.syntax;
    let ui_entries = [
        ("background", ui.background),
        ("foreground", ui.foreground),
        ("gutter", ui.gutter),
        ("gutterActive", ui.gutter_active),
        ("cursorLine", ui.cursor_line),
        ("selection", ui.selection),
        ("statusBg", ui.status_bg),
        ("statusFg", ui.status_fg),
        ("statusAccent", ui.status_accent),
        ("promptBg", ui.prompt_bg),
        ("tabActive", ui.tab_active),
        ("tabInactive", ui.tab_inactive),
        ("error", ui.error),
        ("warning", ui.warning),
    ];
    let syntax_entries = [
        ("plain", s.plain),
        ("keyword", s.keyword),
        ("type", s.type_),
        ("function", s.function),
        ("string", s.string),
        ("number", s.number),
        ("comment", s.comment),
        ("operator", s.operator),
        ("punctuation", s.punctuation),
        ("constant", s.constant),
        ("attribute", s.attribute),
        ("variable", s.variable),
    ];
    let ui_json = json_colors(&ui_entries);
    let syntax_json = json_colors(&syntax_entries);
    format!(
        "{{\"name\":{},\"dark\":{},\"ui\":{{{}}},\"syntax\":{{{}}}}}",
        json_string(&theme.name),
        if theme.dark { "true" } else { "false" },
        ui_json,
        syntax_json,
    )
}

fn json_colors(entries: &[(&str, Rgb)]) -> String {
    entries
        .iter()
        .map(|(key, color)| format!("{}:{}", json_string(key), json_string(&rgb_hex(*color))))
        .collect::<Vec<_>>()
        .join(",")
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
    fn exports_the_shared_keymap() {
        let json = keybindings();
        assert!(json.contains("\"command\":\"file_finder\""), "{json}");
        assert!(json.contains("\"key\":\"Ctrl+P\""), "{json}");
        assert!(json.contains("\"title\":\"Save\""), "{json}");
        assert_eq!(command_for_key("Ctrl+P").as_deref(), Some("file_finder"));
        assert_eq!(
            command_title_for_key("Ctrl+Shift+P").as_deref(),
            Some("Command Palette…")
        );
        assert!(command_for_key("Ctrl+Nonsense").is_none());
    }

    #[test]
    fn exports_the_shared_themes_as_css_and_json() {
        let names = theme_names();
        assert!(names.contains("\"monokai\""));
        assert!(names.contains("\"tokyo-night\""));
        let css = theme_css("monokai");
        assert!(css.starts_with(":root {"), "{css}");
        assert!(css.contains("--keyword: #"), "{css}");
        let json = theme_colors("monokai");
        assert!(json.contains("\"syntax\":{"), "{json}");
        assert!(json.contains("\"keyword\":\"#"), "{json}");
        assert_eq!(theme_css("does-not-exist"), "");
        assert_eq!(theme_colors("does-not-exist"), "{}");
    }

    #[test]
    fn replace_range_is_undoable() {
        let mut ed = WasmEditor::new("hello world", None);
        ed.replace_range(6, 11, "there");
        assert_eq!(ed.text(), "hello there");
        ed.undo();
        assert_eq!(ed.text(), "hello world");
    }

    #[test]
    fn multi_cursor_helpers_are_exposed() {
        let mut ed = WasmEditor::new("a\nb\nc", None);
        ed.add_caret_below();
        assert_eq!(ed.selection_count(), 2);

        let mut ed = WasmEditor::new("foo foo\nbar", None);
        // The first call selects the word; the second adds the next occurrence.
        assert!(ed.select_next_occurrence(true));
        assert_eq!(ed.selection_count(), 1);
        assert!(ed.select_next_occurrence(true));
        assert_eq!(ed.selection_count(), 2);
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
