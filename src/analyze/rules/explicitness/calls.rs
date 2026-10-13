use std::collections::{BTreeMap, HashSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::Expr;

use super::super::{
    is_mutator, patterns, resolved_call_key, storage, INPUT_CALLS, OUTPUT_CALLS, OUTPUT_MACROS,
};
use super::{
    plain_site, solver, Binding, Bindings, Callback, Collector, Direction, Owner, ValueType,
};

pub(super) fn call<'a>(collector: &mut Collector<'a, '_>, call: &'a syn::ExprCall) {
    collector.visit_expr(&call.func);
    let callable = super::values::binding(collector, &call.func);
    let args: Vec<_> = call
        .args
        .iter()
        .map(|arg| super::values::binding(collector, arg))
        .collect();
    let line = call.span().start().line;
    if !callable.callables.is_empty() || callable.callback.is_some() {
        super::calls::invoke(collector, callable, args, line);
    } else {
        super::calls::unresolved_call(collector, call, line);
    }
    for arg in &call.args {
        collector.visit_expr(arg);
    }
}

pub(super) fn invoke(
    collector: &mut Collector<'_, '_>,
    callable: Binding,
    args: Vec<Binding>,
    line: usize,
) {
    for (target, captures) in &callable.callables {
        if !collector.tasks[*target].asynchronous {
            collector.edge(*target, line, captures.clone(), args.clone());
        }
    }
    if let Some(name) = callable.callback {
        let mut site = plain_site(
            Direction::Unknown,
            line,
            format!("it calls unresolved '{name}'"),
        );
        site.callback = Some(Callback {
            name,
            arguments: args,
        });
        collector.sites.insert(site);
    }
    if callable.unresolved {
        collector.unknown(
            line,
            "it invokes an unresolved callable alternative".to_string(),
        );
    }
}

pub(super) fn unresolved_call<'a>(
    collector: &mut Collector<'a, '_>,
    call: &'a syn::ExprCall,
    line: usize,
) {
    let Expr::Path(path) = &*call.func else {
        collector.unknown(line, "it invokes an unresolved callable".to_string());
        return;
    };
    let key = resolved_call_key(path, collector.model, collector.scope());
    if INPUT_CALLS.contains(&key.as_str()) {
        collector.site(Direction::Input, line, format!("it calls '{key}'"));
    } else if OUTPUT_CALLS.contains(&key.as_str()) {
        collector.site(Direction::Output, line, format!("it calls '{key}'"));
    } else if matches!(key.as_str(), "mem::swap" | "mem::replace" | "mem::take") {
        for arg in &call.args {
            collector.note_write(arg, true);
        }
    } else if !super::calls::known_constructor(collector, path) {
        collector.unknown(line, format!("it calls unresolved '{key}'"));
    }
}

pub(super) fn known_constructor(collector: &Collector<'_, '_>, path: &syn::ExprPath) -> bool {
    let key = resolved_call_key(path, collector.model, collector.scope());
    if matches!(
        key.as_str(),
        "Some" | "Ok" | "Err" | "Rc::clone" | "Arc::clone"
    ) {
        return true;
    }
    let Some(last) = path.path.segments.last() else {
        return false;
    };
    if !matches!(
        last.ident.to_string().as_str(),
        "new" | "from" | "with_capacity" | "default"
    ) {
        return false;
    }
    super::super::receiver_type(path)
        .is_some_and(|ty| storage::value_type(&ty, collector.model, collector.scope()).standard())
}

pub(super) fn force(collector: &mut Collector<'_, '_>, value: Binding, line: usize) {
    for (target, arguments) in value.deferred {
        collector.edges.push(super::Edge {
            target,
            line,
            arguments,
        });
    }
    for (name, arguments) in value.deferred_callbacks {
        let mut site = plain_site(
            Direction::Unknown,
            line,
            format!("it invokes unresolved callback '{name}'"),
        );
        site.callback = Some(Callback { name, arguments });
        collector.sites.insert(site);
    }
    if value.unresolved {
        collector.unknown(line, "it forces an unresolved deferred value".to_string());
    }
}

pub(super) fn method_call<'a>(collector: &mut Collector<'a, '_>, call: &'a syn::ExprMethodCall) {
    let line = call.span().start().line;
    let method = call.method.to_string();
    let receiver = super::values::binding(collector, &call.receiver);
    if let Some(target) = super::calls::method_target(collector, &receiver, &method) {
        let mut args = vec![receiver.clone()];
        args.extend(
            call.args
                .iter()
                .map(|arg| super::values::binding(collector, arg)),
        );
        if !collector.tasks[target].asynchronous {
            collector.edge(target, line, Bindings::new(), args);
        }
    } else {
        super::calls::standard_call(collector, call, receiver, &method, line);
    }
    if is_mutator(&method) || matches!(method.as_str(), "with" | "with_borrow_mut") {
        super::visit_write_target(collector, &call.receiver);
    } else {
        collector.visit_expr(&call.receiver);
    }
    for arg in &call.args {
        collector.visit_expr(arg);
    }
}

pub(super) fn method_target(
    collector: &Collector<'_, '_>,
    receiver: &Binding,
    method: &str,
) -> Option<usize> {
    let ValueType::Custom(key) = &receiver.ty else {
        return None;
    };
    if collector
        .model
        .types
        .iter()
        .any(|ty| ty.key == *key && ty.node_type == "trait")
    {
        return None;
    }
    let key = super::qualify(key, method);
    let matches: Vec<_> = collector
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| task.key == key)
        .map(|(index, _)| index)
        .collect();
    (matches.len() == 1).then(|| matches[0])
}

pub(super) fn standard_call<'a>(
    collector: &mut Collector<'a, '_>,
    call: &'a syn::ExprMethodCall,
    receiver: Binding,
    method: &str,
    line: usize,
) {
    if !receiver.ty.standard()
        && !receiver
            .outer
            .iter()
            .any(|owner| matches!(owner, Owner::Global(_)))
    {
        collector.unknown(line, format!("it calls unresolved method '{method}'"));
        return;
    }
    standard_effect(collector, call, &receiver, method, line);
    standard_callbacks(collector, call, receiver, method, line);
}

fn standard_effect<'a>(
    collector: &mut Collector<'a, '_>,
    call: &'a syn::ExprMethodCall,
    receiver: &Binding,
    method: &str,
    line: usize,
) {
    if is_mutator(method) {
        collector.note_write(&call.receiver, true);
    } else if !standard_method(method) {
        collector.unknown(line, format!("it calls unresolved method '{method}'"));
    }
    if method.starts_with("fetch_")
        || matches!(method, "compare_exchange" | "compare_exchange_weak")
    {
        collector.read_binding(receiver.clone(), line);
    }
    if matches!(receiver.ty, ValueType::Iterator(_)) && consumer(method) {
        super::calls::force(collector, receiver.clone(), line);
    }
}

fn standard_callbacks<'a>(
    collector: &mut Collector<'a, '_>,
    call: &'a syn::ExprMethodCall,
    receiver: Binding,
    method: &str,
    line: usize,
) {
    if matches!(method, "with" | "with_borrow_mut") {
        super::calls::thread_local(collector, call, receiver, line);
    } else if callback_method(method)
        && !(matches!(receiver.ty, ValueType::Iterator(_)) && adapter(method))
    {
        let args = callback_arguments(
            method,
            &receiver,
            call.args
                .first()
                .map(|arg| super::values::binding(collector, arg)),
        );
        for expr in callback_expressions(call, method) {
            let value = super::values::binding(collector, expr);
            super::calls::learn_callback(collector, &value, &args);
            super::calls::invoke(collector, value, args.clone(), line);
        }
    }
}

pub(super) fn method_value<'a>(
    collector: &mut Collector<'a, '_>,
    call: &'a syn::ExprMethodCall,
) -> Binding {
    let receiver = super::values::binding(collector, &call.receiver);
    let method = call.method.to_string();
    if let Some(target) = super::calls::method_target(collector, &receiver, &method) {
        let mut arguments = Bindings::new();
        arguments.insert("self".to_string(), receiver);
        let params = collector.tasks[target]
            .params
            .iter()
            .skip(1)
            .cloned()
            .collect::<Vec<_>>();
        arguments.extend(
            params.into_iter().zip(
                call.args
                    .iter()
                    .map(|arg| super::values::binding(collector, arg)),
            ),
        );
        if collector.tasks[target].asynchronous {
            return Binding {
                ty: ValueType::Future,
                deferred: BTreeMap::from([(target, arguments)]),
                ..Binding::default()
            };
        }
        return collector
            .previous
            .get(target)
            .map(|summary| solver::substitute_binding(&summary.return_binding, &arguments))
            .unwrap_or_default();
    }
    if matches!(receiver.ty, ValueType::Iterator(_)) && adapter(&method) {
        return super::calls::iterator_value(collector, call, receiver, &method);
    }
    if method == "collect" && matches!(receiver.ty, ValueType::Iterator(_)) {
        return Binding {
            ty: ValueType::Sequence(Box::new(receiver.ty.element())),
            ..Binding::default()
        };
    }
    if method == "into_iter" && matches!(receiver.ty, ValueType::Sequence(_)) {
        let mut value = receiver;
        value.ty = ValueType::Iterator(Box::new(value.ty.element()));
        return value;
    }
    storage::method_value(&method, receiver)
}

pub(super) fn iterator_value<'a>(
    collector: &mut Collector<'a, '_>,
    call: &'a syn::ExprMethodCall,
    mut receiver: Binding,
    method: &str,
) -> Binding {
    if matches!(
        method,
        "map" | "filter" | "filter_map" | "flat_map" | "inspect"
    ) {
        let args = vec![receiver.clone().indexed()];
        for expr in &call.args {
            let callback = super::values::binding(collector, expr);
            super::calls::learn_callback(collector, &callback, &args);
            for (target, mut captures) in callback.callables {
                captures.extend(
                    collector.tasks[target]
                        .params
                        .iter()
                        .cloned()
                        .zip(args.clone()),
                );
                receiver.deferred.insert(target, captures);
                if method == "map" {
                    let ty = collector
                        .previous
                        .get(target)
                        .map(|summary| summary.return_binding.ty.clone())
                        .unwrap_or_default();
                    receiver.ty = ValueType::Iterator(Box::new(ty));
                }
            }
            if let Some(name) = callback.callback {
                receiver.deferred_callbacks.insert(name, args.clone());
            }
            receiver.unresolved |= callback.unresolved;
        }
    }
    receiver
}

pub(super) fn learn_callback(
    collector: &mut Collector<'_, '_>,
    value: &Binding,
    arguments: &[Binding],
) {
    for target in value.callables.keys() {
        let params = collector.tasks[*target].params.clone();
        for (name, actual) in params.into_iter().zip(arguments) {
            if let Some(binding) = collector.tasks[*target].bindings.get_mut(&name) {
                if matches!(binding.ty, ValueType::Unknown) {
                    binding.ty = actual.ty.clone();
                }
            }
        }
    }
}

pub(super) fn thread_local<'a>(
    collector: &mut Collector<'a, '_>,
    call: &'a syn::ExprMethodCall,
    receiver: Binding,
    line: usize,
) {
    for expr in &call.args {
        let callback = super::values::binding(collector, expr);
        for target in callback.callables.keys() {
            collector.tasks[*target].summary.report = false;
            if let Some(name) = collector.tasks[*target].params.first().cloned() {
                collector.tasks[*target]
                    .bound_params
                    .insert(name, receiver.clone());
            }
        }
        super::calls::learn_callback(collector, &callback, &[receiver.clone()]);
        super::calls::invoke(collector, callback, vec![receiver.clone()], line);
    }
}

pub(super) fn note_operator<'a>(collector: &mut Collector<'a, '_>, expr: &'a Expr, line: usize) {
    let ty = super::values::binding(collector, expr).ty;
    if !ty.standard() {
        collector.unknown(line, "it uses unresolved operator dispatch".to_string());
    }
}

pub(super) fn note_drop(collector: &mut Collector<'_, '_>, pat: &syn::Pat, line: usize) {
    let has_drop = patterns::names(pat)
        .iter()
        .filter_map(|name| collector.bindings.get(name))
        .any(|binding| match &binding.ty {
            ValueType::Custom(key) => collector
                .tasks
                .iter()
                .any(|task| task.key == super::qualify(key, "drop")),
            _ => false,
        });
    if has_drop {
        collector.unknown(line, "it can run an unresolved destructor".to_string());
    }
}

pub(super) fn note_parameter_drops(
    collector: &mut Collector<'_, '_>,
    signature: &syn::Signature,
    line: usize,
) {
    for input in &signature.inputs {
        if let syn::FnArg::Typed(param) = input {
            if !matches!(&*param.ty, syn::Type::Reference(_)) {
                note_drop(collector, &param.pat, line);
            }
        }
    }
}

pub(super) fn note_indirect_assignment<'a>(
    collector: &mut Collector<'a, '_>,
    left: &'a Expr,
    right: &'a Expr,
) {
    let value = super::values::binding(collector, left);
    if matches!(value.ty, ValueType::Pointer) {
        collector.unknown(
            left.span().start().line,
            "it writes through a raw pointer".to_string(),
        );
    }
    let assigned = super::values::binding(collector, right);
    if (matches!(left, Expr::Unary(_)) || value.outer.iter().any(|owner| matches!(owner, Owner::Private(allocation) if allocation.callable != collector.current)))
        && !assigned.outer.is_empty() && !matches!(assigned.ty, ValueType::Scalar) {
        collector.unknown(left.span().start().line, "it changes a storage alias through an indirect binding".to_string());
    }
}

pub(super) fn macro_call<'a>(collector: &mut Collector<'a, '_>, node: &'a syn::Macro) {
    let name = node
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_default();
    let line = node.span().start().line;
    macro_effect(collector, &name, line);
    visit_macro_arguments(collector, node, &name);
    let args = collector.macro_args.get(node);
    if matches!(name.as_str(), "write" | "writeln") {
        if let Some(target) = args.first() {
            collector.note_write(target, true);
        }
    }
    read_format_captures(collector, node);
}

fn macro_effect(collector: &mut Collector<'_, '_>, name: &str, line: usize) {
    let scoped = super::qualify(collector.scope(), name);
    if collector.model.local_macros.contains(name) || collector.model.local_macros.contains(&scoped)
    {
        collector.unknown(line, format!("it uses local macro '{name}!'"));
    } else if OUTPUT_MACROS.contains(&name) {
        collector.site(Direction::Output, line, format!("it uses '{name}!'"));
    } else if !known_macro(name) {
        collector.unknown(line, format!("it uses unresolved macro '{name}!'"));
    }
}

fn visit_macro_arguments<'a>(collector: &mut Collector<'a, '_>, node: &'a syn::Macro, name: &str) {
    let line = node.span().start().line;
    let args = collector.macro_args.get(node);
    for arg in args {
        collector.visit_expr(arg);
        if matches!(
            name,
            "format" | "format_args" | "print" | "println" | "write" | "writeln"
        ) && formatting_dispatch(&super::values::binding(collector, arg).ty)
        {
            collector.unknown(line, "it uses unresolved formatting dispatch".to_string());
        }
    }
}

fn read_format_captures(collector: &mut Collector<'_, '_>, node: &syn::Macro) {
    let line = node.span().start().line;
    let mut names = HashSet::new();
    crate::analyze::model::collect_format_captures(node.tokens.clone(), &mut names);
    for name in names {
        if let Some(binding) = collector.bindings.get(&name) {
            collector.read_binding(binding.clone(), line);
        } else if collector.model.shared_statics.contains(&name) {
            collector.read_binding(
                Binding {
                    outer: [Owner::Global(name)].into_iter().collect(),
                    ..Binding::default()
                },
                line,
            );
        }
    }
}

fn formatting_dispatch(ty: &ValueType) -> bool {
    matches!(
        ty,
        ValueType::Custom(_) | ValueType::Unknown | ValueType::Pointer
    )
}

fn callback_arguments(method: &str, receiver: &Binding, initial: Option<Binding>) -> Vec<Binding> {
    match method {
        "fold" => vec![initial.unwrap_or_default(), receiver.clone().indexed()],
        "sort_by" | "sort_unstable_by" => {
            vec![receiver.clone().indexed(), receiver.clone().indexed()]
        }
        "unwrap_or_else" => Vec::new(),
        _ => vec![receiver.clone().indexed()],
    }
}

fn callback_expressions<'a>(call: &'a syn::ExprMethodCall, method: &str) -> Vec<&'a Expr> {
    if matches!(method, "fold" | "map_or") {
        call.args.iter().skip(1).collect()
    } else {
        call.args.iter().collect()
    }
}

fn callback_method(method: &str) -> bool {
    matches!(
        method,
        "map"
            | "map_or"
            | "filter"
            | "filter_map"
            | "flat_map"
            | "for_each"
            | "any"
            | "all"
            | "find"
            | "position"
            | "retain"
            | "sort_by"
            | "sort_unstable_by"
            | "fold"
            | "unwrap_or_else"
    )
}

fn adapter(method: &str) -> bool {
    matches!(
        method,
        "map"
            | "filter"
            | "filter_map"
            | "flat_map"
            | "inspect"
            | "copied"
            | "cloned"
            | "take"
            | "skip"
            | "enumerate"
            | "rev"
    )
}

fn consumer(method: &str) -> bool {
    matches!(
        method,
        "collect"
            | "sum"
            | "count"
            | "for_each"
            | "fold"
            | "any"
            | "all"
            | "find"
            | "position"
            | "next"
            | "last"
            | "nth"
    )
}

fn standard_method(method: &str) -> bool {
    adapter(method)
        || consumer(method)
        || callback_method(method)
        || matches!(
            method,
            "len"
                | "is_empty"
                | "iter"
                | "iter_mut"
                | "into_iter"
                | "to_vec"
                | "to_string"
                | "to_owned"
                | "clone"
                | "unwrap"
                | "expect"
                | "ok"
                | "as_ref"
                | "as_mut"
                | "get"
                | "borrow"
                | "borrow_mut"
                | "get_mut"
                | "last_mut"
                | "first_mut"
                | "as_slice"
                | "as_mut_slice"
                | "lock"
                | "try_lock"
                | "read"
                | "write"
                | "load"
                | "with"
                | "with_borrow_mut"
                | "is_some"
                | "is_none"
                | "unwrap_or"
                | "cmp"
        )
}

fn known_macro(name: &str) -> bool {
    matches!(
        name,
        "vec"
            | "format"
            | "format_args"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
            | "matches"
            | "write"
            | "writeln"
            | "panic"
            | "todo"
            | "unreachable"
    )
}
