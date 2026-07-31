//! Syntax-only analysis and the first real rule.

use std::path::Path;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{FnArg, ItemFn, Pat, PatType};

use crate::report::{ProcessingError, Report, Violation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleId {
    ExcessiveParameterList,
}

const EXCESSIVE_PARAMETER_MINIMUM: usize = 10;

pub fn analyze_files(files: &[std::path::PathBuf], rules: &[RuleId]) -> Report {
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

fn analyze_one(path: &Path, rules: &[RuleId]) -> Result<Vec<Violation>, String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file = syn::parse_file(&src).map_err(|e| format!("{}: {e}", path.display()))?;

    let mut violations = Vec::new();
    if rules.contains(&RuleId::ExcessiveParameterList) {
        let mut visitor = ExcessiveParameterListVisitor {
            file: path.display().to_string(),
            violations: &mut violations,
        };
        visitor.visit_file(&file);
    }
    Ok(violations)
}

struct ExcessiveParameterListVisitor<'a> {
    file: String,
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
    violations: &mut Vec<Violation>,
) {
    let count = count_params(inputs);
    if count >= EXCESSIVE_PARAMETER_MINIMUM {
        violations.push(Violation {
            file: file.to_string(),
            begin_line,
            rule_name: "ExcessiveParameterList".to_string(),
            description: format!(
                "The {kind} {name} has {count} parameters. Consider reducing the number of parameters to less than {EXCESSIVE_PARAMETER_MINIMUM}."
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
            // Count typed args; receiver excluded (messgo does not count receiver).
        })
        .count()
}
