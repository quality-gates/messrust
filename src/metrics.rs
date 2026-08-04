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
    npath_control_flow(expr)
        .or_else(|| npath_block_expression(expr))
        .or_else(|| npath_return_expression(expr))
        .unwrap_or(1)
}

fn npath_control_flow(expr: &Expr) -> Option<usize> {
    match expr {
        Expr::If(node) => Some(npath_if(node)),
        Expr::Match(node) => Some(npath_match(node)),
        Expr::ForLoop(node) => {
            Some(expression_complexity(&node.expr) + 1 + npath_block(&node.body))
        }
        Expr::While(node) => Some(expression_complexity(&node.cond) + 1 + npath_block(&node.body)),
        Expr::Loop(node) => Some(1 + npath_block(&node.body)),
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
    let complexity = node.expr.as_deref().map(expression_complexity).unwrap_or(0);
    Some(complexity.max(1))
}

fn npath_if(node: &ExprIf) -> usize {
    let expr = expression_complexity(&node.cond);
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

fn line_has_code(line: &str, in_block: bool) -> (bool, bool) {
    if !in_block {
        return scan_visible_line(line);
    }
    match line.find("*/") {
        Some(end) => scan_visible_line(&line[end + 2..]),
        None => (false, true),
    }
}

fn scan_visible_line(line: &str) -> (bool, bool) {
    let line_comment = line.find("//");
    let block_comment = line.find("/*");
    if line_comment
        .is_some_and(|line_pos| block_comment.is_none_or(|block_pos| line_pos < block_pos))
    {
        let visible = &line[..line_comment.unwrap()];
        return (contains_code(visible), false);
    }
    let Some(start) = block_comment else {
        return (contains_code(line), false);
    };
    let before_has_code = contains_code(&line[..start]);
    let after_start = start + 2;
    match line[after_start..].find("*/") {
        Some(relative_end) => {
            let (after_has_code, in_block) =
                scan_visible_line(&line[after_start + relative_end + 2..]);
            (before_has_code || after_has_code, in_block)
        }
        None => (before_has_code, true),
    }
}

fn contains_code(text: &str) -> bool {
    text.bytes()
        .any(|byte| !matches!(byte, b' ' | b'\t' | b'\r'))
}
