//! Syntax-only analysis for codesize, naming, unusedcode, cleancode, design,
//! and controversial rules.

mod helpers;
mod kind;
mod model;
mod rules;

pub use kind::RuleKind;

use syn::spanned::Spanned;
use syn::Item;

use crate::report::{ProcessingError, Report, Violation};
use crate::ruleset::LoadedRule;
use crate::suppressions::Suppressions;

use self::model::FileModel;
use self::rules::codesize::*;
use self::rules::controversial::*;
use self::rules::cleancode::*;
use self::rules::design::*;
use self::rules::naming::*;
use self::rules::unusedcode::*;


pub fn analyze_files(
    files: &[std::path::PathBuf],
    rules: &[LoadedRule],
    strict: bool,
    ignore_tests: bool,
) -> Report {
    let mut report = Report::default();
    for path in files {
        match analyze_one(path, rules, strict, ignore_tests) {
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


pub(crate) fn analyze_one(
    path: &std::path::Path,
    rules: &[LoadedRule],
    strict: bool,
    ignore_tests: bool,
) -> Result<Vec<Violation>, String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file = syn::parse_file(&src).map_err(|e| e.to_string())?;
    let file_name = path.display().to_string();
    let model = FileModel::from_file(&file, &src);
    let mut violations = Vec::new();
    for rule in rules {
        apply_rule(rule, &file_name, &model, &mut violations);
    }
    if ignore_tests {
        let test_modules = test_module_ranges(&file);
        violations.retain(|violation| {
            !test_modules
                .iter()
                .any(|(start, end)| (*start..=*end).contains(&violation.begin_line))
        });
    }
    let suppressions = Suppressions::from_source(&src);
    violations.retain_mut(|violation| {
        if !suppressions.contains(violation.begin_line, &violation.rule_name) {
            return true;
        }
        if strict {
            violation.suppressed = true;
            true
        } else {
            false
        }
    });
    Ok(violations)
}


pub(crate) fn test_module_ranges(file: &syn::File) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    collect_test_module_ranges(&file.items, &mut ranges);
    ranges
}


pub(crate) fn collect_test_module_ranges(items: &[Item], ranges: &mut Vec<(usize, usize)>) {
    for item in items {
        let Item::Mod(module) = item else {
            continue;
        };
        let is_test = module.attrs.iter().any(|attribute| {
            if !attribute.path().is_ident("cfg") {
                return false;
            }
            attribute.meta.require_list().ok().is_some_and(|list| {
                list.tokens
                    .to_string()
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .any(|part| part == "test")
            })
        });
        if is_test {
            let span = module.span();
            ranges.push((span.start().line, span.end().line));
        }
        if let Some((_, nested)) = &module.content {
            collect_test_module_ranges(nested, ranges);
        }
    }
}


pub(crate) type RuleHandler = fn(&LoadedRule, &str, &FileModel<'_>, &mut Vec<Violation>);


pub(crate) const RULE_HANDLERS: &[RuleHandler] = &[
    apply_cyclomatic_complexity,
    apply_npath_complexity,
    apply_excessive_method_length,
    apply_excessive_class_length,
    apply_excessive_parameter_list,
    apply_excessive_public_count,
    apply_too_many_fields,
    apply_too_many_methods,
    apply_too_many_public_methods,
    apply_excessive_class_complexity,
    apply_short_class_name,
    apply_long_class_name,
    apply_short_variable,
    apply_long_variable,
    apply_short_method_name,
    apply_constant_naming,
    apply_boolean_get_method_name,
    apply_unused_private_field,
    apply_unused_local_variable,
    apply_unused_private_method,
    apply_unused_formal_parameter,
    apply_boolean_argument_flag,
    apply_else_expression,
    apply_if_statement_assignment,
    apply_duplicated_array_key,
    apply_static_access,
    apply_exit_expression,
    apply_goto_statement,
    apply_count_in_loop_expression,
    apply_development_code_fragment,
    apply_empty_catch_block,
    apply_coupling_between_objects,
    apply_global_variable,
    apply_lack_of_cohesion,
    apply_camel_case_class_name,
    apply_camel_case_method_name,
    apply_camel_case_property_name,
    apply_camel_case_parameter_name,
    apply_camel_case_variable_name,
];


pub(crate) fn apply_rule(rule: &LoadedRule, file: &str, model: &FileModel<'_>, out: &mut Vec<Violation>) {
    RULE_HANDLERS[rule.kind as usize](rule, file, model, out);
}

