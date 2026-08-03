//! Unused-code rule handlers.

use crate::report::Violation;
use crate::ruleset::LoadedRule;

use crate::analyze::helpers::{
    format_message, is_rust_unused_name, name_violation, property_list,
};
use crate::analyze::model::FileModel;


pub(crate) fn apply_unused_local_variable(
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


pub(crate) fn apply_unused_formal_parameter(
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


pub(crate) fn apply_unused_private_field(
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


pub(crate) fn apply_unused_private_method(
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

