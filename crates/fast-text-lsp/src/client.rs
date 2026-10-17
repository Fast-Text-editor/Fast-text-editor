use std::collections::HashMap;
use std::fmt;
use std::io::{self, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use crate::protocol::{
    CodeAction, CompletionItem, Diagnostic, DocumentSymbol, Location, LspRange, TextEdit,
    WorkspaceEdit,
};
use crate::transport::{write_message, Transport};

/// Errors from the LSP client.
#[derive(Debug)]
pub enum LspError {
    Io(io::Error),
    Protocol(String),
    Server(String),
    Timeout,
}

impl fmt::Display for LspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LspError::Io(e) => write!(f, "io error: {e}"),
            LspError::Protocol(e) => write!(f, "protocol error: {e}"),
            LspError::Server(e) => write!(f, "server error: {e}"),
            LspError::Timeout => write!(f, "server did not respond in time"),
        }
    }
}

impl std::error::Error for LspError {}

impl From<io::Error> for LspError {
    fn from(e: io::Error) -> Self {
        LspError::Io(e)
    }
}

type Pending = Arc<Mutex<HashMap<i64, Sender<Value>>>>;
type Diagnostics = Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>;

/// A handle to a running language server.
pub struct LspClient {
    child: Child,
    writer: Arc<Mutex<ChildStdin>>,
    pending: Pending,
    diagnostics: Diagnostics,
    next_id: AtomicI64,
    /// Server messages other than responses (logged for debugging).
    logs: Arc<Mutex<Vec<String>>>,
    timeout: Duration,
}

impl LspClient {
    /// Spawns a language server process.
    pub fn spawn(program: &str, args: &[String], root: &Path) -> Result<Self, LspError> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| LspError::Server(format!("failed to start {program}: {e}")))?;

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        let writer = Arc::new(Mutex::new(stdin));
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: Diagnostics = Arc::new(Mutex::new(HashMap::new()));
        let logs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        spawn_reader(stdout, pending.clone(), diagnostics.clone(), logs.clone());

        Ok(Self {
            child,
            writer,
            pending,
            diagnostics,
            next_id: AtomicI64::new(1),
            logs,
            timeout: Duration::from_secs(30),
        })
    }

    /// Sends a request and blocks until the matching response arrives.
    pub fn request(&self, method: &str, params: Value) -> Result<Value, LspError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, tx);

        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        {
            let mut w = self.writer.lock().unwrap();
            write_message(&mut *w, &msg)?;
        }

        let response = rx
            .recv_timeout(self.timeout)
            .map_err(|_| LspError::Timeout)?;
        if let Some(err) = response.get("error") {
            return Err(LspError::Server(err.to_string()));
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Sends a notification (no response expected).
    pub fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let mut w = self.writer.lock().unwrap();
        write_message(&mut *w, &msg)?;
        Ok(())
    }

    /// Performs the LSP handshake.
    pub fn initialize(&self, root: &Path) -> Result<Value, LspError> {
        let root_uri = path_to_uri(root);
        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "completion": { "completionItem": { "snippetSupport": false } },
                    "hover": { "contentFormat": ["plaintext", "markdown"] },
                    "publishDiagnostics": {},
                    "definition": {},
                    "rename": { "prepareSupport": false },
                    "formatting": {},
                    "codeAction": { "codeActionLiteralSupport": { "codeActionKind": { "valueSet": [] } } },
                    "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
                }
            },
            "workspaceFolders": [ { "uri": root_uri, "name": "workspace" } ]
        });
        let result = self.request("initialize", params)?;
        self.notify("initialized", json!({}))?;
        Ok(result)
    }

    /// Notifies the server that a document was opened.
    pub fn did_open(&self, uri: &str, language_id: &str, text: &str) -> Result<(), LspError> {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text
                }
            }),
        )
    }

    /// Notifies the server that a document changed (full-text sync).
    pub fn did_change(&self, uri: &str, version: i64, text: &str) -> Result<(), LspError> {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [ { "text": text } ]
            }),
        )
    }

    pub fn did_close(&self, uri: &str) -> Result<(), LspError> {
        self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        )
    }

    pub fn completion(
        &self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<CompletionItem>, LspError> {
        let result = self.request(
            "textDocument/completion",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )?;
        let items = match result {
            Value::Array(items) => items,
            Value::Object(mut obj) => obj
                .remove("items")
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        Ok(items
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    pub fn hover(&self, uri: &str, line: u32, character: u32) -> Result<Option<String>, LspError> {
        let result = self.request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )?;
        Ok(extract_hover(&result))
    }

    pub fn definition(
        &self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<Option<Location>, LspError> {
        let result = self.request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )?;
        let value = match result {
            Value::Array(items) => items.into_iter().next().unwrap_or(Value::Null),
            other => other,
        };
        Ok(serde_json::from_value(value).ok())
    }

    /// Asks the server to rename the symbol at a position.
    pub fn rename(
        &self,
        uri: &str,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> Result<WorkspaceEdit, LspError> {
        let result = self.request(
            "textDocument/rename",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
                "newName": new_name
            }),
        )?;
        normalise_workspace_edit(result)
    }

    /// Asks the server to format a whole document.
    pub fn formatting(&self, uri: &str) -> Result<Vec<TextEdit>, LspError> {
        let result = self.request(
            "textDocument/formatting",
            json!({
                "textDocument": { "uri": uri },
                "options": { "tabSize": 4, "insertSpaces": true }
            }),
        )?;
        Ok(serde_json::from_value(result).unwrap_or_default())
    }

    /// Code actions offered for a range.
    pub fn code_actions(
        &self,
        uri: &str,
        start: (u32, u32),
        end: (u32, u32),
    ) -> Result<Vec<CodeAction>, LspError> {
        let result = self.request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": uri },
                "range": {
                    "start": { "line": start.0, "character": start.1 },
                    "end": { "line": end.0, "character": end.1 }
                },
                "context": { "diagnostics": [] }
            }),
        )?;
        let items = match result {
            Value::Array(items) => items,
            _ => Vec::new(),
        };
        Ok(items
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect())
    }

    /// The document outline.
    pub fn document_symbols(&self, uri: &str) -> Result<Vec<DocumentSymbol>, LspError> {
        let result = self.request(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )?;
        Ok(flatten_symbols(&result, None))
    }

    /// Diagnostics for a document URI, if any have been published.
    pub fn diagnostics_for(&self, uri: &str) -> Vec<Diagnostic> {
        self.diagnostics
            .lock()
            .unwrap()
            .get(uri)
            .cloned()
            .unwrap_or_default()
    }

    /// Total diagnostics across all documents.
    pub fn diagnostics_count(&self) -> usize {
        self.diagnostics
            .lock()
            .unwrap()
            .values()
            .map(Vec::len)
            .sum()
    }

    /// Recent non-response messages from the server.
    pub fn logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().clone()
    }

    /// Gracefully shuts the server down.
    pub fn shutdown(mut self) {
        let _ = self.request("shutdown", Value::Null);
        let _ = self.notify("exit", Value::Null);
        let _ = self.child.wait();
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_reader(
    stdout: ChildStdout,
    pending: Pending,
    diagnostics: Diagnostics,
    logs: Arc<Mutex<Vec<String>>>,
) {
    std::thread::spawn(move || {
        let mut transport = Transport::new(BufReader::new(stdout), io::sink());
        while let Ok(msg) = transport.receive() {
            // Responses carry an id and a result/error.
            if let Some(id) = msg.get("id").and_then(Value::as_i64) {
                if msg.get("result").is_some() || msg.get("error").is_some() {
                    if let Some(tx) = pending.lock().unwrap().remove(&id) {
                        let _ = tx.send(msg);
                    }
                    continue;
                }
            }

            // Notifications.
            match msg.get("method").and_then(Value::as_str) {
                Some("textDocument/publishDiagnostics") => {
                    if let Some(params) = msg.get("params") {
                        let uri = params.get("uri").and_then(Value::as_str).unwrap_or("");
                        let diags: Vec<Diagnostic> = params
                            .get("diagnostics")
                            .and_then(|d| serde_json::from_value(d.clone()).ok())
                            .unwrap_or_default();
                        diagnostics.lock().unwrap().insert(uri.to_string(), diags);
                    }
                }
                Some(method) => {
                    let mut logs = logs.lock().unwrap();
                    if logs.len() < 200 {
                        logs.push(method.to_string());
                    }
                }
                None => {}
            }
        }

        // The server exited: fail any still-outstanding requests so callers
        // don't wait out the full timeout.
        let mut pending = pending.lock().unwrap();
        for (_, tx) in pending.drain() {
            let _ = tx.send(json!({
                "jsonrpc": "2.0",
                "error": { "message": "language server exited" }
            }));
        }
    });
}

/// Extracts plain text from an LSP `Hover.contents` value.
fn extract_hover(result: &Value) -> Option<String> {
    let contents = result.get("contents")?;
    match contents {
        Value::String(s) => Some(s.clone()),
        Value::Object(map) => map.get("value").and_then(Value::as_str).map(str::to_string),
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|item| match item {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(m) => m.get("value").and_then(Value::as_str).map(str::to_string),
                    _ => None,
                })
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}

/// Accepts either `changes` or `documentChanges` from a rename response.
fn normalise_workspace_edit(result: Value) -> Result<WorkspaceEdit, LspError> {
    if result.is_null() {
        return Ok(WorkspaceEdit::default());
    }
    if let Ok(edit) = serde_json::from_value::<WorkspaceEdit>(result.clone()) {
        if !edit.is_empty() {
            return Ok(edit);
        }
    }
    // `documentChanges` is an array of `TextDocumentEdit`s.
    let mut edit = WorkspaceEdit::default();
    if let Some(changes) = result.get("documentChanges").and_then(Value::as_array) {
        for change in changes {
            let Some(uri) = change
                .get("textDocument")
                .and_then(|d| d.get("uri"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let edits: Vec<TextEdit> = change
                .get("edits")
                .and_then(Value::as_array)
                .map(|edits| {
                    edits
                        .iter()
                        .filter_map(|e| serde_json::from_value(e.clone()).ok())
                        .collect()
                })
                .unwrap_or_default();
            edit.changes
                .entry(uri.to_string())
                .or_default()
                .extend(edits);
        }
    }
    Ok(edit)
}

/// Flattens a `documentSymbol` result into a flat outline.
///
/// Servers answer with either hierarchical `DocumentSymbol[]` (with `children`)
/// or a flat `SymbolInformation[]` (with `location`). Both are accepted.
fn flatten_symbols(value: &Value, uri: Option<&str>) -> Vec<DocumentSymbol> {
    let mut out = Vec::new();
    let Some(items) = value.as_array() else {
        return out;
    };
    for item in items {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let kind = item.get("kind").and_then(Value::as_u64).map(|k| k as u8);
        let detail = item
            .get("detail")
            .and_then(Value::as_str)
            .map(str::to_string);

        // A flat `SymbolInformation` carries its own location.
        if let Some(location) = item.get("location") {
            let item_uri = location
                .get("uri")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| uri.map(str::to_string));
            if let Some(range) = location.get("range").and_then(parse_range) {
                out.push(DocumentSymbol {
                    name,
                    detail,
                    kind,
                    uri: item_uri,
                    range,
                });
                continue;
            }
        }

        // Hierarchical: `selectionRange` is the better jump target.
        let range = item
            .get("selectionRange")
            .or_else(|| item.get("range"))
            .and_then(parse_range);
        if let Some(range) = range {
            out.push(DocumentSymbol {
                name: name.clone(),
                detail,
                kind,
                uri: uri.map(str::to_string),
                range,
            });
        }
        if let Some(children) = item.get("children") {
            out.extend(flatten_symbols(children, uri));
        }
    }
    out
}

/// Parses a `{ start: { line, character }, end: … }` value.
fn parse_range(value: &Value) -> Option<LspRange> {
    serde_json::from_value(value.clone()).ok()
}

/// Converts a filesystem path into a `file://` URI.
pub fn path_to_uri(path: &Path) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", abs.to_string_lossy())
}

/// Converts a `file://` URI back into a filesystem path.
pub fn uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    uri.strip_prefix("file://").map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn range(line: u64, character: u64) -> Value {
        json!({ "start": { "line": line, "character": character },
                "end": { "line": line, "character": character + 1 } })
    }

    #[test]
    fn flattens_hierarchical_document_symbols() {
        let result = json!([{
            "name": "struct Point",
            "kind": 23,
            "range": range(0, 0),
            "selectionRange": range(0, 7),
            "children": [{ "name": "x", "kind": 8, "range": range(1, 4), "selectionRange": range(1, 4) }]
        }]);
        let symbols = flatten_symbols(&result, Some("file:///a.rs"));
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "struct Point");
        assert_eq!(symbols[0].range.start.character, 7, "selection range wins");
        assert_eq!(symbols[1].name, "x");
        assert_eq!(symbols[1].uri.as_deref(), Some("file:///a.rs"));
    }

    #[test]
    fn flattens_flat_symbol_information() {
        let result = json!([{
            "name": "main",
            "kind": 12,
            "location": { "uri": "file:///b.rs", "range": range(3, 0) }
        }]);
        let symbols = flatten_symbols(&result, Some("file:///ignored.rs"));
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].uri.as_deref(), Some("file:///b.rs"));
        assert_eq!(symbols[0].range.start.line, 3);
    }

    #[test]
    fn workspace_edit_reads_changes_and_document_changes() {
        let with_changes = normalise_workspace_edit(json!({
            "changes": { "file:///a.rs": [{ "range": range(0, 0), "newText": "b" }] }
        }))
        .unwrap();
        assert_eq!(with_changes.len(), 1);
        assert_eq!(with_changes.changes["file:///a.rs"][0].new_text, "b");

        let with_document_changes = normalise_workspace_edit(json!({
            "documentChanges": [{
                "textDocument": { "uri": "file:///c.rs", "version": 3 },
                "edits": [{ "range": range(1, 2), "newText": "z" }]
            }]
        }))
        .unwrap();
        assert_eq!(with_document_changes.len(), 1);
        assert_eq!(
            with_document_changes.changes["file:///c.rs"][0]
                .range
                .start
                .line,
            1
        );
    }

    #[test]
    fn an_empty_rename_response_is_an_empty_edit() {
        assert!(normalise_workspace_edit(Value::Null).unwrap().is_empty());
    }
}
