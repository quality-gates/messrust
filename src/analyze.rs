//! Syntax-only analysis for codesize, naming, unusedcode, cleancode, design,
//! and controversial rules.
//
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use proc_macro2::{TokenStream, TokenTree};
use regex::Regex;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Fields, FnArg, Item, ItemEnum, ItemFn, ItemImpl, ItemStruct, ItemTrait, ItemUnion, Member, Pat,
    PatType, ReturnType, Visibility,
};

use crate::metrics::{cyclomatic_complexity, effective_lines_of_code, npath_complexity};
use crate::report::{ProcessingError, Report, Violation};
use crate::ruleset::LoadedRule;
use crate::suppressions::Suppressions;

#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleKind {
    CyclomaticComplexity,
    NPathComplexity,
    ExcessiveMethodLength,
    ExcessiveClassLength,
    ExcessiveParameterList,
    ExcessivePublicCount,
    TooManyFields,
    TooManyMethods,
    TooManyPublicMethods,
    ExcessiveClassComplexity,
    ShortClassName,
    LongClassName,
    ShortVariable,
    LongVariable,
    ShortMethodName,
    ConstantNamingConventions,
    BooleanGetMethodName,
    UnusedPrivateField,
    UnusedLocalVariable,
    UnusedPrivateMethod,
    UnusedFormalParameter,
    BooleanArgumentFlag,
    ElseExpression,
    IfStatementAssignment,
    DuplicatedArrayKey,
    StaticAccess,
    ExitExpression,
    GotoStatement,
    CountInLoopExpression,
    DevelopmentCodeFragment,
    EmptyCatchBlock,
    CouplingBetweenObjects,
    GlobalVariable,
    LackOfCohesionOfMethods,
    CamelCaseClassName,
    CamelCaseMethodName,
    CamelCasePropertyName,
    CamelCaseParameterName,
    CamelCaseVariableName,
}

impl RuleKind {
    const COUNT: usize = Self::CamelCaseVariableName as usize + 1;
}

const DEFAULT_CCN: usize = 10;
const DEFAULT_NPATH: usize = 200;
const DEFAULT_METHOD_LOC: usize = 100;
const DEFAULT_CLASS_LOC: usize = 1000;
const DEFAULT_PARAMS: usize = 10;
const DEFAULT_PUBLIC: usize = 45;
const DEFAULT_FIELDS: usize = 15;
const DEFAULT_METHODS: usize = 25;
const DEFAULT_PUBLIC_METHODS: usize = 10;
const DEFAULT_WMC: usize = 50;
const DEFAULT_IGNORE_PATTERN: &str = "(^(set|get|is|has|with))i";
const DEFAULT_SHORT_NAME: usize = 3;
const DEFAULT_LONG_CLASS: usize = 40;
const DEFAULT_LONG_VAR: usize = 20;
const DEFAULT_CBO: usize = 13;
const DEFAULT_LCOM4: usize = 1;
const DEFAULT_DEV_FUNCS: &str = "println,print,eprintln,dbg";

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

fn analyze_one(
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

fn test_module_ranges(file: &syn::File) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    collect_test_module_ranges(&file.items, &mut ranges);
    ranges
}

fn collect_test_module_ranges(items: &[Item], ranges: &mut Vec<(usize, usize)>) {
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

type RuleHandler = fn(&LoadedRule, &str, &FileModel<'_>, &mut Vec<Violation>);

const RULE_HANDLERS: [RuleHandler; RuleKind::COUNT] = [
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

fn apply_rule(rule: &LoadedRule, file: &str, model: &FileModel<'_>, out: &mut Vec<Violation>) {
    RULE_HANDLERS[rule.kind as usize](rule, file, model, out);
}

fn apply_cyclomatic_complexity(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "reportLevel", DEFAULT_CCN);
    for f in &model.functions {
        let value = cyclomatic_complexity(f.body);
        if value >= threshold {
            out.push(func_violation(
                rule,
                file,
                f,
                format_message(
                    &rule.message,
                    &[
                        f.kind_label(),
                        &f.name,
                        &value.to_string(),
                        &threshold.to_string(),
                    ],
                ),
            ));
        }
    }
}

fn apply_npath_complexity(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "minimum", DEFAULT_NPATH);
    for f in &model.functions {
        let value = npath_complexity(f.body);
        if value >= threshold {
            out.push(func_violation(
                rule,
                file,
                f,
                format_message(
                    &rule.message,
                    &[
                        f.kind_label(),
                        &f.name,
                        &value.to_string(),
                        &threshold.to_string(),
                    ],
                ),
            ));
        }
    }
}

fn apply_excessive_method_length(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "minimum", DEFAULT_METHOD_LOC);
    let ignore_ws = property_bool(rule, "ignore-whitespace", false);
    for f in &model.functions {
        let value = fn_loc(f, model.src, ignore_ws);
        if value >= threshold {
            out.push(func_violation(
                rule,
                file,
                f,
                format_message(
                    &rule.message,
                    &[
                        f.kind_label(),
                        &f.name,
                        &value.to_string(),
                        &threshold.to_string(),
                    ],
                ),
            ));
        }
    }
}

fn apply_excessive_class_length(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "minimum", DEFAULT_CLASS_LOC);
    let ignore_ws = property_bool(rule, "ignore-whitespace", false);
    for t in &model.types {
        let value = type_loc(t, model, ignore_ws);
        if value >= threshold {
            out.push(type_violation(
                rule,
                file,
                t,
                format_message(
                    &rule.message,
                    &[&t.name, &value.to_string(), &threshold.to_string()],
                ),
            ));
        }
    }
}

fn apply_excessive_parameter_list(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "minimum", DEFAULT_PARAMS);
    for f in &model.functions {
        let value = f.param_count;
        if value >= threshold {
            out.push(func_violation(
                rule,
                file,
                f,
                format_message(
                    &rule.message,
                    &[
                        f.kind_label(),
                        &f.name,
                        &value.to_string(),
                        &threshold.to_string(),
                    ],
                ),
            ));
        }
    }
}

fn apply_excessive_public_count(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "minimum", DEFAULT_PUBLIC);
    for t in &model.types {
        let value = t.public_fields + t.methods.iter().filter(|m| m.is_public).count();
        if value >= threshold {
            out.push(type_violation(
                rule,
                file,
                t,
                format_message(
                    &rule.message,
                    &[
                        &t.node_type,
                        &t.name,
                        &value.to_string(),
                        &threshold.to_string(),
                    ],
                ),
            ));
        }
    }
}

fn apply_too_many_fields(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "maxfields", DEFAULT_FIELDS);
    for t in &model.types {
        if t.node_type != "struct" && t.node_type != "union" {
            continue;
        }
        let value = t.field_count;
        if value > threshold {
            out.push(type_violation(
                rule,
                file,
                t,
                format_message(
                    &rule.message,
                    &[
                        &t.node_type,
                        &t.name,
                        &value.to_string(),
                        &threshold.to_string(),
                    ],
                ),
            ));
        }
    }
}

fn apply_too_many_methods(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "maxmethods", DEFAULT_METHODS);
    let ignore = compile_phpmd_regex(
        rule.properties
            .get("ignorepattern")
            .map(String::as_str)
            .unwrap_or(DEFAULT_IGNORE_PATTERN),
    );
    for t in &model.types {
        let value = t
            .methods
            .iter()
            .filter(|m| !ignored_name(&ignore, &m.name))
            .count();
        if value > threshold {
            out.push(type_violation(
                rule,
                file,
                t,
                format_message(
                    &rule.message,
                    &[
                        &t.node_type,
                        &t.name,
                        &value.to_string(),
                        &threshold.to_string(),
                    ],
                ),
            ));
        }
    }
}

fn apply_too_many_public_methods(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "maxmethods", DEFAULT_PUBLIC_METHODS);
    let ignore = compile_phpmd_regex(
        rule.properties
            .get("ignorepattern")
            .map(String::as_str)
            .unwrap_or(DEFAULT_IGNORE_PATTERN),
    );
    for t in &model.types {
        let value = t
            .methods
            .iter()
            .filter(|m| m.is_public && !ignored_name(&ignore, &m.name))
            .count();
        if value > threshold {
            out.push(type_violation(
                rule,
                file,
                t,
                format_message(
                    &rule.message,
                    &[
                        &t.node_type,
                        &t.name,
                        &value.to_string(),
                        &threshold.to_string(),
                    ],
                ),
            ));
        }
    }
}

fn apply_excessive_class_complexity(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "maximum", DEFAULT_WMC);
    for t in &model.types {
        let value: usize = t
            .methods
            .iter()
            .map(|m| cyclomatic_complexity(m.body))
            .sum();
        if value >= threshold {
            out.push(type_violation(
                rule,
                file,
                t,
                format_message(
                    &rule.message,
                    &[&t.name, &value.to_string(), &threshold.to_string()],
                ),
            ));
        }
    }
}

fn apply_short_class_name(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let minimum = property_usize(rule, "minimum", DEFAULT_SHORT_NAME);
    let exceptions = property_list(rule, "exceptions");
    for t in &model.types {
        if t.name.chars().count() >= minimum {
            continue;
        }
        if exceptions.iter().any(|e| e == &t.name) {
            continue;
        }
        out.push(type_violation(
            rule,
            file,
            t,
            format_message(&rule.message, &[&t.name, &minimum.to_string()]),
        ));
    }
}

fn apply_long_class_name(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let maximum = property_usize(rule, "maximum", DEFAULT_LONG_CLASS);
    let prefixes = property_list(rule, "subtract-prefixes");
    let suffixes = property_list(rule, "subtract-suffixes");
    for t in &model.types {
        let effective = length_without(&t.name, &prefixes, &suffixes);
        if effective <= maximum {
            continue;
        }
        out.push(type_violation(
            rule,
            file,
            t,
            format_message(&rule.message, &[&t.name, &maximum.to_string()]),
        ));
    }
}

fn apply_short_variable(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let minimum = property_usize(rule, "minimum", DEFAULT_SHORT_NAME);
    let exceptions = property_list(rule, "exceptions");
    for v in &model.variables {
        if v.is_loop_binder {
            continue;
        }
        if v.name.chars().count() >= minimum {
            continue;
        }
        if exceptions.iter().any(|e| e == &v.name) {
            continue;
        }
        out.push(name_violation(
            rule,
            file,
            v.begin_line,
            format_message(&rule.message, &[&v.name, &minimum.to_string()]),
        ));
    }
}

fn apply_long_variable(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let maximum = property_usize(rule, "maximum", DEFAULT_LONG_VAR);
    let prefixes = property_list(rule, "subtract-prefixes");
    let suffixes = property_list(rule, "subtract-suffixes");
    for v in &model.variables {
        if v.is_loop_binder {
            continue;
        }
        let effective = length_without(&v.name, &prefixes, &suffixes);
        if effective <= maximum {
            continue;
        }
        out.push(name_violation(
            rule,
            file,
            v.begin_line,
            format_message(&rule.message, &[&v.name, &maximum.to_string()]),
        ));
    }
}

fn apply_short_method_name(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let minimum = property_usize(rule, "minimum", DEFAULT_SHORT_NAME);
    let exceptions = property_list(rule, "exceptions");
    for f in &model.functions {
        if f.name.chars().count() >= minimum {
            continue;
        }
        if exceptions.iter().any(|e| e == &f.name) {
            continue;
        }
        let parent = f.parent.as_deref().unwrap_or("");
        out.push(func_violation(
            rule,
            file,
            f,
            format_message(&rule.message, &[parent, &f.name, &minimum.to_string()]),
        ));
    }
}

fn apply_constant_naming(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let convention = rule
        .properties
        .get("convention")
        .map(String::as_str)
        .unwrap_or("upper");
    let pascal = convention.eq_ignore_ascii_case("pascal");
    for c in &model.constants {
        let ok = if pascal {
            is_pascal_case(&c.name)
        } else {
            is_upper_case(&c.name)
        };
        if ok {
            continue;
        }
        let description = if pascal {
            format_message("Constant {0} should be defined in PascalCase", &[&c.name])
        } else {
            format_message(&rule.message, &[&c.name])
        };
        out.push(name_violation(rule, file, c.begin_line, description));
    }
}

fn apply_boolean_get_method_name(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let check_parameterized = property_bool(rule, "checkParameterizedMethods", false);
    for f in &model.functions {
        if !is_getter_name(&f.name) || !f.returns_bool {
            continue;
        }
        if !check_parameterized && f.param_count > 0 {
            continue;
        }
        out.push(func_violation(
            rule,
            file,
            f,
            format_message(&rule.message, &[&f.name]),
        ));
    }
}

fn apply_unused_local_variable(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let exceptions = property_list(rule, "exceptions");
    for local in &model.usage.locals {
        if is_rust_unused_name(&local.name) {
            continue;
        }
        if exceptions.iter().any(|e| e == &local.name) {
            continue;
        }
        if model.usage.ident_reads.contains(&local.name) {
            continue;
        }
        out.push(name_violation(
            rule,
            file,
            local.begin_line,
            format_message(&rule.message, &[&local.name]),
        ));
    }
}

fn apply_unused_formal_parameter(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for param in &model.usage.params {
        if is_rust_unused_name(&param.name) {
            continue;
        }
        if model.usage.ident_reads.contains(&param.name) {
            continue;
        }
        out.push(name_violation(
            rule,
            file,
            param.begin_line,
            format_message(&rule.message, &[&param.name]),
        ));
    }
}

fn apply_unused_private_field(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for field in &model.usage.private_fields {
        if is_rust_unused_name(&field.name) {
            continue;
        }
        if model.usage.field_reads.contains(&field.name) {
            continue;
        }
        out.push(name_violation(
            rule,
            file,
            field.begin_line,
            format_message(&rule.message, &[&field.name]),
        ));
    }
}

fn apply_unused_private_method(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for method in &model.usage.private_methods {
        if is_rust_unused_name(&method.name) {
            continue;
        }
        if model.usage.method_calls.contains(&method.name)
            || model.usage.ident_reads.contains(&method.name)
        {
            continue;
        }
        out.push(name_violation(
            rule,
            file,
            method.begin_line,
            format_message(&rule.message, &[&method.name]),
        ));
    }
}

fn apply_boolean_argument_flag(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let exceptions = property_list(rule, "exceptions");
    let ignore = compile_phpmd_regex(
        rule.properties
            .get("ignorepattern")
            .map(String::as_str)
            .unwrap_or(""),
    );
    for f in &model.functions {
        if ignored_name(&ignore, &f.name) {
            continue;
        }
        if let Some(parent) = &f.parent {
            if exceptions.iter().any(|e| e == parent) {
                continue;
            }
        }
        let image = match &f.parent {
            Some(parent) => format!("{parent}::{}", f.name),
            None => f.name.clone(),
        };
        for p in &f.bool_params {
            if is_rust_unused_name(&p.name) {
                continue;
            }
            out.push(name_violation(
                rule,
                file,
                p.begin_line,
                format_message(&rule.message, &[&image, &p.name]),
            ));
        }
    }
}

fn apply_else_expression(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for f in &model.functions {
        let Some(body) = f.body else {
            continue;
        };
        for line in terminal_else_lines(body) {
            out.push(name_violation(
                rule,
                file,
                line,
                format_message(&rule.message, &[&f.name]),
            ));
        }
    }
}

fn apply_if_statement_assignment(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for f in &model.functions {
        let Some(body) = f.body else {
            continue;
        };
        for pos in assignment_in_condition_positions(body) {
            out.push(name_violation(
                rule,
                file,
                pos.line,
                format_message(
                    &rule.message,
                    &[&pos.line.to_string(), &pos.column.to_string()],
                ),
            ));
        }
    }
}

fn apply_duplicated_array_key(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for dup in &model.duplicate_struct_keys {
        out.push(name_violation(
            rule,
            file,
            dup.line,
            format_message(&rule.message, &[&dup.display, &dup.first_line.to_string()]),
        ));
    }
}

fn apply_static_access(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let exceptions = property_list(rule, "exceptions");
    let ignore = compile_phpmd_regex(
        rule.properties
            .get("ignorepattern")
            .map(String::as_str)
            .unwrap_or(""),
    );
    for f in &model.functions {
        if ignored_name(&ignore, &f.name) {
            continue;
        }
        let Some(body) = f.body else {
            continue;
        };
        for access in static_accesses(body, f.parent.as_deref(), &exceptions) {
            out.push(name_violation(
                rule,
                file,
                access.line,
                format_message(&rule.message, &[&access.type_name, &f.name]),
            ));
        }
    }
}

fn apply_exit_expression(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for f in &model.functions {
        let Some(body) = f.body else {
            continue;
        };
        if let Some(line) = exit_expression_line(body) {
            out.push(name_violation(
                rule,
                file,
                line,
                format_message(&rule.message, &[f.kind_label(), &f.name]),
            ));
        }
    }
}

// Rust has no goto; keep the rule loadable and quiet.
fn apply_goto_statement(
    _rule: &LoadedRule,
    _file: &str,
    _model: &FileModel<'_>,
    _out: &mut Vec<Violation>,
) {
}

fn apply_count_in_loop_expression(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for f in &model.functions {
        let Some(body) = f.body else {
            continue;
        };
        for hit in count_in_loop_hits(body) {
            out.push(name_violation(
                rule,
                file,
                hit.line,
                format_message(&rule.message, &[&hit.func_name, &hit.loop_kind]),
            ));
        }
    }
}

fn apply_development_code_fragment(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let extra = rule
        .properties
        .get("unwanted-functions")
        .map(String::as_str)
        .unwrap_or("");
    let unwanted = unwanted_function_set(extra);
    for f in &model.functions {
        let Some(body) = f.body else {
            continue;
        };
        let image = match &f.parent {
            Some(parent) => format!("{parent}::{}", f.name),
            None => f.name.clone(),
        };
        for hit in development_fragment_hits(body, &unwanted) {
            out.push(name_violation(
                rule,
                file,
                hit.line,
                format_message(&rule.message, &[f.kind_label(), &image, &hit.func_name]),
            ));
        }
    }
}

fn apply_empty_catch_block(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for f in &model.functions {
        let Some(body) = f.body else {
            continue;
        };
        for line in empty_catch_lines(body) {
            out.push(name_violation(
                rule,
                file,
                line,
                format_message(&rule.message, &[&f.name]),
            ));
        }
    }
}

fn apply_coupling_between_objects(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "maximum", DEFAULT_CBO);
    for t in &model.types {
        if t.node_type != "struct" && t.node_type != "enum" && t.node_type != "union" {
            continue;
        }
        let value = coupling_between_objects(t, model);
        if value >= threshold {
            out.push(type_violation(
                rule,
                file,
                t,
                format_message(
                    &rule.message,
                    &[&t.name, &value.to_string(), &threshold.to_string()],
                ),
            ));
        }
    }
}

fn apply_global_variable(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let report_immutable = property_bool(rule, "report-immutable", false);
    for g in &model.static_muts {
        if model.mutated_statics.contains(&g.name) || report_immutable {
            out.push(name_violation(
                rule,
                file,
                g.begin_line,
                format_message(&rule.message, &[&g.name]),
            ));
        }
    }
}

fn apply_lack_of_cohesion(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let threshold = property_usize(rule, "maximum", DEFAULT_LCOM4);
    for t in &model.types {
        if t.node_type != "struct" && t.node_type != "enum" && t.node_type != "union" {
            continue;
        }
        let value = lcom4(t);
        if value > threshold {
            out.push(type_violation(
                rule,
                file,
                t,
                format_message(&rule.message, &[&t.name, &value.to_string()]),
            ));
        }
    }
}

fn apply_camel_case_class_name(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    let strict_abbr = property_bool(rule, "camelcase-abbreviations", false);
    for t in &model.types {
        let ok = if strict_abbr {
            is_pascal_case_no_abbrev(&t.name)
        } else {
            is_pascal_case(&t.name)
        };
        if ok {
            continue;
        }
        out.push(type_violation(
            rule,
            file,
            t,
            format_message(&rule.message, &[&t.name]),
        ));
    }
}

fn apply_camel_case_method_name(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for f in &model.functions {
        if f.name == "_" || is_snake_case(&f.name) {
            continue;
        }
        out.push(func_violation(
            rule,
            file,
            f,
            format_message(&rule.message, &[&f.name]),
        ));
    }
}

fn apply_camel_case_property_name(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for t in &model.types {
        for field in &t.fields {
            if is_tuple_field_name(&field.name) || field.name == "_" || is_snake_case(&field.name) {
                continue;
            }
            out.push(name_violation(
                rule,
                file,
                field.begin_line,
                format_message(&rule.message, &[&field.name]),
            ));
        }
    }
}

fn apply_camel_case_parameter_name(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for p in &model.usage.params {
        if p.name == "_" || is_snake_case(&p.name) {
            continue;
        }
        out.push(name_violation(
            rule,
            file,
            p.begin_line,
            format_message(&rule.message, &[&p.name]),
        ));
    }
}

fn apply_camel_case_variable_name(
    rule: &LoadedRule,
    file: &str,
    model: &FileModel<'_>,
    out: &mut Vec<Violation>,
) {
    for v in &model.usage.locals {
        if v.name == "_" || is_snake_case(&v.name) {
            continue;
        }
        out.push(name_violation(
            rule,
            file,
            v.begin_line,
            format_message(&rule.message, &[&v.name]),
        ));
    }
}

fn fn_loc(f: &FnModel<'_>, src: &str, ignore_ws: bool) -> usize {
    if ignore_ws {
        effective_lines_of_code(src, f.begin_line, f.end_line)
    } else {
        f.end_line.saturating_sub(f.begin_line).saturating_add(1)
    }
}

fn type_loc(t: &TypeModel<'_>, model: &FileModel<'_>, ignore_ws: bool) -> usize {
    let mut loc = if ignore_ws {
        effective_lines_of_code(model.src, t.begin_line, t.end_line)
    } else {
        t.end_line.saturating_sub(t.begin_line).saturating_add(1)
    };
    for m in &t.methods {
        loc += if ignore_ws {
            effective_lines_of_code(model.src, m.begin_line, m.end_line)
        } else {
            m.end_line.saturating_sub(m.begin_line).saturating_add(1)
        };
    }
    loc
}

fn func_violation(
    rule: &LoadedRule,
    file: &str,
    f: &FnModel<'_>,
    description: String,
) -> Violation {
    let (function, class, method) = match f.parent {
        Some(ref class) => (String::new(), class.clone(), f.name.clone()),
        None => (f.name.clone(), String::new(), String::new()),
    };
    Violation {
        file: file.to_string(),
        begin_line: f.begin_line,
        end_line: f.begin_line,
        rule_name: rule.name.clone(),
        ruleset_name: rule.ruleset_name.clone(),
        description,
        priority: rule.priority,
        package: String::new(),
        function,
        class,
        method,
        external_info_url: String::new(),
        suppressed: false,
    }
}

fn type_violation(
    rule: &LoadedRule,
    file: &str,
    t: &TypeModel<'_>,
    description: String,
) -> Violation {
    Violation {
        file: file.to_string(),
        begin_line: t.begin_line,
        end_line: t.begin_line,
        rule_name: rule.name.clone(),
        ruleset_name: rule.ruleset_name.clone(),
        description,
        priority: rule.priority,
        package: String::new(),
        function: String::new(),
        class: t.name.clone(),
        method: String::new(),
        external_info_url: String::new(),
        suppressed: false,
    }
}

fn name_violation(
    rule: &LoadedRule,
    file: &str,
    begin_line: usize,
    description: String,
) -> Violation {
    Violation {
        file: file.to_string(),
        begin_line,
        end_line: begin_line,
        rule_name: rule.name.clone(),
        ruleset_name: rule.ruleset_name.clone(),
        description,
        priority: rule.priority,
        package: String::new(),
        function: String::new(),
        class: String::new(),
        method: String::new(),
        external_info_url: String::new(),
        suppressed: false,
    }
}

fn format_message(template: &str, args: &[&str]) -> String {
    let mut out = template.to_string();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), arg);
    }
    out
}

fn property_usize(rule: &LoadedRule, key: &str, default: usize) -> usize {
    rule.properties
        .get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn property_bool(rule: &LoadedRule, key: &str, default: bool) -> bool {
    rule.properties
        .get(key)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(default)
}

fn property_list(rule: &LoadedRule, key: &str) -> Vec<String> {
    rule.properties
        .get(key)
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn length_without(name: &str, prefixes: &[String], suffixes: &[String]) -> usize {
    let mut effective = name;
    for p in prefixes {
        if !p.is_empty() && effective.starts_with(p.as_str()) {
            effective = &effective[p.len()..];
            break;
        }
    }
    for s in suffixes {
        if !s.is_empty() && effective.ends_with(s.as_str()) {
            effective = &effective[..effective.len() - s.len()];
            break;
        }
    }
    effective.chars().count()
}

fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => !name.contains('_'),
        _ => false,
    }
}

fn is_pascal_case_no_abbrev(name: &str) -> bool {
    if !is_pascal_case(name) {
        return false;
    }
    let chars: Vec<char> = name.chars().collect();
    !chars
        .windows(2)
        .any(|w| w[0].is_uppercase() && w[1].is_uppercase())
}

fn is_snake_case(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut saw_letter = false;
    for c in name.chars() {
        if c == '_' {
            continue;
        }
        if c.is_ascii_lowercase() {
            saw_letter = true;
            continue;
        }
        if c.is_ascii_digit() {
            continue;
        }
        return false;
    }
    saw_letter
}

fn is_tuple_field_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_digit())
}

fn is_upper_case(name: &str) -> bool {
    let mut saw_letter = false;
    for c in name.chars() {
        if c == '_' {
            continue;
        }
        if !c.is_ascii_uppercase() && !c.is_ascii_digit() {
            return false;
        }
        if c.is_ascii_uppercase() {
            saw_letter = true;
        }
    }
    saw_letter
}

fn is_getter_name(name: &str) -> bool {
    name.len() >= 3
        && name.as_bytes()[0].eq_ignore_ascii_case(&b'g')
        && name.as_bytes()[1].eq_ignore_ascii_case(&b'e')
        && name.as_bytes()[2].eq_ignore_ascii_case(&b't')
}

fn compile_phpmd_regex(pat: &str) -> Option<Regex> {
    if pat.is_empty() {
        return None;
    }
    // PHPMD form: "(pattern)flags" e.g. "(^(set|get|is|has|with))i"
    let (body, flags) = if let Some(close) = pat.rfind(')') {
        if pat.starts_with('(') {
            let body = &pat[1..close];
            let flags = &pat[close + 1..];
            (body.to_string(), flags.replace('u', ""))
        } else {
            (pat.to_string(), String::new())
        }
    } else {
        (pat.to_string(), String::new())
    };
    let pattern = if flags.is_empty() {
        body
    } else {
        format!("(?{flags}){body}")
    };
    Regex::new(&pattern).ok()
}

fn ignored_name(re: &Option<Regex>, name: &str) -> bool {
    re.as_ref().is_some_and(|r| r.is_match(name))
}

fn is_public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn is_private(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Inherited)
}

fn is_rust_unused_name(name: &str) -> bool {
    name == "_" || name.starts_with('_')
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

fn bool_params(inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>) -> Vec<BoolParam> {
    let mut out = Vec::new();
    for arg in inputs {
        let FnArg::Typed(PatType { pat, ty, .. }) = arg else {
            continue;
        };
        if type_name_from_path(ty) != "bool" {
            continue;
        }
        collect_bool_param_names(pat, &mut out);
    }
    out
}

fn collect_bool_param_names(pat: &Pat, out: &mut Vec<BoolParam>) {
    match pat {
        Pat::Ident(id) => out.push(BoolParam {
            name: id.ident.to_string(),
            begin_line: id.ident.span().start().line,
        }),
        Pat::Tuple(t) => {
            for p in &t.elems {
                collect_bool_param_names(p, out);
            }
        }
        Pat::Paren(p) => collect_bool_param_names(&p.pat, out),
        Pat::Reference(r) => collect_bool_param_names(&r.pat, out),
        _ => {}
    }
}

fn terminal_else_lines(body: &syn::Block) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut visitor = ElseCollector { lines: &mut lines };
    visitor.visit_block(body);
    lines
}

struct ElseCollector<'a> {
    lines: &'a mut Vec<usize>,
}

impl<'ast> Visit<'ast> for ElseCollector<'_> {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        if let Some((_, else_branch)) = &node.else_branch {
            if !matches!(else_branch.as_ref(), syn::Expr::If(_)) {
                self.lines.push(else_branch.span().start().line);
            }
        }
        syn::visit::visit_expr_if(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}

fn assignment_in_condition_positions(body: &syn::Block) -> Vec<SourcePos> {
    let mut positions = Vec::new();
    let mut visitor = CondAssignCollector {
        positions: &mut positions,
    };
    visitor.visit_block(body);
    positions
}

struct CondAssignCollector<'a> {
    positions: &'a mut Vec<SourcePos>,
}

impl CondAssignCollector<'_> {
    fn scan_condition(&mut self, cond: &syn::Expr) {
        if matches!(cond, syn::Expr::Let(_)) {
            return;
        }
        let mut finder = AssignFinder {
            positions: self.positions,
        };
        finder.visit_expr(cond);
    }
}

impl<'ast> Visit<'ast> for CondAssignCollector<'_> {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.scan_condition(&node.cond);
        self.visit_block(&node.then_branch);
        if let Some((_, else_branch)) = &node.else_branch {
            self.visit_expr(else_branch);
        }
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.scan_condition(&node.cond);
        self.visit_block(&node.body);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}

struct AssignFinder<'a> {
    positions: &'a mut Vec<SourcePos>,
}

impl<'ast> Visit<'ast> for AssignFinder<'_> {
    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        let start = node.span().start();
        self.positions.push(SourcePos {
            line: start.line,
            column: start.column + 1,
        });
        syn::visit::visit_expr_assign(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}

fn static_accesses(
    body: &syn::Block,
    parent: Option<&str>,
    exceptions: &[String],
) -> Vec<StaticAccessHit> {
    let mut hits = Vec::new();
    let mut visitor = StaticAccessCollector {
        parent,
        exceptions,
        hits: &mut hits,
    };
    visitor.visit_block(body);
    hits
}

struct StaticAccessCollector<'a> {
    parent: Option<&'a str>,
    exceptions: &'a [String],
    hits: &'a mut Vec<StaticAccessHit>,
}

impl StaticAccessCollector<'_> {
    fn consider_path(&mut self, path: &syn::Path, line: usize) {
        let Some(type_name) = static_receiver_type(path) else {
            return;
        };
        if type_name == "Self" {
            return;
        }
        if self.parent == Some(type_name.as_str()) {
            return;
        }
        if self.exceptions.iter().any(|e| e == &type_name) {
            return;
        }
        self.hits.push(StaticAccessHit { type_name, line });
    }
}

impl<'ast> Visit<'ast> for StaticAccessCollector<'_> {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            self.consider_path(&p.path, p.span().start().line);
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_item_fn(&mut self, _node: &'ast ItemFn) {}
    fn visit_impl_item_fn(&mut self, _node: &'ast syn::ImplItemFn) {}
    fn visit_trait_item_fn(&mut self, _node: &'ast syn::TraitItemFn) {}
    fn visit_expr_closure(&mut self, _node: &'ast syn::ExprClosure) {}
}

fn static_receiver_type(path: &syn::Path) -> Option<String> {
    if path.segments.len() < 2 {
        return None;
    }
    // Prefer the rightmost PascalCase segment before the final call name.
    let mut segs: Vec<_> = path.segments.iter().collect();
    segs.pop()?; // method / associated fn name
    for seg in segs.into_iter().rev() {
        let name = seg.ident.to_string();
        if name == "Self" {
            return Some(name);
        }
        if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            return Some(name);
        }
    }
    None
}

fn exit_expression_line(body: &syn::Block) -> Option<usize> {
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

#[derive(Default)]
struct DuplicateKeyCollector {
    keys: Vec<DuplicateKey>,
}

impl<'ast> Visit<'ast> for DuplicateKeyCollector {
    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        let mut seen: HashMap<String, usize> = HashMap::new();
        for field in &node.fields {
            let Member::Named(ident) = &field.member else {
                continue;
            };
            let name = ident.to_string();
            let line = ident.span().start().line;
            if let Some(first) = seen.get(&name) {
                self.keys.push(DuplicateKey {
                    display: name,
                    line,
                    first_line: *first,
                });
            } else {
                seen.insert(name, line);
            }
        }
        syn::visit::visit_expr_struct(self, node);
    }
}

fn field_stats(fields: &Fields) -> (usize, usize) {
    match fields {
        Fields::Named(n) => {
            let total = n.named.len();
            let public = n.named.iter().filter(|f| is_public(&f.vis)).count();
            (total, public)
        }
        Fields::Unnamed(u) => {
            let total = u.unnamed.len();
            let public = u.unnamed.iter().filter(|f| is_public(&f.vis)).count();
            (total, public)
        }
        Fields::Unit => (0, 0),
    }
}

fn type_name_from_path(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn returns_bool(output: &ReturnType) -> bool {
    match output {
        ReturnType::Type(_, ty) => type_name_from_path(ty) == "bool",
        ReturnType::Default => false,
    }
}

struct NamedBinding {
    name: String,
    begin_line: usize,
    is_loop_binder: bool,
}

struct BoolParam {
    name: String,
    begin_line: usize,
}

struct SourcePos {
    line: usize,
    column: usize,
}

struct DuplicateKey {
    display: String,
    line: usize,
    first_line: usize,
}

struct StaticAccessHit {
    type_name: String,
    line: usize,
}

struct FnModel<'a> {
    name: String,
    parent: Option<String>,
    begin_line: usize,
    end_line: usize,
    param_count: usize,
    bool_params: Vec<BoolParam>,
    body: Option<&'a syn::Block>,
    returns_bool: bool,
    dep_types: Vec<String>,
    counts_for_type_metrics: bool,
}

impl FnModel<'_> {
    fn kind_label(&self) -> &'static str {
        if self.parent.is_some() {
            "method"
        } else {
            "function"
        }
    }
}

struct MethodRef<'a> {
    name: String,
    begin_line: usize,
    end_line: usize,
    is_public: bool,
    body: Option<&'a syn::Block>,
}

#[derive(Clone)]
struct FieldInfo {
    name: String,
    begin_line: usize,
    type_names: Vec<String>,
}

struct TypeModel<'a> {
    name: String,
    node_type: String,
    begin_line: usize,
    end_line: usize,
    field_count: usize,
    public_fields: usize,
    fields: Vec<FieldInfo>,
    methods: Vec<MethodRef<'a>>,
}

struct FileModel<'a> {
    src: &'a str,
    functions: Vec<FnModel<'a>>,
    types: Vec<TypeModel<'a>>,
    variables: Vec<NamedBinding>,
    constants: Vec<NamedBinding>,
    usage: UseDefModel,
    duplicate_struct_keys: Vec<DuplicateKey>,
    static_muts: Vec<NamedSite>,
    mutated_statics: HashSet<String>,
}

#[derive(Default)]
struct UseDefModel {
    locals: Vec<NamedSite>,
    params: Vec<NamedSite>,
    private_fields: Vec<NamedSite>,
    private_methods: Vec<NamedSite>,
    ident_reads: HashSet<String>,
    field_reads: HashSet<String>,
    method_calls: HashSet<String>,
}

#[derive(Clone, Debug)]
struct NamedSite {
    name: String,
    begin_line: usize,
}

impl<'a> FileModel<'a> {
    fn from_file(file: &'a syn::File, src: &'a str) -> Self {
        let mut types: HashMap<String, TypeModel<'a>> = HashMap::new();
        let mut functions = Vec::new();
        collect_items(&file.items, &mut types, &mut functions);

        let mut binder = BindingCollector {
            variables: Vec::new(),
            constants: Vec::new(),
            loop_pat_depth: 0,
        };
        binder.visit_file(file);

        let mut usage = UseDefCollector::new();
        usage.visit_file(file);

        let mut dup = DuplicateKeyCollector::default();
        dup.visit_file(file);

        let mut statics = StaticMutCollector::default();
        statics.visit_file(file);

        let mut types: Vec<_> = types.into_values().collect();
        types.sort_by(|a, b| a.begin_line.cmp(&b.begin_line).then(a.name.cmp(&b.name)));
        functions.sort_by(|a, b| a.begin_line.cmp(&b.begin_line).then(a.name.cmp(&b.name)));
        Self {
            src,
            functions,
            types,
            variables: binder.variables,
            constants: binder.constants,
            usage: usage.into_model(),
            duplicate_struct_keys: dup.keys,
            static_muts: statics.static_muts,
            mutated_statics: statics.mutated,
        }
    }
}

struct UseDefCollector {
    model: UseDefModel,
    binding_mode: BindingMode,
    in_trait_impl: bool,
    derived_fields_are_used: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BindingMode {
    None,
    Local,
    Param,
}

impl UseDefCollector {
    fn new() -> Self {
        Self {
            model: UseDefModel::default(),
            binding_mode: BindingMode::None,
            in_trait_impl: false,
            derived_fields_are_used: false,
        }
    }

    fn into_model(self) -> UseDefModel {
        self.model
    }
}

fn with_binding_mode<F>(collector: &mut UseDefCollector, mode: BindingMode, visit: F)
where
    F: FnOnce(&mut UseDefCollector),
{
    let previous = collector.binding_mode;
    collector.binding_mode = mode;
    visit(collector);
    collector.binding_mode = previous;
}

fn record_params_from_sig(collector: &mut UseDefCollector, signature: &syn::Signature) {
    for input in &signature.inputs {
        if let FnArg::Typed(parameter) = input {
            with_binding_mode(collector, BindingMode::Param, |visitor| {
                visitor.visit_pat(&parameter.pat)
            });
        }
    }
}

fn visit_assignment_target(collector: &mut UseDefCollector, target: &syn::Expr) {
    match target {
        syn::Expr::Path(_) | syn::Expr::Infer(_) => {}
        syn::Expr::Tuple(tuple) => {
            for element in &tuple.elems {
                visit_assignment_target(collector, element);
            }
        }
        syn::Expr::Array(array) => {
            for element in &array.elems {
                visit_assignment_target(collector, element);
            }
        }
        syn::Expr::Struct(structure) => {
            for field in &structure.fields {
                visit_assignment_target(collector, &field.expr);
            }
        }
        _ => visit_assignment_place(collector, target),
    }
}

fn visit_assignment_place(collector: &mut UseDefCollector, target: &syn::Expr) {
    match target {
        syn::Expr::Field(field) => collector.visit_expr(&field.base),
        syn::Expr::Index(index) => {
            collector.visit_expr(&index.expr);
            collector.visit_expr(&index.index);
        }
        syn::Expr::Paren(paren) => visit_assignment_target(collector, &paren.expr),
        syn::Expr::Group(group) => visit_assignment_target(collector, &group.expr),
        _ => collector.visit_expr(target),
    }
}

impl<'ast> Visit<'ast> for UseDefCollector {
    fn visit_local(&mut self, node: &'ast syn::Local) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        with_binding_mode(self, BindingMode::Local, |visitor| {
            visitor.visit_pat(&node.pat)
        });
        if let Some(init) = &node.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        record_params_from_sig(self, &node.sig);
        self.visit_block(&node.block);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev = self.in_trait_impl;
        self.in_trait_impl = node.trait_.is_some();
        syn::visit::visit_item_impl(self, node);
        self.in_trait_impl = prev;
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        let previous = self.derived_fields_are_used;
        self.derived_fields_are_used = derive_uses_fields(&node.attrs);
        syn::visit::visit_item_struct(self, node);
        self.derived_fields_are_used = previous;
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        let previous = self.derived_fields_are_used;
        self.derived_fields_are_used = derive_uses_fields(&node.attrs);
        syn::visit::visit_item_enum(self, node);
        self.derived_fields_are_used = previous;
    }

    fn visit_item_union(&mut self, node: &'ast ItemUnion) {
        let previous = self.derived_fields_are_used;
        self.derived_fields_are_used = derive_uses_fields(&node.attrs);
        syn::visit::visit_item_union(self, node);
        self.derived_fields_are_used = previous;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if !self.in_trait_impl && is_private(&node.vis) {
            self.model.private_methods.push(NamedSite {
                name: node.sig.ident.to_string(),
                begin_line: node.sig.fn_token.span().start().line,
            });
        }
        record_params_from_sig(self, &node.sig);
        self.visit_block(&node.block);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        record_params_from_sig(self, &node.sig);
        if let Some(body) = &node.default {
            self.visit_block(body);
        }
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        if let Some(ident) = &node.ident {
            if is_private(&node.vis) && !self.derived_fields_are_used {
                self.model.private_fields.push(NamedSite {
                    name: ident.to_string(),
                    begin_line: ident.span().start().line,
                });
            }
        }
        syn::visit::visit_field(self, node);
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        with_binding_mode(self, BindingMode::Local, |visitor| {
            visitor.visit_pat(&node.pat)
        });
        if let Some((_, guard)) = &node.guard {
            self.visit_expr(guard);
        }
        self.visit_expr(&node.body);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        with_binding_mode(self, BindingMode::Local, |visitor| {
            visitor.visit_pat(&node.pat)
        });
        self.visit_expr(&node.expr);
        self.visit_block(&node.body);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        if let syn::Expr::Let(l) = &*node.cond {
            with_binding_mode(self, BindingMode::Local, |visitor| {
                visitor.visit_pat(&l.pat)
            });
            self.visit_expr(&l.expr);
        } else {
            self.visit_expr(&node.cond);
        }
        self.visit_block(&node.body);
    }

    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        if let syn::Expr::Let(l) = &*node.cond {
            with_binding_mode(self, BindingMode::Local, |visitor| {
                visitor.visit_pat(&l.pat)
            });
            self.visit_expr(&l.expr);
        } else {
            self.visit_expr(&node.cond);
        }
        self.visit_block(&node.then_branch);
        if let Some((_, else_branch)) = &node.else_branch {
            self.visit_expr(else_branch);
        }
    }

    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        let name = node.ident.to_string();
        if is_binding_name(&name) {
            let site = NamedSite {
                name,
                begin_line: node.ident.span().start().line,
            };
            match self.binding_mode {
                BindingMode::Local => self.model.locals.push(site),
                BindingMode::Param => self.model.params.push(site),
                BindingMode::None => {}
            }
        }
        if let Some((_, sub)) = &node.subpat {
            self.visit_pat(sub);
        }
    }

    fn visit_field_pat(&mut self, node: &'ast syn::FieldPat) {
        if let Member::Named(ident) = &node.member {
            self.model.field_reads.insert(ident.to_string());
        }
        self.visit_pat(&node.pat);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if let Some(ident) = path_single_ident(node) {
            if ident != "self" && ident != "Self" {
                self.model.ident_reads.insert(ident);
            }
        } else if let Some(ident) = path_last_ident(node) {
            self.model.method_calls.insert(ident);
        }
        syn::visit::visit_expr_path(self, node);
    }

    fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
        self.visit_expr(&node.base);
        if let Member::Named(ident) = &node.member {
            self.model.field_reads.insert(ident.to_string());
        }
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.model.method_calls.insert(node.method.to_string());
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        visit_assignment_target(self, &node.left);
        self.visit_expr(&node.right);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        collect_macro_reads(
            node.tokens.clone(),
            &mut self.model.ident_reads,
            &mut self.model.field_reads,
        );
        if is_format_macro(node) {
            collect_format_captures(node.tokens.clone(), &mut self.model.ident_reads);
        }
        syn::visit::visit_macro(self, node);
    }
}

fn collect_macro_reads(
    tokens: TokenStream,
    ident_reads: &mut HashSet<String>,
    field_reads: &mut HashSet<String>,
) {
    let mut after_dot = false;
    for token in tokens {
        match token {
            TokenTree::Group(group) => {
                collect_macro_reads(group.stream(), ident_reads, field_reads);
                after_dot = false;
            }
            TokenTree::Ident(ident) => {
                let name = ident.to_string();
                ident_reads.insert(name.clone());
                if after_dot {
                    field_reads.insert(name);
                }
                after_dot = false;
            }
            TokenTree::Punct(punctuation) => after_dot = punctuation.as_char() == '.',
            _ => after_dot = false,
        }
    }
}

fn derive_uses_fields(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        if !attribute.path().is_ident("derive") {
            return false;
        }
        let mut uses_fields = false;
        let _ = attribute.parse_nested_meta(|meta| {
            let name = meta.path.segments.last().map(|segment| &segment.ident);
            if name.is_some_and(|name| name == "Serialize" || name == "Deserialize") {
                uses_fields = true;
            }
            Ok(())
        });
        uses_fields
    })
}

fn is_format_macro(node: &syn::Macro) -> bool {
    let Some(name) = node
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
    else {
        return false;
    };
    matches!(
        name.as_str(),
        "format"
            | "format_args"
            | "print"
            | "println"
            | "eprint"
            | "eprintln"
            | "write"
            | "writeln"
            | "panic"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
    )
}

fn collect_format_captures(tokens: TokenStream, reads: &mut HashSet<String>) {
    for token in tokens {
        match token {
            TokenTree::Group(group) => collect_format_captures(group.stream(), reads),
            TokenTree::Literal(literal) => {
                let Ok(syn::Lit::Str(value)) = syn::parse_str::<syn::Lit>(&literal.to_string())
                else {
                    continue;
                };
                reads.extend(format_capture_names(&value.value()));
            }
            _ => {}
        }
    }
}

fn format_capture_names(format: &str) -> Vec<String> {
    static CAPTURE: OnceLock<Regex> = OnceLock::new();
    let capture = CAPTURE.get_or_init(|| {
        Regex::new(r"\{([A-Za-z_][A-Za-z0-9_]*)(?:[}:])").expect("valid format capture regex")
    });
    let unescaped = format.replace("{{", "");
    capture
        .captures_iter(&unescaped)
        .filter_map(|captures| captures.get(1).map(|name| name.as_str().to_string()))
        .collect()
}

fn path_single_ident(path: &syn::ExprPath) -> Option<String> {
    if path.qself.is_some() {
        return None;
    }
    let mut segs = path.path.segments.iter();
    let first = segs.next()?;
    if segs.next().is_some() {
        return None;
    }
    Some(first.ident.to_string())
}

fn path_last_ident(path: &syn::ExprPath) -> Option<String> {
    path.path.segments.last().map(|s| s.ident.to_string())
}

fn is_binding_name(name: &str) -> bool {
    name != "self" && name.starts_with(|ch: char| ch.is_lowercase() || ch == '_')
}

struct BindingCollector {
    variables: Vec<NamedBinding>,
    constants: Vec<NamedBinding>,
    loop_pat_depth: usize,
}

impl<'ast> Visit<'ast> for BindingCollector {
    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        self.loop_pat_depth += 1;
        self.visit_pat(&node.pat);
        self.loop_pat_depth -= 1;
        self.visit_expr(&node.expr);
        self.visit_block(&node.body);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        if let syn::Expr::Let(l) = &*node.cond {
            self.loop_pat_depth += 1;
            self.visit_pat(&l.pat);
            self.loop_pat_depth -= 1;
            self.visit_expr(&l.expr);
        } else {
            self.visit_expr(&node.cond);
        }
        self.visit_block(&node.body);
    }

    fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
        if is_binding_name(&node.ident.to_string()) {
            self.variables.push(NamedBinding {
                name: node.ident.to_string(),
                begin_line: node.ident.span().start().line,
                is_loop_binder: self.loop_pat_depth > 0,
            });
        }
        syn::visit::visit_pat_ident(self, node);
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        if let Some(ident) = &node.ident {
            self.variables.push(NamedBinding {
                name: ident.to_string(),
                begin_line: ident.span().start().line,
                is_loop_binder: false,
            });
        }
        syn::visit::visit_field(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.constants.push(NamedBinding {
            name: node.ident.to_string(),
            begin_line: node.ident.span().start().line,
            is_loop_binder: false,
        });
        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.constants.push(NamedBinding {
            name: node.ident.to_string(),
            begin_line: node.ident.span().start().line,
            is_loop_binder: false,
        });
        syn::visit::visit_item_static(self, node);
    }

    fn visit_impl_item_const(&mut self, node: &'ast syn::ImplItemConst) {
        self.constants.push(NamedBinding {
            name: node.ident.to_string(),
            begin_line: node.ident.span().start().line,
            is_loop_binder: false,
        });
        syn::visit::visit_impl_item_const(self, node);
    }

    fn visit_trait_item_const(&mut self, node: &'ast syn::TraitItemConst) {
        self.constants.push(NamedBinding {
            name: node.ident.to_string(),
            begin_line: node.ident.span().start().line,
            is_loop_binder: false,
        });
        syn::visit::visit_trait_item_const(self, node);
    }
}

fn collect_items<'a>(
    items: &'a [Item],
    types: &mut HashMap<String, TypeModel<'a>>,
    functions: &mut Vec<FnModel<'a>>,
) {
    for item in items {
        match item {
            Item::Struct(s) => insert_struct(types, s),
            Item::Enum(e) => insert_enum(types, e),
            Item::Union(u) => insert_union(types, u),
            Item::Trait(t) => insert_trait(types, t, functions),
            Item::Fn(f) => functions.push(fn_from_item(f)),
            Item::Impl(im) => attach_impl(types, functions, im),
            Item::Mod(module) => collect_module_items(module, types, functions),
            _ => {}
        }
    }
}

fn collect_module_items<'a>(
    module: &'a syn::ItemMod,
    types: &mut HashMap<String, TypeModel<'a>>,
    functions: &mut Vec<FnModel<'a>>,
) {
    if let Some((_, nested)) = &module.content {
        collect_items(nested, types, functions);
    }
}

struct TypeDefinition {
    name: String,
    node_type: &'static str,
    begin_line: usize,
    end_line: usize,
    field_count: usize,
    public_fields: usize,
    fields: Vec<FieldInfo>,
}

fn upsert_type<'a>(types: &mut HashMap<String, TypeModel<'a>>, definition: TypeDefinition) {
    let TypeDefinition {
        name,
        node_type,
        begin_line,
        end_line,
        field_count,
        public_fields,
        fields,
    } = definition;
    match types.entry(name.clone()) {
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            let existing = entry.get_mut();
            existing.node_type = node_type.to_string();
            existing.begin_line = begin_line;
            existing.end_line = end_line;
            existing.field_count = field_count;
            existing.public_fields = public_fields;
            if !fields.is_empty() {
                existing.fields = fields;
            }
        }
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(TypeModel {
                name,
                node_type: node_type.to_string(),
                begin_line,
                end_line,
                field_count,
                public_fields,
                fields,
                methods: Vec::new(),
            });
        }
    }
}

fn field_infos(fields: &Fields) -> Vec<FieldInfo> {
    match fields {
        Fields::Named(n) => n
            .named
            .iter()
            .filter_map(|f| {
                let ident = f.ident.as_ref()?;
                Some(FieldInfo {
                    name: ident.to_string(),
                    begin_line: ident.span().start().line,
                    type_names: type_names_in(&f.ty),
                })
            })
            .collect(),
        Fields::Unnamed(u) => u
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| FieldInfo {
                name: i.to_string(),
                begin_line: f.ty.span().start().line,
                type_names: type_names_in(&f.ty),
            })
            .collect(),
        Fields::Unit => Vec::new(),
    }
}

fn insert_struct<'a>(types: &mut HashMap<String, TypeModel<'a>>, s: &'a ItemStruct) {
    let (field_count, public_fields) = field_stats(&s.fields);
    upsert_type(
        types,
        TypeDefinition {
            name: s.ident.to_string(),
            node_type: "struct",
            begin_line: s.struct_token.span().start().line,
            end_line: s.span().end().line,
            field_count,
            public_fields,
            fields: field_infos(&s.fields),
        },
    );
}

fn insert_enum<'a>(types: &mut HashMap<String, TypeModel<'a>>, e: &'a ItemEnum) {
    let mut fields = Vec::new();
    for v in &e.variants {
        for f in field_infos(&v.fields) {
            fields.push(f);
        }
    }
    upsert_type(
        types,
        TypeDefinition {
            name: e.ident.to_string(),
            node_type: "enum",
            begin_line: e.enum_token.span().start().line,
            end_line: e.span().end().line,
            field_count: e.variants.len(),
            public_fields: 0,
            fields,
        },
    );
}

fn insert_union<'a>(types: &mut HashMap<String, TypeModel<'a>>, u: &'a ItemUnion) {
    let field_count = u.fields.named.len();
    let public_fields = u.fields.named.iter().filter(|f| is_public(&f.vis)).count();
    let fields = u
        .fields
        .named
        .iter()
        .filter_map(|f| {
            let ident = f.ident.as_ref()?;
            Some(FieldInfo {
                name: ident.to_string(),
                begin_line: ident.span().start().line,
                type_names: type_names_in(&f.ty),
            })
        })
        .collect();
    upsert_type(
        types,
        TypeDefinition {
            name: u.ident.to_string(),
            node_type: "union",
            begin_line: u.union_token.span().start().line,
            end_line: u.span().end().line,
            field_count,
            public_fields,
            fields,
        },
    );
}

fn insert_trait<'a>(
    types: &mut HashMap<String, TypeModel<'a>>,
    t: &'a ItemTrait,
    functions: &mut Vec<FnModel<'a>>,
) {
    let trait_public = is_public(&t.vis);
    let mut methods = Vec::new();
    for item in &t.items {
        if let syn::TraitItem::Fn(m) = item {
            let begin = m.sig.fn_token.span().start().line;
            let end = m.span().end().line;
            let body = m.default.as_ref();
            let name = m.sig.ident.to_string();
            methods.push(MethodRef {
                name: name.clone(),
                begin_line: begin,
                end_line: end,
                is_public: trait_public,
                body,
            });
            functions.push(FnModel {
                name,
                parent: Some(t.ident.to_string()),
                begin_line: begin,
                end_line: end,
                param_count: count_params(&m.sig.inputs),
                bool_params: bool_params(&m.sig.inputs),
                body,
                returns_bool: returns_bool(&m.sig.output),
                dep_types: sig_dep_types(&m.sig),
                counts_for_type_metrics: true,
            });
        }
    }
    let name = t.ident.to_string();
    types
        .entry(name.clone())
        .and_modify(|existing| {
            existing.node_type = "trait".to_string();
            existing.begin_line = t.trait_token.span().start().line;
            existing.end_line = t.span().end().line;
            existing.field_count = 0;
            existing.public_fields = 0;
            existing.methods.append(&mut methods);
        })
        .or_insert_with(|| TypeModel {
            name,
            node_type: "trait".to_string(),
            begin_line: t.trait_token.span().start().line,
            end_line: t.span().end().line,
            field_count: 0,
            public_fields: 0,
            fields: Vec::new(),
            methods,
        });
}

fn fn_from_item(f: &ItemFn) -> FnModel<'_> {
    FnModel {
        name: f.sig.ident.to_string(),
        parent: None,
        begin_line: f.sig.fn_token.span().start().line,
        end_line: f.span().end().line,
        param_count: count_params(&f.sig.inputs),
        bool_params: bool_params(&f.sig.inputs),
        body: Some(&f.block),
        returns_bool: returns_bool(&f.sig.output),
        dep_types: sig_dep_types(&f.sig),
        counts_for_type_metrics: false,
    }
}

fn attach_impl<'a>(
    types: &mut HashMap<String, TypeModel<'a>>,
    functions: &mut Vec<FnModel<'a>>,
    im: &'a ItemImpl,
) {
    let ty_name = type_name_from_path(&im.self_ty);
    if ty_name.is_empty() {
        return;
    }
    types.entry(ty_name.clone()).or_insert_with(|| TypeModel {
        name: ty_name.clone(),
        node_type: "struct".to_string(),
        begin_line: im.impl_token.span().start().line,
        end_line: im.span().end().line,
        field_count: 0,
        public_fields: 0,
        fields: Vec::new(),
        methods: Vec::new(),
    });
    for item in &im.items {
        if let syn::ImplItem::Fn(m) = item {
            let begin = m.sig.fn_token.span().start().line;
            let end = m.span().end().line;
            let name = m.sig.ident.to_string();
            let is_pub = is_public(&m.vis);
            if im.trait_.is_none() {
                types.get_mut(&ty_name).unwrap().methods.push(MethodRef {
                    name: name.clone(),
                    begin_line: begin,
                    end_line: end,
                    is_public: is_pub,
                    body: Some(&m.block),
                });
            }
            functions.push(FnModel {
                name,
                parent: Some(ty_name.clone()),
                begin_line: begin,
                end_line: end,
                param_count: count_params(&m.sig.inputs),
                bool_params: bool_params(&m.sig.inputs),
                body: Some(&m.block),
                returns_bool: returns_bool(&m.sig.output),
                dep_types: sig_dep_types(&m.sig),
                counts_for_type_metrics: im.trait_.is_none(),
            });
        }
    }
}

fn sig_dep_types(sig: &syn::Signature) -> Vec<String> {
    let mut names = Vec::new();
    for input in &sig.inputs {
        if let FnArg::Typed(pt) = input {
            names.extend(type_names_in(&pt.ty));
        }
    }
    if let ReturnType::Type(_, ty) = &sig.output {
        names.extend(type_names_in(ty));
    }
    names
}

fn type_names_in(ty: &syn::Type) -> Vec<String> {
    let mut out = Vec::new();
    collect_type_names(ty, &mut out);
    out
}

fn collect_type_names(ty: &syn::Type, out: &mut Vec<String>) {
    let mut collector = TypeNameCollector { names: out };
    collector.visit_type(ty);
}

struct TypeNameCollector<'a> {
    names: &'a mut Vec<String>,
}

impl<'ast> Visit<'ast> for TypeNameCollector<'_> {
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if let Some(segment) = node.path.segments.last() {
            self.names.push(segment.ident.to_string());
        }
        syn::visit::visit_type_path(self, node);
    }
}

fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "char"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
            | "Self"
            | "self"
    )
}

fn unwanted_function_set(extra: &str) -> HashSet<String> {
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

struct CountLoopHit {
    line: usize,
    func_name: String,
    loop_kind: String,
}

fn count_in_loop_hits(body: &syn::Block) -> Vec<CountLoopHit> {
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

struct DevHit {
    line: usize,
    func_name: String,
}

fn development_fragment_hits(body: &syn::Block, unwanted: &HashSet<String>) -> Vec<DevHit> {
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

fn empty_catch_lines(body: &syn::Block) -> Vec<usize> {
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
        Pat::Ident(id) => id.ident == "Err",
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

fn coupling_between_objects(t: &TypeModel<'_>, model: &FileModel<'_>) -> usize {
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

#[derive(Default)]
struct StaticMutCollector {
    static_muts: Vec<NamedSite>,
    mutated: HashSet<String>,
    assign_lhs: bool,
}

impl<'ast> Visit<'ast> for StaticMutCollector {
    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        if !matches!(node.mutability, syn::StaticMutability::None) {
            self.static_muts.push(NamedSite {
                name: node.ident.to_string(),
                begin_line: node.ident.span().start().line,
            });
        }
        syn::visit::visit_item_static(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if self.assign_lhs {
            if let Some(ident) = path_single_ident(node) {
                self.mutated.insert(ident);
            }
        }
        syn::visit::visit_expr_path(self, node);
    }

    fn visit_expr_assign(&mut self, node: &'ast syn::ExprAssign) {
        self.assign_lhs = true;
        self.visit_expr(&node.left);
        self.assign_lhs = false;
        self.visit_expr(&node.right);
    }

    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        if matches!(
            node.op,
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
        ) {
            self.assign_lhs = true;
            self.visit_expr(&node.left);
            self.assign_lhs = false;
            self.visit_expr(&node.right);
        } else {
            syn::visit::visit_expr_binary(self, node);
        }
    }
}

fn lcom4(t: &TypeModel<'_>) -> usize {
    let field_names: HashSet<String> = t.fields.iter().map(|f| f.name.clone()).collect();
    let method_idx: HashMap<String, usize> = t
        .methods
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name.clone(), i))
        .collect();
    let accessor_of = accessor_fields(t, &field_names);
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

fn accessor_fields(
    model: &TypeModel<'_>,
    field_names: &HashSet<String>,
) -> HashMap<String, String> {
    let mut accessors = HashMap::new();
    for method in &model.methods {
        if let Some(field) = accessor_field(method, field_names) {
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

fn accessor_field(m: &MethodRef<'_>, fields: &HashSet<String>) -> Option<String> {
    let body = m.body?;
    if body.stmts.len() != 1 {
        return None;
    }
    match &body.stmts[0] {
        syn::Stmt::Expr(syn::Expr::Field(field), _) => receiver_field(field, fields),
        syn::Stmt::Expr(syn::Expr::Assign(assign), _) => assigned_receiver_field(assign, fields),
        _ => None,
    }
}

fn assigned_receiver_field(
    assignment: &syn::ExprAssign,
    fields: &HashSet<String>,
) -> Option<String> {
    let syn::Expr::Field(field) = &*assignment.left else {
        return None;
    };
    receiver_field(field, fields)
}

fn receiver_field(field: &syn::ExprField, fields: &HashSet<String>) -> Option<String> {
    let (syn::Expr::Path(base), Member::Named(identifier)) = (&*field.base, &field.member) else {
        return None;
    };
    let name = identifier.to_string();
    (path_is_self(base) && fields.contains(&name)).then_some(name)
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
    used_fields.sort();
    used_fields.dedup();
    called.sort();
    called.dedup();
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
