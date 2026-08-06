//! Use-def collection for unused-code rules.

use std::collections::HashSet;
use std::sync::OnceLock;

use proc_macro2::{TokenStream, TokenTree};
use regex::Regex;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{FnArg, ItemEnum, ItemFn, ItemImpl, ItemStruct, ItemUnion, Member};

use crate::analyze::helpers::is_private;

use super::{NamedSite, UseDefModel};

pub(crate) struct UseDefCollector {
    pub(crate) model: UseDefModel,
    pub(crate) binding_mode: BindingMode,
    pub(crate) in_trait_impl: bool,
    pub(crate) derived_fields_are_used: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]


pub(crate) enum BindingMode {
    None,
    Local,
    Param,
}


impl UseDefCollector {
    pub(crate) fn new() -> Self {
        Self {
            model: UseDefModel::default(),
            binding_mode: BindingMode::None,
            in_trait_impl: false,
            derived_fields_are_used: false,
        }
    }

    pub(crate) fn into_model(self) -> UseDefModel {
        self.model
    }
}


pub(crate) fn with_binding_mode<F>(collector: &mut UseDefCollector, mode: BindingMode, visit: F)
where
    F: FnOnce(&mut UseDefCollector),
{
    let previous = collector.binding_mode;
    collector.binding_mode = mode;
    visit(collector);
    collector.binding_mode = previous;
}


pub(crate) fn record_params_from_sig(collector: &mut UseDefCollector, signature: &syn::Signature) {
    for input in &signature.inputs {
        if let FnArg::Typed(parameter) = input {
            with_binding_mode(collector, BindingMode::Param, |visitor| {
                visitor.visit_pat(&parameter.pat)
            });
        }
    }
}


pub(crate) fn visit_assignment_target(collector: &mut UseDefCollector, target: &syn::Expr) {
    match target {
        syn::Expr::Path(_) | syn::Expr::Infer(_) => {}
        syn::Expr::Tuple(tuple) => {
            for element in &tuple.elems {
                visit_assignment_target(collector, element);
            }
        }
        syn::Expr::Array(array) => {
            for element in &array.elems {
                visit_assignment_target(collector, element);
            }
        }
        syn::Expr::Struct(structure) => {
            for field in &structure.fields {
                visit_assignment_target(collector, &field.expr);
            }
        }
        _ => visit_assignment_place(collector, target),
    }
}


pub(crate) fn visit_assignment_place(collector: &mut UseDefCollector, target: &syn::Expr) {
    match target {
        syn::Expr::Field(field) => collector.visit_expr(&field.base),
        syn::Expr::Index(index) => {
            collector.visit_expr(&index.expr);
            collector.visit_expr(&index.index);
        }
        syn::Expr::Paren(paren) => visit_assignment_target(collector, &paren.expr),
        _ => collector.visit_expr(target),
    }
}


impl<'ast> Visit<'ast> for UseDefCollector {
    fn visit_local(&mut self, node: &'ast syn::Local) {
        with_binding_mode(self, BindingMode::Local, |visitor| {
            visitor.visit_pat(&node.pat)
        });
        if let Some(init) = &node.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        record_params_from_sig(self, &node.sig);
        self.visit_block(&node.block);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        self.in_trait_impl = node.trait_.is_some();
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        self.derived_fields_are_used = derive_uses_fields(&node.attrs);
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        self.derived_fields_are_used = derive_uses_fields(&node.attrs);
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_union(&mut self, node: &'ast ItemUnion) {
        self.derived_fields_are_used = derive_uses_fields(&node.attrs);
        syn::visit::visit_item_union(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if !self.in_trait_impl && is_private(&node.vis) {
            self.model.private_methods.push(NamedSite {
                name: node.sig.ident.to_string(),
                begin_line: node.sig.fn_token.span().start().line,
            });
        }
        record_params_from_sig(self, &node.sig);
        self.visit_block(&node.block);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        record_params_from_sig(self, &node.sig);
        if let Some(body) = &node.default {
            self.visit_block(body);
        }
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        if let Some(ident) = &node.ident {
            if is_private(&node.vis) && !self.derived_fields_are_used {
                self.model.private_fields.push(NamedSite {
                    name: ident.to_string(),
                    begin_line: ident.span().start().line,
                });
            }
        }
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        with_binding_mode(self, BindingMode::Local, |visitor| {
            visitor.visit_pat(&node.pat)
        });
        if let Some((_, guard)) = &node.guard {
            self.visit_expr(guard);
        }
        self.visit_expr(&node.body);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        with_binding_mode(self, BindingMode::Local, |visitor| {
            visitor.visit_pat(&node.pat)
        });
        self.visit_expr(&node.expr);
        self.visit_block(&node.body);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        if let syn::Expr::Let(l) = &*node.cond {
            with_binding_mode(self, BindingMode::Local, |visitor| {
                visitor.visit_pat(&l.pat)
            });
            self.visit_expr(&l.expr);
        } else {
            self.visit_expr(&node.cond);
        }
        self.visit_block(&node.body);
    }

    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        if let syn::Expr::Let(l) = &*node.cond {
            with_binding_mode(self, BindingMode::Local, |visitor| {
                visitor.visit_pat(&l.pat)
            });
            self.visit_expr(&l.expr);
        } else {
            self.visit_expr(&node.cond);
        }
        self.visit_block(&node.then_branch);
        if let Some((_, else_branch)) = &node.else_branch {
            self.visit_expr(else_branch);
        }
    }

    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        let name = node.ident.to_string();
        if is_binding_name(&name) {
            let site = NamedSite {
                name,
                begin_line: node.ident.span().start().line,
            };
            match self.binding_mode {
                BindingMode::Local => self.model.locals.push(site),
                BindingMode::Param => self.model.params.push(site),
                BindingMode::None => {}
            }
        }
        if let Some((_, sub)) = &node.subpat {
            self.visit_pat(sub);
        }
    }

    fn visit_field_pat(&mut self, node: &'ast syn::FieldPat) {
        if let Member::Named(ident) = &node.member {
            self.model.field_reads.insert(ident.to_string());
        }
        self.visit_pat(&node.pat);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if let Some(ident) = path_single_ident(node) {
            self.model.ident_reads.insert(ident);
        } else if let Some(ident) = path_last_ident(node) {
            self.model.method_calls.insert(ident);
        }
    }

    fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
        self.visit_expr(&node.base);
        if let Member::Named(ident) = &node.member {
            self.model.field_reads.insert(ident.to_string());
        }
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.model.method_calls.insert(node.method.to_string());
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        visit_assignment_target(self, &node.left);
        self.visit_expr(&node.right);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        collect_macro_reads(
            node.tokens.clone(),
            &mut self.model.ident_reads,
            &mut self.model.field_reads,
        );
        if is_format_macro(node) {
            collect_format_captures(node.tokens.clone(), &mut self.model.ident_reads);
        }
    }
}


pub(crate) fn collect_macro_reads(
    tokens: TokenStream,
    ident_reads: &mut HashSet<String>,
    field_reads: &mut HashSet<String>,
) {
    let mut after_dot = false;
    for token in tokens {
        match token {
            TokenTree::Group(group) => {
                collect_macro_reads(group.stream(), ident_reads, field_reads);
                after_dot = false;
            }
            TokenTree::Ident(ident) => {
                let name = ident.to_string();
                ident_reads.insert(name.clone());
                if after_dot {
                    field_reads.insert(name);
                }
                after_dot = false;
            }
            TokenTree::Punct(punctuation) => after_dot = punctuation.as_char() == '.',
            _ => after_dot = false,
        }
    }
}


pub(crate) fn derive_uses_fields(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        if !attribute.path().is_ident("derive") {
            return false;
        }
        let mut uses_fields = false;
        let _ = attribute.parse_nested_meta(|meta| {
            let name = meta.path.segments.last().map(|segment| &segment.ident);
            if name.is_some_and(|name| name == "Serialize" || name == "Deserialize") {
                uses_fields = true;
            }
            Ok(())
        });
        uses_fields
    })
}


pub(crate) fn is_format_macro(node: &syn::Macro) -> bool {
    let Some(name) = node
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
    else {
        return false;
    };
    matches!(
        name.as_str(),
        "format"
            | "format_args"
            | "print"
            | "println"
            | "eprint"
            | "eprintln"
            | "write"
            | "writeln"
            | "panic"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
    )
}


pub(crate) fn collect_format_captures(tokens: TokenStream, reads: &mut HashSet<String>) {
    for token in tokens {
        match token {
            TokenTree::Group(group) => collect_format_captures(group.stream(), reads),
            TokenTree::Literal(literal) => {
                let Ok(syn::Lit::Str(value)) = syn::parse_str::<syn::Lit>(&literal.to_string())
                else {
                    continue;
                };
                reads.extend(format_capture_names(&value.value()));
            }
            _ => {}
        }
    }
}


pub(crate) fn format_capture_names(format: &str) -> Vec<String> {
    static CAPTURE: OnceLock<Regex> = OnceLock::new();
    let capture = CAPTURE.get_or_init(|| {
        Regex::new(r"\{([A-Za-z_][A-Za-z0-9_]*)(?:[}:])").expect("valid format capture regex")
    });
    let unescaped = format.replace("{{", "");
    capture
        .captures_iter(&unescaped)
        .filter_map(|captures| captures.get(1).map(|name| name.as_str().to_string()))
        .collect()
}


pub(crate) fn path_single_ident(path: &syn::ExprPath) -> Option<String> {
    if path.qself.is_some() {
        return None;
    }
    let mut segs = path.path.segments.iter();
    let first = segs.next()?;
    if segs.next().is_some() {
        return None;
    }
    Some(first.ident.to_string())
}


pub(crate) fn path_last_ident(path: &syn::ExprPath) -> Option<String> {
    path.path.segments.last().map(|s| s.ident.to_string())
}


pub(crate) fn is_binding_name(name: &str) -> bool {
    // Syn never feeds the `self` receiver through PatIdent bindings we record.
    name.starts_with(|ch: char| ch.is_lowercase() || ch == '_')
}


