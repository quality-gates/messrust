use std::collections::{BTreeMap, BTreeSet, HashSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, Pat};

use super::super::{collect_token_idents, patterns, storage};
use super::{
    empty_summary, owner_for_synthetic, qualify, signature_params, solver, Binding, Bindings, Body,
    Callable, Collector, Owner, ValueType,
};
use storage::{Allocation, Capture};

pub(super) fn capture_key(target: usize, name: &str) -> String {
    format!("$capture{target}:{name}")
}

pub(super) fn note_return_value<'a>(collector: &mut Collector<'a, '_>, expr: &'a Expr) {
    let returned = binding(collector, expr);
    collector.return_binding = collector.return_binding.clone().join(returned);
}

pub(super) fn binding<'a>(collector: &mut Collector<'a, '_>, expr: &'a Expr) -> Binding {
    if let Some(value) = wrapped_value(collector, expr) {
        return value;
    }
    match expr {
        Expr::Path(path) => super::values::path_binding(collector, path),
        Expr::Closure(closure) => super::values::closure(collector, closure),
        Expr::Async(node) => super::values::async_block(collector, node),
        Expr::Call(call) => super::values::call_value(collector, call),
        Expr::MethodCall(call) => super::calls::method_value(collector, call),
        _ => compound_value(collector, expr),
    }
}

fn compound_value<'a>(collector: &mut Collector<'a, '_>, expr: &'a Expr) -> Binding {
    match expr {
        Expr::If(node) => if_value(collector, node),
        Expr::Match(node) => match_value(collector, node),
        Expr::Block(node) => block_value(collector, &node.block),
        Expr::Struct(node) => struct_value(collector, node),
        _ => storage::expression(
            expr,
            &collector.bindings,
            collector.model,
            collector.scope(),
        ),
    }
}

fn struct_value<'a>(collector: &mut Collector<'a, '_>, node: &'a syn::ExprStruct) -> Binding {
    let mut value = storage::expression(
        &Expr::Struct(node.clone()),
        &collector.bindings,
        collector.model,
        collector.scope(),
    );
    let ValueType::Custom(key) = &value.ty else {
        return value;
    };
    let Some(fields) = collector.model.types.iter().find(|ty| &ty.key == key)
        .map(|ty| ty.fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>()) else {
        return value;
    };
    value.parts = node.rest.as_ref()
        .map(|rest| binding(collector, rest).parts)
        .filter(|parts| parts.len() == fields.len())
        .unwrap_or_else(|| vec![Binding::default(); fields.len()]);
    for field in &node.fields {
        let name = match &field.member {
            syn::Member::Named(name) => name.to_string(),
            syn::Member::Unnamed(index) => index.index.to_string(),
        };
        if let Some(index) = fields.iter().position(|field| field == &name) {
            value.parts[index] = binding(collector, &field.expr);
        }
    }
    if let Some(name) = super::deref_mut_field(collector, &value) {
        if let Some(index) = fields.iter().position(|field| field == &name) {
            value.elements.extend(value.parts[index].outer.iter().cloned());
        }
    }
    value
}

fn wrapped_value<'a>(collector: &mut Collector<'a, '_>, expr: &'a Expr) -> Option<Binding> {
    match expr {
        Expr::Paren(node) => Some(binding(collector, &node.expr)),
        Expr::Group(node) => Some(binding(collector, &node.expr)),
        Expr::Reference(node) => Some(binding(collector, &node.expr)),
        Expr::Unary(node) => Some(binding(collector, &node.expr)),
        Expr::Try(node) => Some(binding(collector, &node.expr).indexed()),
        Expr::Index(node) => Some(binding(collector, &node.expr).indexed()),
        Expr::Await(node) => Some(await_value(collector, node)),
        Expr::Tuple(node) => {
            let parts: Vec<_> = node
                .elems
                .iter()
                .map(|expr| binding(collector, expr))
                .collect();
            Some(Binding {
                ty: ValueType::Tuple(parts.iter().map(|part| part.ty.clone()).collect()),
                parts,
                ..Binding::default()
            })
        }
        _ => None,
    }
}

pub(super) fn path_binding(collector: &Collector<'_, '_>, path: &syn::ExprPath) -> Binding {
    let mut value = storage::expression(
        &Expr::Path(path.clone()),
        &collector.bindings,
        collector.model,
        collector.scope(),
    );
    let text = storage::path_text(&path.path);
    if path.path.segments.len() == 1 && collector.bindings.contains_key(&text) {
        return value;
    }
    if let Some(target) = super::values::resolve(collector, path) {
        value.callables.insert(target, Bindings::new());
    } else if value == Binding::default() {
        value.unresolved = true;
    }
    value
}

pub(super) fn resolve(collector: &Collector<'_, '_>, path: &syn::ExprPath) -> Option<usize> {
    if path.qself.is_some() {
        return None;
    }
    let text = storage::path_text(&path.path);
    let key = super::resolve_type_path(&text, collector.scope(), &collector.model.imports);
    let nested = qualify(&collector.tasks[collector.current].key, &text);
    let matches: Vec<_> = collector
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| task.key == nested || task.key == key)
        .map(|(index, _)| index)
        .collect();
    (matches.len() == 1).then(|| matches[0])
}

pub(super) fn call_value<'a>(
    collector: &mut Collector<'a, '_>,
    call: &'a syn::ExprCall,
) -> Binding {
    let callable = super::values::binding(collector, &call.func);
    if callable.callables.is_empty() {
        return storage::expression(
            &Expr::Call(call.clone()),
            &collector.bindings,
            collector.model,
            collector.scope(),
        );
    }
    let args: Vec<_> = call
        .args
        .iter()
        .map(|arg| super::values::binding(collector, arg))
        .collect();
    let mut returned = Binding::default();
    for (target, mut captures) in callable.callables {
        captures.extend(
            collector.tasks[target]
                .params
                .iter()
                .cloned()
                .zip(args.clone()),
        );
        let value = if collector.tasks[target].asynchronous {
            Binding {
                ty: ValueType::Future,
                deferred: BTreeMap::from([(target, captures)]),
                ..Binding::default()
            }
        } else {
            super::values::return_value(collector, target, &captures)
        };
        returned = returned.join(value);
    }
    returned.unresolved |= callable.unresolved;
    returned
}

pub(super) fn return_value(
    collector: &Collector<'_, '_>,
    target: usize,
    arguments: &Bindings,
) -> Binding {
    let mut value = collector
        .previous
        .get(target)
        .map(|summary| solver::substitute_binding(&summary.return_binding, arguments))
        .unwrap_or_default();
    if matches!(value.ty, ValueType::Unknown) {
        if let Some(signature) = collector.tasks[target].signature {
            if let syn::ReturnType::Type(_, ty) = &signature.output {
                value.ty = storage::value_type(ty, collector.model, &collector.tasks[target].scope);
            }
        }
    }
    value.outer.retain(
        |owner| !matches!(owner, Owner::Private(allocation) if allocation.callable == target),
    );
    value
}

pub(super) fn await_value<'a>(
    collector: &mut Collector<'a, '_>,
    node: &'a syn::ExprAwait,
) -> Binding {
    let future = super::values::binding(collector, &node.base);
    future
        .deferred
        .iter()
        .fold(Binding::default(), |value, (target, args)| {
            value.join(super::values::return_value(collector, *target, args))
        })
}

pub(super) fn closure<'a>(
    collector: &mut Collector<'a, '_>,
    closure: &'a syn::ExprClosure,
) -> Binding {
    let address = closure as *const _ as usize;
    let params: Vec<_> = closure
        .inputs
        .iter()
        .enumerate()
        .map(|(index, pat)| patterns::name(pat, index))
        .collect();
    let (target, captures) = super::values::synthetic(
        collector,
        address,
        "<lambda>",
        Body::Expression(&closure.body),
        params.clone(),
        false,
    );
    for (pat, name) in closure.inputs.iter().zip(params) {
        let mut binding = match pat {
            Pat::Type(typed) => {
                storage::parameter(&name, &typed.ty, collector.model, collector.scope())
            }
            _ => Binding {
                outer: BTreeSet::from([Owner::Parameter(name.clone())]),
                callback: Some(name.clone()),
                ..Binding::default()
            },
        };
        if let Some(known) = collector.tasks[target].bindings.get(&name) {
            if matches!(binding.ty, ValueType::Unknown) {
                binding.ty = known.ty.clone();
            }
        }
        let scope = collector.scope().to_string();
        patterns::bind(
            pat,
            binding,
            &mut collector.tasks[target].bindings,
            Some((collector.model, &scope)),
        );
    }
    Binding {
        callables: BTreeMap::from([(target, captures)]),
        ..Binding::default()
    }
}

pub(super) fn async_block<'a>(
    collector: &mut Collector<'a, '_>,
    node: &'a syn::ExprAsync,
) -> Binding {
    let (target, captures) = super::values::synthetic(
        collector,
        node as *const _ as usize,
        "<async>",
        Body::Block(&node.block),
        Vec::new(),
        true,
    );
    Binding {
        ty: ValueType::Future,
        deferred: BTreeMap::from([(target, captures)]),
        ..Binding::default()
    }
}

pub(super) fn synthetic<'a>(
    collector: &mut Collector<'a, '_>,
    address: usize,
    name: &str,
    body: Body<'a>,
    params: Vec<String>,
    asynchronous: bool,
) -> (usize, Bindings) {
    let target = collector
        .tasks
        .iter()
        .position(|task| task.origin == Some(address))
        .unwrap_or(collector.tasks.len());
    if target == collector.tasks.len() {
        collector.tasks.push(Callable {
            summary: empty_summary(
                collector.tasks[collector.current].summary.function,
                name.to_string(),
                owner_for_synthetic(name.to_string()),
            ),
            key: format!(
                "{}::<body {target}>",
                collector.tasks[collector.current].key
            ),
            scope: collector.scope().to_string(),
            params,
            signature: None,
            body: Some(body),
            bindings: Bindings::new(),
            bound_params: Bindings::new(),
            origin: Some(address),
            asynchronous,
        });
    }
    let mut captures = Bindings::new();
    for name in free_names(body) {
        if let Some(binding) = collector.bindings.get(&name) {
            captures.insert(capture_key(target, &name), binding.clone());
            collector.tasks[target]
                .bindings
                .insert(name.clone(), capture_binding(binding, target, &name));
        }
    }
    (target, captures)
}

pub(super) fn nested_function<'a>(collector: &mut Collector<'a, '_>, function: &'a syn::ItemFn) {
    let key = qualify(
        &collector.tasks[collector.current].key,
        &function.sig.ident.to_string(),
    );
    if collector.tasks.iter().any(|task| task.key == key) {
        return;
    }
    let name = function.sig.ident.to_string();
    collector.tasks.push(Callable {
        summary: empty_summary(
            collector.tasks[collector.current].summary.function,
            name.clone(),
            owner_for_synthetic(name),
        ),
        key,
        scope: collector.scope().to_string(),
        params: signature_params(&function.sig),
        signature: Some(&function.sig),
        body: Some(Body::Block(&function.block)),
        bindings: Bindings::new(),
        bound_params: Bindings::new(),
        origin: None,
        asynchronous: function.sig.asyncness.is_some(),
    });
}

pub(super) fn assign_private_origins(collector: &mut Collector<'_, '_>, pat: &Pat, line: usize) {
    let mutable = mutable_pattern(pat);
    for name in patterns::names(pat) {
        if let Some(binding) = collector.bindings.get_mut(&name) {
            if binding.outer.is_empty()
                && (mutable
                    || matches!(
                        binding.ty,
                        ValueType::Sequence(_) | ValueType::Shared(_) | ValueType::Interior(_)
                    ))
            {
                binding.outer.insert(Owner::Private(Allocation {
                    callable: collector.current,
                    key: format!("{line}:{}:{name}", pat.span().start().column),
                    name,
                }));
            }
        }
    }
}

pub(super) fn bind_condition<'a>(collector: &mut Collector<'a, '_>, expr: &'a Expr) {
    if let Expr::Let(node) = expr {
        let value = super::values::binding(collector, &node.expr);
        collector.bind_pattern(&node.pat, value);
    }
}

pub(super) fn if_value<'a>(collector: &mut Collector<'a, '_>, node: &'a syn::ExprIf) -> Binding {
    let before = collector.bindings.clone();
    super::values::bind_condition(collector, &node.cond);
    let then_value =
        if super::branch_reachability(&node.cond) != super::BranchReachability::ElseOnly {
            super::values::block_value(collector, &node.then_branch)
        } else {
            Binding::default()
        };
    collector.bindings = before;
    let else_value =
        if super::branch_reachability(&node.cond) != super::BranchReachability::ThenOnly {
            node.else_branch
                .as_ref()
                .map(|(_, expr)| super::values::binding(collector, expr))
                .unwrap_or_default()
        } else {
            Binding::default()
        };
    then_value.join(else_value)
}

pub(super) fn match_value<'a>(
    collector: &mut Collector<'a, '_>,
    node: &'a syn::ExprMatch,
) -> Binding {
    let before = collector.bindings.clone();
    let source = super::values::binding(collector, &node.expr);
    let mut result = Binding::default();
    for arm in &node.arms {
        collector.bindings = before.clone();
        collector.bind_pattern(&arm.pat, source.clone());
        result = result.join(super::values::binding(collector, &arm.body));
    }
    collector.bindings = before;
    result
}

pub(super) fn block_value<'a>(collector: &mut Collector<'a, '_>, block: &'a syn::Block) -> Binding {
    let before = collector.bindings.clone();
    let mut result = Binding::default();
    for stmt in &block.stmts {
        match stmt {
            syn::Stmt::Local(local) => {
                let value = local
                    .init
                    .as_ref()
                    .map(|init| super::values::binding(collector, &init.expr))
                    .unwrap_or_default();
                collector.bind_pattern(&local.pat, value);
                super::values::assign_private_origins(
                    collector,
                    &local.pat,
                    local.span().start().line,
                );
            }
            syn::Stmt::Expr(expr, None) => result = super::values::binding(collector, expr),
            _ => {}
        }
    }
    collector.bindings = before;
    result
}

fn mutable_pattern(pat: &Pat) -> bool {
    match pat {
        Pat::Ident(ident) => ident.mutability.is_some(),
        Pat::Type(typed) => mutable_pattern(&typed.pat),
        _ => false,
    }
}

fn capture_binding(binding: &Binding, target: usize, name: &str) -> Binding {
    let mut value = binding.clone();
    let capture = |owner: &Owner| match owner {
        Owner::Parameter(_) | Owner::Captured(_) | Owner::Receiver => Owner::Captured(Capture {
            callable: target,
            name: name.to_string(),
        }),
        _ => owner.clone(),
    };
    value.outer = value.outer.iter().map(capture).collect();
    value.elements = value.elements.iter().map(capture).collect();
    if value.callback.is_some() {
        value.callback = Some(capture_key(target, name));
    }
    value
}

fn free_names(body: Body<'_>) -> HashSet<String> {
    let mut reader = Names {
        names: HashSet::new(),
    };
    match body {
        Body::Block(block) => reader.visit_block(block),
        Body::Expression(expr) => reader.visit_expr(expr),
    }
    reader.names
}

struct Names {
    names: HashSet<String>,
}

impl<'ast> Visit<'ast> for Names {
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if path.path.segments.len() == 1 {
            self.names.insert(storage::path_text(&path.path));
        }
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        collect_token_idents(node.tokens.clone(), &mut self.names);
        crate::analyze::model::collect_format_captures(node.tokens.clone(), &mut self.names);
    }
}
