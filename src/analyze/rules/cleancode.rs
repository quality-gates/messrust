//! Cleancode rule handlers and body walkers.

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::ItemFn;

use crate::report::Violation;
use crate::ruleset::LoadedRule;

use crate::analyze::helpers::{
    compile_phpmd_regex, format_message, ignored_name, is_rust_unused_name, name_violation,
    property_list,
};
use crate::analyze::model::FileModel;


pub(crate) fn apply_boolean_argument_flag(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let exceptions = property_list(rule, "exceptions");
    let ignore = compile_phpmd_regex(
        rule.properties
            .get("ignorepattern")
            .map(String::as_str)
            .unwrap_or(""),
    );
    for f in &model.functions {
        if ignored_name(&ignore, &f.name) {
            continue;
        }
        if let Some(parent) = &f.parent {
            if exceptions.iter().any(|e| e == parent) {
                continue;
            }
        }
        let image = match &f.parent {
            Some(parent) => format!("{parent}::{}", f.name),
            None => f.name.clone(),
        };
        for p in &f.bool_params {
            if is_rust_unused_name(&p.name) {
                continue;
            }
            out.push(name_violation(
                rule,
                file,
                p.begin_line,
                format_message(&rule.message, &[&image, &p.name]),
            ));
        }
    }
}


pub(crate) fn apply_else_expression(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for f in &model.functions {
        let Some(body) = f.body else {
            continue;
        };
        for line in terminal_else_lines(body) {
            out.push(name_violation(
                rule,
                file,
                line,
                format_message(&rule.message, &[&f.name]),
            ));
        }
    }
}


pub(crate) fn apply_if_statement_assignment(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for f in &model.functions {
        let Some(body) = f.body else {
            continue;
        };
        for pos in assignment_in_condition_positions(body) {
            out.push(name_violation(
                rule,
                file,
                pos.line,
                format_message(
                    &rule.message,
                    &[&pos.line.to_string(), &pos.column.to_string()],
                ),
            ));
        }
    }
}


pub(crate) fn apply_duplicated_array_key(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for dup in &model.duplicate_struct_keys {
        out.push(name_violation(
            rule,
            file,
            dup.line,
            format_message(&rule.message, &[&dup.display, &dup.first_line.to_string()]),
        ));
    }
}


pub(crate) fn apply_static_access(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let exceptions = property_list(rule, "exceptions");
    let ignore = compile_phpmd_regex(
        rule.properties
            .get("ignorepattern")
            .map(String::as_str)
            .unwrap_or(""),
    );
    for f in &model.functions {
        if ignored_name(&ignore, &f.name) {
            continue;
        }
        let Some(body) = f.body else {
            continue;
        };
        for access in static_accesses(body, f.parent.as_deref(), &exceptions) {
            out.push(name_violation(
                rule,
                file,
                access.line,
                format_message(&rule.message, &[&access.type_name, &f.name]),
            ));
        }
    }
}


pub(crate) fn terminal_else_lines(body: &syn::Block) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut visitor = ElseCollector { lines: &mut lines };
    visitor.visit_block(body);
    lines
}


pub(crate) struct ElseCollector<'a> {
    pub(crate) lines: &'a mut Vec<usize>,
}


impl<'ast> Visit<'ast> for ElseCollector<'_> {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        if let Some((_, else_branch)) = &node.else_branch {
            if !matches!(else_branch.as_ref(), syn::Expr::If(_)) {
                self.lines.push(else_branch.span().start().line);
            }
        }
        syn::visit::visit_expr_if(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}


pub(crate) fn assignment_in_condition_positions(body: &syn::Block) -> Vec<SourcePos> {
    let mut positions = Vec::new();
    let mut visitor = CondAssignCollector {
        positions: &mut positions,
    };
    visitor.visit_block(body);
    positions
}


pub(crate) struct CondAssignCollector<'a> {
    pub(crate) positions: &'a mut Vec<SourcePos>,
}


impl CondAssignCollector<'_> {
    pub(crate) fn scan_condition(&mut self, cond: &syn::Expr) {
        if matches!(cond, syn::Expr::Let(_)) {
            return;
        }
        let mut finder = AssignFinder {
            positions: self.positions,
        };
        finder.visit_expr(cond);
    }
}


impl<'ast> Visit<'ast> for CondAssignCollector<'_> {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.scan_condition(&node.cond);
        self.visit_block(&node.then_branch);
        if let Some((_, else_branch)) = &node.else_branch {
            self.visit_expr(else_branch);
        }
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.scan_condition(&node.cond);
        self.visit_block(&node.body);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}


pub(crate) struct AssignFinder<'a> {
    pub(crate) positions: &'a mut Vec<SourcePos>,
}


impl<'ast> Visit<'ast> for AssignFinder<'_> {
    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        let start = node.span().start();
        self.positions.push(SourcePos {
            line: start.line,
            column: start.column + 1,
        });
        syn::visit::visit_expr_assign(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}


pub(crate) fn static_accesses(
    body: &syn::Block,
    parent: Option<&str>,
    exceptions: &[String],
) -> Vec<StaticAccessHit> {
    let mut hits = Vec::new();
    let mut visitor = StaticAccessCollector {
        parent,
        exceptions,
        hits: &mut hits,
    };
    visitor.visit_block(body);
    hits
}


pub(crate) struct StaticAccessCollector<'a> {
    pub(crate) parent: Option<&'a str>,
    pub(crate) exceptions: &'a [String],
    pub(crate) hits: &'a mut Vec<StaticAccessHit>,
}


impl StaticAccessCollector<'_> {
    pub(crate) fn consider_path(&mut self, path: &syn::Path, line: usize) {
        let Some(type_name) = static_receiver_type(path) else {
            return;
        };
        if type_name == "Self" {
            return;
        }
        if self.parent == Some(type_name.as_str()) {
            return;
        }
        if self.exceptions.iter().any(|e| e == &type_name) {
            return;
        }
        self.hits.push(StaticAccessHit { type_name, line });
    }
}


impl<'ast> Visit<'ast> for StaticAccessCollector<'_> {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            self.consider_path(&p.path, p.span().start().line);
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}


pub(crate) fn static_receiver_type(path: &syn::Path) -> Option<String> {
    if path.segments.len() < 2 {
        return None;
    }
    // Prefer the rightmost PascalCase segment before the final call name.
    let mut segs: Vec<_> = path.segments.iter().collect();
    segs.pop()?; // method / associated fn name
    for seg in segs.into_iter().rev() {
        let name = seg.ident.to_string();
        if name == "Self" {
            return Some(name);
        }
        if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            return Some(name);
        }
    }
    None
}


pub(crate) struct SourcePos {
    pub(crate) line: usize,
    pub(crate) column: usize,
}


pub(crate) struct StaticAccessHit {
    pub(crate) type_name: String,
    pub(crate) line: usize,
}

