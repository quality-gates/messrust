//! Syntax-only analysis for codesize, naming, unusedcode, cleancode, design,
//! and controversial rules.

mod helpers;
mod kind;
mod model;
mod parse;
mod rules;

pub use kind::RuleKind;

use syn::spanned::Spanned;
use syn::Item;

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static TEST_RANGE_COMPARISONS: Cell<usize> = const { Cell::new(0) };
}

use crate::report::{ProcessingError, Report, Violation};
use crate::ruleset::LoadedRule;
use crate::suppressions::Suppressions;

use self::model::FileModel;
use self::rules::cleancode::*;
use self::rules::codesize::*;
use self::rules::controversial::*;
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
    let file = parse::parse_file(&src)?;
    let file_name = path.display().to_string();
    let model = FileModel::from_file(&file, &src, ignore_tests);
    let mut violations = Vec::new();
    for rule in rules {
        apply_rule(rule, &file_name, &model, &mut violations);
    }
    if ignore_tests {
        let test_modules = test_module_ranges(&file);
        violations.retain(|violation| !test_modules.contains(violation.begin_line));
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


pub(crate) struct TestModuleRanges {
    ranges: Vec<(usize, usize)>,
}

impl TestModuleRanges {
    fn new(mut ranges: Vec<(usize, usize)>) -> Self {
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
        for (start, end) in ranges {
            if let Some((_, previous_end)) = merged.last_mut() {
                if start <= *previous_end {
                    *previous_end = (*previous_end).max(end);
                    continue;
                }
            }
            merged.push((start, end));
        }
        Self { ranges: merged }
    }

    fn contains(&self, line: usize) -> bool {
        let index = self.ranges.partition_point(|(start, _)| {
            #[cfg(test)]
            TEST_RANGE_COMPARISONS.with(|comparisons| comparisons.set(comparisons.get() + 1));
            *start <= line
        });
        index > 0 && self.ranges[index - 1].1 >= line
    }
}

pub(crate) fn test_module_ranges(file: &syn::File) -> TestModuleRanges {
    let mut ranges = Vec::new();
    collect_test_module_ranges(&file.items, &mut ranges);
    TestModuleRanges::new(ranges)
}


fn eval_cfg_meta_for_test(meta: &syn::Meta, test_val: bool) -> bool {
    match meta {
        syn::Meta::Path(path) => {
            if path.is_ident("test") {
                test_val
            } else {
                true
            }
        }
        syn::Meta::NameValue(_) => true,
        syn::Meta::List(list) => {
            let Ok(inners) = list
                .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
            else {
                return true;
            };
            if list.path.is_ident("not") {
                if let Some(first) = inners.first() {
                    !eval_cfg_meta_for_test(first, test_val)
                } else {
                    true
                }
            } else if list.path.is_ident("all") {
                inners.iter().all(|inner| eval_cfg_meta_for_test(inner, test_val))
            } else if list.path.is_ident("any") {
                inners.iter().any(|inner| eval_cfg_meta_for_test(inner, test_val))
            } else {
                true
            }
        }
    }
}

fn eval_cfg_attribute_for_test(attribute: &syn::Attribute, test_val: bool) -> bool {
    let Ok(metas) = attribute
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
    else {
        return true;
    };
    metas.iter().all(|meta| eval_cfg_meta_for_test(meta, test_val))
}

pub(crate) fn is_test_module(attrs: &[syn::Attribute]) -> bool {
    let cfg_attrs: Vec<_> = attrs.iter().filter(|a| a.path().is_ident("cfg")).collect();
    if cfg_attrs.is_empty() {
        return false;
    }
    let compiles_in_prod = cfg_attrs.iter().all(|a| eval_cfg_attribute_for_test(a, false));
    let compiles_in_test = cfg_attrs.iter().all(|a| eval_cfg_attribute_for_test(a, true));
    !compiles_in_prod && compiles_in_test
}

pub(crate) fn collect_test_module_ranges(items: &[Item], ranges: &mut Vec<(usize, usize)>) {
    for item in items {
        let Item::Mod(module) = item else {
            continue;
        };
        if is_test_module(&module.attrs) {
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


pub(crate) fn apply_rule(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    RULE_HANDLERS[rule.kind as usize](rule, file, model, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_test_module_distinguishes_positive_and_negative_test_guards() {
        let test_mod: syn::ItemMod = syn::parse_quote!(
            #[cfg(test)]
            mod a;
        );
        assert!(is_test_module(&test_mod.attrs));

        let not_test_mod: syn::ItemMod = syn::parse_quote!(
            #[cfg(not(test))]
            mod b;
        );
        assert!(!is_test_module(&not_test_mod.attrs));

        let compound_test: syn::ItemMod = syn::parse_quote!(
            #[cfg(all(test, feature = "foo"))]
            mod c;
        );
        assert!(is_test_module(&compound_test.attrs));

        let compound_not_test: syn::ItemMod = syn::parse_quote!(
            #[cfg(all(not(test), feature = "foo"))]
            mod d;
        );
        assert!(!is_test_module(&compound_not_test.attrs));

        let comma_test: syn::ItemMod = syn::parse_quote!(
            #[cfg(test, feature = "foo")]
            mod e;
        );
        assert!(is_test_module(&comma_test.attrs));

        let regular_mod: syn::ItemMod = syn::parse_quote!(
            #[cfg(feature = "foo")]
            mod f;
        );
        assert!(!is_test_module(&regular_mod.attrs));
    }

    #[test]
    fn many_production_findings_use_logarithmic_range_queries() {
        let count = 4_096;
        let ranges = (0..count)
            .map(|index| (10_000 + index * 2, 10_000 + index * 2))
            .collect();
        let test_modules = TestModuleRanges::new(ranges);
        TEST_RANGE_COMPARISONS.with(|comparisons| comparisons.set(0));

        for line in 1..=count {
            assert!(!test_modules.contains(line));
        }

        let comparisons = TEST_RANGE_COMPARISONS.with(Cell::get);
        assert!(comparisons < count * 20, "comparisons: {comparisons}");
    }
}
