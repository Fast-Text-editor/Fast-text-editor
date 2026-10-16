//! Embedding hook: let a host program expose native functions to a script.
//!
//! TextWrite itself stays dependency-free, but it is often embedded in a larger
//! program (an editor, a game, a web page) that wants to expose its own
//! functions to scripts. Implement [`Host`] and install it with
//! [`Interpreter::set_host`](crate::Interpreter::set_host); host functions are
//! then callable exactly like builtins and take precedence over them.
//!
//! ```no_run
//! use std::cell::RefCell;
//! use std::rc::Rc;
//! use textwrite::{Host, Interpreter, Span, Value};
//!
//! struct Greeter;
//! impl Host for Greeter {
//!     fn functions(&self) -> &'static [&'static str] {
//!         &["greet"]
//!     }
//!     fn call(&mut self, name: &str, args: Vec<Value>, span: Span) -> textwrite::Result<Value> {
//!         let _ = span;
//!         match name {
//!             "greet" => Ok(Value::str(format!("hello, {}", args[0].to_display()))),
//!             _ => unreachable!(),
//!         }
//!     }
//! }
//!
//! let mut tw = Interpreter::new();
//! tw.set_host(Rc::new(RefCell::new(Greeter)));
//! let v = tw.eval_expression("greet(\"world\")").unwrap();
//! assert_eq!(v.to_display(), "hello, world");
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::{Result, Span};
use crate::value::Value;

/// A provider of native functions available to scripts.
pub trait Host {
    /// The names of every function this host provides.
    ///
    /// The list must be `'static` and stable for the host's lifetime; it is
    /// consulted on each name lookup.
    fn functions(&self) -> &'static [&'static str];

    /// Invokes the named host function with already-evaluated arguments.
    ///
    /// `name` is guaranteed to be one of [`Host::functions`].
    fn call(&mut self, name: &str, args: Vec<Value>, span: Span) -> Result<Value>;
}

/// A shared reference to a host.
pub type HostRef = Rc<RefCell<dyn Host>>;
