//! Syntax-only analysis for codesize, naming, and unusedcode rules.

use std::collections::{HashMap, HashSet};

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

pub fn analyze_files(files: &[std::path::PathBuf], rules: &[LoadedRule]) -> Report {
    let mut report = Report::default();
    for path in files {
        match analyze_one(path, rules) {
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

fn analyze_one(path: &std::path::Path, rules: &[LoadedRule]) -> Result<Vec<Violation>, String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file = syn::parse_file(&src).map_err(|e| e.to_string())?;
    let file_name = path.display().to_string();
    let model = FileModel::from_file(&file, &src);
    let mut violations = Vec::new();
    for rule in rules {
        apply_rule(rule, &file_name, &model, &mut violations);
    }
    Ok(violations)
}

fn apply_rule(rule: &LoadedRule, file: &str, model: &FileModel<'_>, out: &mut Vec<Violation>) {
    match rule.kind {
        RuleKind::CyclomaticComplexity => {
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
                                &f.kind_label(),
                                &f.name,
                                &value.to_string(),
                                &threshold.to_string(),
                            ],
                        ),
                    ));
                }
            }
        }
        RuleKind::NPathComplexity => {
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
                                &f.kind_label(),
                                &f.name,
                                &value.to_string(),
                                &threshold.to_string(),
                            ],
                        ),
                    ));
                }
            }
        }
        RuleKind::ExcessiveMethodLength => {
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
                                &f.kind_label(),
                                &f.name,
                                &value.to_string(),
                                &threshold.to_string(),
                            ],
                        ),
                    ));
                }
            }
        }
        RuleKind::ExcessiveClassLength => {
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
        RuleKind::ExcessiveParameterList => {
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
                                &f.kind_label(),
                                &f.name,
                                &value.to_string(),
                                &threshold.to_string(),
                            ],
                        ),
                    ));
                }
            }
        }
        RuleKind::ExcessivePublicCount => {
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
        RuleKind::TooManyFields => {
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
        RuleKind::TooManyMethods => {
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
        RuleKind::TooManyPublicMethods => {
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
        RuleKind::ExcessiveClassComplexity => {
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
        RuleKind::ShortClassName => {
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
        RuleKind::LongClassName => {
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
        RuleKind::ShortVariable => {
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
        RuleKind::LongVariable => {
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
        RuleKind::ShortMethodName => {
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
                    format_message(
                        &rule.message,
                        &[parent, &f.name, &minimum.to_string()],
                    ),
                ));
            }
        }
        RuleKind::ConstantNamingConventions => {
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
                    format_message(
                        "Constant {0} should be defined in PascalCase",
                        &[&c.name],
                    )
                } else {
                    format_message(&rule.message, &[&c.name])
                };
                out.push(name_violation(rule, file, c.begin_line, description));
            }
        }
        RuleKind::BooleanGetMethodName => {
            let check_parameterized =
                property_bool(rule, "checkParameterizedMethods", false);
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
        RuleKind::UnusedLocalVariable => {
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
        RuleKind::UnusedFormalParameter => {
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
        RuleKind::UnusedPrivateField => {
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
        RuleKind::UnusedPrivateMethod => {
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

fn func_violation(rule: &LoadedRule, file: &str, f: &FnModel<'_>, description: String) -> Violation {
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

struct FnModel<'a> {
    name: String,
    parent: Option<String>,
    begin_line: usize,
    end_line: usize,
    param_count: usize,
    body: Option<&'a syn::Block>,
    returns_bool: bool,
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

struct TypeModel<'a> {
    name: String,
    node_type: String,
    begin_line: usize,
    end_line: usize,
    field_count: usize,
    public_fields: usize,
    methods: Vec<MethodRef<'a>>,
}

struct FileModel<'a> {
    src: &'a str,
    functions: Vec<FnModel<'a>>,
    types: Vec<TypeModel<'a>>,
    variables: Vec<NamedBinding>,
    constants: Vec<NamedBinding>,
    usage: UseDefModel,
}

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
        }
    }
}

struct UseDefCollector {
    locals: Vec<NamedSite>,
    params: Vec<NamedSite>,
    private_fields: Vec<NamedSite>,
    private_methods: Vec<NamedSite>,
    ident_reads: HashSet<String>,
    field_reads: HashSet<String>,
    method_calls: HashSet<String>,
    binding_mode: BindingMode,
    assign_lhs: bool,
    in_trait_impl: bool,
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
            locals: Vec::new(),
            params: Vec::new(),
            private_fields: Vec::new(),
            private_methods: Vec::new(),
            ident_reads: HashSet::new(),
            field_reads: HashSet::new(),
            method_calls: HashSet::new(),
            binding_mode: BindingMode::None,
            assign_lhs: false,
            in_trait_impl: false,
        }
    }

    fn into_model(self) -> UseDefModel {
        UseDefModel {
            locals: self.locals,
            params: self.params,
            private_fields: self.private_fields,
            private_methods: self.private_methods,
            ident_reads: self.ident_reads,
            field_reads: self.field_reads,
            method_calls: self.method_calls,
        }
    }

    fn with_binding_mode<F>(&mut self, mode: BindingMode, f: F)
    where
        F: FnOnce(&mut Self),
    {
        let prev = self.binding_mode;
        self.binding_mode = mode;
        f(self);
        self.binding_mode = prev;
    }

    fn record_params_from_sig(&mut self, sig: &syn::Signature) {
        for input in &sig.inputs {
            if let FnArg::Typed(pt) = input {
                self.with_binding_mode(BindingMode::Param, |this| this.visit_pat(&pt.pat));
            }
        }
    }
}

impl<'ast> Visit<'ast> for UseDefCollector {
    fn visit_local(&mut self, node: &'ast syn::Local) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        self.with_binding_mode(BindingMode::Local, |this| this.visit_pat(&node.pat));
        if let Some(init) = &node.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.record_params_from_sig(&node.sig);
        self.visit_block(&node.block);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let prev = self.in_trait_impl;
        self.in_trait_impl = node.trait_.is_some();
        syn::visit::visit_item_impl(self, node);
        self.in_trait_impl = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if !self.in_trait_impl && is_private(&node.vis) {
            self.private_methods.push(NamedSite {
                name: node.sig.ident.to_string(),
                begin_line: node.sig.fn_token.span().start().line,
            });
        }
        self.record_params_from_sig(&node.sig);
        self.visit_block(&node.block);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        self.record_params_from_sig(&node.sig);
        if let Some(body) = &node.default {
            self.visit_block(body);
        }
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        if let Some(ident) = &node.ident {
            if is_private(&node.vis) {
                self.private_fields.push(NamedSite {
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
        self.with_binding_mode(BindingMode::Local, |this| this.visit_pat(&node.pat));
        if let Some((_, guard)) = &node.guard {
            self.visit_expr(guard);
        }
        self.visit_expr(&node.body);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        self.with_binding_mode(BindingMode::Local, |this| this.visit_pat(&node.pat));
        self.visit_expr(&node.expr);
        self.visit_block(&node.body);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        if let syn::Expr::Let(l) = &*node.cond {
            self.with_binding_mode(BindingMode::Local, |this| this.visit_pat(&l.pat));
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
            self.with_binding_mode(BindingMode::Local, |this| this.visit_pat(&l.pat));
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
        if name != "self" {
            let site = NamedSite {
                name,
                begin_line: node.ident.span().start().line,
            };
            match self.binding_mode {
                BindingMode::Local => self.locals.push(site),
                BindingMode::Param => self.params.push(site),
                BindingMode::None => {}
            }
        }
        if let Some((_, sub)) = &node.subpat {
            self.visit_pat(sub);
        }
    }

    fn visit_field_pat(&mut self, node: &'ast syn::FieldPat) {
        if let Member::Named(ident) = &node.member {
            self.field_reads.insert(ident.to_string());
        }
        self.visit_pat(&node.pat);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if !self.assign_lhs {
            if let Some(ident) = path_single_ident(node) {
                if ident != "self" && ident != "Self" {
                    self.ident_reads.insert(ident);
                }
            } else if let Some(ident) = path_last_ident(node) {
                self.method_calls.insert(ident);
            }
        }
        syn::visit::visit_expr_path(self, node);
    }

    fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
        self.visit_expr(&node.base);
        if !self.assign_lhs {
            if let Member::Named(ident) = &node.member {
                self.field_reads.insert(ident.to_string());
            }
        }
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.method_calls.insert(node.method.to_string());
        syn::visit::visit_expr_method_call(self, node);
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
        if node.ident != "self" {
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
            Item::Mod(m) => {
                if let Some((_, nested)) = &m.content {
                    collect_items(nested, types, functions);
                }
            }
            _ => {}
        }
    }
}

fn upsert_type<'a>(
    types: &mut HashMap<String, TypeModel<'a>>,
    name: String,
    node_type: &str,
    begin_line: usize,
    end_line: usize,
    field_count: usize,
    public_fields: usize,
) {
    types
        .entry(name.clone())
        .and_modify(|existing| {
            existing.node_type = node_type.to_string();
            existing.begin_line = begin_line;
            existing.end_line = end_line;
            existing.field_count = field_count;
            existing.public_fields = public_fields;
        })
        .or_insert_with(|| TypeModel {
            name,
            node_type: node_type.to_string(),
            begin_line,
            end_line,
            field_count,
            public_fields,
            methods: Vec::new(),
        });
}

fn insert_struct<'a>(types: &mut HashMap<String, TypeModel<'a>>, s: &'a ItemStruct) {
    let (field_count, public_fields) = field_stats(&s.fields);
    upsert_type(
        types,
        s.ident.to_string(),
        "struct",
        s.struct_token.span().start().line,
        s.span().end().line,
        field_count,
        public_fields,
    );
}

fn insert_enum<'a>(types: &mut HashMap<String, TypeModel<'a>>, e: &'a ItemEnum) {
    upsert_type(
        types,
        e.ident.to_string(),
        "enum",
        e.enum_token.span().start().line,
        e.span().end().line,
        e.variants.len(),
        0,
    );
}

fn insert_union<'a>(types: &mut HashMap<String, TypeModel<'a>>, u: &'a ItemUnion) {
    let field_count = u.fields.named.len();
    let public_fields = u.fields.named.iter().filter(|f| is_public(&f.vis)).count();
    upsert_type(
        types,
        u.ident.to_string(),
        "union",
        u.union_token.span().start().line,
        u.span().end().line,
        field_count,
        public_fields,
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
                body,
                returns_bool: returns_bool(&m.sig.output),
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
            existing.methods.extend(methods.drain(..));
        })
        .or_insert_with(|| TypeModel {
            name,
            node_type: "trait".to_string(),
            begin_line: t.trait_token.span().start().line,
            end_line: t.span().end().line,
            field_count: 0,
            public_fields: 0,
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
        body: Some(&f.block),
        returns_bool: returns_bool(&f.sig.output),
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
        methods: Vec::new(),
    });
    let type_entry = types.get_mut(&ty_name).unwrap();
    for item in &im.items {
        if let syn::ImplItem::Fn(m) = item {
            let begin = m.sig.fn_token.span().start().line;
            let end = m.span().end().line;
            let name = m.sig.ident.to_string();
            let is_pub = is_public(&m.vis);
            type_entry.methods.push(MethodRef {
                name: name.clone(),
                begin_line: begin,
                end_line: end,
                is_public: is_pub,
                body: Some(&m.block),
            });
            functions.push(FnModel {
                name,
                parent: Some(ty_name.clone()),
                begin_line: begin,
                end_line: end,
                param_count: count_params(&m.sig.inputs),
                body: Some(&m.block),
                returns_bool: returns_bool(&m.sig.output),
            });
        }
    }
}
