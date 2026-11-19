//! Language Server integration for the TUI.
//!
//! [`LspManager`] lazily starts a language server per language (if one is
//! installed), keeps the server's view of each document in sync, and exposes
//! diagnostics, completion, hover, and go-to-definition.
//!
//! Servers are started on a background thread so a slow (or missing) server can
//! never freeze the interface. Everything degrades gracefully: with no server
//! installed, the editor behaves exactly as before.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use fast_text_lsp::{
    path_to_uri, CodeAction, CompletionItem, Diagnostic, DocumentSymbol, Location, LspClient,
    TextEdit, WorkspaceEdit,
};
use fast_text_syntax::LanguageId;

use crate::app::Editor;

/// Per-document sync bookkeeping.
#[derive(Clone)]
struct OpenDoc {
    revision: u64,
    version: i64,
}

/// A ready-to-use session.
struct Session {
    client: LspClient,
    opened: HashMap<String, OpenDoc>,
}

/// A session that may still be starting.
enum State {
    Starting(Receiver<Result<LspClient, ()>>),
    Ready(Session),
}

/// How to install one missing language server.
///
/// The hint is plain data so it can be shown in the consent dialog and tested
/// without running anything. Only the user saying yes actually spawns the
/// installer (see `App::begin_lsp_install`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallHint {
    /// The language that needs the server.
    pub language: LanguageId,
    /// The executable that will be on `PATH` after installing.
    pub program: String,
    /// The installer command, ready to spawn (program first).
    pub command: Vec<String>,
    /// Where to read more, shown in the dialog.
    pub docs: &'static str,
}

impl InstallHint {
    /// The command as a single shell-like line, for display.
    pub fn command_line(&self) -> String {
        self.command.join(" ")
    }
}

/// The install recipe for a language, when we know one.
///
/// Only languages with a well-known, package-manager install are listed; for
/// anything else the editor simply stays quiet and keeps working without LSP.
pub fn install_hint(lang: LanguageId) -> Option<InstallHint> {
    let (program, command, docs): (&str, &[&str], &str) = match lang.name() {
        "Rust" => (
            "rust-analyzer",
            &["rustup", "component", "add", "rust-analyzer"],
            "https://rust-analyzer.github.io/manual.html#installation",
        ),
        "JavaScript" | "TypeScript" => (
            "typescript-language-server",
            &[
                "npm",
                "install",
                "-g",
                "typescript-language-server",
                "typescript",
            ],
            "https://github.com/typescript-language-server/typescript-language-server",
        ),
        "Python" => (
            "pylsp",
            &["pip", "install", "python-lsp-server"],
            "https://github.com/python-lsp/python-lsp-server",
        ),
        "Shell" => (
            "bash-language-server",
            &["npm", "install", "-g", "bash-language-server"],
            "https://github.com/bash-lsp/bash-language-server",
        ),
        "Go" => (
            "gopls",
            &["go", "install", "golang.org/x/tools/gopls@latest"],
            "https://go.dev/gopls",
        ),
        "Ruby" => (
            "solargraph",
            &["gem", "install", "solargraph"],
            "https://solargraph.org/",
        ),
        _ => return None,
    };
    Some(InstallHint {
        language: lang,
        program: program.to_string(),
        command: command.iter().map(|part| (*part).to_string()).collect(),
        docs,
    })
}

/// Manages language-server sessions for the open buffers.
#[derive(Default)]
pub struct LspManager {
    enabled: bool,
    sessions: HashMap<LanguageId, State>,
    /// Languages for which no server is installed and no offer is pending.
    unsupported: HashSet<LanguageId>,
    /// Languages we have already offered to install (so we do not nag).
    offered: HashSet<LanguageId>,
}

impl LspManager {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            sessions: HashMap::new(),
            unsupported: HashSet::new(),
            offered: HashSet::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Languages with a live, ready server session.
    pub fn active_languages(&self) -> Vec<LanguageId> {
        self.sessions
            .iter()
            .filter(|(_, s)| matches!(s, State::Ready(_)))
            .map(|(lang, _)| *lang)
            .collect()
    }

    fn is_active(&self, lang: LanguageId) -> bool {
        matches!(self.sessions.get(&lang), Some(State::Ready(_)))
    }

    /// Starts a server for the buffer's language if needed and syncs the text.
    /// Never blocks.
    ///
    /// Returns the language when no server is installed but we know how to
    /// install one and have not asked the user yet. The caller shows the
    /// consent dialog; every other path returns `None`.
    pub fn sync(&mut self, ed: &Editor) -> Option<LanguageId> {
        if !self.enabled || ed.is_untitled() {
            return None;
        }
        let lsp_id = ed.language.lsp_id()?;
        let lang = ed.language;
        let path = ed.doc.path()?;

        // Start the server if we have not tried yet.
        if !self.sessions.contains_key(&lang) {
            if self.unsupported.contains(&lang) {
                return None;
            }
            let Some(spec) = resolve_server(lang) else {
                return self.offer_for(lang);
            };
            let root = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(Path::to_path_buf)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_default();
            self.sessions
                .insert(lang, State::Starting(spawn_server(spec, root)));
            return None;
        }

        // Promote a starting session to ready once the background thread is done.
        match self.sessions.get_mut(&lang) {
            Some(State::Starting(rx)) => match rx.try_recv() {
                Ok(Ok(client)) => {
                    self.sessions.insert(
                        lang,
                        State::Ready(Session {
                            client,
                            opened: HashMap::new(),
                        }),
                    );
                }
                Ok(Err(())) | Err(TryRecvError::Disconnected) => {
                    self.sessions.remove(&lang);
                    self.unsupported.insert(lang);
                    return None;
                }
                Err(TryRecvError::Empty) => return None,
            },
            Some(State::Ready(_)) => {}
            None => return None,
        }

        // Sync document text.
        let uri = path_to_uri(path);
        let revision = ed.doc.buffer().revision();
        let Some(State::Ready(session)) = self.sessions.get_mut(&lang) else {
            return None;
        };

        match session.opened.get(&uri).cloned() {
            None => {
                let text = ed.doc.text();
                if session.client.did_open(&uri, lsp_id, &text).is_ok() {
                    session.opened.insert(
                        uri,
                        OpenDoc {
                            revision,
                            version: 1,
                        },
                    );
                }
            }
            Some(doc) => {
                if doc.revision != revision {
                    let version = doc.version + 1;
                    let text = ed.doc.text();
                    if session.client.did_change(&uri, version, &text).is_ok() {
                        session.opened.insert(uri, OpenDoc { revision, version });
                    }
                }
            }
        }
        None
    }

    /// Decides whether to offer an install for `lang`.
    ///
    /// A language is offered once per session, and only when we have a recipe.
    /// The caller must call [`Self::mark_offered`] when it actually shows the
    /// dialog; that lets a modal belonging to another language finish first.
    pub(crate) fn offer_for(&mut self, lang: LanguageId) -> Option<LanguageId> {
        if install_hint(lang).is_none() {
            self.unsupported.insert(lang);
            return None;
        }
        if self.offered.contains(&lang) {
            None
        } else {
            Some(lang)
        }
    }

    /// Remembers that the offer has been shown (or declined) for `lang`.
    pub fn mark_offered(&mut self, lang: LanguageId) {
        self.offered.insert(lang);
    }

    /// Remembers that the user declined the install, so we do not ask again.
    pub fn decline(&mut self, lang: LanguageId) {
        self.offered.insert(lang);
        self.unsupported.insert(lang);
    }

    /// Forgets what we knew about `lang`, so the next `sync` probes again.
    ///
    /// Called after an install succeeds (or the user retries from the command
    /// palette), which makes a freshly installed server usable immediately.
    pub fn retry(&mut self, lang: LanguageId) {
        self.offered.remove(&lang);
        self.unsupported.remove(&lang);
        self.sessions.remove(&lang);
    }

    /// Diagnostics the server has published for this buffer.
    pub fn diagnostics(&self, ed: &Editor) -> Vec<Diagnostic> {
        let Some(path) = ed.doc.path() else {
            return Vec::new();
        };
        match self.sessions.get(&ed.language) {
            Some(State::Ready(session)) => session.client.diagnostics_for(&path_to_uri(path)),
            _ => Vec::new(),
        }
    }

    pub fn completion(&self, ed: &Editor) -> Result<Vec<CompletionItem>, String> {
        let (session, uri, line, character) = self.locate(ed)?;
        session
            .client
            .completion(&uri, line, character)
            .map_err(|e| e.to_string())
    }

    pub fn hover(&self, ed: &Editor) -> Result<Option<String>, String> {
        let (session, uri, line, character) = self.locate(ed)?;
        session
            .client
            .hover(&uri, line, character)
            .map_err(|e| e.to_string())
    }

    pub fn definition(&self, ed: &Editor) -> Result<Option<Location>, String> {
        let (session, uri, line, character) = self.locate(ed)?;
        session
            .client
            .definition(&uri, line, character)
            .map_err(|e| e.to_string())
    }

    /// Renames the symbol at the cursor.
    pub fn rename(&self, ed: &Editor, new_name: &str) -> Result<WorkspaceEdit, String> {
        let (session, uri, line, character) = self.locate(ed)?;
        session
            .client
            .rename(&uri, line, character, new_name)
            .map_err(|e| e.to_string())
    }

    /// Formats the whole buffer.
    pub fn format(&self, ed: &Editor) -> Result<Vec<TextEdit>, String> {
        let (session, uri, _, _) = self.locate(ed)?;
        session.client.formatting(&uri).map_err(|e| e.to_string())
    }

    /// Code actions at the cursor (or over the current selection).
    pub fn code_actions(&self, ed: &Editor) -> Result<Vec<CodeAction>, String> {
        if !self.is_active(ed.language) {
            return Err("no language server is running for this buffer".into());
        }
        let Some(State::Ready(session)) = self.sessions.get(&ed.language) else {
            return Err("no language server is running for this buffer".into());
        };
        if ed.is_untitled() {
            return Err("no language server for untitled buffers".into());
        }
        let path = ed.doc.path().ok_or("buffer has no path")?;
        let uri = path_to_uri(path);
        let (start, end) = ed.doc.selection().range();
        let start = lsp_position(ed, start);
        let end = lsp_position(ed, end);
        session
            .client
            .code_actions(&uri, start, end)
            .map_err(|e| e.to_string())
    }

    /// The document outline.
    pub fn document_symbols(&self, ed: &Editor) -> Result<Vec<DocumentSymbol>, String> {
        if !self.is_active(ed.language) {
            return Err("no language server is running for this buffer".into());
        }
        let Some(State::Ready(session)) = self.sessions.get(&ed.language) else {
            return Err("no language server is running for this buffer".into());
        };
        if ed.is_untitled() {
            return Err("no language server for untitled buffers".into());
        }
        let path = ed.doc.path().ok_or("buffer has no path")?;
        let uri = path_to_uri(path);
        session
            .client
            .document_symbols(&uri)
            .map_err(|e| e.to_string())
    }

    /// Resolves the session, URI, and UTF-16 position for the current cursor.
    #[allow(clippy::type_complexity)]
    fn locate(&self, ed: &Editor) -> Result<(&Session, String, u32, u32), String> {
        if ed.is_untitled() {
            return Err("no language server for untitled buffers".into());
        }
        if !self.is_active(ed.language) {
            return Err("no language server is running for this buffer".into());
        }
        let Some(State::Ready(session)) = self.sessions.get(&ed.language) else {
            return Err("no language server is running for this buffer".into());
        };
        let path = ed.doc.path().ok_or("buffer has no path")?;
        let uri = path_to_uri(path);
        let (line, character) = lsp_position(ed, ed.doc.selection().head);
        Ok((session, uri, line, character))
    }

    /// Shuts every server down.
    pub fn shutdown_all(&mut self) {
        for (_, state) in self.sessions.drain() {
            if let State::Ready(session) = state {
                session.client.shutdown();
            }
        }
    }
}

impl Drop for LspManager {
    fn drop(&mut self) {
        self.shutdown_all();
    }
}

/// Spawns and initializes a server on a background thread.
fn spawn_server(spec: ServerSpec, root: std::path::PathBuf) -> Receiver<Result<LspClient, ()>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match LspClient::spawn(&spec.program, &spec.args, &root) {
            Ok(client) if client.initialize(&root).is_ok() => Ok(client),
            _ => Err(()),
        };
        let _ = tx.send(result);
    });
    rx
}

/// UTF-16 code-unit offset of the first `col_chars` characters of `line`.
fn utf16_col(line: &str, col_chars: usize) -> u32 {
    line.chars()
        .take(col_chars)
        .map(|c| c.len_utf16() as u32)
        .sum()
}

/// The LSP `(line, character)` for a character index in a buffer.
fn lsp_position(ed: &Editor, index: usize) -> (u32, u32) {
    let pos = ed.doc.position_of(index);
    let line_text = ed.doc.buffer().line_str(pos.line);
    (pos.line as u32, utf16_col(&line_text, pos.col))
}

struct ServerSpec {
    program: String,
    args: Vec<String>,
}

/// Chooses an installed language server for a language, if any.
///
/// Server names and commands come from the Helix-derived table in
/// `fast-text-syntax`; we simply pick the first one that is on `PATH`.
fn resolve_server(lang: LanguageId) -> Option<ServerSpec> {
    for name in lang.lsp_servers() {
        if let Some((command, args)) = fast_text_syntax::language_server(name) {
            if which(command) {
                return Some(ServerSpec {
                    program: command.to_string(),
                    args: args.iter().map(|s| (*s).to_string()).collect(),
                });
            }
        }
    }
    None
}

/// Whether an executable is on `PATH`.
fn which(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_count_code_units() {
        assert_eq!(utf16_col("hello", 3), 3);
        // An emoji is two UTF-16 code units.
        assert_eq!(utf16_col("a😀b", 2), 3);
        assert_eq!(utf16_col("a😀b", 0), 0);
    }

    #[test]
    fn disabled_manager_does_not_sync() {
        let mut mgr = LspManager::new(false);
        let ed = Editor::from_text("fn main() {}", LanguageId::Rust);
        mgr.sync(&ed);
        assert!(mgr.active_languages().is_empty());
        assert!(mgr.diagnostics(&ed).is_empty());
    }

    #[test]
    fn sync_never_blocks_for_untitled_and_plain() {
        let mut mgr = LspManager::new(true);
        // Plain text has no LSP id; sync must be a no-op.
        let ed = Editor::from_text("hello", LanguageId::Plain);
        mgr.sync(&ed);
        assert!(mgr.active_languages().is_empty());
    }

    #[test]
    fn server_resolution_is_sensible() {
        assert!(resolve_server(LanguageId::Plain).is_none());
        assert!(!which("this-binary-definitely-does-not-exist-xyz"));
    }

    #[test]
    fn untitled_buffers_have_no_diagnostics() {
        let mgr = LspManager::new(true);
        let ed = Editor::from_text("hello", LanguageId::Plain);
        assert!(mgr.diagnostics(&ed).is_empty());
    }

    #[test]
    fn known_languages_have_install_recipes() {
        for lang in [
            LanguageId::Rust,
            LanguageId::TypeScript,
            LanguageId::Python,
            LanguageId::Shell,
            LanguageId::Go,
            LanguageId::Ruby,
        ] {
            let hint = install_hint(lang).unwrap_or_else(|| panic!("no hint for {}", lang.name()));
            assert!(!hint.command.is_empty());
            assert!(!hint.program.is_empty());
            assert!(hint.command_line().starts_with(&hint.command[0]));
            // The recipe must install the program we will look for on PATH.
            assert_eq!(
                hint.program,
                match lang.name() {
                    "Rust" => "rust-analyzer",
                    "TypeScript" => "typescript-language-server",
                    "Python" => "pylsp",
                    "Shell" => "bash-language-server",
                    "Go" => "gopls",
                    "Ruby" => "solargraph",
                    other => panic!("unexpected language {other}"),
                }
            );
        }
        // Languages without a well-known server get no recipe.
        assert!(install_hint(LanguageId::Plain).is_none());
        assert!(install_hint(LanguageId::Markdown).is_none());
    }

    #[test]
    fn the_javascript_hint_installs_the_typescript_server() {
        let hint = install_hint(LanguageId::JavaScript).unwrap();
        assert_eq!(hint.program, "typescript-language-server");
        assert!(hint.command_line().contains("typescript-language-server"));
        assert!(hint.command.iter().any(|part| part == "typescript"));
    }

    #[test]
    fn a_language_is_offered_once_until_it_is_retried() {
        let mut mgr = LspManager::new(true);
        assert_eq!(mgr.offer_for(LanguageId::Rust), Some(LanguageId::Rust));
        // The manager keeps suggesting it until the caller confirms the dialog.
        assert_eq!(mgr.offer_for(LanguageId::Rust), Some(LanguageId::Rust));
        mgr.mark_offered(LanguageId::Rust);
        assert_eq!(mgr.offer_for(LanguageId::Rust), None);
        mgr.retry(LanguageId::Rust);
        assert_eq!(mgr.offer_for(LanguageId::Rust), Some(LanguageId::Rust));
    }

    #[test]
    fn a_decline_is_remembered_and_retry_forgets_it() {
        let mut mgr = LspManager::new(true);
        mgr.decline(LanguageId::Python);
        assert_eq!(mgr.offer_for(LanguageId::Python), None);
        mgr.retry(LanguageId::Python);
        assert_eq!(mgr.offer_for(LanguageId::Python), Some(LanguageId::Python));
    }

    #[test]
    fn languages_without_a_hint_are_never_offered() {
        let mut mgr = LspManager::new(true);
        assert_eq!(mgr.offer_for(LanguageId::Plain), None);
        assert_eq!(mgr.offer_for(LanguageId::Markdown), None);
    }
}
