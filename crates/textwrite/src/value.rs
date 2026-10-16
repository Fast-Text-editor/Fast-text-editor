//! Runtime values for TextWrite.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::Stmt;
use crate::interp::EnvRef;

/// A user-defined function, with the environment it closed over.
pub struct Function {
    pub name: Option<String>,
    pub params: Vec<String>,
    pub body: Rc<Vec<Stmt>>,
    pub env: EnvRef,
}

impl std::fmt::Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.name {
            Some(n) => write!(f, "<fn {n}>"),
            None => write!(f, "<fn>"),
        }
    }
}

/// Insertion-ordered map entries. Lookup is linear, which is plenty here.
pub type MapData = Vec<(Rc<str>, Value)>;

/// A TextWrite runtime value.
#[derive(Clone)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    List(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<MapData>>),
    Func(Rc<Function>),
    Builtin(&'static str),
}

impl Value {
    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(Rc::from(s.into()))
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(items)))
    }

    pub fn map(entries: MapData) -> Value {
        Value::Map(Rc::new(RefCell::new(entries)))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::None => "none",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::Func(_) | Value::Builtin(_) => "function",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.borrow().is_empty(),
            Value::Map(m) => !m.borrow().is_empty(),
            Value::Func(_) | Value::Builtin(_) => true,
        }
    }

    /// Structural equality, backing the `is` / `==` operators.
    pub fn deep_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::None, Value::None) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => {
                (*a as f64) == *b
            }
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::List(a), Value::List(b)) => {
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                let a = a.borrow();
                let b = b.borrow();
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.deep_eq(y))
            }
            (Value::Map(a), Value::Map(b)) => {
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                let a = a.borrow();
                let b = b.borrow();
                a.len() == b.len()
                    && a.iter().all(|(k, v)| {
                        b.iter()
                            .find(|(k2, _)| k == k2)
                            .is_some_and(|(_, v2)| v.deep_eq(v2))
                    })
            }
            (Value::Func(a), Value::Func(b)) => Rc::ptr_eq(a, b),
            (Value::Builtin(a), Value::Builtin(b)) => a == b,
            _ => false,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A debug-style rendering, quoting strings (used inside lists and maps).
    pub fn repr(&self) -> String {
        match self {
            Value::None => "none".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(f) => format_float(*f),
            Value::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            Value::List(items) => {
                let items = items.borrow();
                let inner: Vec<String> = items.iter().map(Value::repr).collect();
                format!("[{}]", inner.join(", "))
            }
            Value::Map(map) => {
                let map = map.borrow();
                let inner: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.repr()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            Value::Func(f) => match &f.name {
                Some(n) => format!("<fn {n}>"),
                None => "<fn>".to_string(),
            },
            Value::Builtin(n) => format!("<builtin {n}>"),
        }
    }

    /// Rendering used by `print` and string interpolation: strings unquoted.
    pub fn to_display(&self) -> String {
        match self {
            Value::Str(s) => s.to_string(),
            other => other.repr(),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_display())
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.repr())
    }
}

/// Formats a float without a trailing `.0` for integral values.
pub fn format_float(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e15 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

// --- Map helpers -----------------------------------------------------------

pub fn map_get(map: &Rc<RefCell<MapData>>, key: &str) -> Option<Value> {
    map.borrow()
        .iter()
        .find(|(k, _)| &**k == key)
        .map(|(_, v)| v.clone())
}

pub fn map_contains(map: &Rc<RefCell<MapData>>, key: &str) -> bool {
    map.borrow().iter().any(|(k, _)| &**k == key)
}

pub fn map_insert(map: &Rc<RefCell<MapData>>, key: &str, value: Value) {
    let mut map = map.borrow_mut();
    if let Some(slot) = map.iter_mut().find(|(k, _)| &**k == key) {
        *slot = (slot.0.clone(), value);
    } else {
        map.push((Rc::from(key), value));
    }
}
