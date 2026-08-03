//! Controversial (camel-case) rule handlers.

use crate::report::Violation;
use crate::ruleset::LoadedRule;

use crate::analyze::helpers::{
    format_message, func_violation, is_pascal_case, is_pascal_case_no_abbrev, is_snake_case,
    is_tuple_field_name, name_violation, property_bool, type_violation,
};
use crate::analyze::model::FileModel;


pub(crate) fn apply_camel_case_class_name(
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


pub(crate) fn apply_camel_case_method_name(
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


pub(crate) fn apply_camel_case_property_name(
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


pub(crate) fn apply_camel_case_parameter_name(
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


pub(crate) fn apply_camel_case_variable_name(
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

