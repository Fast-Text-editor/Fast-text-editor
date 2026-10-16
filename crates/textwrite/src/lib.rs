//! TextWrite — the Fast Text scripting language.
//!
//! TextWrite is a small, hand-written, dependency-free scripting language with
//! Rust-style braces and English operators (`is`, `and`, `not`, `repeat 3 times`).
//! This crate contains the whole pipeline:
//!
//! * [`lexer`] turns source text into tokens,
//! * [`parser`] builds an [`ast`],
//! * [`interp`] walks the tree, and
//! * [`stdlib`] provides the built-in functions.
//!
//! ```
//! use textwrite::Interpreter;
//!
//! let mut tw = Interpreter::new();
//! let value = tw.eval_expression("2 + 3 * 4").unwrap();
//! assert_eq!(value.to_display(), "14");
//! ```

pub mod ast;
pub mod error;
pub mod host;
pub mod interp;
pub mod lexer;
pub mod parser;
pub mod stdlib;
pub mod value;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;

pub use error::{Error, ErrorKind, Pos, Result, Span};
pub use host::{Host, HostRef};
pub use interp::Interpreter;
pub use value::Value;

/// The TextWrite language version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
