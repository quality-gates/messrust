//! Design rule handlers.

use crate::report::Violation;
use crate::ruleset::LoadedRule;

use crate::analyze::helpers::{
    format_message, name_violation, property_bool, property_usize, type_violation,
};
use crate::analyze::model::FileModel;

use super::design_support::{
    count_in_loop_hits, coupling_between_objects, development_fragment_hits, empty_catch_lines,
    exit_expression_line, lcom4, unwanted_function_set,
};

pub(crate) const DEFAULT_CBO: usize = 13;

pub(crate) const DEFAULT_LCOM4: usize = 1;


pub(crate) fn apply_exit_expression(
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


// Rust has no goto; keep the rule loadable and quiet.
pub(crate) fn apply_goto_statement(
    _rule: &LoadedRule,
    _file: &str,
    _model: &FileModel<'_>,
    _out: &mut Vec<Violation>,
) {
}


pub(crate) fn apply_count_in_loop_expression(
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


pub(crate) fn apply_development_code_fragment(
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


pub(crate) fn apply_empty_catch_block(
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


pub(crate) fn apply_coupling_between_objects(
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


pub(crate) fn apply_global_variable(
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


pub(crate) fn apply_lack_of_cohesion(
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


