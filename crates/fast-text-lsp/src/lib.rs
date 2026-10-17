//! A minimal, dependency-light Language Server Protocol client.
//!
//! Fast Text speaks LSP over stdio: it spawns a language server, performs the
//! handshake, syncs documents, and consumes diagnostics, completion, hover, and
//! go-to-definition results. The transport and protocol layers are decoupled,
//! so most of it is unit-testable without a real server.

mod client;
mod protocol;
mod transport;

pub use client::{path_to_uri, uri_to_path, LspClient, LspError};
pub use protocol::{
    CodeAction, CompletionItem, Diagnostic, DocumentSymbol, Location, LspPosition, LspRange,
    Severity, TextEdit, WorkspaceEdit,
};
pub use transport::{encode_message, write_message, Transport};
