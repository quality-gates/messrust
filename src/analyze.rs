//! Syntax-only analysis and the first real rule.

use std::path::Path;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{FnArg, ItemFn, ItemImpl, Pat, PatType, Type};

use crate::report::{ProcessingError, Report, Violation};
use crate::ruleset::LoadedRule;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleKind {
    ExcessiveParameterList,
}

const DEFAULT_EXCESSIVE_PARAMETER_MINIMUM: usize = 10;

pub fn analyze_files(files: &[std::path::PathBuf], rules: &[LoadedRule]) -> Report {
    let mut report = Report::default();
    for path in files {
        match analyze_one(path, rules) {
            Ok(violations) => report.violations.extend(violations),
            Err(message) => report.errors.push(ProcessingError {
                file: path.display().to_string(),
                message,
            }),
        }
    }
    report
        .violations
        .sort_by(|a, b| (&a.file, a.begin_line).cmp(&(&b.file, b.begin_line)));
    report
}

fn analyze_one(path: &Path, rules: &[LoadedRule]) -> Result<Vec<Violation>, String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file = syn::parse_file(&src).map_err(|e| e.to_string())?;

    let mut violations = Vec::new();
    for rule in rules {
        match rule.kind {
            RuleKind::ExcessiveParameterList => {
                let minimum = property_usize(rule, "minimum", DEFAULT_EXCESSIVE_PARAMETER_MINIMUM);
                let mut visitor = ExcessiveParameterListVisitor {
                    file: path.display().to_string(),
                    rule,
                    minimum,
                    current_type: String::new(),
                    violations: &mut violations,
                };
                visitor.visit_file(&file);
            }
        }
    }
    Ok(violations)
}

fn property_usize(rule: &LoadedRule, key: &str, default: usize) -> usize {
    rule.properties
        .get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn type_name(ty: &Type) -> String {
    match ty {
        Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

struct ExcessiveParameterListVisitor<'a> {
    file: String,
    rule: &'a LoadedRule,
    minimum: usize,
    current_type: String,
    violations: &'a mut Vec<Violation>,
}

impl<'ast> Visit<'ast> for ExcessiveParameterListVisitor<'_> {
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev = std::mem::replace(&mut self.current_type, type_name(&node.self_ty));
        syn::visit::visit_item_impl(self, node);
        self.current_type = prev;
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        push_violation(
            self,
            Artifact::Function {
                name: node.sig.ident.to_string(),
            },
            &node.sig.inputs,
            node.sig.fn_token.span().start().line,
        );
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        push_violation(
            self,
            Artifact::Method {
                class: self.current_type.clone(),
                name: node.sig.ident.to_string(),
            },
            &node.sig.inputs,
            node.sig.fn_token.span().start().line,
        );
        syn::visit::visit_impl_item_fn(self, node);
    }
}

enum Artifact {
    Function { name: String },
    Method { class: String, name: String },
}

fn push_violation(
    visitor: &mut ExcessiveParameterListVisitor<'_>,
    artifact: Artifact,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    begin_line: usize,
) {
    let count = count_params(inputs);
    if count < visitor.minimum {
        return;
    }
    let (kind, class, function, method, name) = match artifact {
        Artifact::Function { name } => ("function", String::new(), name.clone(), String::new(), name),
        Artifact::Method { class, name } => {
            ("method", class, String::new(), name.clone(), name)
        }
    };
    visitor.violations.push(Violation {
        file: visitor.file.clone(),
        begin_line,
        end_line: begin_line,
        rule_name: visitor.rule.name.clone(),
        ruleset_name: visitor.rule.ruleset_name.clone(),
        description: format!(
            "The {kind} {name} has {count} parameters. Consider reducing the number of parameters to less than {}.",
            visitor.minimum
        ),
        priority: visitor.rule.priority,
        package: String::new(),
        function,
        class,
        method,
        external_info_url: String::new(),
        suppressed: false,
    });
}

fn count_params(inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>) -> usize {
    inputs
        .iter()
        .filter(|arg| match arg {
            FnArg::Receiver(_) => false,
            FnArg::Typed(PatType { pat, .. }) => !matches!(**pat, Pat::Wild(_)),
        })
        .count()
}
