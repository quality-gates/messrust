//! Codesize rule handlers.

use crate::metrics::{cyclomatic_complexity, npath_complexity};
use crate::report::Violation;
use crate::ruleset::LoadedRule;

use crate::analyze::helpers::{
    compile_phpmd_regex, format_message, func_violation, ignored_name, property_bool,
    property_usize, type_violation, fn_loc, type_loc,
};
use crate::analyze::model::FileModel;


pub(crate) const DEFAULT_CCN: usize = 10;

pub(crate) const DEFAULT_NPATH: usize = 200;

pub(crate) const DEFAULT_METHOD_LOC: usize = 100;

pub(crate) const DEFAULT_CLASS_LOC: usize = 1000;

pub(crate) const DEFAULT_PARAMS: usize = 10;

pub(crate) const DEFAULT_PUBLIC: usize = 45;

pub(crate) const DEFAULT_FIELDS: usize = 15;

pub(crate) const DEFAULT_METHODS: usize = 25;

pub(crate) const DEFAULT_PUBLIC_METHODS: usize = 10;

pub(crate) const DEFAULT_WMC: usize = 50;

pub(crate) const DEFAULT_IGNORE_PATTERN: &str = "(^(set|get|is|has|with))i";


pub(crate) fn apply_cyclomatic_complexity(
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


pub(crate) fn apply_npath_complexity(
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


pub(crate) fn apply_excessive_method_length(
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


pub(crate) fn apply_excessive_class_length(
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


pub(crate) fn apply_excessive_parameter_list(
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


pub(crate) fn apply_excessive_public_count(
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


pub(crate) fn apply_too_many_fields(
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


pub(crate) fn apply_too_many_methods(
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


pub(crate) fn apply_too_many_public_methods(
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


pub(crate) fn apply_excessive_class_complexity(
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

