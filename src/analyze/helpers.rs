//! Shared helpers for rule handlers (properties, messages, naming checks).

use regex::Regex;
use syn::Visibility;

use crate::report::Violation;
use crate::ruleset::LoadedRule;

use super::model::{FileModel, FnModel, TypeModel};


pub(crate) fn fn_loc(f: &FnModel<'_>, model: &FileModel<'_>, ignore_ws: bool) -> usize {
    if ignore_ws {
        model.effective_lines.count(f.begin_line, f.end_line)
    } else {
        f.end_line.saturating_sub(f.begin_line).saturating_add(1)
    }
}


pub(crate) fn type_loc(t: &TypeModel<'_>, model: &FileModel<'_>, ignore_ws: bool) -> usize {
    let mut loc = if ignore_ws {
        model.effective_lines.count(t.begin_line, t.end_line)
    } else {
        t.end_line.saturating_sub(t.begin_line).saturating_add(1)
    };
    for m in &t.methods {
        loc += if ignore_ws {
            model.effective_lines.count(m.begin_line, m.end_line)
        } else {
            m.end_line.saturating_sub(m.begin_line).saturating_add(1)
        };
    }
    loc
}


pub(crate) fn func_violation(
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


pub(crate) fn type_violation(
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


pub(crate) fn name_violation(
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


pub(crate) fn format_message(template: &str, args: &[&str]) -> String {
    let mut out = template.to_string();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), arg);
    }
    out
}


pub(crate) fn property_usize(rule: &LoadedRule, key: &str, default: usize) -> usize {
    rule.properties
        .get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}


pub(crate) fn property_bool(rule: &LoadedRule, key: &str, default: bool) -> bool {
    rule.properties
        .get(key)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(default)
}


pub(crate) fn property_list(rule: &LoadedRule, key: &str) -> Vec<String> {
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


pub(crate) fn length_without(name: &str, prefixes: &[String], suffixes: &[String]) -> usize {
    // Callers pass lists from property_list, which already drops empty entries.
    let mut effective = name;
    for p in prefixes {
        if effective.starts_with(p.as_str()) {
            effective = &effective[p.len()..];
            break;
        }
    }
    for s in suffixes {
        if effective.ends_with(s.as_str()) {
            effective = &effective[..effective.len() - s.len()];
            break;
        }
    }
    effective.chars().count()
}


pub(crate) fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => !name.contains('_'),
        _ => false,
    }
}


pub(crate) fn is_pascal_case_no_abbrev(name: &str) -> bool {
    if !is_pascal_case(name) {
        return false;
    }
    let chars: Vec<char> = name.chars().collect();
    !chars
        .windows(2)
        .any(|w| w[0].is_uppercase() && w[1].is_uppercase())
}


pub(crate) fn is_snake_case(name: &str) -> bool {
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


pub(crate) fn is_tuple_field_name(name: &str) -> bool {
    // Model tuple fields use decimal index strings ("0", "1", …); empty never appears.
    name.chars().all(|c| c.is_ascii_digit())
}


pub(crate) fn is_upper_case(name: &str) -> bool {
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


pub(crate) fn is_getter_name(name: &str) -> bool {
    name.len() >= 3
        && name.as_bytes()[0].eq_ignore_ascii_case(&b'g')
        && name.as_bytes()[1].eq_ignore_ascii_case(&b'e')
        && name.as_bytes()[2].eq_ignore_ascii_case(&b't')
}


pub(crate) fn compile_phpmd_regex(pat: &str) -> Option<Regex> {
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


pub(crate) fn ignored_name(re: &Option<Regex>, name: &str) -> bool {
    re.as_ref().is_some_and(|r| r.is_match(name))
}


pub(crate) fn is_public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}


pub(crate) fn is_private(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Inherited)
}


pub(crate) fn is_rust_unused_name(name: &str) -> bool {
    name.starts_with('_')
}
