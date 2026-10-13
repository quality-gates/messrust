use syn::{Expr, ExprLit, Lit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BranchReachability {
    ThenOnly,
    ElseOnly,
    Both,
}

pub(super) fn branch_reachability(condition: &Expr) -> BranchReachability {
    match condition {
        Expr::Lit(ExprLit {
            lit: Lit::Bool(value),
            ..
        }) if value.value() => BranchReachability::ThenOnly,
        Expr::Lit(ExprLit {
            lit: Lit::Bool(value),
            ..
        }) if !value.value() => BranchReachability::ElseOnly,
        Expr::Paren(paren) => branch_reachability(&paren.expr),
        Expr::Group(group) => branch_reachability(&group.expr),
        _ => BranchReachability::Both,
    }
}
