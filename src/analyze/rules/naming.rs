//! Naming rule handlers.

use crate::report::Violation;
use crate::ruleset::LoadedRule;

use crate::analyze::helpers::{
    format_message, func_violation, is_getter_name, is_pascal_case, is_upper_case, length_without,
    name_violation, property_bool, property_list, property_usize, type_violation,
};
use crate::analyze::model::FileModel;

pub(crate) const DEFAULT_SHORT_NAME: usize = 3;

pub(crate) const DEFAULT_LONG_CLASS: usize = 40;

pub(crate) const DEFAULT_LONG_VAR: usize = 20;


pub(crate) fn apply_short_class_name(
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


pub(crate) fn apply_long_class_name(
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


pub(crate) fn apply_short_variable(
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


pub(crate) fn apply_long_variable(
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


pub(crate) fn apply_short_method_name(
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


pub(crate) fn apply_constant_naming(
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


pub(crate) fn apply_boolean_get_method_name(
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

