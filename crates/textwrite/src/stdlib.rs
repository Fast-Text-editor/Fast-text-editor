//! TextWrite's built-in functions.

use std::cmp::Ordering;

use crate::error::{Error, Result, Span};
use crate::interp::Interpreter;
use crate::value::{map_get, Value};

/// Every builtin's canonical name.
pub const BUILTINS: &[&str] = &[
    // Console / IO
    "print",
    "write",
    "input",
    // Types and conversions
    "len",
    "type",
    "str",
    "int",
    "float",
    "bool",
    // Math
    "abs",
    "min",
    "max",
    "floor",
    "ceil",
    "round",
    "sqrt",
    "pow",
    // Sequences
    "range",
    "push",
    "pop",
    "insert",
    "remove",
    "first",
    "last",
    "slice",
    "reverse",
    "sort",
    "sort_by",
    "join",
    "split",
    "contains",
    "index_of",
    "keys",
    "values",
    "has",
    "get",
    // Strings
    "upper",
    "lower",
    "trim",
    "replace",
    "starts_with",
    "ends_with",
    "chars",
    "lines",
    "words",
    // Higher order
    "map",
    "filter",
    "reduce",
    "each",
    "any",
    "all",
    // Errors / control
    "assert",
    "error",
    "exit",
    // Files (native only)
    "read_file",
    "write_file",
    "append_file",
    "exists",
];

/// Returns the canonical `'static` name for `name`, if it is a builtin.
pub fn builtin_static(name: &str) -> Option<&'static str> {
    BUILTINS.iter().copied().find(|b| *b == name)
}

/// All builtin names (for docs and error messages).
pub fn builtin_names() -> &'static [&'static str] {
    BUILTINS
}

fn need(args: &[Value], index: usize, fname: &str, span: Span) -> Result<Value> {
    args.get(index).cloned().ok_or_else(|| {
        Error::runtime(
            format!("`{fname}` expects at least {} argument(s)", index + 1),
            span,
        )
    })
}

fn as_number(v: &Value, fname: &str, span: Span) -> Result<f64> {
    v.as_f64().ok_or_else(|| {
        Error::runtime(
            format!("`{fname}` expects a number, found {}", v.type_name()),
            span,
        )
    })
}

fn as_index(v: &Value, fname: &str, span: Span) -> Result<i64> {
    v.as_i64().ok_or_else(|| {
        Error::runtime(
            format!("`{fname}` expects an index, found {}", v.type_name()),
            span,
        )
    })
}

fn as_text(v: &Value, fname: &str, span: Span) -> Result<String> {
    match v {
        Value::Str(s) => Ok(s.to_string()),
        other => Err(Error::runtime(
            format!("`{fname}` expects a string, found {}", other.type_name()),
            span,
        )),
    }
}

fn compare_values(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => a.partial_cmp(&b),
            _ => None,
        },
    }
}

fn slice_bounds(start: i64, end: Option<i64>, len: usize) -> (usize, usize) {
    let len = len as i64;
    let norm = |i: i64| -> i64 {
        if i < 0 {
            (i + len).max(0)
        } else {
            i.min(len)
        }
    };
    let start = norm(start);
    let end = norm(end.unwrap_or(len));
    if end < start {
        (start as usize, start as usize)
    } else {
        (start as usize, end as usize)
    }
}

impl Interpreter {
    pub(crate) fn call_builtin(
        &mut self,
        name: &str,
        args: Vec<Value>,
        span: Span,
    ) -> Result<Value> {
        match name {
            // --- Console / IO ---------------------------------------------
            "print" => {
                let text: Vec<String> = args.iter().map(Value::to_display).collect();
                self.write_out(&format!("{}\n", text.join(" ")));
                Ok(Value::None)
            }
            "write" => {
                let text: Vec<String> = args.iter().map(Value::to_display).collect();
                self.write_out(&text.join(""));
                Ok(Value::None)
            }
            "input" => {
                if let Some(prompt) = args.first() {
                    self.write_out(&prompt.to_display());
                }
                Ok(read_line_stdin().map(Value::str).unwrap_or(Value::None))
            }

            // --- Types / conversions --------------------------------------
            "len" => {
                let v = need(&args, 0, "len", span)?;
                let n = match &v {
                    Value::Str(s) => s.chars().count(),
                    Value::List(l) => l.borrow().len(),
                    Value::Map(m) => m.borrow().len(),
                    other => {
                        return Err(Error::runtime(
                            format!("`len` does not apply to a {}", other.type_name()),
                            span,
                        ))
                    }
                };
                Ok(Value::Int(n as i64))
            }
            "type" => {
                let v = need(&args, 0, "type", span)?;
                Ok(Value::str(v.type_name()))
            }
            "str" => {
                let v = need(&args, 0, "str", span)?;
                Ok(Value::str(v.to_display()))
            }
            "int" => {
                let v = need(&args, 0, "int", span)?;
                let n = match &v {
                    Value::Int(n) => *n,
                    Value::Float(f) => *f as i64,
                    Value::Bool(b) => i64::from(*b),
                    Value::Str(s) => s
                        .trim()
                        .parse::<i64>()
                        .or_else(|_| s.trim().parse::<f64>().map(|f| f as i64))
                        .map_err(|_| {
                            Error::runtime(format!("cannot convert `{s}` to an int"), span)
                        })?,
                    other => {
                        return Err(Error::runtime(
                            format!("cannot convert a {} to an int", other.type_name()),
                            span,
                        ))
                    }
                };
                Ok(Value::Int(n))
            }
            "float" => {
                let v = need(&args, 0, "float", span)?;
                let f = match &v {
                    Value::Str(s) => s.trim().parse::<f64>().map_err(|_| {
                        Error::runtime(format!("cannot convert `{s}` to a float"), span)
                    })?,
                    other => as_number(other, "float", span)?,
                };
                Ok(Value::Float(f))
            }
            "bool" => {
                let v = need(&args, 0, "bool", span)?;
                Ok(Value::Bool(v.is_truthy()))
            }

            // --- Math -----------------------------------------------------
            "abs" => {
                let v = need(&args, 0, "abs", span)?;
                match v {
                    Value::Int(n) => Ok(Value::Int(n.abs())),
                    _ => Ok(Value::Float(as_number(&v, "abs", span)?.abs())),
                }
            }
            "floor" | "ceil" | "round" => {
                let v = need(&args, 0, name, span)?;
                let n = as_number(&v, name, span)?;
                let r = match name {
                    "floor" => n.floor(),
                    "ceil" => n.ceil(),
                    _ => f64::round(n),
                };
                Ok(Value::Int(r as i64))
            }
            "sqrt" => {
                let v = need(&args, 0, "sqrt", span)?;
                let n = as_number(&v, "sqrt", span)?;
                if n < 0.0 {
                    return Err(Error::runtime("`sqrt` of a negative number", span));
                }
                Ok(Value::Float(n.sqrt()))
            }
            "pow" => {
                let base = as_number(&need(&args, 0, "pow", span)?, "pow", span)?;
                let exp = as_number(&need(&args, 1, "pow", span)?, "pow", span)?;
                Ok(Value::Float(base.powf(exp)))
            }
            "min" | "max" => {
                let values: Vec<Value> = if args.len() == 1 {
                    match &args[0] {
                        Value::List(l) => l.borrow().clone(),
                        other => vec![other.clone()],
                    }
                } else {
                    args.clone()
                };
                if values.is_empty() {
                    return Ok(Value::None);
                }
                let mut best = values[0].clone();
                for v in &values[1..] {
                    let ord = compare_values(v, &best).ok_or_else(|| {
                        Error::runtime(
                            format!(
                                "`{name}` cannot compare {} and {}",
                                v.type_name(),
                                best.type_name()
                            ),
                            span,
                        )
                    })?;
                    let take = if name == "min" {
                        ord == Ordering::Less
                    } else {
                        ord == Ordering::Greater
                    };
                    if take {
                        best = v.clone();
                    }
                }
                Ok(best)
            }

            // --- Sequences -------------------------------------------------
            "range" => {
                let (start, end, step) = match args.len() {
                    1 => (0, as_index(&args[0], "range", span)?, 1),
                    2 => (
                        as_index(&args[0], "range", span)?,
                        as_index(&args[1], "range", span)?,
                        1,
                    ),
                    3 => (
                        as_index(&args[0], "range", span)?,
                        as_index(&args[1], "range", span)?,
                        as_index(&args[2], "range", span)?,
                    ),
                    _ => return Err(Error::runtime("`range` expects 1, 2 or 3 arguments", span)),
                };
                if step == 0 {
                    return Err(Error::runtime("`range` step cannot be zero", span));
                }
                let mut out = Vec::new();
                let mut i = start;
                let mut guard: u64 = 0;
                while (step > 0 && i < end) || (step < 0 && i > end) {
                    out.push(Value::Int(i));
                    i += step;
                    guard += 1;
                    if guard > 10_000_000 {
                        return Err(Error::runtime("`range` is too large", span));
                    }
                }
                Ok(Value::list(out))
            }
            "push" => {
                let list = need(&args, 0, "push", span)?;
                let value = need(&args, 1, "push", span)?;
                match &list {
                    Value::List(l) => {
                        l.borrow_mut().push(value);
                        Ok(list)
                    }
                    other => Err(Error::runtime(
                        format!("`push` expects a list, found {}", other.type_name()),
                        span,
                    )),
                }
            }
            "pop" => {
                let list = need(&args, 0, "pop", span)?;
                match &list {
                    Value::List(l) => {
                        let mut l = l.borrow_mut();
                        let idx = match args.get(1) {
                            Some(v) => {
                                let i = as_index(v, "pop", span)?;
                                if i < 0 {
                                    (l.len() as i64 + i).max(0) as usize
                                } else {
                                    i as usize
                                }
                            }
                            None => l.len().saturating_sub(1),
                        };
                        if idx >= l.len() {
                            Ok(Value::None)
                        } else {
                            Ok(l.remove(idx))
                        }
                    }
                    other => Err(Error::runtime(
                        format!("`pop` expects a list, found {}", other.type_name()),
                        span,
                    )),
                }
            }
            "insert" => {
                let list = need(&args, 0, "insert", span)?;
                let index = as_index(&need(&args, 1, "insert", span)?, "insert", span)?;
                let value = need(&args, 2, "insert", span)?;
                match &list {
                    Value::List(l) => {
                        let i = index.clamp(0, l.borrow().len() as i64) as usize;
                        l.borrow_mut().insert(i, value);
                        Ok(list)
                    }
                    other => Err(Error::runtime(
                        format!("`insert` expects a list, found {}", other.type_name()),
                        span,
                    )),
                }
            }
            "remove" => {
                let list = need(&args, 0, "remove", span)?;
                match &list {
                    Value::List(l) => {
                        let mut l = l.borrow_mut();
                        let i = as_index(&need(&args, 1, "remove", span)?, "remove", span)?;
                        let i = if i < 0 { l.len() as i64 + i } else { i };
                        if i < 0 || i as usize >= l.len() {
                            Ok(Value::None)
                        } else {
                            Ok(l.remove(i as usize))
                        }
                    }
                    other => Err(Error::runtime(
                        format!("`remove` expects a list, found {}", other.type_name()),
                        span,
                    )),
                }
            }
            "first" | "last" => {
                let seq = need(&args, 0, name, span)?;
                match &seq {
                    Value::List(l) => {
                        let l = l.borrow();
                        if l.is_empty() {
                            Ok(Value::None)
                        } else if name == "first" {
                            Ok(l[0].clone())
                        } else {
                            Ok(l[l.len() - 1].clone())
                        }
                    }
                    Value::Str(s) => {
                        let chars: Vec<char> = s.chars().collect();
                        if chars.is_empty() {
                            Ok(Value::None)
                        } else if name == "first" {
                            Ok(Value::str(chars[0].to_string()))
                        } else {
                            Ok(Value::str(chars[chars.len() - 1].to_string()))
                        }
                    }
                    other => Err(Error::runtime(
                        format!("`{name}` does not apply to a {}", other.type_name()),
                        span,
                    )),
                }
            }
            "slice" => {
                let seq = need(&args, 0, "slice", span)?;
                let start = as_index(&need(&args, 1, "slice", span)?, "slice", span)?;
                let end = match args.get(2) {
                    Some(v) => Some(as_index(v, "slice", span)?),
                    None => None,
                };
                match &seq {
                    Value::List(items) => {
                        let items = items.borrow();
                        let (a, b) = slice_bounds(start, end, items.len());
                        Ok(Value::list(items[a..b].to_vec()))
                    }
                    Value::Str(s) => {
                        let chars: Vec<char> = s.chars().collect();
                        let (a, b) = slice_bounds(start, end, chars.len());
                        Ok(Value::str(chars[a..b].iter().collect::<String>()))
                    }
                    other => Err(Error::runtime(
                        format!("`slice` does not apply to a {}", other.type_name()),
                        span,
                    )),
                }
            }
            "reverse" => {
                let seq = need(&args, 0, "reverse", span)?;
                match &seq {
                    Value::List(l) => {
                        let mut items = l.borrow().clone();
                        items.reverse();
                        Ok(Value::list(items))
                    }
                    Value::Str(s) => Ok(Value::str(s.chars().rev().collect::<String>())),
                    other => Err(Error::runtime(
                        format!("`reverse` does not apply to a {}", other.type_name()),
                        span,
                    )),
                }
            }
            "sort" => {
                let seq = need(&args, 0, "sort", span)?;
                match &seq {
                    Value::List(l) => {
                        let mut items = l.borrow().clone();
                        items.sort_by(|a, b| compare_values(a, b).unwrap_or(Ordering::Equal));
                        Ok(Value::list(items))
                    }
                    other => Err(Error::runtime(
                        format!("`sort` expects a list, found {}", other.type_name()),
                        span,
                    )),
                }
            }
            "sort_by" => {
                let seq = need(&args, 0, "sort_by", span)?;
                let func = need(&args, 1, "sort_by", span)?;
                let items = match &seq {
                    Value::List(l) => l.borrow().clone(),
                    other => {
                        return Err(Error::runtime(
                            format!("`sort_by` expects a list, found {}", other.type_name()),
                            span,
                        ))
                    }
                };
                let mut items = items;
                // Insertion sort so the comparator can call back into the interpreter.
                for i in 1..items.len() {
                    let mut j = i;
                    while j > 0 {
                        let order = self.call_value(
                            &func,
                            vec![items[j - 1].clone(), items[j].clone()],
                            span,
                        )?;
                        let swap = order.as_i64().map(|n| n > 0).unwrap_or(false);
                        if swap {
                            items.swap(j - 1, j);
                            j -= 1;
                        } else {
                            break;
                        }
                    }
                }
                Ok(Value::list(items))
            }
            "join" => {
                let list = need(&args, 0, "join", span)?;
                let sep = as_text(&need(&args, 1, "join", span)?, "join", span)?;
                match &list {
                    Value::List(l) => {
                        let parts: Vec<String> = l.borrow().iter().map(Value::to_display).collect();
                        Ok(Value::str(parts.join(&sep)))
                    }
                    other => Err(Error::runtime(
                        format!("`join` expects a list, found {}", other.type_name()),
                        span,
                    )),
                }
            }
            "split" => {
                let text = as_text(&need(&args, 0, "split", span)?, "split", span)?;
                let sep = as_text(&need(&args, 1, "split", span)?, "split", span)?;
                let parts: Vec<Value> = if sep.is_empty() {
                    text.chars().map(|c| Value::str(c.to_string())).collect()
                } else {
                    text.split(&sep).map(Value::str).collect()
                };
                Ok(Value::list(parts))
            }
            "contains" | "has" => {
                let container = need(&args, 0, name, span)?;
                let item = need(&args, 1, name, span)?;
                let found = match &container {
                    Value::List(l) => l.borrow().iter().any(|v| v.deep_eq(&item)),
                    Value::Str(s) => s.contains(&item.to_display()),
                    Value::Map(m) => m
                        .borrow()
                        .iter()
                        .any(|(k, _)| k.as_ref() == item.to_display()),
                    other => {
                        return Err(Error::runtime(
                            format!("`{name}` does not apply to a {}", other.type_name()),
                            span,
                        ))
                    }
                };
                Ok(Value::Bool(found))
            }
            "index_of" => {
                let container = need(&args, 0, "index_of", span)?;
                let item = need(&args, 1, "index_of", span)?;
                let index = match &container {
                    Value::List(l) => l
                        .borrow()
                        .iter()
                        .position(|v| v.deep_eq(&item))
                        .map(|i| i as i64)
                        .unwrap_or(-1),
                    Value::Str(s) => s
                        .find(&item.to_display())
                        .map(|byte| s[..byte].chars().count() as i64)
                        .unwrap_or(-1),
                    other => {
                        return Err(Error::runtime(
                            format!("`index_of` does not apply to a {}", other.type_name()),
                            span,
                        ))
                    }
                };
                Ok(Value::Int(index))
            }
            "keys" => {
                let map = need(&args, 0, "keys", span)?;
                match &map {
                    Value::Map(m) => Ok(Value::list(
                        m.borrow()
                            .iter()
                            .map(|(k, _)| Value::str(k.to_string()))
                            .collect(),
                    )),
                    other => Err(Error::runtime(
                        format!("`keys` expects a map, found {}", other.type_name()),
                        span,
                    )),
                }
            }
            "values" => {
                let map = need(&args, 0, "values", span)?;
                match &map {
                    Value::Map(m) => Ok(Value::list(
                        m.borrow().iter().map(|(_, v)| v.clone()).collect(),
                    )),
                    other => Err(Error::runtime(
                        format!("`values` expects a map, found {}", other.type_name()),
                        span,
                    )),
                }
            }
            "get" => {
                let container = need(&args, 0, "get", span)?;
                let key = need(&args, 1, "get", span)?;
                let default = args.get(2).cloned().unwrap_or(Value::None);
                match &container {
                    Value::Map(m) => Ok(map_get(m, &key.to_display()).unwrap_or(default)),
                    Value::List(l) => {
                        let items = l.borrow();
                        match key.as_i64() {
                            Some(raw) => {
                                let len = items.len() as i64;
                                let i = if raw < 0 { raw + len } else { raw };
                                if i < 0 || i >= len {
                                    Ok(default)
                                } else {
                                    Ok(items[i as usize].clone())
                                }
                            }
                            None => Ok(default),
                        }
                    }
                    other => Err(Error::runtime(
                        format!("`get` does not apply to a {}", other.type_name()),
                        span,
                    )),
                }
            }

            // --- Strings ---------------------------------------------------
            "upper" => Ok(Value::str(
                as_text(&need(&args, 0, "upper", span)?, "upper", span)?.to_uppercase(),
            )),
            "lower" => Ok(Value::str(
                as_text(&need(&args, 0, "lower", span)?, "lower", span)?.to_lowercase(),
            )),
            "trim" => Ok(Value::str(
                as_text(&need(&args, 0, "trim", span)?, "trim", span)?.trim(),
            )),
            "replace" => {
                let text = as_text(&need(&args, 0, "replace", span)?, "replace", span)?;
                let from = as_text(&need(&args, 1, "replace", span)?, "replace", span)?;
                let to = as_text(&need(&args, 2, "replace", span)?, "replace", span)?;
                Ok(Value::str(text.replace(&from, &to)))
            }
            "starts_with" | "ends_with" => {
                let text = as_text(&need(&args, 0, name, span)?, name, span)?;
                let affix = as_text(&need(&args, 1, name, span)?, name, span)?;
                let ok = if name == "starts_with" {
                    text.starts_with(&affix)
                } else {
                    text.ends_with(&affix)
                };
                Ok(Value::Bool(ok))
            }
            "chars" => {
                let text = as_text(&need(&args, 0, "chars", span)?, "chars", span)?;
                Ok(Value::list(
                    text.chars().map(|c| Value::str(c.to_string())).collect(),
                ))
            }
            "lines" => {
                let text = as_text(&need(&args, 0, "lines", span)?, "lines", span)?;
                Ok(Value::list(text.lines().map(Value::str).collect()))
            }

            // --- Functional -----------------------------------------------
            "map" => {
                let seq = need(&args, 0, "map", span)?;
                let func = need(&args, 1, "map", span)?;
                let items = sequence_items(&seq, "map", span)?;
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.call_value(&func, vec![item], span)?);
                }
                Ok(Value::list(out))
            }
            "filter" => {
                let seq = need(&args, 0, "filter", span)?;
                let func = need(&args, 1, "filter", span)?;
                let items = sequence_items(&seq, "filter", span)?;
                let mut out = Vec::new();
                for item in items {
                    if self
                        .call_value(&func, vec![item.clone()], span)?
                        .is_truthy()
                    {
                        out.push(item);
                    }
                }
                Ok(Value::list(out))
            }
            "each" => {
                let seq = need(&args, 0, "each", span)?;
                let func = need(&args, 1, "each", span)?;
                for item in sequence_items(&seq, "each", span)? {
                    self.call_value(&func, vec![item], span)?;
                }
                Ok(Value::None)
            }
            "reduce" => {
                let seq = need(&args, 0, "reduce", span)?;
                let func = need(&args, 1, "reduce", span)?;
                let items = sequence_items(&seq, "reduce", span)?;
                let mut acc = args.get(2).cloned().unwrap_or(Value::None);
                for item in items {
                    acc = self.call_value(&func, vec![acc, item], span)?;
                }
                Ok(acc)
            }
            "any" | "all" => {
                let seq = need(&args, 0, name, span)?;
                let func = need(&args, 1, name, span)?;
                let mut result = name == "all";
                for item in sequence_items(&seq, name, span)? {
                    let ok = self.call_value(&func, vec![item], span)?.is_truthy();
                    if name == "any" && ok {
                        result = true;
                        break;
                    }
                    if name == "all" && !ok {
                        result = false;
                        break;
                    }
                }
                Ok(Value::Bool(result))
            }

            // --- Asserts / errors -----------------------------------------
            "assert" => {
                let cond = need(&args, 0, "assert", span)?;
                if cond.is_truthy() {
                    Ok(Value::None)
                } else {
                    let msg = args
                        .get(1)
                        .map(Value::to_display)
                        .unwrap_or_else(|| "assertion failed".to_string());
                    Err(Error::runtime(msg, span))
                }
            }
            "error" => {
                let msg = args
                    .first()
                    .map(Value::to_display)
                    .unwrap_or_else(|| "error".to_string());
                Err(Error::runtime(msg, span))
            }
            "exit" => {
                let code = args.first().and_then(Value::as_i64).unwrap_or(0) as i32;
                std::process::exit(code);
            }

            // --- File IO (native only) ------------------------------------
            #[cfg(not(target_arch = "wasm32"))]
            "read_file" => {
                let path = as_text(&need(&args, 0, "read_file", span)?, "read_file", span)?;
                std::fs::read_to_string(&path)
                    .map(Value::str)
                    .map_err(|e| Error::runtime(format!("cannot read {path}: {e}"), span))
            }
            #[cfg(not(target_arch = "wasm32"))]
            "write_file" => {
                let path = as_text(&need(&args, 0, "write_file", span)?, "write_file", span)?;
                let text = need(&args, 1, "write_file", span)?.to_display();
                std::fs::write(&path, text)
                    .map_err(|e| Error::runtime(format!("cannot write {path}: {e}"), span))?;
                Ok(Value::None)
            }
            #[cfg(not(target_arch = "wasm32"))]
            "append_file" => {
                use std::io::Write as _;
                let path = as_text(&need(&args, 0, "append_file", span)?, "append_file", span)?;
                let text = need(&args, 1, "append_file", span)?.to_display();
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|e| Error::runtime(format!("cannot open {path}: {e}"), span))?;
                file.write_all(text.as_bytes())
                    .map_err(|e| Error::runtime(format!("cannot write {path}: {e}"), span))?;
                Ok(Value::None)
            }
            #[cfg(not(target_arch = "wasm32"))]
            "exists" => {
                let path = as_text(&need(&args, 0, "exists", span)?, "exists", span)?;
                Ok(Value::Bool(std::path::Path::new(&path).exists()))
            }

            other => Err(Error::runtime(format!("unknown builtin `{other}`"), span)),
        }
    }
}

fn sequence_items(seq: &Value, fname: &str, span: Span) -> Result<Vec<Value>> {
    match seq {
        Value::List(l) => Ok(l.borrow().clone()),
        Value::Str(s) => Ok(s.chars().map(|c| Value::str(c.to_string())).collect()),
        Value::Map(m) => Ok(m
            .borrow()
            .iter()
            .map(|(k, _)| Value::str(k.to_string()))
            .collect()),
        other => Err(Error::runtime(
            format!(
                "`{fname}` expects a list or string, found {}",
                other.type_name()
            ),
            span,
        )),
    }
}

fn read_line_stdin() -> Option<String> {
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => {
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            Some(line)
        }
        Err(_) => None,
    }
}
