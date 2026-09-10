//! Code metrics aligned with phpmd / pdepend / messgo.
//!
//! Cyclomatic and NPath values are pinned to the phpmd 2.15.0 reference
//! fixture (CCN=12, NPath=324) when expressed as equivalent Rust.
//!
//! # Rust adaptations
//!
//! - Decision points: `if` / `if let`, `while` / `while let`, `for`, `loop`,
//!   non-wildcard `match` arms, match guards, `&&`, `||`.
//! - No `catch`, `??`, or C-style ternary; those phpmd points do not apply.
//! - `match` arms map to phpmd/pdepend switch case labels; a lone `_` arm is
//!   the default and does not increment CCN (same as Go `default`).
//! - `for` maps to phpmd foreach / Go range for NPath (`E(iter) + 1 + NP(body)`).
use syn::visit::Visit;
use syn::{BinOp, Block, Expr, ExprIf, Pat, Stmt};

use crate::suppressions::{char_literal_end, lifetime_end, skip_quoted};

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static EFFECTIVE_LINE_SCANS: Cell<usize> = const { Cell::new(0) };
    static EFFECTIVE_LINE_QUERIES: Cell<usize> = const { Cell::new(0) };
}

/// Cyclomatic complexity: base 1 + one per decision point.
pub fn cyclomatic_complexity(body: Option<&Block>) -> usize {
    let Some(body) = body else {
        return 1;
    };
    let mut v = CcnVisitor { ccn: 1 };
    v.visit_block(body);
    v.ccn
}

struct CcnVisitor {
    ccn: usize,
}

impl<'ast> Visit<'ast> for CcnVisitor {
    fn visit_expr_if(&mut self, node: &'ast ExprIf) {
        self.ccn += 1;
        syn::visit::visit_expr_if(self, node);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.ccn += 1;
        syn::visit::visit_expr_while(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.ccn = self.ccn.saturating_add(1);
        syn::visit::visit_expr_for_loop(self, node);
    }

    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.ccn = self.ccn.saturating_add(1);
        syn::visit::visit_expr_loop(self, node);
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        if !is_default_match_pat(&node.pat) {
            self.ccn = self.ccn.saturating_add(1);
        }
        if node.guard.is_some() {
            self.ccn = self.ccn.saturating_add(1);
        }
        syn::visit::visit_arm(self, node);
    }

    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        if matches!(node.op, BinOp::And(_) | BinOp::Or(_)) {
            self.ccn = self.ccn.saturating_add(1);
        }
        syn::visit::visit_expr_binary(self, node);
    }
}

// `_` always parses to `Pat::Wild`; syn rejects `_` as an `Ident` token, so
// there is no `Pat::Ident` case to match here.
fn is_default_match_pat(pat: &Pat) -> bool {
    matches!(pat, Pat::Wild(_))
}

/// NPath complexity (Nejmeh / pdepend), adapted to Rust control flow.
pub fn npath_complexity(body: Option<&Block>) -> usize {
    let Some(body) = body else {
        return 1;
    };
    npath_block(body)
}

fn npath_block(block: &Block) -> usize {
    npath_stmts(&block.stmts)
}

fn npath_stmts(stmts: &[Stmt]) -> usize {
    let mut product = 1usize;
    for s in stmts {
        product = product.saturating_mul(npath_stmt(s));
    }
    product
}

fn npath_stmt(stmt: &Stmt) -> usize {
    match stmt {
        Stmt::Expr(expr, _) => npath_expr(expr),
        Stmt::Macro(m) => {
            // Treat macro invocation statements as opaque linear code.
            let _ = m;
            1
        }
        Stmt::Local(local) => npath_local(local),
        Stmt::Item(_) => 1,
    }
}

fn npath_local(local: &syn::Local) -> usize {
    let Some(init) = &local.init else {
        return 1;
    };
    let initializer = npath_expr(&init.expr);
    match &init.diverge {
        Some((_, diverge)) => initializer.saturating_add(npath_expr(diverge)),
        None => initializer,
    }
}

fn npath_expr(expr: &Expr) -> usize {
    npath_control_flow(expr)
        .or_else(|| npath_block_expression(expr))
        .or_else(|| npath_return_expression(expr))
        .unwrap_or_else(|| npath_nested_expressions(expr))
}

fn npath_nested_expressions(expr: &Expr) -> usize {
    let mut visitor = NpathVisitor { paths: 1 };
    visitor.visit_expr(expr);
    visitor.paths
}

struct NpathVisitor {
    paths: usize,
}

impl<'ast> Visit<'ast> for NpathVisitor {
    fn visit_expr(&mut self, node: &'ast Expr) {
        if let Some(paths) = npath_control_flow(node)
            .or_else(|| npath_block_expression(node))
            .or_else(|| npath_return_expression(node))
        {
            self.paths = self.paths.saturating_mul(paths);
        } else {
            syn::visit::visit_expr(self, node);
        }
    }
}

fn npath_expression_complexity(expr: &Expr) -> usize {
    expression_complexity(expr).max(npath_expr(expr).saturating_sub(1))
}

fn npath_control_flow(expr: &Expr) -> Option<usize> {
    match expr {
        Expr::If(node) => Some(npath_if(node)),
        Expr::Match(node) => Some(npath_match(node)),
        Expr::ForLoop(node) => Some(
            npath_expression_complexity(&node.expr)
                .saturating_add(1)
                .saturating_add(npath_block(&node.body)),
        ),
        Expr::While(node) => Some(
            npath_expression_complexity(&node.cond)
                .saturating_add(1)
                .saturating_add(npath_block(&node.body)),
        ),
        Expr::Loop(node) => Some(1usize.saturating_add(npath_block(&node.body))),
        _ => None,
    }
}

fn npath_block_expression(expr: &Expr) -> Option<usize> {
    match expr {
        Expr::Block(node) => Some(npath_block(&node.block)),
        Expr::Async(node) => Some(npath_block(&node.block)),
        Expr::TryBlock(node) => Some(npath_block(&node.block)),
        Expr::Unsafe(node) => Some(npath_block(&node.block)),
        _ => None,
    }
}

fn npath_return_expression(expr: &Expr) -> Option<usize> {
    let Expr::Return(node) = expr else {
        return None;
    };
    let complexity = node
        .expr
        .as_deref()
        .map(npath_expression_complexity)
        .unwrap_or(0);
    Some(complexity.max(1))
}

fn npath_if(node: &ExprIf) -> usize {
    let expr = npath_expression_complexity(&node.cond);
    let body = npath_block(&node.then_branch);
    // Rust grammar allows only a block or a nested `if` after `else`; no
    // other expression form parses, so there is no third case here.
    let else_part = match &node.else_branch {
        None => 1,
        Some((_, else_expr)) => match else_expr.as_ref() {
            Expr::If(nested) => npath_if(nested),
            Expr::Block(b) => npath_block(&b.block),
            _ => unreachable!("else branch is always a block or an `if`"),
        },
    };
    else_part.saturating_add(body).saturating_add(expr)
}

fn npath_match(node: &syn::ExprMatch) -> usize {
    let mut npath = npath_expression_complexity(&node.expr);
    for arm in &node.arms {
        if let Some((_, guard)) = &arm.guard {
            npath = npath.saturating_add(npath_expression_complexity(guard));
        }
        npath = npath.saturating_add(npath_expr(&arm.body));
    }
    if npath == 0 {
        1
    } else {
        npath
    }
}

/// Counts `&&` and `||` in an expression (pdepend expressionComplexity).
fn expression_complexity(expr: &Expr) -> usize {
    let mut v = BoolOpVisitor { count: 0 };
    v.visit_expr(expr);
    v.count
}

struct BoolOpVisitor {
    count: usize,
}

impl<'ast> Visit<'ast> for BoolOpVisitor {
    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        if matches!(node.op, BinOp::And(_) | BinOp::Or(_)) {
            self.count = self.count.saturating_add(1);
        }
        syn::visit::visit_expr_binary(self, node);
    }
}

pub(crate) fn effective_line_prefix(src: &str) -> Vec<usize> {
    let mut prefix = Vec::new();
    prefix.push(0);
    let mut count = 0usize;
    let mut state = LineState::Code;
    for raw in src.split('\n') {
        #[cfg(test)]
        EFFECTIVE_LINE_SCANS.with(|scans| scans.set(scans.get() + 1));
        let (has_code, after) = line_has_code(raw, state);
        state = after;
        if has_code {
            count += 1;
        }
        prefix.push(count);
    }
    prefix
}

pub(crate) fn effective_line_count(prefix: &[usize], start_line: usize, end_line: usize) -> usize {
    #[cfg(test)]
    EFFECTIVE_LINE_QUERIES.with(|queries| queries.set(queries.get() + 1));
    if start_line == 0 || end_line < start_line {
        return 0;
    }
    let last_line = prefix.len().saturating_sub(1);
    let end = end_line.min(last_line);
    if start_line > end {
        return 0;
    }
    prefix[end].saturating_sub(prefix[start_line - 1])
}

/// Scan state carried from one raw line to the next. String literals are
/// code, not comments: a `//` or `/*` inside a quoted literal must not open
/// comment state, and comment text is not string content, so `"` inside a
/// block comment must not open a string. This mirrors the directive scanner
/// in `suppressions.rs` and rustc's lexer.
#[derive(Clone, Copy)]
enum LineState {
    /// Not inside a comment or a raw string carried over from earlier lines.
    Code,
    /// Inside a block comment that has not closed yet.
    Block,
    /// Inside a raw string that continues on the next line.
    RawString { hashes: usize },
}

fn line_has_code(line: &str, state: LineState) -> (bool, LineState) {
    match state {
        LineState::Block => match line.find("*/") {
            Some(end) => scan_code_line(&line[end + 2..]),
            None => (false, LineState::Block),
        },
        LineState::RawString { hashes } => match raw_string_close(line.as_bytes(), 0, hashes) {
            Some(end) => scan_code_line(&line[end..]),
            None => (contains_code(line), LineState::RawString { hashes }),
        },
        LineState::Code => scan_code_line(line),
    }
}

/// Scans one line in `Code` state. String content counts as code; comment
/// text does not. Comment markers inside string literals are skipped with
/// the literal, so they never open comment state.
fn scan_code_line(line: &str) -> (bool, LineState) {
    let bytes = line.as_bytes();
    let mut has_code = false;
    let mut index = 0;
    while index < bytes.len() {
        match code_token(bytes, index) {
            CodeToken::LineComment => return (has_code, LineState::Code),
            CodeToken::BlockComment => {
                let (code_after, after) = line_has_code(&line[index + 2..], LineState::Block);
                return (has_code || code_after, after);
            }
            CodeToken::QuotedString { end } | CodeToken::RawString { end } => {
                has_code = true;
                index = end;
            }
            CodeToken::CharOrLifetime { end } => index = end,
            CodeToken::RawStringContinues { hashes } => {
                return (true, LineState::RawString { hashes });
            }
            CodeToken::Plain => {
                has_code |= counts_as_code(bytes[index]);
                index += 1;
            }
        }
    }
    (has_code, LineState::Code)
}

/// One scan step in `Code` state, at the byte at `index`.
enum CodeToken {
    LineComment,
    BlockComment,
    /// A quoted string that ends on this line; `end` is the index after it.
    QuotedString {
        end: usize,
    },
    /// A character literal, lifetime, or loop label; `end` is after it.
    CharOrLifetime {
        end: usize,
    },
    /// A raw string that closes on this line; `end` is the index after it.
    RawString {
        end: usize,
    },
    /// A raw string that continues on the next line.
    RawStringContinues {
        hashes: usize,
    },
    /// Any other byte.
    Plain,
}

fn code_token(bytes: &[u8], index: usize) -> CodeToken {
    match bytes[index] {
        b'/' if bytes.get(index + 1) == Some(&b'/') => CodeToken::LineComment,
        b'/' if bytes.get(index + 1) == Some(&b'*') => CodeToken::BlockComment,
        b'"' => CodeToken::QuotedString {
            end: skip_quoted(bytes, index + 1, b'"'),
        },
        b'\'' => CodeToken::CharOrLifetime {
            end: char_literal_end(str_from(bytes, index), index)
                .or_else(|| lifetime_end(str_from(bytes, index), index))
                .unwrap_or(index + 1),
        },
        b'r' | b'b' => string_prefix_token(bytes, index),
        _ => CodeToken::Plain,
    }
}

/// Classifies a `r`, `b`, or `br` token: raw string, byte string, or plain.
fn string_prefix_token(bytes: &[u8], index: usize) -> CodeToken {
    match raw_string_start(bytes, index) {
        Some((quote, hashes)) => match raw_string_close(bytes, quote + 1, hashes) {
            Some(end) => CodeToken::RawString { end },
            None => CodeToken::RawStringContinues { hashes },
        },
        None if bytes[index] == b'b' && bytes.get(index + 1) == Some(&b'"') => {
            CodeToken::QuotedString {
                end: skip_quoted(bytes, index + 2, b'"'),
            }
        }
        None => CodeToken::Plain,
    }
}

/// The source text from `index` to the end of its line, for the
/// `char_literal_end` / `lifetime_end` helpers that expect `str` input.
fn str_from(bytes: &[u8], index: usize) -> &str {
    std::str::from_utf8(&bytes[index..]).unwrap_or("")
}

fn counts_as_code(byte: u8) -> bool {
    !matches!(byte, b' ' | b'\t' | b'\r')
}

/// If the bytes at `index` start a raw string (`r"…"`, `r#"…"#`, `br"…"`,
/// `br#"…"#`), returns the index of its opening quote and the `#` count.
fn raw_string_start(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    let after_prefix = if bytes[index] == b'b' {
        if bytes.get(index + 1) != Some(&b'r') {
            return None;
        }
        index + 2
    } else {
        index + 1
    };
    let mut hashes = 0;
    while bytes.get(after_prefix + hashes) == Some(&b'#') {
        hashes += 1;
    }
    if bytes.get(after_prefix + hashes) != Some(&b'"') {
        return None;
    }
    Some((after_prefix + hashes, hashes))
}

/// Returns the index just after a raw string close (`"` plus `hashes`
/// hashes) found at or after `start`, if the literal ends on this line.
fn raw_string_close(bytes: &[u8], start: usize, hashes: usize) -> Option<usize> {
    let mut cursor = start;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && (0..hashes).all(|offset| bytes.get(cursor + 1 + offset) == Some(&b'#'))
        {
            return Some(cursor + 1 + hashes);
        }
        cursor += 1;
    }
    None
}

fn contains_code(text: &str) -> bool {
    text.bytes().any(counts_as_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn many_one_line_spans_scan_the_source_once() {
        let line_count = 4_000;
        let source = (0..line_count)
            .map(|index| format!("fn function_{index}() {{}}"))
            .collect::<Vec<_>>()
            .join("\n");
        EFFECTIVE_LINE_SCANS.with(|scans| scans.set(0));
        EFFECTIVE_LINE_QUERIES.with(|queries| queries.set(0));

        let prefix = effective_line_prefix(&source);
        for line in 1..=line_count {
            assert_eq!(effective_line_count(&prefix, line, line), 1);
        }

        assert_eq!(EFFECTIVE_LINE_SCANS.with(Cell::get), line_count);
        assert_eq!(EFFECTIVE_LINE_QUERIES.with(Cell::get), line_count);
    }
}
