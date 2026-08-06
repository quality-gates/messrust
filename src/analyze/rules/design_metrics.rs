//! Design-rule AST walkers and cohesion/coupling metrics.

use std::collections::{HashMap, HashSet};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{ItemFn, Member, Pat};

use crate::analyze::model::{is_builtin_type, path_single_ident, FileModel, MethodRef, TypeModel};

const DEFAULT_DEV_FUNCS: &str = "println,print,eprintln,dbg";

pub(crate) fn exit_expression_line(body: &syn::Block) -> Option<usize> {
    let mut hit = None;
    let mut visitor = ExitCallCollector { hit: &mut hit };
    visitor.visit_block(body);
    hit
}


struct ExitCallCollector<'a> {
    hit: &'a mut Option<usize>,
}


impl ExitCallCollector<'_> {
    fn consider_path(&mut self, path: &syn::Path, line: usize) {
        if self.hit.is_some() {
            return;
        }
        let Some(last) = path.segments.last() else {
            return;
        };
        let name = last.ident.to_string();
        if name == "exit" || name == "abort" {
            *self.hit = Some(line);
        }
    }
}


impl<'ast> Visit<'ast> for ExitCallCollector<'_> {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.hit.is_none() {
            if let syn::Expr::Path(p) = &*node.func {
                self.consider_path(&p.path, p.span().start().line);
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}



pub(crate) fn unwanted_function_set(extra: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for part in DEFAULT_DEV_FUNCS.split(',') {
        set.insert(part.trim().to_ascii_lowercase());
    }
    for part in extra.split(',') {
        let name = part.trim();
        if !name.is_empty() {
            set.insert(name.to_ascii_lowercase());
        }
    }
    set
}


pub(crate) struct CountLoopHit {
    pub(crate) line: usize,
    pub(crate) func_name: String,
    pub(crate) loop_kind: String,
}


pub(crate) fn count_in_loop_hits(body: &syn::Block) -> Vec<CountLoopHit> {
    let mut hits = Vec::new();
    let mut visitor = CountInLoopCollector { hits: &mut hits };
    visitor.visit_block(body);
    hits
}


struct CountInLoopCollector<'a> {
    hits: &'a mut Vec<CountLoopHit>,
}


impl CountInLoopCollector<'_> {
    fn scan_expr(&mut self, expr: &syn::Expr, loop_kind: &str) {
        let mut finder = LenCallFinder {
            hits: self.hits,
            loop_kind: loop_kind.to_string(),
        };
        finder.visit_expr(expr);
    }
}


impl<'ast> Visit<'ast> for CountInLoopCollector<'_> {
    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.scan_expr(&node.cond, "while");
        syn::visit::visit_expr_while(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.scan_expr(&node.expr, "for");
        syn::visit::visit_expr_for_loop(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}


struct LenCallFinder<'a> {
    hits: &'a mut Vec<CountLoopHit>,
    loop_kind: String,
}


impl<'ast> Visit<'ast> for LenCallFinder<'_> {
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let name = node.method.to_string();
        if name == "len" || name == "capacity" {
            self.hits.push(CountLoopHit {
                line: node.method.span().start().line,
                func_name: name,
                loop_kind: self.loop_kind.clone(),
            });
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            if let Some(last) = p.path.segments.last() {
                let name = last.ident.to_string();
                if name == "len" || name == "capacity" {
                    self.hits.push(CountLoopHit {
                        line: last.ident.span().start().line,
                        func_name: name,
                        loop_kind: self.loop_kind.clone(),
                    });
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}


pub(crate) struct DevHit {
    pub(crate) line: usize,
    pub(crate) func_name: String,
}


pub(crate) fn development_fragment_hits(body: &syn::Block, unwanted: &HashSet<String>) -> Vec<DevHit> {
    let mut hits = Vec::new();
    let mut visitor = DevFragmentCollector {
        unwanted,
        hits: &mut hits,
    };
    visitor.visit_block(body);
    hits
}


struct DevFragmentCollector<'a> {
    unwanted: &'a HashSet<String>,
    hits: &'a mut Vec<DevHit>,
}


impl DevFragmentCollector<'_> {
    fn consider(&mut self, name: &str, line: usize) {
        if self.unwanted.contains(&name.to_ascii_lowercase()) {
            self.hits.push(DevHit {
                line,
                func_name: name.to_string(),
            });
        }
    }
}


impl<'ast> Visit<'ast> for DevFragmentCollector<'_> {
    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        if let Some(last) = node.mac.path.segments.last() {
            self.consider(&last.ident.to_string(), last.ident.span().start().line);
        }
    }

    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        if let Some(last) = node.mac.path.segments.last() {
            self.consider(&last.ident.to_string(), last.ident.span().start().line);
        }
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            if let Some(last) = p.path.segments.last() {
                self.consider(&last.ident.to_string(), last.ident.span().start().line);
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}


pub(crate) fn empty_catch_lines(body: &syn::Block) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut visitor = EmptyCatchCollector { lines: &mut lines };
    visitor.visit_block(body);
    lines
}


struct EmptyCatchCollector<'a> {
    lines: &'a mut Vec<usize>,
}


fn block_is_empty(block: &syn::Block) -> bool {
    block.stmts.is_empty()
}


fn expr_is_empty_block(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Block(b) => block_is_empty(&b.block),
        syn::Expr::Tuple(t) if t.elems.is_empty() => true,
        _ => false,
    }
}


fn pat_is_err(pat: &Pat) -> bool {
    match pat {
        Pat::TupleStruct(ts) => ts.path.segments.last().is_some_and(|s| s.ident == "Err"),
        Pat::Or(p) => p.cases.iter().any(pat_is_err),
        _ => false,
    }
}


impl<'ast> Visit<'ast> for EmptyCatchCollector<'_> {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        if let syn::Expr::Let(l) = &*node.cond {
            if pat_is_err(&l.pat) && block_is_empty(&node.then_branch) {
                self.lines.push(node.if_token.span().start().line);
            }
        }
        syn::visit::visit_expr_if(self, node);
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        if pat_is_err(&node.pat) && expr_is_empty_block(&node.body) {
            self.lines.push(node.pat.span().start().line);
        }
        syn::visit::visit_arm(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}


pub(crate) fn coupling_between_objects(t: &TypeModel<'_>, model: &FileModel<'_>) -> usize {
    let mut deps = HashSet::new();
    for field in &t.fields {
        add_type_dependencies(&mut deps, &field.type_names, &t.name);
    }
    for f in &model.functions {
        if !f.counts_for_type_metrics || f.parent.as_deref() != Some(t.name.as_str()) {
            continue;
        }
        add_type_dependencies(&mut deps, &f.dep_types, &t.name);
    }
    deps.len()
}


fn add_type_dependencies(deps: &mut HashSet<String>, names: &[String], owner: &str) {
    for name in names {
        if !is_builtin_type(name) && name != owner {
            deps.insert(name.clone());
        }
    }
}



pub(crate) fn lcom4(t: &TypeModel<'_>) -> usize {
    let field_names: HashSet<String> = t.fields.iter().map(|f| f.name.clone()).collect();
    let method_idx: HashMap<String, usize> = t
        .methods
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.clone(), i))
        .collect();
    let accessor_of = accessor_fields(t);
    let mut graph = CohesionGraph::new(t.methods.len());

    for (i, m) in t.methods.iter().enumerate() {
        if accessor_of.contains_key(&m.name) {
            continue;
        }
        let Some(body) = m.body else {
            continue;
        };
        let (used_fields, called) = receiver_uses(body, &field_names, &method_idx);
        connect_receiver_uses(
            &mut graph,
            i,
            used_fields,
            called,
            &accessor_of,
            &method_idx,
        );
    }
    graph.component_count()
}


fn accessor_fields(model: &TypeModel<'_>) -> HashMap<String, String> {
    let mut accessors = HashMap::new();
    for method in &model.methods {
        if let Some(field) = accessor_field(method) {
            accessors.insert(method.name.clone(), field);
        }
    }
    accessors
}


fn connect_receiver_uses(
    graph: &mut CohesionGraph,
    method: usize,
    used_fields: Vec<String>,
    called_methods: Vec<String>,
    accessors: &HashMap<String, String>,
    method_indexes: &HashMap<String, usize>,
) {
    for field in used_fields {
        graph.connect_field(method, field);
    }
    for called in called_methods {
        if let Some(field) = accessors.get(&called) {
            graph.connect_field(method, field.clone());
        } else if let Some(called_method) = method_indexes.get(&called) {
            graph.connect_methods(method, *called_method);
        }
    }
}


struct CohesionGraph {
    parent: Vec<usize>,
    active: Vec<bool>,
    field_owner: HashMap<String, usize>,
}


impl CohesionGraph {
    fn new(method_count: usize) -> Self {
        Self {
            parent: (0..method_count).collect(),
            active: vec![false; method_count],
            field_owner: HashMap::new(),
        }
    }

    fn find(&mut self, mut method: usize) -> usize {
        while self.parent[method] != method {
            self.parent[method] = self.parent[self.parent[method]];
            method = self.parent[method];
        }
        method
    }

    fn union(&mut self, left: usize, right: usize) {
        let left_root = self.find(left);
        let right_root = self.find(right);
        self.parent[left_root] = right_root;
    }

    fn connect_field(&mut self, method: usize, field: String) {
        self.active[method] = true;
        if let Some(owner) = self.field_owner.get(&field).copied() {
            self.union(method, owner);
        } else {
            self.field_owner.insert(field, method);
        }
    }

    fn connect_methods(&mut self, caller: usize, called: usize) {
        self.active[caller] = true;
        self.active[called] = true;
        self.union(caller, called);
    }

    fn component_count(mut self) -> usize {
        let mut roots = HashSet::new();
        for method in 0..self.active.len() {
            if self.active[method] {
                roots.insert(self.find(method));
            }
        }
        roots.len().max(1)
    }
}


fn accessor_field(m: &MethodRef<'_>) -> Option<String> {
    let body = m.body?;
    if body.stmts.len() != 1 {
        return None;
    }
    match &body.stmts[0] {
        syn::Stmt::Expr(syn::Expr::Field(field), _) => receiver_field(field),
        syn::Stmt::Expr(syn::Expr::Assign(assign), _) => assigned_receiver_field(assign),
        _ => None,
    }
}


fn assigned_receiver_field(assignment: &syn::ExprAssign) -> Option<String> {
    let syn::Expr::Field(field) = &*assignment.left else {
        return None;
    };
    receiver_field(field)
}


fn receiver_field(field: &syn::ExprField) -> Option<String> {
    let (syn::Expr::Path(base), Member::Named(identifier)) = (&*field.base, &field.member) else {
        return None;
    };
    let name = identifier.to_string();
    path_is_self(base).then_some(name)
}


fn path_is_self(path: &syn::ExprPath) -> bool {
    path_single_ident(path).as_deref() == Some("self")
}


fn receiver_uses(
    body: &syn::Block,
    fields: &HashSet<String>,
    methods: &HashMap<String, usize>,
) -> (Vec<String>, Vec<String>) {
    let mut used_fields = Vec::new();
    let mut called = Vec::new();
    let mut visitor = ReceiverUseCollector {
        fields,
        methods,
        used_fields: &mut used_fields,
        called: &mut called,
    };
    visitor.visit_block(body);
    (used_fields, called)
}


struct ReceiverUseCollector<'a> {
    fields: &'a HashSet<String>,
    methods: &'a HashMap<String, usize>,
    used_fields: &'a mut Vec<String>,
    called: &'a mut Vec<String>,
}


impl<'ast> Visit<'ast> for ReceiverUseCollector<'_> {
    fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
        if let syn::Expr::Path(p) = &*node.base {
            if path_is_self(p) {
                if let Member::Named(ident) = &node.member {
                    let name = ident.to_string();
                    if self.fields.contains(&name) {
                        self.used_fields.push(name);
                    }
                }
            }
        }
        syn::visit::visit_expr_field(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if let syn::Expr::Path(p) = &*node.receiver {
            if path_is_self(p) {
                let name = node.method.to_string();
                if self.methods.contains_key(&name) {
                    self.called.push(name);
                }
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}

