//! The TextWrite abstract syntax tree.

use crate::error::Span;

/// A statement.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let name = value`
    Let {
        name: String,
        value: Expr,
        span: Span,
    },
    /// `target = value` (also `set target = value` and compound assignment).
    Assign {
        target: Expr,
        op: AssignOp,
        value: Expr,
        span: Span,
    },
    /// An expression evaluated for its side effects (or its value at the REPL).
    Expr(Expr),
    /// `if cond { .. } else if ... else { ... }`.
    If {
        branches: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
        span: Span,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `for name in iter { ... }`
    For {
        name: String,
        iter: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `repeat count times { ... }`
    Repeat {
        count: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    Return {
        value: Option<Expr>,
        span: Span,
    },
    /// `fn name(params) { ... }`
    FnDecl {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
        span: Span,
    },
    /// `import "spec"` (merge exports) or `import "spec" as name` (namespace).
    Import {
        spec: String,
        alias: Option<String>,
        span: Span,
    },
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. }
            | Stmt::Assign { span, .. }
            | Stmt::For { span, .. }
            | Stmt::Repeat { span, .. }
            | Stmt::While { span, .. }
            | Stmt::FnDecl { span, .. }
            | Stmt::If { span, .. }
            | Stmt::Import { span, .. }
            | Stmt::Return { span, .. } => *span,
            Stmt::Break(s) | Stmt::Continue(s) => *s,
            Stmt::Expr(e) => e.span(),
        }
    }
}

/// The operator in an assignment statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

/// A TextWrite expression.
#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    None(Span),
    Str(StrLit, Span),
    Var(String, Span),
    List(Vec<Expr>, Span),
    Map(Vec<(String, Expr)>, Span),
    Unary {
        op: UnOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Field {
        object: Box<Expr>,
        name: String,
        span: Span,
    },
    /// An anonymous function / closure: `fn(a, b) { ... }` or `|a, b| { ... }`.
    Func {
        params: Vec<String>,
        body: Vec<Stmt>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, s)
            | Expr::Float(_, s)
            | Expr::Bool(_, s)
            | Expr::None(s)
            | Expr::Str(_, s)
            | Expr::Var(_, s)
            | Expr::List(_, s)
            | Expr::Map(_, s)
            | Expr::Unary { span: s, .. }
            | Expr::Binary { span: s, .. }
            | Expr::Call { span: s, .. }
            | Expr::Index { span: s, .. }
            | Expr::Field { span: s, .. }
            | Expr::Func { span: s, .. } => *s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// An interpolated string literal, e.g. `"Hello, {name}!"`.
#[derive(Debug, Clone)]
pub struct StrLit {
    pub parts: Vec<StrPart>,
}

#[derive(Debug, Clone)]
pub enum StrPart {
    Lit(String),
    Interp(Expr),
}
