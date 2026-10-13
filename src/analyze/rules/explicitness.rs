use std::collections::{BTreeMap, HashSet};

use proc_macro2::{TokenStream, TokenTree};

use crate::report::Violation;
use crate::ruleset::LoadedRule;

use crate::analyze::helpers::{format_message, func_violation, property_bool};
use crate::analyze::model::FileModel;

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

const OUTPUT_MACROS: &[&str] = &[
    "print", "println", "eprint", "eprintln", "dbg", "trace", "debug", "info", "warn", "error",
];

const MUTATOR_METHODS: &[&str] = &[
    "push",
    "pop",
    "insert",
    "remove",
    "clear",
    "append",
    "extend",
    "extend_from_slice",
    "push_str",
    "truncate",
    "retain",
    "sort",
    "sort_unstable",
    "sort_by",
    "sort_unstable_by",
    "reverse",
    "fill",
    "store",
    "swap",
    "set",
    "replace",
    "take",
    "with_borrow_mut",
    "compare_exchange",
    "compare_exchange_weak",
];

mod control;
mod effects;
mod macros;
mod patterns;
mod storage;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Direction {
    Input,
    Output,
    Unknown,
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

pub(crate) fn apply_unknown_effect(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    report_sites(rule, file, model, out, Direction::Unknown);
}

fn report_sites(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
    direction: Direction,
) {
    let summaries = effects::summaries(model, property_bool(rule, "include-self", false));
    for summary in summaries {
        if !summary.report {
            continue;
        }
        let function = &model.functions[summary.function];
        let name = summary.name;
        let mut first = BTreeMap::new();
        for site in summary
            .sites
            .iter()
            .filter(|site| site.direction == direction)
        {
            first
                .entry(site.detail.clone())
                .and_modify(|line: &mut usize| {
                    *line = (*line).min(site.line);
                })
                .or_insert(site.line);
        }
        let mut ordered: Vec<_> = first.into_iter().collect();
        ordered.sort_by_key(|(_, line)| *line);
        for (detail, line) in ordered {
            let description =
                format_message(&rule.message, &[function.kind_label(), &name, &detail]);
            let mut violation = func_violation(rule, file, function, description);
            violation.function = summary.owner.function.clone();
            violation.class = summary.owner.class.clone();
            violation.method = summary.owner.method.clone();
            violation.begin_line = line;
            violation.end_line = line;
            out.push(violation);
        }
    }
}

fn call_path(expr: &syn::ExprPath) -> String {
    let path = storage::path_text(&expr.path);
    let receiver = expr
        .qself
        .as_ref()
        .filter(|qself| qself.position == 0)
        .and_then(|qself| match &*qself.ty {
            syn::Type::Path(ty) if ty.qself.is_none() => Some(&ty.path),
            _ => None,
        });
    match receiver {
        Some(ty) => format!("{}::{path}", storage::path_text(ty)),
        None => path,
    }
}

fn receiver_type(expr: &syn::ExprPath) -> Option<syn::Type> {
    if let Some(qself) = &expr.qself {
        return (qself.position == 0).then(|| (*qself.ty).clone());
    }
    let mut path = expr.path.clone();
    path.segments.pop()?;
    if path.segments.is_empty() {
        return None;
    }
    if path.segments.trailing_punct() {
        path.segments.pop_punct();
    }
    Some(syn::Type::Path(syn::TypePath { qself: None, path }))
}

fn resolved_call_key(expr: &syn::ExprPath, model: &FileModel<'_>, scope: &str) -> String {
    let path = call_path(expr);
    let resolved = crate::analyze::model::resolve_type_path(&path, scope, &model.imports);
    let relative = resolved
        .strip_prefix(&format!("{scope}::"))
        .unwrap_or(&resolved);
    if !["std::", "core::", "alloc::", "rand::", "chrono::"]
        .iter()
        .any(|prefix| relative.starts_with(prefix))
    {
        return relative.to_string();
    }
    let names: Vec<_> = relative.split("::").collect();
    names[names.len().saturating_sub(2)..].join("::")
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
