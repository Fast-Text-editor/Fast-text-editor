//! Recursive-descent parser turning tokens into an [`crate::ast`].

use std::mem::discriminant;

use crate::ast::*;
use crate::error::{Error, Result, Span};
use crate::lexer::{tokenize, Tok, Token};

/// Parses a whole program.
pub fn parse(src: &str) -> Result<Vec<Stmt>> {
    let tokens = tokenize(src)?;
    let mut p = Parser::new(tokens);
    p.parse_program()
}

/// Parses a single expression (used by the REPL and `textwrite eval`).
pub fn parse_expression(src: &str) -> Result<Expr> {
    let tokens = tokenize(src)?;
    let mut p = Parser::new(tokens);
    let expr = p.parse_expr()?;
    p.skip_separators();
    if !p.at_eof() {
        return Err(Error::parse(
            format!("unexpected {} after expression", p.cur().tok.describe()),
            p.cur().span,
        ));
    }
    Ok(expr)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    // --- Token helpers -----------------------------------------------------

    fn cur(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn at_eof(&self) -> bool {
        matches!(self.cur().tok, Tok::Eof)
    }

    fn check(&self, k: &Tok) -> bool {
        discriminant(&self.cur().tok) == discriminant(k)
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, k: &Tok) -> bool {
        if self.check(k) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, k: &Tok) -> Result<Token> {
        if self.check(k) {
            Ok(self.advance())
        } else if self.check(&Tok::Eof) {
            Err(Error::incomplete(
                format!("expected {}, found end of input", k.symbol_expect()),
                self.cur().span,
            ))
        } else {
            Err(Error::parse(
                format!(
                    "expected {}, found {}",
                    k.symbol_expect(),
                    self.cur().tok.describe()
                ),
                self.cur().span,
            ))
        }
    }

    fn skip_newlines(&mut self) {
        while self.check(&Tok::Newline) {
            self.advance();
        }
    }

    fn skip_separators(&mut self) {
        while self.check(&Tok::Newline) || self.check(&Tok::Semicolon) {
            self.advance();
        }
    }

    fn end_statement(&mut self) -> Result<()> {
        if self.check(&Tok::Eof) || self.check(&Tok::RBrace) {
            return Ok(());
        }
        if self.check(&Tok::Newline) || self.check(&Tok::Semicolon) {
            self.skip_separators();
            return Ok(());
        }
        Err(Error::parse(
            format!(
                "expected end of statement, found {}",
                self.cur().tok.describe()
            ),
            self.cur().span,
        ))
    }

    // --- Statements --------------------------------------------------------

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>> {
        let mut stmts = Vec::new();
        loop {
            self.skip_separators();
            if self.check(&Tok::Eof) {
                break;
            }
            stmts.push(self.parse_stmt()?);
            self.end_statement()?;
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt> {
        let start = self.cur().span;
        match self.cur().tok.clone() {
            Tok::Let => {
                self.advance();
                let (name, _) = self.expect_ident("variable name")?;
                self.expect(&Tok::Assign)?;
                self.skip_newlines();
                let value = self.parse_expr()?;
                let span = start.merge(value.span());
                Ok(Stmt::Let { name, value, span })
            }
            Tok::Set => {
                self.advance();
                let target = self.parse_postfix()?;
                self.expect(&Tok::Assign)?;
                self.skip_newlines();
                let value = self.parse_expr()?;
                let span = start.merge(value.span());
                Ok(Stmt::Assign {
                    target,
                    op: AssignOp::Set,
                    value,
                    span,
                })
            }
            Tok::Fn => {
                // Distinguish `fn name(...)` (declaration) from an anonymous
                // function used as an expression.
                if matches!(self.peek_tok(1), Tok::Ident(_)) {
                    self.advance();
                    let (name, _) = self.expect_ident("function name")?;
                    let (params, body, _) = self.parse_fn_tail()?;
                    let span = start.merge(self.prev_span());
                    Ok(Stmt::FnDecl {
                        name,
                        params,
                        body,
                        span,
                    })
                } else {
                    Ok(Stmt::Expr(self.parse_expr()?))
                }
            }
            Tok::Return => {
                self.advance();
                let value = if self.check(&Tok::Newline)
                    || self.check(&Tok::Semicolon)
                    || self.check(&Tok::RBrace)
                    || self.check(&Tok::Eof)
                {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                let span = match &value {
                    Some(v) => start.merge(v.span()),
                    None => start,
                };
                Ok(Stmt::Return { value, span })
            }
            Tok::If => self.parse_if(),
            Tok::While => self.parse_while(),
            Tok::For => self.parse_for(),
            Tok::Repeat => self.parse_repeat(),
            Tok::Break => {
                self.advance();
                Ok(Stmt::Break(start))
            }
            Tok::Continue => {
                self.advance();
                Ok(Stmt::Continue(start))
            }
            Tok::Import => self.parse_import(),
            _ => {
                let expr = self.parse_expr()?;
                let op = match self.cur().tok {
                    Tok::Assign => Some(AssignOp::Set),
                    Tok::PlusAssign => Some(AssignOp::Add),
                    Tok::MinusAssign => Some(AssignOp::Sub),
                    Tok::StarAssign => Some(AssignOp::Mul),
                    Tok::SlashAssign => Some(AssignOp::Div),
                    Tok::PercentAssign => Some(AssignOp::Rem),
                    _ => None,
                };
                match op {
                    Some(op) => {
                        if !is_lvalue(&expr) {
                            return Err(Error::parse("invalid assignment target", expr.span()));
                        }
                        self.advance();
                        self.skip_newlines();
                        let value = self.parse_expr()?;
                        let span = expr.span().merge(value.span());
                        Ok(Stmt::Assign {
                            target: expr,
                            op,
                            value,
                            span,
                        })
                    }
                    None => {
                        // Command-call syntax: `print "hello"`, `greet name`.
                        if matches!(expr, Expr::Var(_, _)) && self.starts_command_arg() {
                            let callee_span = expr.span();
                            let args = self.parse_command_args()?;
                            let span = args
                                .last()
                                .map(|a| callee_span.merge(a.span()))
                                .unwrap_or(callee_span);
                            return Ok(Stmt::Expr(Expr::Call {
                                callee: Box::new(expr),
                                args,
                                span,
                            }));
                        }
                        Ok(Stmt::Expr(expr))
                    }
                }
            }
        }
    }

    /// True when the current token can begin a command-call argument.
    fn starts_command_arg(&self) -> bool {
        matches!(
            self.cur().tok,
            Tok::Int(_)
                | Tok::Float(_)
                | Tok::DStr(_)
                | Tok::SStr(_)
                | Tok::Ident(_)
                | Tok::True
                | Tok::False
                | Tok::None
                | Tok::LParen
                | Tok::LBracket
                | Tok::LBrace
                | Tok::Fn
        )
    }

    /// Parses comma-separated arguments for command-call syntax.
    fn parse_command_args(&mut self) -> Result<Vec<Expr>> {
        let mut args = vec![self.parse_expr()?];
        while self.eat(&Tok::Comma) {
            self.skip_newlines();
            args.push(self.parse_expr()?);
        }
        Ok(args)
    }

    fn parse_if(&mut self) -> Result<Stmt> {
        let start = self.cur().span;
        self.expect(&Tok::If)?;
        self.skip_newlines();
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        let mut branches = vec![(cond, body)];
        let mut else_body = None;

        loop {
            // Peek past newlines for `else`, but put them back if there is none
            // so the surrounding statement still sees its separator.
            let before_else = self.pos;
            self.skip_newlines();
            if !self.check(&Tok::Else) {
                self.pos = before_else;
                break;
            }
            self.advance();
            self.skip_newlines();
            if self.check(&Tok::If) {
                self.advance();
                self.skip_newlines();
                let cond = self.parse_expr()?;
                let body = self.parse_block()?;
                branches.push((cond, body));
            } else {
                else_body = Some(self.parse_block()?);
                break;
            }
        }

        Ok(Stmt::If {
            branches,
            else_body,
            span: start,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt> {
        let start = self.cur().span;
        self.expect(&Tok::While)?;
        self.skip_newlines();
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::While {
            cond,
            body,
            span: start,
        })
    }

    fn parse_for(&mut self) -> Result<Stmt> {
        let start = self.cur().span;
        self.expect(&Tok::For)?;
        let (name, _) = self.expect_ident("loop variable")?;
        self.expect(&Tok::In)?;
        self.skip_newlines();
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::For {
            name,
            iter,
            body,
            span: start,
        })
    }

    fn parse_repeat(&mut self) -> Result<Stmt> {
        let start = self.cur().span;
        self.expect(&Tok::Repeat)?;
        self.skip_newlines();
        let count = self.parse_expr()?;
        self.expect(&Tok::Times)?;
        let body = self.parse_block()?;
        Ok(Stmt::Repeat {
            count,
            body,
            span: start,
        })
    }

    /// Parses `import "spec"` or `import "spec" as name`.
    ///
    /// The spec is a plain string (`process_escapes` applies, but no `{...}`
    /// interpolation), so paths read literally. `as` is a soft keyword: it is
    /// only special here, so scripts may still use `as` as an identifier.
    fn parse_import(&mut self) -> Result<Stmt> {
        let start = self.cur().span;
        self.expect(&Tok::Import)?;
        self.skip_newlines();
        let (raw, raw_span) = match self.cur().tok.clone() {
            Tok::DStr(raw) | Tok::SStr(raw) => {
                let span = self.cur().span;
                self.advance();
                (raw, span)
            }
            other => {
                return Err(Error::parse(
                    format!("expected a module name string, found {}", other.describe()),
                    self.cur().span,
                ))
            }
        };
        let spec = self.process_escapes(&raw, raw_span)?;
        let alias = match self.cur().tok.clone() {
            Tok::Ident(word) if word == "as" => {
                self.advance();
                let (name, _) = self.expect_ident("import alias")?;
                Some(name)
            }
            _ => None,
        };
        Ok(Stmt::Import {
            spec,
            alias,
            span: start.merge(self.prev_span()),
        })
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>> {
        self.expect(&Tok::LBrace)?;
        let mut stmts = Vec::new();
        loop {
            self.skip_separators();
            if self.eat(&Tok::RBrace) {
                break;
            }
            if self.check(&Tok::Eof) {
                return Err(Error::incomplete(
                    "unclosed block, expected `}`",
                    self.cur().span,
                ));
            }
            stmts.push(self.parse_stmt()?);
            self.end_statement()?;
        }
        Ok(stmts)
    }

    /// Parses `(params) { body }` after the `fn`/name has been consumed.
    fn parse_fn_tail(&mut self) -> Result<(Vec<String>, Vec<Stmt>, Span)> {
        self.expect(&Tok::LParen)?;
        let params = self.parse_params()?;
        let body = self.parse_block()?;
        let span = self.prev_span();
        Ok((params, body, span))
    }

    fn parse_params(&mut self) -> Result<Vec<String>> {
        let mut params = Vec::new();
        self.skip_newlines();
        if self.eat(&Tok::RParen) {
            return Ok(params);
        }
        loop {
            self.skip_newlines();
            let (name, _) = self.expect_ident("parameter name")?;
            params.push(name);
            self.skip_newlines();
            if self.eat(&Tok::Comma) {
                continue;
            }
            self.expect(&Tok::RParen)?;
            break;
        }
        Ok(params)
    }

    // --- Expressions -------------------------------------------------------

    pub fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;
        while self.check(&Tok::Or) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_and()?;
            let span = left.span().merge(right.span());
            left = bin(BinOp::Or, left, right, span);
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_not()?;
        while self.check(&Tok::And) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_not()?;
            let span = left.span().merge(right.span());
            left = bin(BinOp::And, left, right, span);
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr> {
        if self.check(&Tok::Not) {
            let start = self.advance().span;
            self.skip_newlines();
            let expr = self.parse_not()?;
            let span = start.merge(expr.span());
            Ok(Expr::Unary {
                op: UnOp::Not,
                expr: Box::new(expr),
                span,
            })
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        let mut left = self.parse_add()?;
        loop {
            let op = match self.cur().tok {
                Tok::Eq => BinOp::Eq,
                Tok::Ne => BinOp::Ne,
                Tok::Lt => BinOp::Lt,
                Tok::Le => BinOp::Le,
                Tok::Gt => BinOp::Gt,
                Tok::Ge => BinOp::Ge,
                Tok::Is => {
                    self.advance();
                    let negated = self.eat(&Tok::Not);
                    self.skip_newlines();
                    let right = self.parse_add()?;
                    let span = left.span().merge(right.span());
                    left = bin(
                        if negated { BinOp::Ne } else { BinOp::Eq },
                        left,
                        right,
                        span,
                    );
                    continue;
                }
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_add()?;
            let span = left.span().merge(right.span());
            left = bin(op, left, right, span);
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.cur().tok {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_mul()?;
            let span = left.span().merge(right.span());
            left = bin(op, left, right, span);
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.cur().tok {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Rem,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let right = self.parse_unary()?;
            let span = left.span().merge(right.span());
            left = bin(op, left, right, span);
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if self.check(&Tok::Minus) {
            let start = self.advance().span;
            self.skip_newlines();
            let expr = self.parse_unary()?;
            let span = start.merge(expr.span());
            Ok(Expr::Unary {
                op: UnOp::Neg,
                expr: Box::new(expr),
                span,
            })
        } else {
            self.parse_power()
        }
    }

    fn parse_power(&mut self) -> Result<Expr> {
        let left = self.parse_postfix()?;
        if self.check(&Tok::StarStar) {
            self.advance();
            self.skip_newlines();
            let right = self.parse_unary()?;
            let span = left.span().merge(right.span());
            Ok(bin(BinOp::Pow, left, right, span))
        } else {
            Ok(left)
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(&Tok::LParen) {
                let start = expr.span();
                self.advance();
                let args = self.parse_args()?;
                let span = start.merge(self.prev_span());
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
            } else if self.check(&Tok::LBracket) {
                let start = expr.span();
                self.advance();
                self.skip_newlines();
                if self.check(&Tok::RBracket) {
                    return Err(Error::parse("empty index is not allowed", self.cur().span));
                }
                let index = self.parse_expr()?;
                self.skip_newlines();
                self.expect(&Tok::RBracket)?;
                let span = start.merge(self.prev_span());
                expr = Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                    span,
                };
            } else if self.check(&Tok::Dot) {
                self.advance();
                let (name, name_span) = self.expect_ident("field or method name")?;
                let span = expr.span().merge(name_span);
                expr = Expr::Field {
                    object: Box::new(expr),
                    name,
                    span,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>> {
        let mut args = Vec::new();
        self.skip_newlines();
        if self.eat(&Tok::RParen) {
            return Ok(args);
        }
        loop {
            self.skip_newlines();
            args.push(self.parse_expr()?);
            self.skip_newlines();
            if self.eat(&Tok::Comma) {
                continue;
            }
            self.expect(&Tok::RParen)?;
            break;
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let token = self.cur().clone();
        match token.tok {
            Tok::Int(n) => {
                self.advance();
                Ok(Expr::Int(n, token.span))
            }
            Tok::Float(n) => {
                self.advance();
                Ok(Expr::Float(n, token.span))
            }
            Tok::True => {
                self.advance();
                Ok(Expr::Bool(true, token.span))
            }
            Tok::False => {
                self.advance();
                Ok(Expr::Bool(false, token.span))
            }
            Tok::None => {
                self.advance();
                Ok(Expr::None(token.span))
            }
            Tok::DStr(raw) => {
                self.advance();
                Ok(Expr::Str(self.build_dstring(&raw, token.span)?, token.span))
            }
            Tok::SStr(raw) => {
                self.advance();
                let lit = self.process_escapes(&raw, token.span)?;
                Ok(Expr::Str(
                    StrLit {
                        parts: vec![StrPart::Lit(lit)],
                    },
                    token.span,
                ))
            }
            Tok::Ident(name) => {
                self.advance();
                Ok(Expr::Var(name, token.span))
            }
            Tok::LParen => {
                self.advance();
                self.skip_newlines();
                let expr = self.parse_expr()?;
                self.skip_newlines();
                self.expect(&Tok::RParen)?;
                Ok(expr)
            }
            Tok::LBracket => self.parse_list(),
            Tok::LBrace => self.parse_map(),
            Tok::Fn => {
                self.advance();
                let (params, body, span) = self.parse_fn_tail()?;
                Ok(Expr::Func { params, body, span })
            }
            Tok::Eof => Err(Error::incomplete(
                "expected an expression, found end of input",
                token.span,
            )),
            other => Err(Error::parse(
                format!("expected an expression, found {}", other.describe()),
                token.span,
            )),
        }
    }

    fn parse_list(&mut self) -> Result<Expr> {
        let start = self.advance().span; // [
        let mut items = Vec::new();
        self.skip_newlines();
        if self.eat(&Tok::RBracket) {
            return Ok(Expr::List(items, start.merge(self.prev_span())));
        }
        loop {
            self.skip_newlines();
            items.push(self.parse_expr()?);
            self.skip_newlines();
            if self.eat(&Tok::Comma) {
                self.skip_newlines();
                if self.eat(&Tok::RBracket) {
                    break;
                }
                continue;
            }
            self.expect(&Tok::RBracket)?;
            break;
        }
        Ok(Expr::List(items, start.merge(self.prev_span())))
    }

    fn parse_map(&mut self) -> Result<Expr> {
        let start = self.advance().span; // {
        let mut entries = Vec::new();
        self.skip_newlines();
        if self.eat(&Tok::RBrace) {
            return Ok(Expr::Map(entries, start.merge(self.prev_span())));
        }
        loop {
            self.skip_newlines();
            let key = match self.cur().tok.clone() {
                Tok::Ident(name) => {
                    self.advance();
                    name
                }
                Tok::DStr(raw) => {
                    self.advance();
                    self.process_escapes(&raw, start)?
                }
                Tok::SStr(raw) => {
                    self.advance();
                    self.process_escapes(&raw, start)?
                }
                other => {
                    return Err(Error::parse(
                        format!("expected a map key, found {}", other.describe()),
                        self.cur().span,
                    ))
                }
            };
            self.expect(&Tok::Colon)?;
            self.skip_newlines();
            let value = self.parse_expr()?;
            entries.push((key, value));
            self.skip_newlines();
            if self.eat(&Tok::Comma) {
                self.skip_newlines();
                if self.eat(&Tok::RBrace) {
                    break;
                }
                continue;
            }
            self.expect(&Tok::RBrace)?;
            break;
        }
        Ok(Expr::Map(entries, start.merge(self.prev_span())))
    }

    // --- String literals ---------------------------------------------------

    fn build_dstring(&self, raw: &str, span: Span) -> Result<StrLit> {
        let chars: Vec<char> = raw.chars().collect();
        let mut parts: Vec<StrPart> = Vec::new();
        let mut literal = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            match c {
                '\\' => {
                    i += 1;
                    if i < chars.len() {
                        push_escape(&mut literal, chars[i]);
                        i += 1;
                    } else {
                        literal.push('\\');
                    }
                }
                '{' if chars.get(i + 1) == Some(&'{') => {
                    literal.push('{');
                    i += 2;
                }
                '}' if chars.get(i + 1) == Some(&'}') => {
                    literal.push('}');
                    i += 2;
                }
                '{' => {
                    // Find the matching (possibly nested) close brace.
                    let mut depth = 0;
                    let mut j = i + 1;
                    let mut end = None;
                    while j < chars.len() {
                        match chars[j] {
                            '{' => depth += 1,
                            '}' if depth == 0 => {
                                end = Some(j);
                                break;
                            }
                            '}' => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    let Some(close) = end else {
                        return Err(Error::parse("unclosed `{` in string interpolation", span));
                    };
                    let inner: String = chars[i + 1..close].iter().collect();
                    let expr = self.parse_interpolation(&inner, span)?;
                    if !literal.is_empty() {
                        parts.push(StrPart::Lit(std::mem::take(&mut literal)));
                    }
                    parts.push(StrPart::Interp(expr));
                    i = close + 1;
                }
                _ => {
                    literal.push(c);
                    i += 1;
                }
            }
        }
        if !literal.is_empty() || parts.is_empty() {
            parts.push(StrPart::Lit(literal));
        }
        Ok(StrLit { parts })
    }

    fn process_escapes(&self, raw: &str, _span: Span) -> Result<String> {
        let chars: Vec<char> = raw.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' {
                i += 1;
                if i < chars.len() {
                    push_escape(&mut out, chars[i]);
                } else {
                    out.push('\\');
                }
            } else {
                out.push(chars[i]);
            }
            i += 1;
        }
        Ok(out)
    }

    fn parse_interpolation(&self, text: &str, span: Span) -> Result<Expr> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(Error::parse("empty string interpolation `{}`", span));
        }
        let tokens = tokenize(trimmed)
            .map_err(|e| Error::parse(format!("in string interpolation: {}", e.message), span))?;
        let mut p = Parser::new(tokens);
        let expr = p
            .parse_expr()
            .map_err(|e| Error::parse(format!("in string interpolation: {}", e.message), span))?;
        p.skip_separators();
        if !p.check(&Tok::Eof) {
            return Err(Error::parse(
                format!("in string interpolation: unexpected `{}`", trimmed),
                span,
            ));
        }
        Ok(expr)
    }

    // --- Small helpers -----------------------------------------------------

    fn expect_ident(&mut self, what: &str) -> Result<(String, Span)> {
        match self.cur().tok.clone() {
            Tok::Ident(name) => {
                let span = self.advance().span;
                Ok((name, span))
            }
            Tok::Eof => Err(Error::incomplete(
                format!("expected {what}, found end of input"),
                self.cur().span,
            )),
            other => Err(Error::parse(
                format!("expected {what}, found {}", other.describe()),
                self.cur().span,
            )),
        }
    }

    fn peek_tok(&self, ahead: usize) -> &Tok {
        self.tokens
            .get(self.pos + ahead)
            .map(|t| &t.tok)
            .unwrap_or(&Tok::Eof)
    }

    fn prev_span(&self) -> Span {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or_else(|| self.cur().span)
    }
}

impl Tok {
    /// Human wording used in "expected X" parse errors.
    fn symbol_expect(&self) -> String {
        match self {
            Tok::Ident(_) => "an identifier".to_string(),
            other => format!("`{}`", other.symbol_for_expect()),
        }
    }

    fn symbol_for_expect(&self) -> String {
        // `Tok::symbol` is private to lexer; duplicate the handful we need here.
        match self {
            Tok::Assign => "=".into(),
            Tok::LParen => "(".into(),
            Tok::RParen => ")".into(),
            Tok::LBrace => "{".into(),
            Tok::RBrace => "}".into(),
            Tok::LBracket => "[".into(),
            Tok::RBracket => "]".into(),
            Tok::Colon => ":".into(),
            Tok::Comma => ",".into(),
            Tok::In => "in".into(),
            Tok::Times => "times".into(),
            Tok::If => "if".into(),
            Tok::Else => "else".into(),
            Tok::While => "while".into(),
            Tok::For => "for".into(),
            Tok::Repeat => "repeat".into(),
            other => other.describe(),
        }
    }
}

fn is_lvalue(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Var(..) | Expr::Index { .. } | Expr::Field { .. }
    )
}

fn bin(op: BinOp, left: Expr, right: Expr, span: Span) -> Expr {
    Expr::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
        span,
    }
}

fn push_escape(out: &mut String, c: char) {
    match c {
        'n' => out.push('\n'),
        't' => out.push('\t'),
        'r' => out.push('\r'),
        '0' => out.push('\0'),
        other => out.push(other),
    }
}
