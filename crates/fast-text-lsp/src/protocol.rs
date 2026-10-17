use std::collections::HashMap;

use serde::Deserialize;

/// An LSP position (0-based line and UTF-16 code-unit character offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

/// A half-open range in a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

impl Severity {
    fn from_raw(raw: Option<u8>) -> Severity {
        match raw {
            Some(1) => Severity::Error,
            Some(2) => Severity::Warning,
            Some(3) => Severity::Information,
            Some(4) => Severity::Hint,
            _ => Severity::Error,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Information => "info",
            Severity::Hint => "hint",
        }
    }
}

/// A diagnostic reported by the server.
#[derive(Debug, Clone, Deserialize)]
pub struct Diagnostic {
    pub range: LspRange,
    #[serde(default)]
    pub severity: Option<u8>,
    pub message: String,
    #[serde(default)]
    pub source: Option<String>,
}

impl Diagnostic {
    pub fn severity(&self) -> Severity {
        Severity::from_raw(self.severity)
    }

    /// A short one-line summary.
    pub fn summary(&self) -> String {
        let src = self
            .source
            .as_deref()
            .map(|s| format!("[{s}] "))
            .unwrap_or_default();
        format!(
            "{}{} ({}:{})",
            src,
            self.message.lines().next().unwrap_or(""),
            self.range.start.line + 1,
            self.range.start.character + 1
        )
    }
}

/// A completion proposal.
#[derive(Debug, Clone, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub kind: Option<u8>,
    #[serde(default, rename = "insertText")]
    pub insert_text: Option<String>,
}

impl CompletionItem {
    /// Text to insert for this item.
    pub fn insertion(&self) -> &str {
        self.insert_text.as_deref().unwrap_or(&self.label)
    }

    /// A single-character kind badge (roughly matching LSP completion kinds).
    pub fn kind_badge(&self) -> &'static str {
        match self.kind {
            Some(2) => "m",  // method
            Some(3) => "f",  // function
            Some(4) => "c",  // constructor
            Some(5) => "F",  // field
            Some(6) => "v",  // variable
            Some(7) => "C",  // class
            Some(8) => "I",  // interface
            Some(9) => "M",  // module
            Some(13) => "e", // enum
            Some(14) => "k", // keyword
            Some(21) => "c", // constant
            _ => " ",
        }
    }
}

/// A location in the workspace (used for go-to-definition).
#[derive(Debug, Clone, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: LspRange,
}

/// A text edit returned by formatting, rename or a code action.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TextEdit {
    pub range: LspRange,
    #[serde(rename = "newText")]
    pub new_text: String,
}

/// A set of edits across several documents.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct WorkspaceEdit {
    /// `uri -> edits`. Servers that only return `documentChanges` are ignored.
    #[serde(default)]
    pub changes: HashMap<String, Vec<TextEdit>>,
}

impl WorkspaceEdit {
    /// True when there is nothing to apply.
    pub fn is_empty(&self) -> bool {
        self.changes.values().all(|edits| edits.is_empty())
    }

    /// Total number of edits in the workspace edit.
    pub fn len(&self) -> usize {
        self.changes.values().map(Vec::len).sum()
    }
}

/// An action the server offers at a position.
#[derive(Debug, Clone, Deserialize)]
pub struct CodeAction {
    pub title: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub edit: Option<WorkspaceEdit>,
}

/// One entry in the document outline.
#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    pub name: String,
    pub detail: Option<String>,
    pub kind: Option<u8>,
    /// The document the symbol lives in (flat `SymbolInformation` only).
    pub uri: Option<String>,
    /// Where the symbol starts (its selection range when the server gives one).
    pub range: LspRange,
}

impl DocumentSymbol {
    /// A single-character badge roughly matching the LSP symbol kinds.
    pub fn kind_badge(&self) -> &'static str {
        match self.kind {
            Some(1) => "M",  // module / file
            Some(2) => "n",  // namespace
            Some(3) => "m",  // package
            Some(5) => "C",  // class
            Some(6) => "m",  // method
            Some(7) => "p",  // property
            Some(8) => "F",  // field
            Some(9) => "c",  // constructor
            Some(10) => "e", // enum
            Some(11) => "I", // interface
            Some(12) => "f", // function
            Some(13) => "v", // variable
            Some(14) => "K", // constant
            _ => " ",
        }
    }
}
