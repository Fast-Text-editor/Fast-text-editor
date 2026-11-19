//! The Fast Text core engine.
//!
//! A fast, `rope`-backed text buffer with editing, undo/redo, selections,
//! cursor movement, and search. Frontends (TUI, GUI, web) are thin shells
//! over this crate, so the editor behaves identically everywhere.

mod buffer;
mod document;
mod edit;
mod error;
pub mod fuzzy;
mod history;
mod position;
mod search;
mod selection;
pub mod undo;

pub use buffer::{Buffer, LineEnding};
pub use document::Document;
pub use edit::Edit;
pub use error::{Error, Result};
pub use history::{History, Transaction, TxnKind};
pub use position::Position;
pub use search::{Match, SearchDirection};
pub use selection::Selection;

/// Re-export of the rope type used for the backing store.
pub use ropey::Rope;
