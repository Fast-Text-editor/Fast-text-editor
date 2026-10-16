//! The Fast Text plugin system.
//!
//! Plugins are small [Rhai](https://rhai.rs) scripts that register named
//! commands and keybindings. They describe edits as [`EditorAction`]s rather
//! than touching the editor directly, so they are safe, sandboxed, and easy to
//! apply from any frontend.
//!
//! Example plugin:
//!
//! ```rhai
//! // Wrap the selection in quotes.
//! fn quote_selection() {
//!     insert("\"" + selected_text + "\"");
//! }
//! command_with_desc("Wrap in quotes", "Surround the selection with \"", "quote_selection");
//! bind("ctrl+shift+q", "Wrap in quotes");
//! ```
//!
//! Globals available to scripts: `cursor`, `selection_start`, `selection_end`,
//! `has_selection`, `selected_text`, `line`, `col`, `doc_len`, `line_count`,
//! `file_name`, `language`.

mod action;
mod host;
mod key;

pub use action::{CursorStyle, EditorAction};
pub use host::{CommandInfo, EditorContext, PluginHost};
pub use key::{KeyName, KeySpec};
