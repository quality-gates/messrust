//! Syntax-only analysis for codesize, naming, unusedcode, cleancode, design,
//! controversial, and explicitness rules.

mod helpers;
mod kind;
mod model;
mod parse;
mod rules;

pub use kind::RuleKind;

use syn::Item;

use crate::report::{ProcessingError, Report, Violation};
use crate::ruleset::LoadedRule;
use crate::suppressions::Suppressions;

use self::model::FileModel;
use self::rules::cleancode::*;
use self::rules::codesize::*;
use self::rules::controversial::*;
use self::rules::design::*;
use self::rules::explicitness::*;
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
    let options = AnalysisOptions {
        strict,
        ignore_tests,
    };
    analyze_source(&path.display().to_string(), &src, rules, &options)
}


/// Options that change how `analyze_source` treats one source text.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AnalysisOptions {
    /// Keep suppressed findings and mark them `suppressed`.
    pub strict: bool,
    /// Skip `#[cfg(test)]` modules.
    pub ignore_tests: bool,
}


/// Analyzes in-memory `source` with `rules`. `path` is only the display name
/// of each finding; this function does no file I/O.
pub(crate) fn analyze_source(
    path: &str,
    source: &str,
    rules: &[LoadedRule],
    options: &AnalysisOptions,
) -> Result<Vec<Violation>, String> {
    let file = parse::parse_file(source)?;
    let production = options.ignore_tests.then(|| without_test_modules(&file));
    let model = FileModel::from_file(&file, production.as_ref(), source);
    let mut violations = Vec::new();
    for rule in rules {
        apply_rule(rule, path, &model, &mut violations);
    }
    let suppressions = Suppressions::from_source(source);
    violations.retain_mut(|violation| {
        if !suppressions.contains(violation.begin_line, &violation.rule_name) {
            return true;
        }
        if options.strict {
            violation.suppressed = true;
            true
        } else {
            false
        }
    });
    Ok(violations)
}


/// Returns a copy of `file` without the `#[cfg(test)]` modules at all module
/// depths. The spans stay the same, so findings keep their source lines.
pub(crate) fn without_test_modules(file: &syn::File) -> syn::File {
    let mut production = file.clone();
    retain_production_modules(&mut production.items);
    production
}

fn retain_production_modules(items: &mut Vec<Item>) {
    items.retain(|item| !matches!(item, Item::Mod(module) if is_test_module(&module.attrs)));
    for item in items {
        if let Item::Mod(syn::ItemMod {
            content: Some((_, nested)),
            ..
        }) = item
        {
            retain_production_modules(nested);
        }
    }
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
    apply_implicit_input,
    apply_implicit_output,
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
    use crate::ruleset::{load_and_filter, LoadOptions};

    fn naming_rule(name: &str) -> Vec<LoadedRule> {
        let opts = LoadOptions::default();
        load_and_filter(
            &["naming".to_string()],
            &[name.to_string()],
            &[],
            &opts,
            &mut |_| {},
        )
        .unwrap()
    }

    #[test]
    fn analyze_source_reports_finding_with_display_path() {
        let rules = naming_rule("ShortClassName");
        let options = AnalysisOptions::default();

        let violations = analyze_source("mem/a.rs", "\nstruct A;\n", &rules, &options).unwrap();

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule_name, "ShortClassName");
        assert_eq!(violations[0].file, "mem/a.rs");
        assert_eq!(violations[0].begin_line, 2);
        assert!(!violations[0].suppressed);
    }

    #[test]
    fn analyze_source_drops_suppressed_finding_unless_strict() {
        let rules = naming_rule("ShortClassName");
        let source = "// messrust-disable-next-line ShortClassName\nstruct A;\n";

        let lenient = AnalysisOptions {
            strict: false,
            ignore_tests: false,
        };
        assert!(analyze_source("a.rs", source, &rules, &lenient)
            .unwrap()
            .is_empty());

        let strict = AnalysisOptions {
            strict: true,
            ignore_tests: false,
        };
        let violations = analyze_source("a.rs", source, &rules, &strict).unwrap();
        assert_eq!(violations.len(), 1);
        assert!(violations[0].suppressed);
    }

    #[test]
    fn analyze_source_skips_test_modules_when_ignoring_tests() {
        let rules = naming_rule("ShortClassName");
        let source = "#[cfg(test)]\nmod tests {\n    struct A;\n}\n";

        let with_tests = AnalysisOptions::default();
        assert_eq!(
            analyze_source("a.rs", source, &rules, &with_tests)
                .unwrap()
                .len(),
            1
        );

        let without_tests = AnalysisOptions {
            strict: false,
            ignore_tests: true,
        };
        assert!(analyze_source("a.rs", source, &rules, &without_tests)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn analyze_source_returns_parse_error_message() {
        let rules = naming_rule("ShortClassName");
        let source = "fn broken( {\n";

        let error =
            analyze_source("a.rs", source, &rules, &AnalysisOptions::default()).unwrap_err();

        assert_eq!(Some(error), parse::parse_file(source).err());
    }

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
}
