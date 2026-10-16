//! The TextWrite tree-walking interpreter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

use crate::ast::*;
use crate::error::{Error, Result, Span};
use crate::host::HostRef;
use crate::parser;
use crate::value::{map_get, map_insert, Function, MapData, Value};

/// A lexical scope with an optional parent.
pub type EnvRef = Rc<RefCell<Env>>;

pub struct Env {
    vars: std::collections::HashMap<String, Value>,
    parent: Option<EnvRef>,
}

impl Env {
    fn root() -> EnvRef {
        Rc::new(RefCell::new(Env {
            vars: std::collections::HashMap::new(),
            parent: None,
        }))
    }

    fn child(parent: &EnvRef) -> EnvRef {
        Rc::new(RefCell::new(Env {
            vars: std::collections::HashMap::new(),
            parent: Some(parent.clone()),
        }))
    }
}

fn env_get(env: &EnvRef, name: &str) -> Option<Value> {
    let (found, parent) = {
        let e = env.borrow();
        (e.vars.get(name).cloned(), e.parent.clone())
    };
    match found {
        Some(v) => Some(v),
        None => parent.and_then(|p| env_get(&p, name)),
    }
}

fn env_define(env: &EnvRef, name: &str, value: Value) {
    env.borrow_mut().vars.insert(name.to_string(), value);
}

/// Assigns the nearest existing binding; false when the name is undefined.
fn env_assign(env: &EnvRef, name: &str, value: Value) -> bool {
    let parent = {
        let mut e = env.borrow_mut();
        if let Some(slot) = e.vars.get_mut(name) {
            *slot = value;
            return true;
        }
        e.parent.clone()
    };
    match parent {
        Some(p) => env_assign(&p, name, value),
        None => false,
    }
}

/// The names a module defines at its top level, i.e. its exports.
fn module_exports(env: &EnvRef) -> MapData {
    env.borrow()
        .vars
        .iter()
        .map(|(k, v)| (Rc::from(k.as_str()), v.clone()))
        .collect()
}

enum Flow {
    Normal,
    Return(Value),
    Break,
    Continue,
}

/// A TextWrite interpreter. Globals persist across `run`/`eval` calls.
pub struct Interpreter {
    globals: EnvRef,
    out: Box<dyn Write>,
    host: Option<HostRef>,
    depth: usize,
    max_depth: usize,
    /// Built-in module sources, keyed by the name used in `import`.
    modules: HashMap<String, String>,
    /// Directory that relative `import "./file.tw"` paths resolve against.
    module_base: Option<PathBuf>,
    /// Resolved module exports, keyed by canonical name/path.
    module_cache: HashMap<String, Rc<MapData>>,
    /// Modules currently being imported (for cycle detection).
    importing: Vec<String>,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            globals: Env::root(),
            out: Box::new(std::io::stdout()),
            host: None,
            depth: 0,
            max_depth: 256,
            modules: HashMap::new(),
            module_base: None,
            module_cache: HashMap::new(),
            importing: Vec::new(),
        }
    }

    pub fn with_output(out: Box<dyn Write>) -> Self {
        Self { out, ..Self::new() }
    }

    /// Installs a [`Host`](crate::host::Host) providing native functions.
    ///
    /// Only one host may be installed; setting a new one replaces the old.
    pub fn set_host(&mut self, host: HostRef) {
        self.host = Some(host);
    }

    /// True when a host function with this name is available.
    pub fn has_host_function(&self, name: &str) -> bool {
        match &self.host {
            Some(host) => host.borrow().functions().contains(&name),
            None => false,
        }
    }

    /// Defines (or overwrites) a global variable.
    ///
    /// This is how an embedder injects context — file names, cursor positions,
    /// configuration — into a script's environment before running it.
    pub fn set_global(&mut self, name: &str, value: Value) {
        env_define(&self.globals, name, value);
    }

    /// Reads a global variable, if it is defined.
    pub fn get_global(&self, name: &str) -> Option<Value> {
        env_get(&self.globals, name)
    }

    // --- Modules -----------------------------------------------------------

    /// Registers a built-in module that `import "name"` can load.
    ///
    /// Embedders use this to ship packages (for example the Fast Text
    /// `fast-text/automator` package) without touching the filesystem.
    pub fn register_module(&mut self, name: &str, source: &str) {
        self.modules.insert(name.to_string(), source.to_string());
    }

    /// Sets the directory that relative `import "./lib.tw"` paths resolve
    /// against. Defaults to the current working directory.
    pub fn set_module_base(&mut self, dir: impl Into<PathBuf>) {
        self.module_base = Some(dir.into());
    }

    /// Parses `source` and checks that every `import` it mentions resolves to
    /// a readable module, recursively. This is the "compile" half of
    /// compile-and-run: it reports errors without executing anything.
    pub fn compile(&mut self, source: &str) -> Result<()> {
        let stmts = parser::parse(source)?;
        let mut seen = std::collections::HashSet::new();
        self.check_imports(&stmts, &mut seen)
    }

    fn check_imports(
        &mut self,
        stmts: &[Stmt],
        seen: &mut std::collections::HashSet<String>,
    ) -> Result<()> {
        for stmt in stmts {
            match stmt {
                Stmt::Import { spec, span, .. } => {
                    let Some((key, source, dir)) = self.load_module_source(spec) else {
                        return Err(Error::runtime(
                            format!("cannot find module `{spec}`"),
                            *span,
                        ));
                    };
                    if !seen.insert(key) {
                        continue;
                    }
                    let nested = parser::parse(&source)?;
                    let saved = self.module_base.clone();
                    if dir.is_some() {
                        self.module_base = dir;
                    }
                    let result = self.check_imports(&nested, seen);
                    self.module_base = saved;
                    result?;
                }
                Stmt::If {
                    branches,
                    else_body,
                    ..
                } => {
                    for (_, body) in branches {
                        self.check_imports(body, seen)?;
                    }
                    if let Some(body) = else_body {
                        self.check_imports(body, seen)?;
                    }
                }
                Stmt::While { body, .. }
                | Stmt::For { body, .. }
                | Stmt::Repeat { body, .. }
                | Stmt::FnDecl { body, .. } => self.check_imports(body, seen)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Loads a module's exported bindings, running it once and caching the
    /// result. `import "x"` merges them; `import "x" as ns` binds a namespace.
    fn resolve_module(&mut self, spec: &str, span: Span) -> Result<Rc<MapData>> {
        if let Some(cached) = self.module_cache.get(spec) {
            return Ok(cached.clone());
        }
        let Some((key, source, dir)) = self.load_module_source(spec) else {
            return Err(Error::runtime(format!("cannot find module `{spec}`"), span));
        };
        if let Some(cached) = self.module_cache.get(&key) {
            return Ok(cached.clone());
        }
        if self.importing.iter().any(|m| m == &key) {
            return Err(Error::runtime(format!("circular import of `{spec}`"), span));
        }
        let stmts = parser::parse(&source)?;

        self.importing.push(key.clone());
        let saved = self.module_base.clone();
        if dir.is_some() {
            self.module_base = dir;
        }
        let module_env = Env::child(&self.globals);
        let ran = self.exec_block(&module_env, &stmts);
        self.module_base = saved;
        self.importing.pop();
        match ran? {
            Flow::Normal | Flow::Return(_) => {}
            Flow::Break | Flow::Continue => {
                return Err(Error::runtime(
                    "`break`/`continue` used outside of a loop",
                    span,
                ))
            }
        }

        let exports = Rc::new(module_exports(&module_env));
        self.module_cache.insert(spec.to_string(), exports.clone());
        self.module_cache.insert(key, exports.clone());
        Ok(exports)
    }

    /// Finds a module's source: a built-in by name, else a `.tw` file relative
    /// to the module base (or the working directory).
    fn load_module_source(&self, spec: &str) -> Option<(String, String, Option<PathBuf>)> {
        if let Some(source) = self.modules.get(spec) {
            return Some((spec.to_string(), source.clone(), None));
        }
        let base = self
            .module_base
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let mut candidates = vec![base.join(spec)];
        if !spec.ends_with(".tw") {
            candidates.push(base.join(format!("{spec}.tw")));
        }
        for path in candidates {
            if let Ok(source) = std::fs::read_to_string(&path) {
                let key = std::fs::canonicalize(&path)
                    .unwrap_or_else(|_| path.clone())
                    .to_string_lossy()
                    .into_owned();
                let dir = path.parent().map(PathBuf::from);
                return Some((key, source, dir));
            }
        }
        None
    }

    /// Executes a program in the global scope.
    pub fn run(&mut self, source: &str) -> Result<()> {
        let stmts = parser::parse(source)?;
        self.run_parsed(&stmts)
    }

    /// Executes already-parsed statements in the global scope.
    pub fn run_parsed(&mut self, stmts: &[Stmt]) -> Result<()> {
        let env = self.globals.clone();
        let _ = self.exec_block(&env, stmts)?;
        Ok(())
    }

    /// Evaluates an already-parsed expression in the global scope.
    pub fn eval_parsed(&mut self, expr: &Expr) -> Result<Value> {
        let env = self.globals.clone();
        self.eval_expr(&env, expr)
    }

    /// Evaluates one expression in the global scope.
    pub fn eval_expression(&mut self, source: &str) -> Result<Value> {
        let expr = parser::parse_expression(source)?;
        self.eval_parsed(&expr)
    }

    /// Runs `source` as an expression when possible, else as a program.
    pub fn eval(&mut self, source: &str) -> Result<Value> {
        match parser::parse_expression(source) {
            Ok(expr) => self.eval_parsed(&expr),
            Err(_) => {
                self.run(source)?;
                Ok(Value::None)
            }
        }
    }

    /// Writes to the interpreter's output stream (used by builtins).
    pub(crate) fn write_out(&mut self, text: &str) {
        let _ = self.out.write_all(text.as_bytes());
        let _ = self.out.flush();
    }

    // --- Statements --------------------------------------------------------

    fn exec_block(&mut self, env: &EnvRef, stmts: &[Stmt]) -> Result<Flow> {
        for stmt in stmts {
            match self.exec_stmt(env, stmt)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_stmt(&mut self, env: &EnvRef, stmt: &Stmt) -> Result<Flow> {
        match stmt {
            Stmt::Let { name, value, .. } => {
                let v = self.eval_expr(env, value)?;
                env_define(env, name, v);
                Ok(Flow::Normal)
            }
            Stmt::Assign {
                target,
                op,
                value,
                span,
            } => {
                let rhs = self.eval_expr(env, value)?;
                let new_value = match op {
                    AssignOp::Set => rhs,
                    other => {
                        let current = self.eval_expr(env, target)?;
                        let bin = match other {
                            AssignOp::Add => BinOp::Add,
                            AssignOp::Sub => BinOp::Sub,
                            AssignOp::Mul => BinOp::Mul,
                            AssignOp::Div => BinOp::Div,
                            AssignOp::Rem => BinOp::Rem,
                            AssignOp::Set => unreachable!(),
                        };
                        let _ = span;
                        binop(bin, current, rhs, target.span())?
                    }
                };
                self.assign(env, target, new_value)?;
                Ok(Flow::Normal)
            }
            Stmt::Expr(expr) => {
                self.eval_expr(env, expr)?;
                Ok(Flow::Normal)
            }
            Stmt::FnDecl {
                name, params, body, ..
            } => {
                let func = Value::Func(Rc::new(Function {
                    name: Some(name.clone()),
                    params: params.clone(),
                    body: Rc::new(body.clone()),
                    env: env.clone(),
                }));
                env_define(env, name, func);
                Ok(Flow::Normal)
            }
            Stmt::If {
                branches,
                else_body,
                ..
            } => {
                for (cond, body) in branches {
                    if self.eval_expr(env, cond)?.is_truthy() {
                        let child = Env::child(env);
                        return self.exec_block(&child, body);
                    }
                }
                match else_body {
                    Some(body) => {
                        let child = Env::child(env);
                        self.exec_block(&child, body)
                    }
                    None => Ok(Flow::Normal),
                }
            }
            Stmt::While { cond, body, .. } => {
                while self.eval_expr(env, cond)?.is_truthy() {
                    let child = Env::child(env);
                    match self.exec_block(&child, body)? {
                        Flow::Normal => {}
                        Flow::Break => break,
                        Flow::Continue => continue,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::For {
                name,
                iter,
                body,
                span,
            } => {
                let subject = self.eval_expr(env, iter)?;
                for item in self.iter_values(&subject, *span)? {
                    let child = Env::child(env);
                    env_define(&child, name, item);
                    match self.exec_block(&child, body)? {
                        Flow::Normal => {}
                        Flow::Break => break,
                        Flow::Continue => continue,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Repeat { count, body, .. } => {
                let subject = self.eval_expr(env, count)?;
                let n = subject.as_i64().ok_or_else(|| {
                    Error::runtime(
                        format!(
                            "repeat count must be a number, found {}",
                            subject.type_name()
                        ),
                        count.span(),
                    )
                })?;
                for _ in 0..n.max(0) {
                    let child = Env::child(env);
                    match self.exec_block(&child, body)? {
                        Flow::Normal => {}
                        Flow::Break => break,
                        Flow::Continue => continue,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Break(_) => Ok(Flow::Break),
            Stmt::Continue(_) => Ok(Flow::Continue),
            Stmt::Return { value, .. } => {
                let v = match value {
                    Some(e) => self.eval_expr(env, e)?,
                    None => Value::None,
                };
                Ok(Flow::Return(v))
            }
            Stmt::Import { spec, alias, span } => {
                let exports = self.resolve_module(spec, *span)?;
                match alias {
                    Some(name) => {
                        let value = Value::Map(Rc::new(RefCell::new((*exports).clone())));
                        env_define(env, name, value);
                    }
                    None => {
                        for (name, value) in exports.iter() {
                            env_define(env, name, value.clone());
                        }
                    }
                }
                Ok(Flow::Normal)
            }
        }
    }

    fn assign(&mut self, env: &EnvRef, target: &Expr, value: Value) -> Result<()> {
        match target {
            Expr::Var(name, span) => {
                if env_assign(env, name, value) {
                    Ok(())
                } else {
                    Err(Error::runtime(
                        format!("cannot assign to undefined variable `{name}` (use `let` first)"),
                        *span,
                    ))
                }
            }
            Expr::Index {
                object,
                index,
                span,
            } => {
                let obj = self.eval_expr(env, object)?;
                let idx = self.eval_expr(env, index)?;
                match obj {
                    Value::List(list) => {
                        let len = list.borrow().len() as i64;
                        let i = normalize_index(&idx, len, *span)?;
                        *list.borrow_mut().get_mut(i).expect("checked") = value;
                        Ok(())
                    }
                    Value::Map(map) => {
                        map_insert(&map, &idx.to_display(), value);
                        Ok(())
                    }
                    other => Err(Error::runtime(
                        format!("cannot assign into a {}", other.type_name()),
                        *span,
                    )),
                }
            }
            Expr::Field { object, name, span } => {
                let obj = self.eval_expr(env, object)?;
                match obj {
                    Value::Map(map) => {
                        map_insert(&map, name, value);
                        Ok(())
                    }
                    other => Err(Error::runtime(
                        format!("cannot assign field `{name}` on a {}", other.type_name()),
                        *span,
                    )),
                }
            }
            _ => Err(Error::runtime("invalid assignment target", target.span())),
        }
    }

    // --- Expressions -------------------------------------------------------

    fn eval_expr(&mut self, env: &EnvRef, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::Int(n, _) => Ok(Value::Int(*n)),
            Expr::Float(n, _) => Ok(Value::Float(*n)),
            Expr::Bool(b, _) => Ok(Value::Bool(*b)),
            Expr::None(_) => Ok(Value::None),
            Expr::Str(lit, _) => self.eval_str(env, lit),
            Expr::Var(name, span) => env_get(env, name)
                .or_else(|| crate::stdlib::builtin_static(name).map(Value::Builtin))
                .ok_or_else(|| Error::runtime(format!("undefined variable `{name}`"), *span)),
            Expr::List(items, _) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval_expr(env, item)?);
                }
                Ok(Value::list(values))
            }
            Expr::Map(entries, _) => {
                let mut data: MapData = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    data.push((Rc::from(key.as_str()), self.eval_expr(env, value)?));
                }
                Ok(Value::map(data))
            }
            Expr::Unary { op, expr, span } => {
                let v = self.eval_expr(env, expr)?;
                match op {
                    UnOp::Neg => match v {
                        Value::Int(n) => Ok(Value::Int(n.checked_neg().ok_or_else(|| {
                            Error::runtime("integer overflow in negation", *span)
                        })?)),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        other => Err(Error::runtime(
                            format!("cannot negate a {}", other.type_name()),
                            *span,
                        )),
                    },
                    UnOp::Not => Ok(Value::Bool(!v.is_truthy())),
                }
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => self.eval_binary(env, *op, left, right, *span),
            Expr::Call { callee, args, span } => self.eval_call(env, callee, args, *span),
            Expr::Index {
                object,
                index,
                span,
            } => {
                let obj = self.eval_expr(env, object)?;
                let idx = self.eval_expr(env, index)?;
                index_get(&obj, &idx, *span)
            }
            Expr::Field { object, name, span } => {
                let obj = self.eval_expr(env, object)?;
                match obj {
                    Value::Map(map) => map_get(&map, name)
                        .ok_or_else(|| Error::runtime(format!("map has no key `{name}`"), *span)),
                    other => Err(Error::runtime(
                        format!("cannot read field `{name}` from a {}", other.type_name()),
                        *span,
                    )),
                }
            }
            Expr::Func { params, body, .. } => Ok(Value::Func(Rc::new(Function {
                name: None,
                params: params.clone(),
                body: Rc::new(body.clone()),
                env: env.clone(),
            }))),
        }
    }

    fn eval_str(&mut self, env: &EnvRef, lit: &StrLit) -> Result<Value> {
        let mut out = String::new();
        for part in &lit.parts {
            match part {
                StrPart::Lit(text) => out.push_str(text),
                StrPart::Interp(expr) => {
                    let v = self.eval_expr(env, expr)?;
                    out.push_str(&v.to_display());
                }
            }
        }
        Ok(Value::str(out))
    }

    fn eval_binary(
        &mut self,
        env: &EnvRef,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        span: Span,
    ) -> Result<Value> {
        match op {
            BinOp::And => {
                let l = self.eval_expr(env, left)?;
                if !l.is_truthy() {
                    Ok(l)
                } else {
                    self.eval_expr(env, right)
                }
            }
            BinOp::Or => {
                let l = self.eval_expr(env, left)?;
                if l.is_truthy() {
                    Ok(l)
                } else {
                    self.eval_expr(env, right)
                }
            }
            _ => {
                let l = self.eval_expr(env, left)?;
                let r = self.eval_expr(env, right)?;
                binop(op, l, r, span)
            }
        }
    }

    fn eval_call(
        &mut self,
        env: &EnvRef,
        callee: &Expr,
        args: &[Expr],
        span: Span,
    ) -> Result<Value> {
        // Host functions shadow builtins and user functions. A method-style
        // call (`editor.insert(...)`) passes the receiver as the first argument.
        let host_name = match callee {
            Expr::Var(name, _) if self.host_has(name) => Some(name.clone()),
            Expr::Field { name, .. } if self.host_has(name) => Some(name.clone()),
            _ => None,
        };
        if let Some(name) = host_name {
            let mut argv = Vec::with_capacity(args.len() + 1);
            if let Expr::Field { object, .. } = callee {
                argv.push(self.eval_expr(env, object)?);
            }
            for a in args {
                argv.push(self.eval_expr(env, a)?);
            }
            return self.call_host(&name, argv, span);
        }

        let mut argv = Vec::with_capacity(args.len());
        if let Expr::Field { object, name, .. } = callee {
            let receiver = self.eval_expr(env, object)?;
            argv.push(receiver);
            for a in args {
                argv.push(self.eval_expr(env, a)?);
            }
            return self.call_method(name, argv, span);
        }
        let callee_value = self.eval_expr(env, callee)?;
        for a in args {
            argv.push(self.eval_expr(env, a)?);
        }
        self.call_value(&callee_value, argv, span)
    }

    /// Whether the installed host provides a function called `name`.
    fn host_has(&self, name: &str) -> bool {
        match &self.host {
            Some(h) => h.borrow().functions().contains(&name),
            None => false,
        }
    }

    /// Dispatches a call to the installed host.
    fn call_host(&mut self, name: &str, args: Vec<Value>, span: Span) -> Result<Value> {
        let host = self
            .host
            .clone()
            .ok_or_else(|| Error::runtime("no host is installed", span))?;
        let result = host.borrow_mut().call(name, args, span);
        result
    }

    pub(crate) fn call_value(
        &mut self,
        callee: &Value,
        args: Vec<Value>,
        span: Span,
    ) -> Result<Value> {
        match callee {
            Value::Func(f) => {
                let f = f.clone();
                self.call_function(&f, args, span)
            }
            Value::Builtin(name) => self.call_builtin(name, args, span),
            other => Err(Error::runtime(
                format!("{} is not callable", other.type_name()),
                span,
            )),
        }
    }

    pub(crate) fn call_function(
        &mut self,
        func: &Rc<Function>,
        args: Vec<Value>,
        span: Span,
    ) -> Result<Value> {
        if args.len() != func.params.len() {
            let name = func
                .name
                .clone()
                .unwrap_or_else(|| "<anonymous>".to_string());
            return Err(Error::runtime(
                format!(
                    "function `{name}` expects {} argument(s) but got {}",
                    func.params.len(),
                    args.len()
                ),
                span,
            ));
        }
        if self.depth >= self.max_depth {
            return Err(Error::runtime(
                "maximum call depth exceeded (possible infinite recursion)",
                span,
            ));
        }
        self.depth += 1;
        let call_env = Env::child(&func.env);
        for (param, arg) in func.params.iter().zip(args) {
            env_define(&call_env, param, arg);
        }
        let result = self.exec_block(&call_env, &func.body);
        self.depth -= 1;
        match result? {
            Flow::Return(v) => Ok(v),
            Flow::Normal => Ok(Value::None),
            Flow::Break | Flow::Continue => Err(Error::runtime(
                "`break`/`continue` used outside of a loop",
                span,
            )),
        }
    }

    fn call_method(&mut self, name: &str, args: Vec<Value>, span: Span) -> Result<Value> {
        if let Some(static_name) = crate::stdlib::builtin_static(name) {
            return self.call_builtin(static_name, args, span);
        }
        // Object-style method: a map field holding a function.
        if let Value::Map(map) = &args[0] {
            if let Some(Value::Func(f)) = map_get(map, name) {
                let f = f.clone();
                return self.call_function(&f, args[1..].to_vec(), span);
            }
        }
        Err(Error::runtime(
            format!("no method `{name}` on a {}", args[0].type_name()),
            span,
        ))
    }

    fn iter_values(&self, value: &Value, span: Span) -> Result<Vec<Value>> {
        match value {
            Value::List(items) => Ok(items.borrow().clone()),
            Value::Str(s) => Ok(s.chars().map(|c| Value::str(c.to_string())).collect()),
            Value::Map(map) => Ok(map
                .borrow()
                .iter()
                .map(|(k, _)| Value::str(k.to_string()))
                .collect()),
            other => Err(Error::runtime(
                format!("cannot loop over a {}", other.type_name()),
                span,
            )),
        }
    }
}

fn normalize_index(idx: &Value, len: i64, span: Span) -> Result<usize> {
    let raw = idx.as_i64().ok_or_else(|| {
        Error::runtime(
            format!("list index must be a number, found {}", idx.type_name()),
            span,
        )
    })?;
    let i = if raw < 0 { raw + len } else { raw };
    if i < 0 || i >= len {
        Err(Error::runtime(
            format!("index {raw} is out of range (length {len})"),
            span,
        ))
    } else {
        Ok(i as usize)
    }
}

fn index_get(obj: &Value, idx: &Value, span: Span) -> Result<Value> {
    match obj {
        Value::List(items) => {
            let items = items.borrow();
            let i = normalize_index(idx, items.len() as i64, span)?;
            Ok(items[i].clone())
        }
        Value::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            let i = normalize_index(idx, chars.len() as i64, span)?;
            Ok(Value::str(chars[i].to_string()))
        }
        Value::Map(map) => {
            let key = idx.to_display();
            map_get(map, &key)
                .ok_or_else(|| Error::runtime(format!("key `{key}` not found in map"), span))
        }
        other => Err(Error::runtime(
            format!("cannot index a {}", other.type_name()),
            span,
        )),
    }
}

fn type_err(sym: &str, l: &Value, r: &Value, span: Span) -> Error {
    Error::runtime(
        format!(
            "unsupported operand types for `{sym}`: {} and {}",
            l.type_name(),
            r.type_name()
        ),
        span,
    )
}

fn apply_cmp(op: BinOp, ord: std::cmp::Ordering) -> bool {
    use std::cmp::Ordering::*;
    match op {
        BinOp::Lt => ord == Less,
        BinOp::Le => ord != Greater,
        BinOp::Gt => ord == Greater,
        BinOp::Ge => ord != Less,
        _ => unreachable!(),
    }
}

fn binop(op: BinOp, l: Value, r: Value, span: Span) -> Result<Value> {
    match op {
        BinOp::Add => match (&l, &r) {
            (Value::Int(a), Value::Int(b)) => {
                Ok(Value::Int(a.checked_add(*b).ok_or_else(|| {
                    Error::runtime("integer overflow in `+`", span)
                })?))
            }
            (Value::List(a), Value::List(b)) => {
                let mut items = a.borrow().clone();
                items.extend(b.borrow().iter().cloned());
                Ok(Value::list(items))
            }
            _ => {
                if let (Some(a), Some(b)) = (l.as_f64(), r.as_f64()) {
                    Ok(Value::Float(a + b))
                } else if matches!(l, Value::Str(_)) || matches!(r, Value::Str(_)) {
                    Ok(Value::str(format!("{}{}", l.to_display(), r.to_display())))
                } else {
                    Err(type_err("+", &l, &r, span))
                }
            }
        },
        BinOp::Sub => match (&l, &r) {
            (Value::Int(a), Value::Int(b)) => {
                Ok(Value::Int(a.checked_sub(*b).ok_or_else(|| {
                    Error::runtime("integer overflow in `-`", span)
                })?))
            }
            _ => match (l.as_f64(), r.as_f64()) {
                (Some(a), Some(b)) => Ok(Value::Float(a - b)),
                _ => Err(type_err("-", &l, &r, span)),
            },
        },
        BinOp::Mul => match (&l, &r) {
            (Value::Int(a), Value::Int(b)) => {
                Ok(Value::Int(a.checked_mul(*b).ok_or_else(|| {
                    Error::runtime("integer overflow in `*`", span)
                })?))
            }
            (Value::Str(s), Value::Int(n)) | (Value::Int(n), Value::Str(s)) => {
                if *n < 0 {
                    return Err(Error::runtime(
                        "cannot repeat a string a negative number of times",
                        span,
                    ));
                }
                Ok(Value::str(s.repeat(*n as usize)))
            }
            _ => match (l.as_f64(), r.as_f64()) {
                (Some(a), Some(b)) => Ok(Value::Float(a * b)),
                _ => Err(type_err("*", &l, &r, span)),
            },
        },
        BinOp::Div => {
            if r.as_f64() == Some(0.0) {
                return Err(Error::runtime("division by zero", span));
            }
            match (l.as_f64(), r.as_f64()) {
                (Some(a), Some(b)) => Ok(Value::Float(a / b)),
                _ => Err(type_err("/", &l, &r, span)),
            }
        }
        BinOp::Rem => match (&l, &r) {
            (Value::Int(a), Value::Int(b)) => {
                if *b == 0 {
                    Err(Error::runtime("division by zero", span))
                } else {
                    Ok(Value::Int(a % b))
                }
            }
            _ => match (l.as_f64(), r.as_f64()) {
                (Some(_), Some(0.0)) => Err(Error::runtime("division by zero", span)),
                (Some(a), Some(b)) => Ok(Value::Float(a % b)),
                _ => Err(type_err("%", &l, &r, span)),
            },
        },
        BinOp::Pow => match (&l, &r) {
            (Value::Int(a), Value::Int(b)) if *b >= 0 => {
                let exp =
                    u32::try_from(*b).map_err(|_| Error::runtime("exponent too large", span))?;
                a.checked_pow(exp)
                    .map(Value::Int)
                    .ok_or_else(|| Error::runtime("integer overflow in `**`", span))
            }
            _ => match (l.as_f64(), r.as_f64()) {
                (Some(a), Some(b)) => Ok(Value::Float(a.powf(b))),
                _ => Err(type_err("**", &l, &r, span)),
            },
        },
        BinOp::Eq => Ok(Value::Bool(l.deep_eq(&r))),
        BinOp::Ne => Ok(Value::Bool(!l.deep_eq(&r))),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => match (&l, &r) {
            (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(apply_cmp(op, a.cmp(b)))),
            _ => match (l.as_f64(), r.as_f64()) {
                (Some(a), Some(b)) => {
                    let ord = a
                        .partial_cmp(&b)
                        .ok_or_else(|| Error::runtime("cannot compare values (NaN)", span))?;
                    Ok(Value::Bool(apply_cmp(op, ord)))
                }
                _ => Err(Error::runtime(
                    format!("cannot compare {} and {}", l.type_name(), r.type_name()),
                    span,
                )),
            },
        },
        BinOp::And | BinOp::Or => unreachable!("short-circuited in eval_binary"),
    }
}
