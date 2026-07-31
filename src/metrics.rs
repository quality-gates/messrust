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

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{BinOp, Block, Expr, ExprIf, Pat, Stmt};

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
        self.ccn += 1;
        syn::visit::visit_expr_for_loop(self, node);
    }

    fn visit_expr_loop(&mut self, node: &'ast syn::ExprLoop) {
        self.ccn += 1;
        syn::visit::visit_expr_loop(self, node);
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        if !is_default_match_pat(&node.pat) {
            self.ccn += 1;
        }
        if node.guard.is_some() {
            self.ccn += 1;
        }
        syn::visit::visit_arm(self, node);
    }

    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        if matches!(node.op, BinOp::And(_) | BinOp::Or(_)) {
            self.ccn += 1;
        }
        syn::visit::visit_expr_binary(self, node);
    }
}

fn is_default_match_pat(pat: &Pat) -> bool {
    match pat {
        Pat::Wild(_) => true,
        Pat::Ident(id) if id.ident == "_" => true,
        _ => false,
    }
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
        Stmt::Expr(expr, _) => npath_expr_stmt(expr),
        Stmt::Macro(m) => {
            // Treat macro invocation statements as opaque linear code.
            let _ = m;
            1
        }
        Stmt::Local(_) | Stmt::Item(_) => 1,
    }
}

fn npath_expr_stmt(expr: &Expr) -> usize {
    match expr {
        Expr::If(i) => npath_if(i),
        Expr::Match(m) => npath_match(m),
        Expr::ForLoop(f) => {
            expression_complexity(&f.expr) + 1 + npath_block(&f.body)
        }
        Expr::While(w) => expression_complexity(&w.cond) + 1 + npath_block(&w.body),
        Expr::Loop(l) => 1 + npath_block(&l.body),
        Expr::Block(b) => npath_block(&b.block),
        Expr::Return(r) => match &r.expr {
            None => 1,
            Some(e) => {
                let c = expression_complexity(e);
                if c == 0 {
                    1
                } else {
                    c
                }
            }
        },
        Expr::Async(a) => npath_block(&a.block),
        Expr::TryBlock(t) => npath_block(&t.block),
        Expr::Unsafe(u) => npath_block(&u.block),
        _ => 1,
    }
}

fn npath_if(node: &ExprIf) -> usize {
    let expr = expression_complexity(&node.cond);
    let body = npath_block(&node.then_branch);
    let else_part = match &node.else_branch {
        None => 1,
        Some((_, else_expr)) => match else_expr.as_ref() {
            Expr::If(nested) => npath_if(nested),
            Expr::Block(b) => npath_block(&b.block),
            other => npath_expr_stmt(other),
        },
    };
    else_part + body + expr
}

fn npath_match(node: &syn::ExprMatch) -> usize {
    let mut npath = expression_complexity(&node.expr);
    for arm in &node.arms {
        if let Some((_, guard)) = &arm.guard {
            npath += expression_complexity(guard);
        }
        npath += npath_expr_stmt(&arm.body);
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
            self.count += 1;
        }
        syn::visit::visit_expr_binary(self, node);
    }
}

/// Inclusive source lines spanned by a syn node (PHPMD `loc`).
#[allow(dead_code)]
pub fn lines_of_code<T: Spanned>(node: &T) -> usize {
    let start = node.span().start().line;
    let end = node.span().end().line;
    end.saturating_sub(start).saturating_add(1)
}

/// Effective lines of code: skip blank and comment-only lines (PHPMD `eloc`).
pub fn effective_lines_of_code(src: &str, start_line: usize, end_line: usize) -> usize {
    if start_line == 0 || end_line < start_line {
        return 0;
    }
    let mut count = 0usize;
    let mut in_block = false;
    for (idx, raw) in src.split('\n').enumerate() {
        let line_no = idx + 1;
        if line_no > end_line {
            break;
        }
        let (has_code, after) = line_has_code(raw, in_block);
        in_block = after;
        if line_no >= start_line && has_code {
            count += 1;
        }
    }
    count
}

fn line_has_code(line: &str, mut in_block: bool) -> (bool, bool) {
    let bytes = line.as_bytes();
    let mut has_code = false;
    let mut i = 0;
    while i < bytes.len() {
        if in_block {
            if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                in_block = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                return (has_code, false);
            }
            if bytes[i + 1] == b'*' {
                in_block = true;
                i += 2;
                continue;
            }
        }
        if bytes[i] != b' ' && bytes[i] != b'\t' && bytes[i] != b'\r' {
            has_code = true;
        }
        i += 1;
    }
    (has_code, in_block)
}
