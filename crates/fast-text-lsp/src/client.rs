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

use crate::protocol::{CompletionItem, Diagnostic, Location};
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
                    "definition": {}
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

/// Converts a filesystem path into a `file://` URI.
pub fn path_to_uri(path: &Path) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", abs.to_string_lossy())
}

/// Converts a `file://` URI back into a filesystem path.
pub fn uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    uri.strip_prefix("file://").map(std::path::PathBuf::from)
}
