//! Explicitness rule handlers: implicit inputs and implicit outputs.
//!
//! The parameters are the explicit inputs of a function. The return value is
//! its explicit output. Each other path that data uses to go into or out of a
//! function is implicit.

use std::collections::{BTreeMap, HashSet};

use proc_macro2::{TokenStream, TokenTree};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, FnArg, Pat, Type};

use crate::report::Violation;
use crate::ruleset::LoadedRule;

use crate::analyze::helpers::{format_message, func_violation, property_bool};
use crate::analyze::model::{collect_format_captures, FileModel, FnModel};

/// Calls that read the environment, the clock, the file system, or a source
/// of random values. The key is the last two path segments.
const INPUT_CALLS: &[&str] = &[
    "env::var",
    "env::var_os",
    "env::vars",
    "env::vars_os",
    "env::args",
    "env::args_os",
    "env::current_dir",
    "env::current_exe",
    "env::temp_dir",
    "env::home_dir",
    "io::stdin",
    "SystemTime::now",
    "Instant::now",
    "Utc::now",
    "Local::now",
    "fs::read",
    "fs::read_to_string",
    "fs::read_dir",
    "fs::read_link",
    "fs::metadata",
    "fs::symlink_metadata",
    "fs::canonicalize",
    "fs::exists",
    "File::open",
    "rand::random",
    "rand::thread_rng",
    "rand::rng",
];

/// Calls that change the file system, the process environment, or the
/// process, or that open a standard output stream.
const OUTPUT_CALLS: &[&str] = &[
    "fs::write",
    "fs::copy",
    "fs::rename",
    "fs::hard_link",
    "fs::create_dir",
    "fs::create_dir_all",
    "fs::remove_file",
    "fs::remove_dir",
    "fs::remove_dir_all",
    "fs::set_permissions",
    "File::create",
    "File::create_new",
    "io::stdout",
    "io::stderr",
    "env::set_var",
    "env::remove_var",
    "env::set_current_dir",
    "process::exit",
    "process::abort",
    "Command::new",
];

/// Macros that print or log.
const OUTPUT_MACROS: &[&str] = &[
    "print", "println", "eprint", "eprintln", "dbg", "trace", "debug", "info", "warn", "error",
];

/// Methods that change the value behind an interior-mutability type or a
/// thread-local key, or that give mutable access to it.
const MUTATOR_METHODS: &[&str] = &[
    "store",
    "swap",
    "set",
    "replace",
    "take",
    "lock",
    "try_lock",
    "write",
    "try_write",
    "borrow_mut",
    "get_mut",
    "with_borrow_mut",
    "compare_exchange",
    "compare_exchange_weak",
];

const SELF_READ: &str = "it reads the state of 'self'";
const SELF_WRITE: &str = "it changes the state of 'self'";

#[derive(Clone, Copy)]
enum Direction {
    Input,
    Output,
}

struct Site {
    line: usize,
    detail: String,
}

/// Collects the implicit inputs and outputs of one function.
struct EffectCollector<'m> {
    shared_statics: &'m HashSet<String>,
    include_self: bool,
    /// Closure parameters that hold a thread-local value, with their key.
    aliases: Vec<(String, String)>,
    inputs: Vec<Site>,
    outputs: Vec<Site>,
}


pub(crate) fn apply_implicit_input(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    report_sites(rule, file, model, out, Direction::Input);
}


pub(crate) fn apply_implicit_output(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    report_sites(rule, file, model, out, Direction::Output);
}


fn report_sites(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
    direction: Direction,
) {
    let include_self = property_bool(rule, "include-self", false);
    for function in &model.functions {
        let name = match &function.parent {
            Some(parent) => format!("{parent}::{}", function.name),
            None => function.name.clone(),
        };
        let sites = function_sites(function, &model.shared_statics, include_self, direction);
        for (detail, line) in first_sites(sites) {
            let description =
                format_message(&rule.message, &[function.kind_label(), &name, &detail]);
            let mut violation = func_violation(rule, file, function, description);
            violation.begin_line = line;
            violation.end_line = line;
            out.push(violation);
        }
    }
}


fn function_sites(
    function: &FnModel<'_>,
    shared_statics: &HashSet<String>,
    include_self: bool,
    direction: Direction,
) -> Vec<Site> {
    // A trait impl cannot change the signature that the trait gives it, so the
    // parameter and receiver checks do not apply there.
    let mut collector = EffectCollector {
        shared_statics,
        include_self: include_self && !function.in_trait_impl,
        aliases: Vec::new(),
        inputs: Vec::new(),
        outputs: Vec::new(),
    };
    if !function.in_trait_impl {
        collector.note_signature(function.signature);
    }
    if let Some(body) = function.body {
        collector.visit_block(body);
    }
    match direction {
        Direction::Input => collector.inputs,
        Direction::Output => collector.outputs,
    }
}


/// Keeps the first line of each distinct site, in line order.
fn first_sites(sites: Vec<Site>) -> Vec<(String, usize)> {
    let mut first: BTreeMap<String, usize> = BTreeMap::new();
    for site in sites {
        let line = first.entry(site.detail).or_insert(site.line);
        *line = (*line).min(site.line);
    }
    let mut ordered: Vec<_> = first.into_iter().collect();
    ordered.sort_by_key(|(_, line)| *line);
    ordered
}


impl EffectCollector<'_> {
    fn note_signature(&mut self, signature: &syn::Signature) {
        for input in &signature.inputs {
            match input {
                FnArg::Receiver(receiver)
                    if self.include_self && is_mut_reference(&receiver.ty) =>
                {
                    let line = receiver.self_token.span().start().line;
                    self.output(line, SELF_WRITE.to_string());
                }
                FnArg::Typed(param) if is_mut_reference(&param.ty) => {
                    let line = param.pat.span().start().line;
                    let detail = format!(
                        "it writes through the '&mut' parameter '{}'",
                        pattern_name(&param.pat)
                    );
                    self.output(line, detail);
                }
                _ => {}
            }
        }
    }

    fn note_name(&mut self, name: &str, line: usize) {
        if name == "self" {
            if self.include_self {
                self.input(line, SELF_READ.to_string());
            }
        } else if self.shared_statics.contains(name) {
            self.input(line, format!("it reads the global '{name}'"));
        }
    }

    fn note_call(&mut self, path: &syn::Path) {
        let key = last_two_segments(path);
        let line = path.span().start().line;
        if INPUT_CALLS.contains(&key.as_str()) {
            self.input(line, format!("it calls '{key}'"));
        } else if OUTPUT_CALLS.contains(&key.as_str()) {
            self.output(line, format!("it calls '{key}'"));
        }
    }

    /// Records a write to `place`. A place that is not rooted in a shared
    /// static or in `self` is an ordinary expression.
    fn note_write(&mut self, place: &Expr) {
        match place_root(place).map(|(root, line)| (self.resolve(root), line)) {
            Some((root, line)) if self.shared_statics.contains(&root) => {
                self.output(line, format!("it writes the global '{root}'"));
            }
            Some((root, line)) if root == "self" => {
                if self.include_self {
                    self.output(line, SELF_WRITE.to_string());
                }
            }
            _ => self.visit_expr(place),
        }
    }

    /// The thread-local key that `name` holds inside a `with` closure, or
    /// `name` itself.
    fn resolve(&self, name: String) -> String {
        self.aliases
            .iter()
            .rev()
            .find(|(param, _)| *param == name)
            .map_or(name, |(_, key)| key.clone())
    }

    /// Visits `KEY.with(|value| ..)`. A write through `value` is a write to
    /// `KEY`; otherwise the call reads `KEY`. Returns false for other calls.
    fn visit_thread_local_with(&mut self, node: &syn::ExprMethodCall) -> bool {
        let Some((key, param)) = thread_local_with(node) else {
            return false;
        };
        if !self.shared_statics.contains(&key) {
            return false;
        }
        let write = format!("it writes the global '{key}'");
        let before = self.outputs.len();
        self.aliases.push((param, key));
        node.args.iter().for_each(|arg| self.visit_expr(arg));
        self.aliases.pop();
        if !self.outputs[before..]
            .iter()
            .any(|site| site.detail == write)
        {
            self.visit_expr(&node.receiver);
        }
        true
    }

    fn input(&mut self, line: usize, detail: String) {
        self.inputs.push(Site { line, detail });
    }

    fn output(&mut self, line: usize, detail: String) {
        self.outputs.push(Site { line, detail });
    }
}


impl<'ast> Visit<'ast> for EffectCollector<'_> {
    /// A nested item is a different function or type, so its body is not
    /// part of this function.
    fn visit_item(&mut self, _node: &'ast syn::Item) {}

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if let Some(segment) = node.path.segments.last() {
            self.note_name(
                &segment.ident.to_string(),
                segment.ident.span().start().line,
            );
        }
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let Expr::Path(path) = &*node.func {
            self.note_call(&path.path);
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        self.note_write(&node.left);
        self.visit_expr(&node.right);
    }

    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        if is_compound_assign(&node.op) {
            self.note_write(&node.left);
            self.visit_expr(&node.right);
        } else {
            syn::visit::visit_expr_binary(self, node);
        }
    }

    fn visit_expr_reference(&mut self, node: &'ast syn::ExprReference) {
        if node.mutability.is_some() {
            self.note_write(&node.expr);
        } else {
            self.visit_expr(&node.expr);
        }
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if self.visit_thread_local_with(node) {
            return;
        }
        if is_mutator(&node.method.to_string()) {
            self.note_write(&node.receiver);
        } else {
            self.visit_expr(&node.receiver);
        }
        for arg in &node.args {
            self.visit_expr(arg);
        }
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let line = node.path.span().start().line;
        if let Some(segment) = node.path.segments.last() {
            let name = segment.ident.to_string();
            if OUTPUT_MACROS.contains(&name.as_str()) {
                self.output(line, format!("it uses '{name}!'"));
            }
        }
        let parser = Punctuated::<Expr, syn::Token![,]>::parse_terminated;
        let mut names = HashSet::new();
        match parser.parse2(node.tokens.clone()) {
            Ok(args) => args.iter().for_each(|arg| self.visit_expr(arg)),
            Err(_) => collect_token_idents(node.tokens.clone(), &mut names),
        }
        collect_format_captures(node.tokens.clone(), &mut names);
        for name in names {
            self.note_name(&name, line);
        }
    }
}


fn is_mut_reference(ty: &Type) -> bool {
    matches!(ty, Type::Reference(reference) if reference.mutability.is_some())
}


fn pattern_name(pat: &Pat) -> String {
    match pat {
        Pat::Ident(ident) => ident.ident.to_string(),
        _ => "_".to_string(),
    }
}


fn last_two_segments(path: &syn::Path) -> String {
    let names: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    names[names.len().saturating_sub(2)..].join("::")
}


/// The key and the closure parameter of `KEY.with(|value| ..)`.
fn thread_local_with(node: &syn::ExprMethodCall) -> Option<(String, String)> {
    let (Expr::Path(receiver), Some(Expr::Closure(closure))) = (&*node.receiver, node.args.first())
    else {
        return None;
    };
    let param = match closure.inputs.first()? {
        Pat::Type(typed) => &*typed.pat,
        param => param,
    };
    let Pat::Ident(param) = param else {
        return None;
    };
    let key = &receiver.path.segments.last()?.ident;
    (node.method == "with").then(|| (key.to_string(), param.ident.to_string()))
}


fn is_mutator(method: &str) -> bool {
    method.starts_with("fetch_") || MUTATOR_METHODS.contains(&method)
}


fn is_compound_assign(op: &syn::BinOp) -> bool {
    matches!(
        op,
        syn::BinOp::AddAssign(_)
            | syn::BinOp::SubAssign(_)
            | syn::BinOp::MulAssign(_)
            | syn::BinOp::DivAssign(_)
            | syn::BinOp::RemAssign(_)
            | syn::BinOp::BitXorAssign(_)
            | syn::BinOp::BitAndAssign(_)
            | syn::BinOp::BitOrAssign(_)
            | syn::BinOp::ShlAssign(_)
            | syn::BinOp::ShrAssign(_)
    )
}


/// The name and line of the variable that holds `place`, when `place` is a
/// path, a field, an index, or a dereference of one.
fn place_root(place: &Expr) -> Option<(String, usize)> {
    match place {
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| (segment.ident.to_string(), segment.ident.span().start().line)),
        Expr::Field(field) => place_root(&field.base),
        Expr::Index(index) => place_root(&index.expr),
        Expr::Paren(paren) => place_root(&paren.expr),
        Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Deref(_)) => place_root(&unary.expr),
        _ => None,
    }
}


fn collect_token_idents(tokens: TokenStream, names: &mut HashSet<String>) {
    for token in tokens {
        match token {
            TokenTree::Group(group) => collect_token_idents(group.stream(), names),
            TokenTree::Ident(ident) => {
                names.insert(ident.to_string());
            }
            _ => {}
        }
    }
}
