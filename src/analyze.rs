//! Syntax-only analysis and the first real rule.

use std::path::Path;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{FnArg, ItemFn, Pat, PatType};

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
                    minimum,
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

struct ExcessiveParameterListVisitor<'a> {
    file: String,
    minimum: usize,
    violations: &'a mut Vec<Violation>,
}

impl<'ast> Visit<'ast> for ExcessiveParameterListVisitor<'_> {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        check_fn(
            &self.file,
            &node.sig.ident.to_string(),
            "function",
            &node.sig.inputs,
            node.sig.fn_token.span().start().line,
            self.minimum,
            self.violations,
        );
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        check_fn(
            &self.file,
            &node.sig.ident.to_string(),
            "method",
            &node.sig.inputs,
            node.sig.fn_token.span().start().line,
            self.minimum,
            self.violations,
        );
        syn::visit::visit_impl_item_fn(self, node);
    }
}

fn check_fn(
    file: &str,
    name: &str,
    kind: &str,
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    begin_line: usize,
    minimum: usize,
    violations: &mut Vec<Violation>,
) {
    let count = count_params(inputs);
    if count >= minimum {
        violations.push(Violation {
            file: file.to_string(),
            begin_line,
            rule_name: "ExcessiveParameterList".to_string(),
            description: format!(
                "The {kind} {name} has {count} parameters. Consider reducing the number of parameters to less than {minimum}."
            ),
        });
    }
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
