use std::collections::BTreeSet;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, FnArg, Pat};

use crate::analyze::model::{resolve_type_path, FileModel};

use super::control::{branch_reachability, BranchReachability};
use super::storage::{self, Binding, Bindings, Owner, ValueType};
use super::{is_compound_assign, Direction};

#[path = "calls.rs"]
mod calls;
#[path = "solver.rs"]
mod solver;
#[path = "values.rs"]
mod values;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Site {
    pub(super) direction: Direction,
    pub(super) line: usize,
    pub(super) detail: String,
    pub(super) owner: Option<Owner>,
    pub(super) callback: Option<Callback>,
    pub(super) reachable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Callback {
    name: String,
    pub(super) arguments: Vec<Binding>,
}

#[derive(Clone, PartialEq, Eq)]
struct Edge {
    pub(super) target: usize,
    line: usize,
    pub(super) arguments: Bindings,
}

#[derive(Clone)]
pub(super) struct ReportOwner {
    pub(super) function: String,
    pub(super) class: String,
    pub(super) method: String,
}

#[derive(Clone)]
pub(super) struct Summary {
    pub(super) report: bool,
    pub(super) function: usize,
    pub(super) name: String,
    pub(super) owner: ReportOwner,
    return_binding: Binding,
    pub(super) sites: BTreeSet<Site>,
    edges: Vec<Edge>,
}

#[derive(Clone, Copy)]
enum Body<'a> {
    Block(&'a syn::Block),
    Expression(&'a Expr),
}

struct Callable<'a> {
    summary: Summary,
    scope: String,
    pub(super) key: String,
    params: Vec<String>,
    signature: Option<&'a syn::Signature>,
    body: Option<Body<'a>>,
    bindings: Bindings,
    bound_params: Bindings,
    pub(super) origin: Option<usize>,
    pub(super) asynchronous: bool,
}

struct Collector<'a, 'm> {
    model: &'m FileModel<'a>,
    tasks: &'m mut Vec<Callable<'a>>,
    pub(super) previous: &'m [Summary],
    pub(super) macro_args: &'a super::macros::Arguments,
    current: usize,
    bindings: Bindings,
    include_self: bool,
    sites: BTreeSet<Site>,
    edges: Vec<Edge>,
    return_binding: Binding,
}

fn visit_write_target<'a>(collector: &mut Collector<'a, '_>, expr: &'a Expr) {
    if let Some(child) = write_target_child(expr) {
        visit_write_target(collector, child);
        return;
    }
    match expr {
        Expr::Path(_) => {}
        Expr::Index(index) => {
            visit_write_target(collector, &index.expr);
            collector.visit_expr(&index.index);
        }
        Expr::MethodCall(call) if transparent_write_accessor(collector, call) => {
            visit_write_target(collector, &call.receiver);
            for arg in &call.args {
                collector.visit_expr(arg);
            }
        }
        _ => collector.visit_expr(expr),
    }
}

fn write_target_child(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Field(field) => Some(&field.base),
        Expr::Paren(paren) => Some(&paren.expr),
        Expr::Group(group) => Some(&group.expr),
        Expr::Reference(reference) => Some(&reference.expr),
        Expr::Unary(unary) => Some(&unary.expr),
        _ => None,
    }
}

fn transparent_write_accessor<'a>(
    collector: &mut Collector<'a, '_>,
    call: &'a syn::ExprMethodCall,
) -> bool {
    let receiver = values::binding(collector, &call.receiver);
    let method = call.method.to_string();
    matches!(
        method.as_str(),
        "borrow"
            | "borrow_mut"
            | "lock"
            | "try_lock"
            | "read"
            | "write"
            | "unwrap"
            | "expect"
            | "as_ref"
            | "as_mut"
            | "get_mut"
            | "last_mut"
            | "first_mut"
            | "as_slice"
            | "as_mut_slice"
    ) && receiver.ty.standard()
        && calls::method_target(collector, &receiver, &method).is_none()
}

pub(super) fn summaries(model: &FileModel<'_>, include_self: bool) -> Vec<Summary> {
    let macro_args = super::macros::Arguments::collect(model);
    let mut tasks: Vec<_> = model
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| {
            let key = function.parent_key.as_ref().map_or_else(
                || qualify(&function.scope, &function.name),
                |parent| qualify(parent, &function.name),
            );
            let name = function.parent.as_ref().map_or_else(
                || function.name.clone(),
                |parent| format!("{parent}::{}", function.name),
            );
            Callable {
                summary: empty_summary(index, name, owner_for_function(function)),
                scope: function.scope.clone(),
                key,
                params: signature_params(function.signature),
                signature: Some(function.signature),
                body: function.body.map(Body::Block),
                bindings: Bindings::new(),
                bound_params: Bindings::new(),
                origin: None,
                asynchronous: function.signature.asyncness.is_some(),
            }
        })
        .collect();
    for _ in 0..64 {
        let previous: Vec<_> = tasks.iter().map(|task| task.summary.clone()).collect();
        let mut staged = Vec::new();
        let mut current = 0;
        while current < tasks.len() {
            staged.push(collect_callable(
                model,
                &mut tasks,
                &previous,
                &macro_args,
                current,
                include_self,
            ));
            current += 1;
        }
        for (task, summary) in tasks.iter_mut().zip(staged) {
            task.summary = summary;
        }
        solver::solve(&mut tasks);
        if solver::unchanged(&tasks, &previous) {
            return solver::finish(tasks);
        }
    }
    for task in &mut tasks {
        task.summary.sites.insert(plain_site(
            Direction::Unknown,
            body_line(task.body),
            "its value flow did not converge".to_string(),
        ));
    }
    solver::finish(tasks)
}

fn empty_summary(function: usize, name: String, owner: ReportOwner) -> Summary {
    Summary {
        report: true,
        function,
        name,
        owner,
        return_binding: Binding::default(),
        sites: BTreeSet::new(),
        edges: Vec::new(),
    }
}

fn body_line(body: Option<Body<'_>>) -> usize {
    match body {
        Some(Body::Block(block)) => block.span().start().line,
        Some(Body::Expression(expr)) => expr.span().start().line,
        None => 1,
    }
}

fn owner_for_function(function: &crate::analyze::model::FnModel<'_>) -> ReportOwner {
    match &function.parent {
        Some(class) => ReportOwner {
            function: String::new(),
            class: class.clone(),
            method: function.name.clone(),
        },
        None => owner_for_synthetic(function.name.clone()),
    }
}

fn owner_for_synthetic(name: String) -> ReportOwner {
    ReportOwner {
        function: name,
        class: String::new(),
        method: String::new(),
    }
}

fn collect_callable<'a>(
    model: &FileModel<'a>,
    tasks: &mut Vec<Callable<'a>>,
    previous: &[Summary],
    macro_args: &'a super::macros::Arguments,
    current: usize,
    include_self: bool,
) -> Summary {
    let bindings = callable_bindings(model, tasks, current);
    let body = tasks[current].body;
    let signature = tasks[current].signature;
    let mut summary = tasks[current].summary.clone();
    let mut collector = Collector {
        model,
        tasks,
        previous,
        macro_args,
        current,
        bindings,
        include_self,
        sites: BTreeSet::new(),
        edges: Vec::new(),
        return_binding: Binding::default(),
    };
    match body {
        Some(Body::Block(block)) => collector.visit_block(block),
        Some(Body::Expression(expr)) => {
            collector.visit_expr(expr);
            values::note_return_value(&mut collector, expr);
        }
        None => {
            summary.report = false;
            collector.unknown(
                model.functions[summary.function].begin_line,
                "it has no analyzed body".to_string(),
            );
        }
    }
    if let Some(signature) = signature {
        calls::note_parameter_drops(&mut collector, signature, body_end_line(body));
    }
    summary.sites = collector.sites;
    summary.edges = collector.edges;
    summary.return_binding = collector.return_binding;
    summary
}

fn body_end_line(body: Option<Body<'_>>) -> usize {
    match body {
        Some(Body::Block(block)) => block.span().end().line,
        Some(Body::Expression(expr)) => expr.span().end().line,
        None => 0,
    }
}

fn callable_bindings(model: &FileModel<'_>, tasks: &[Callable<'_>], current: usize) -> Bindings {
    let mut bindings = tasks[current].bindings.clone();
    if let Some(signature) = tasks[current].signature {
        for (position, input) in signature.inputs.iter().enumerate() {
            match input {
                FnArg::Typed(param) => {
                    let name = super::patterns::name(&param.pat, position);
                    let binding =
                        storage::parameter(&name, &param.ty, model, &tasks[current].scope);
                    super::patterns::bind(
                        &param.pat,
                        binding,
                        &mut bindings,
                        Some((model, &tasks[current].scope)),
                    );
                }
                FnArg::Receiver(receiver) => {
                    let parent = model.functions[tasks[current].summary.function]
                        .parent_key
                        .clone()
                        .unwrap_or_default();
                    bindings.insert(
                        "self".to_string(),
                        Binding {
                            outer: if receiver.reference.is_some() {
                                BTreeSet::from([Owner::Receiver])
                            } else {
                                BTreeSet::new()
                            },
                            ty: ValueType::Custom(parent),
                            ..Binding::default()
                        },
                    );
                }
            }
        }
    }
    bindings.extend(tasks[current].bound_params.clone());
    bindings
}

fn signature_params(signature: &syn::Signature) -> Vec<String> {
    signature
        .inputs
        .iter()
        .enumerate()
        .map(|(index, input)| match input {
            FnArg::Typed(param) => super::patterns::name(&param.pat, index),
            FnArg::Receiver(_) => "self".to_string(),
        })
        .collect()
}

fn qualify(scope: &str, name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{scope}::{name}")
    }
}

fn plain_site(direction: Direction, line: usize, detail: String) -> Site {
    Site {
        direction,
        line,
        detail,
        owner: None,
        callback: None,
        reachable: false,
    }
}

fn owned_site(direction: Direction, owner: Owner, line: usize, reachable: bool) -> Site {
    let detail = match &owner {
        Owner::Parameter(name) => format!("it writes through the borrowed parameter '{name}'"),
        Owner::Global(name) => global_detail(direction, name),
        Owner::Receiver => receiver_detail(direction),
        Owner::Private(allocation) => capture_detail(direction, &allocation.name),
        Owner::Captured(capture) => capture_detail(direction, &capture.name),
    };
    Site {
        direction,
        line,
        detail,
        owner: Some(owner),
        callback: None,
        reachable,
    }
}

fn global_detail(direction: Direction, name: &str) -> String {
    if direction == Direction::Input {
        format!("it reads the global '{name}'")
    } else {
        format!("it writes the global '{name}'")
    }
}

fn receiver_detail(direction: Direction) -> String {
    if direction == Direction::Input {
        "it reads the state of 'self'".to_string()
    } else {
        "it changes the state of 'self'".to_string()
    }
}

fn capture_detail(direction: Direction, name: &str) -> String {
    if direction == Direction::Input {
        format!("it reads captured storage '{name}'")
    } else {
        format!("it changes captured storage '{name}'")
    }
}

impl<'a> Collector<'a, '_> {
    fn scope(&self) -> &str {
        &self.tasks[self.current].scope
    }

    fn site(&mut self, direction: Direction, line: usize, detail: String) {
        self.sites.insert(plain_site(direction, line, detail));
    }

    fn unknown(&mut self, line: usize, detail: String) {
        self.site(Direction::Unknown, line, detail);
    }

    fn read_binding(&mut self, binding: Binding, line: usize) {
        for owner in binding.outer {
            let implicit = match &owner {
                Owner::Global(_) | Owner::Captured(_) => true,
                Owner::Private(allocation) => allocation.callable != self.current,
                Owner::Receiver => self.include_self,
                Owner::Parameter(_) => false,
            };
            if implicit {
                self.sites
                    .insert(owned_site(Direction::Input, owner, line, false));
            }
        }
    }

    fn note_write(&mut self, expr: &'a Expr, through: bool) {
        let binding = values::binding(self, expr);
        for owner in binding.outer {
            if !through && matches!(owner, Owner::Parameter(_)) {
                continue;
            }
            if matches!(&owner, Owner::Private(allocation) if allocation.callable == self.current) {
                continue;
            }
            self.sites.insert(owned_site(
                Direction::Output,
                owner,
                expr.span().start().line,
                contains_index(expr),
            ));
        }
    }

    pub(super) fn edge(
        &mut self,
        target: usize,
        line: usize,
        mut captures: Bindings,
        args: Vec<Binding>,
    ) {
        captures.extend(self.tasks[target].params.iter().cloned().zip(args));
        self.edges.push(Edge {
            target,
            line,
            arguments: captures,
        });
    }

    fn restore_names(&mut self, before: &Bindings, names: Vec<String>) {
        for name in names {
            if let Some(binding) = before.get(&name) {
                self.bindings.insert(name, binding.clone());
            } else {
                self.bindings.remove(&name);
            }
        }
    }

    fn bind_pattern(&mut self, pat: &Pat, binding: Binding) {
        let scope = self.scope().to_string();
        super::patterns::bind(pat, binding, &mut self.bindings, Some((self.model, &scope)));
    }

    fn loop_body(&mut self, body: &'a syn::Block, before: Bindings) {
        let mut entry = before;
        for _ in 0..32 {
            self.bindings = entry.clone();
            self.visit_block(body);
            let mut joined = entry.clone();
            storage::join_bindings(&mut joined, self.bindings.clone());
            if joined == entry {
                self.bindings = joined;
                return;
            }
            entry = joined;
        }
        self.bindings = entry;
        self.unknown(
            body.span().start().line,
            "its loop value flow did not converge".to_string(),
        );
    }
}

fn contains_index(expr: &Expr) -> bool {
    match expr {
        Expr::Index(_) => true,
        Expr::Field(field) => contains_index(&field.base),
        Expr::Unary(unary) => contains_index(&unary.expr),
        Expr::MethodCall(call) => contains_index(&call.receiver),
        Expr::Paren(paren) => contains_index(&paren.expr),
        _ => false,
    }
}

impl<'a> Visit<'a> for Collector<'a, '_> {
    fn visit_block(&mut self, block: &'a syn::Block) {
        let before = self.bindings.clone();
        let declared: Vec<_> = block
            .stmts
            .iter()
            .filter_map(|stmt| {
                if let syn::Stmt::Local(local) = stmt {
                    Some(super::patterns::names(&local.pat))
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        for stmt in &block.stmts {
            if let syn::Stmt::Item(syn::Item::Fn(function)) = stmt {
                values::nested_function(self, function);
            }
        }
        for stmt in &block.stmts {
            self.visit_stmt(stmt);
        }
        if matches!(self.tasks[self.current].body, Some(Body::Block(root)) if std::ptr::eq(root, block))
        {
            if let Some(syn::Stmt::Expr(expr, None)) = block.stmts.last() {
                values::note_return_value(self, expr);
            }
        }
        self.restore_names(&before, declared);
    }

    fn visit_item_fn(&mut self, function: &'a syn::ItemFn) {
        values::nested_function(self, function);
    }

    fn visit_local(&mut self, local: &'a syn::Local) {
        let mut binding = Binding::default();
        if let Some(init) = &local.init {
            self.visit_expr(&init.expr);
            binding = values::binding(self, &init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
        self.bind_pattern(&local.pat, binding);
        values::assign_private_origins(self, &local.pat, local.span().start().line);
        calls::note_drop(self, &local.pat, local.span().start().line);
    }

    fn visit_expr_closure(&mut self, closure: &'a syn::ExprClosure) {
        values::closure(self, closure);
    }
    fn visit_expr_async(&mut self, node: &'a syn::ExprAsync) {
        values::async_block(self, node);
    }

    fn visit_expr_await(&mut self, node: &'a syn::ExprAwait) {
        self.visit_expr(&node.base);
        let value = values::binding(self, &node.base);
        calls::force(self, value, node.span().start().line);
    }

    fn visit_expr_path(&mut self, path: &'a syn::ExprPath) {
        let value = values::path_binding(self, path);
        self.read_binding(value, path.span().start().line);
    }

    fn visit_expr_field(&mut self, field: &'a syn::ExprField) {
        self.visit_expr(&field.base);
    }
    fn visit_expr_call(&mut self, call: &'a syn::ExprCall) {
        calls::call(self, call);
    }
    fn visit_expr_method_call(&mut self, call: &'a syn::ExprMethodCall) {
        calls::method_call(self, call);
    }

    fn visit_expr_assign(&mut self, assign: &'a syn::ExprAssign) {
        self.note_write(&assign.left, !matches!(&*assign.left, Expr::Path(_)));
        visit_write_target(self, &assign.left);
        self.visit_expr(&assign.right);
        calls::note_indirect_assignment(self, &assign.left, &assign.right);
        if let Expr::Path(path) = &*assign.left {
            let mut value = values::binding(self, &assign.right);
            let key = storage::path_text(&path.path);
            if let Some(before) = self.bindings.get(&key) {
                value.outer.extend(
                    before
                        .outer
                        .iter()
                        .filter(|owner| matches!(owner, Owner::Private(_)))
                        .cloned(),
                );
            }
            self.bindings.insert(key, value);
        }
    }

    fn visit_expr_binary(&mut self, binary: &'a syn::ExprBinary) {
        calls::note_operator(self, &binary.left, binary.span().start().line);
        if is_compound_assign(&binary.op) {
            self.note_write(&binary.left, !matches!(&*binary.left, Expr::Path(_)));
        }
        self.visit_expr(&binary.left);
        self.visit_expr(&binary.right);
    }

    fn visit_expr_unary(&mut self, unary: &'a syn::ExprUnary) {
        let value = values::binding(self, &unary.expr);
        if matches!(value.ty, ValueType::Custom(_) | ValueType::Pointer) {
            self.unknown(
                unary.span().start().line,
                "it uses unresolved unary dispatch".to_string(),
            );
        }
        self.visit_expr(&unary.expr);
    }

    fn visit_expr_index(&mut self, index: &'a syn::ExprIndex) {
        let value = values::binding(self, &index.expr);
        if !matches!(value.ty, ValueType::Sequence(_)) {
            self.unknown(
                index.span().start().line,
                "it uses unresolved indexing".to_string(),
            );
        }
        self.visit_expr(&index.expr);
        self.visit_expr(&index.index);
    }

    fn visit_expr_return(&mut self, node: &'a syn::ExprReturn) {
        if let Some(expr) = &node.expr {
            self.visit_expr(expr);
            values::note_return_value(self, expr);
        }
    }

    fn visit_expr_if(&mut self, branch: &'a syn::ExprIf) {
        self.visit_expr(&branch.cond);
        let before = self.bindings.clone();
        values::bind_condition(self, &branch.cond);
        if branch_reachability(&branch.cond) != BranchReachability::ElseOnly {
            self.visit_block(&branch.then_branch);
        }
        let then_bindings = self.bindings.clone();
        self.bindings = before.clone();
        if branch_reachability(&branch.cond) != BranchReachability::ThenOnly {
            if let Some((_, otherwise)) = &branch.else_branch {
                self.visit_expr(otherwise);
            }
        }
        match branch_reachability(&branch.cond) {
            BranchReachability::ThenOnly => self.bindings = then_bindings,
            BranchReachability::Both => storage::join_bindings(&mut self.bindings, then_bindings),
            BranchReachability::ElseOnly => {}
        }
        if let Expr::Let(node) = &*branch.cond {
            self.restore_names(&before, super::patterns::names(&node.pat));
        }
    }

    fn visit_expr_match(&mut self, node: &'a syn::ExprMatch) {
        self.visit_expr(&node.expr);
        let value = values::binding(self, &node.expr);
        let before = self.bindings.clone();
        let mut joined = before.clone();
        for arm in &node.arms {
            self.bindings = before.clone();
            self.bind_pattern(&arm.pat, value.clone());
            if let Some((_, guard)) = &arm.guard {
                self.visit_expr(guard);
            }
            self.visit_expr(&arm.body);
            self.restore_names(&before, super::patterns::names(&arm.pat));
            storage::join_bindings(&mut joined, self.bindings.clone());
        }
        self.bindings = joined;
    }

    fn visit_expr_for_loop(&mut self, node: &'a syn::ExprForLoop) {
        self.visit_expr(&node.expr);
        let value = values::binding(self, &node.expr);
        calls::force(self, value.clone(), node.span().start().line);
        if !matches!(value.ty, ValueType::Iterator(_) | ValueType::Sequence(_)) {
            self.unknown(
                node.span().start().line,
                "it iterates an unresolved value".to_string(),
            );
        }
        let before = self.bindings.clone();
        self.bind_pattern(&node.pat, value.indexed());
        self.loop_body(&node.body, self.bindings.clone());
        self.restore_names(&before, super::patterns::names(&node.pat));
    }

    fn visit_expr_while(&mut self, node: &'a syn::ExprWhile) {
        self.visit_expr(&node.cond);
        if branch_reachability(&node.cond) == BranchReachability::ElseOnly {
            return;
        }
        let before = self.bindings.clone();
        values::bind_condition(self, &node.cond);
        self.loop_body(&node.body, self.bindings.clone());
        if let Expr::Let(condition) = &*node.cond {
            self.restore_names(&before, super::patterns::names(&condition.pat));
        }
    }

    fn visit_expr_loop(&mut self, node: &'a syn::ExprLoop) {
        self.loop_body(&node.body, self.bindings.clone());
    }
    fn visit_macro(&mut self, node: &'a syn::Macro) {
        calls::macro_call(self, node);
    }
}
