//! Load phpmd-format rulesets and apply CLI filters.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::analyze::RuleKind;

/// A rule selected from a ruleset after load and filter.
#[derive(Clone, Debug)]
pub struct LoadedRule {
    pub name: String,
    pub ruleset_name: String,
    /// Priority 1 (highest) through 5.
    pub priority: u8,
    pub properties: BTreeMap<String, String>,
    pub kind: RuleKind,
    /// phpmd-style message template with `{0}` placeholders.
    pub message: String,
}

pub struct LoadOptions {
    pub min_priority: u8,
    pub max_priority: u8,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            min_priority: 0,
            max_priority: 1,
        }
    }
}

struct XmlRule {
    name: String,
    class: String,
    message: String,
    ref_path: String,
    priority: Option<u8>,
    properties: BTreeMap<String, String>,
    excludes: Vec<String>,
}

struct XmlRuleset {
    name: String,
    rules: Vec<XmlRule>,
}

/// Load comma-separated ruleset names and/or XML paths, then apply filters.
pub fn load_and_filter(
    specs: &[String],
    only: &[String],
    disable: &[String],
    opts: &LoadOptions,
    warn: &mut dyn FnMut(String),
) -> Result<Vec<LoadedRule>, String> {
    let mut rules = Vec::new();
    let mut seen = HashSet::new();
    for spec in specs {
        for rule in load_one(spec, opts, warn)? {
            if seen.insert(rule.name.clone()) {
                rules.push(rule);
            }
        }
    }
    if rules.is_empty() && specs.is_empty() {
        return Err("no rulesets specified".to_string());
    }
    apply_name_filters(&mut rules, only, disable)?;
    Ok(rules)
}

fn apply_name_filters(
    rules: &mut Vec<LoadedRule>,
    only: &[String],
    disable: &[String],
) -> Result<(), String> {
    let loaded: HashSet<&str> = rules.iter().map(|r| r.name.as_str()).collect();
    for name in only.iter().chain(disable.iter()) {
        if !loaded.contains(name.as_str()) {
            return Err(format!(
                "rule '{name}' is not present in the loaded rulesets"
            ));
        }
    }
    if !only.is_empty() {
        let keep: HashSet<&str> = only.iter().map(String::as_str).collect();
        rules.retain(|r| keep.contains(r.name.as_str()));
    }
    if !disable.is_empty() {
        let drop: HashSet<&str> = disable.iter().map(String::as_str).collect();
        rules.retain(|r| !drop.contains(r.name.as_str()));
    }
    Ok(())
}

fn load_one(
    ident: &str,
    opts: &LoadOptions,
    warn: &mut dyn FnMut(String),
) -> Result<Vec<LoadedRule>, String> {
    let (xml, display_name) = read_ruleset(ident)?;
    let parsed = parse_ruleset(&xml)?;
    let set_name = if parsed.name.is_empty() {
        display_name
    } else {
        parsed.name
    };
    let mut out = Vec::new();
    for xr in parsed.rules {
        append_rule(&mut out, &set_name, &xr, opts, warn)?;
    }
    Ok(out)
}

fn append_rule(
    out: &mut Vec<LoadedRule>,
    set_name: &str,
    xr: &XmlRule,
    opts: &LoadOptions,
    warn: &mut dyn FnMut(String),
) -> Result<(), String> {
    if !xr.ref_path.is_empty() {
        return add_ref(out, xr, opts, warn);
    }
    if xr.class.is_empty() {
        return Ok(());
    }
    if let Some(rule) = build_rule(set_name, xr, xr, opts, warn) {
        out.push(rule);
    }
    Ok(())
}

fn add_ref(
    out: &mut Vec<LoadedRule>,
    xr: &XmlRule,
    opts: &LoadOptions,
    warn: &mut dyn FnMut(String),
) -> Result<(), String> {
    let (base, rule_name) = split_ref(&xr.ref_path);
    let Some(src) = read_referenced_ruleset(&base, &xr.ref_path, warn)? else {
        return Ok(());
    };
    let src_name = if src.name.is_empty() {
        base
    } else {
        src.name.clone()
    };
    if !rule_name.is_empty() {
        add_named_rule(out, &src, &src_name, &rule_name, xr, opts, warn);
        return Ok(());
    }
    add_ruleset_rules(out, &src, &src_name, xr, opts, warn);
    Ok(())
}

fn read_referenced_ruleset(
    base: &str,
    reference: &str,
    warn: &mut dyn FnMut(String),
) -> Result<Option<XmlRuleset>, String> {
    match read_ruleset(base) {
        Ok((xml, _)) => parse_ruleset(&xml).map(Some),
        Err(_) => {
            warn(format!("Cannot resolve ref: {reference}"));
            Ok(None)
        }
    }
}

fn add_named_rule(
    out: &mut Vec<LoadedRule>,
    source: &XmlRuleset,
    source_name: &str,
    rule_name: &str,
    override_rule: &XmlRule,
    opts: &LoadOptions,
    warn: &mut dyn FnMut(String),
) {
    let Some(source_rule) = source.rules.iter().find(|rule| rule.name == rule_name) else {
        return;
    };
    if let Some(rule) = build_rule(source_name, source_rule, override_rule, opts, warn) {
        out.push(rule);
    }
}

fn add_ruleset_rules(
    out: &mut Vec<LoadedRule>,
    source: &XmlRuleset,
    source_name: &str,
    override_rule: &XmlRule,
    opts: &LoadOptions,
    warn: &mut dyn FnMut(String),
) {
    let excluded: HashSet<&str> = override_rule.excludes.iter().map(String::as_str).collect();
    for source_rule in &source.rules {
        if source_rule.class.is_empty() || excluded.contains(source_rule.name.as_str()) {
            continue;
        }
        if let Some(rule) = build_rule(source_name, source_rule, source_rule, opts, warn) {
            out.push(rule);
        }
    }
}

fn build_rule(
    set_name: &str,
    def: &XmlRule,
    ov: &XmlRule,
    opts: &LoadOptions,
    warn: &mut dyn FnMut(String),
) -> Option<LoadedRule> {
    let priority = ov.priority.or(def.priority).unwrap_or(3);
    if opts.min_priority > 0 && priority > opts.min_priority {
        return None;
    }
    if opts.max_priority > 0 && priority < opts.max_priority {
        return None;
    }
    let mut properties = def.properties.clone();
    for (k, v) in &ov.properties {
        properties.insert(k.clone(), v.clone());
    }
    let Some(kind) = resolve_kind(&def.class) else {
        warn(format!(
            "Skipping unimplemented rule {} ({})",
            def.name, def.class
        ));
        return None;
    };
    let message = if ov.message.is_empty() {
        def.message.clone()
    } else {
        ov.message.clone()
    };
    Some(LoadedRule {
        name: def.name.clone(),
        ruleset_name: set_name.to_string(),
        priority,
        properties,
        kind,
        message,
    })
}

fn resolve_kind(class: &str) -> Option<RuleKind> {
    RULE_KINDS
        .iter()
        .find(|(known_class, _)| *known_class == class)
        .map(|(_, kind)| *kind)
}

const RULE_KINDS: &[(&str, RuleKind)] = &[
    (
        "PHPMD\\Rule\\CyclomaticComplexity",
        RuleKind::CyclomaticComplexity,
    ),
    (
        "PHPMD\\Rule\\Design\\NpathComplexity",
        RuleKind::NPathComplexity,
    ),
    (
        "PHPMD\\Rule\\Design\\LongMethod",
        RuleKind::ExcessiveMethodLength,
    ),
    (
        "PHPMD\\Rule\\Design\\LongClass",
        RuleKind::ExcessiveClassLength,
    ),
    (
        "PHPMD\\Rule\\Design\\LongParameterList",
        RuleKind::ExcessiveParameterList,
    ),
    (
        "PHPMD\\Rule\\ExcessivePublicCount",
        RuleKind::ExcessivePublicCount,
    ),
    (
        "PHPMD\\Rule\\Design\\TooManyFields",
        RuleKind::TooManyFields,
    ),
    (
        "PHPMD\\Rule\\Design\\TooManyMethods",
        RuleKind::TooManyMethods,
    ),
    (
        "PHPMD\\Rule\\Design\\TooManyPublicMethods",
        RuleKind::TooManyPublicMethods,
    ),
    (
        "PHPMD\\Rule\\Design\\WeightedMethodCount",
        RuleKind::ExcessiveClassComplexity,
    ),
    (
        "PHPMD\\Rule\\Naming\\ShortClassName",
        RuleKind::ShortClassName,
    ),
    (
        "PHPMD\\Rule\\Naming\\LongClassName",
        RuleKind::LongClassName,
    ),
    (
        "PHPMD\\Rule\\Naming\\ShortVariable",
        RuleKind::ShortVariable,
    ),
    ("PHPMD\\Rule\\Naming\\LongVariable", RuleKind::LongVariable),
    (
        "PHPMD\\Rule\\Naming\\ShortMethodName",
        RuleKind::ShortMethodName,
    ),
    (
        "PHPMD\\Rule\\Naming\\ConstantNamingConventions",
        RuleKind::ConstantNamingConventions,
    ),
    (
        "PHPMD\\Rule\\Naming\\BooleanGetMethodName",
        RuleKind::BooleanGetMethodName,
    ),
    (
        "PHPMD\\Rule\\UnusedPrivateField",
        RuleKind::UnusedPrivateField,
    ),
    (
        "PHPMD\\Rule\\UnusedLocalVariable",
        RuleKind::UnusedLocalVariable,
    ),
    (
        "PHPMD\\Rule\\UnusedPrivateMethod",
        RuleKind::UnusedPrivateMethod,
    ),
    (
        "PHPMD\\Rule\\UnusedFormalParameter",
        RuleKind::UnusedFormalParameter,
    ),
    (
        "PHPMD\\Rule\\CleanCode\\BooleanArgumentFlag",
        RuleKind::BooleanArgumentFlag,
    ),
    (
        "PHPMD\\Rule\\CleanCode\\ElseExpression",
        RuleKind::ElseExpression,
    ),
    (
        "PHPMD\\Rule\\CleanCode\\IfStatementAssignment",
        RuleKind::IfStatementAssignment,
    ),
    (
        "PHPMD\\Rule\\CleanCode\\DuplicatedArrayKey",
        RuleKind::DuplicatedArrayKey,
    ),
    (
        "PHPMD\\Rule\\CleanCode\\StaticAccess",
        RuleKind::StaticAccess,
    ),
    (
        "PHPMD\\Rule\\Design\\ExitExpression",
        RuleKind::ExitExpression,
    ),
    (
        "PHPMD\\Rule\\Design\\GotoStatement",
        RuleKind::GotoStatement,
    ),
    (
        "PHPMD\\Rule\\Design\\CountInLoopExpression",
        RuleKind::CountInLoopExpression,
    ),
    (
        "PHPMD\\Rule\\Design\\DevelopmentCodeFragment",
        RuleKind::DevelopmentCodeFragment,
    ),
    (
        "PHPMD\\Rule\\Design\\EmptyCatchBlock",
        RuleKind::EmptyCatchBlock,
    ),
    (
        "PHPMD\\Rule\\Design\\CouplingBetweenObjects",
        RuleKind::CouplingBetweenObjects,
    ),
    (
        "PHPMD\\Rule\\Design\\GlobalVariable",
        RuleKind::GlobalVariable,
    ),
    (
        "PHPMD\\Rule\\Design\\LackOfCohesionOfMethods",
        RuleKind::LackOfCohesionOfMethods,
    ),
    (
        "PHPMD\\Rule\\Controversial\\CamelCaseClassName",
        RuleKind::CamelCaseClassName,
    ),
    (
        "PHPMD\\Rule\\Controversial\\CamelCaseMethodName",
        RuleKind::CamelCaseMethodName,
    ),
    (
        "PHPMD\\Rule\\Controversial\\CamelCasePropertyName",
        RuleKind::CamelCasePropertyName,
    ),
    (
        "PHPMD\\Rule\\Controversial\\CamelCaseParameterName",
        RuleKind::CamelCaseParameterName,
    ),
    (
        "PHPMD\\Rule\\Controversial\\CamelCaseVariableName",
        RuleKind::CamelCaseVariableName,
    ),
];

fn read_ruleset(ident: &str) -> Result<(String, String), String> {
    if let Some((xml, name)) = builtin_xml(ident) {
        return Ok((xml.to_string(), name.to_string()));
    }
    let path = PathBuf::from(ident);
    if path.is_file() {
        let xml = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(ident)
            .to_string();
        return Ok((xml, name));
    }
    Err(format!("unknown ruleset or file: {ident}"))
}

fn builtin_xml(ident: &str) -> Option<(&'static str, &'static str)> {
    let key = normalize_builtin_key(ident)?;
    match key.as_str() {
        "codesize" => Some((include_str!("../rulesets/codesize.xml"), "codesize")),
        "naming" => Some((include_str!("../rulesets/naming.xml"), "naming")),
        "unusedcode" => Some((include_str!("../rulesets/unusedcode.xml"), "unusedcode")),
        "cleancode" => Some((include_str!("../rulesets/cleancode.xml"), "cleancode")),
        "design" => Some((include_str!("../rulesets/design.xml"), "design")),
        "controversial" => Some((
            include_str!("../rulesets/controversial.xml"),
            "controversial",
        )),
        "rust" => Some((include_str!("../rulesets/rust.xml"), "rust")),
        "opinionated" => Some((include_str!("../rulesets/opinionated.xml"), "opinionated")),
        _ => None,
    }
}

fn normalize_builtin_key(ident: &str) -> Option<String> {
    let lower = ident.to_ascii_lowercase().replace('\\', "/");
    let base = Path::new(&lower)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&lower);
    let stem = base.strip_suffix(".xml").unwrap_or(base);
    match stem {
        "codesize" | "naming" | "unusedcode" | "cleancode" | "design" | "controversial"
        | "rust" | "opinionated" => Some(stem.to_string()),
        _ => None,
    }
}

fn split_ref(ref_str: &str) -> (String, String) {
    if is_resolvable(ref_str) {
        return (ref_str.to_string(), String::new());
    }
    if let Some(idx) = ref_str.rfind('/') {
        let base = &ref_str[..idx];
        if is_resolvable(base) {
            return (base.to_string(), ref_str[idx + 1..].to_string());
        }
    }
    (ref_str.to_string(), String::new())
}

fn is_resolvable(ident: &str) -> bool {
    builtin_xml(ident).is_some() || Path::new(ident).is_file()
}

fn parse_ruleset(xml: &str) -> Result<XmlRuleset, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("invalid ruleset XML: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "ruleset" {
        return Err("ruleset XML root must be <ruleset>".to_string());
    }
    let name = root.attribute("name").unwrap_or("").to_string();
    let mut rules = Vec::new();
    for child in root.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "rule" {
            rules.push(parse_rule(child));
        }
    }
    Ok(XmlRuleset { name, rules })
}

fn parse_rule(node: roxmltree::Node<'_, '_>) -> XmlRule {
    let mut rule = XmlRule {
        name: node.attribute("name").unwrap_or("").to_string(),
        class: node.attribute("class").unwrap_or("").to_string(),
        message: node.attribute("message").unwrap_or("").to_string(),
        ref_path: node.attribute("ref").unwrap_or("").to_string(),
        priority: None,
        properties: BTreeMap::new(),
        excludes: Vec::new(),
    };
    for child in node.children().filter(|n| n.is_element()) {
        parse_rule_child(&mut rule, child);
    }
    rule
}

fn parse_rule_child(rule: &mut XmlRule, child: roxmltree::Node<'_, '_>) {
    match child.tag_name().name() {
        "priority" => {
            if let Ok(priority) = child.text().unwrap_or("").trim().parse::<u8>() {
                rule.priority = Some(priority);
            }
        }
        "properties" => parse_properties(rule, child),
        "exclude" => {
            if let Some(name) = child.attribute("name") {
                rule.excludes.push(name.to_string());
            }
        }
        _ => {}
    }
}

fn parse_properties(rule: &mut XmlRule, properties: roxmltree::Node<'_, '_>) {
    for property in properties.children().filter(|node| node.is_element()) {
        if property.tag_name().name() != "property" {
            continue;
        }
        let name = property.attribute("name").unwrap_or("");
        if !name.is_empty() {
            rule.properties
                .insert(name.to_string(), property_value(property));
        }
    }
}

fn property_value(property: roxmltree::Node<'_, '_>) -> String {
    let attribute = property.attribute("value").unwrap_or("");
    if !attribute.is_empty() {
        return attribute.to_string();
    }
    property
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "value")
        .and_then(|node| node.text())
        .unwrap_or("")
        .trim()
        .to_string()
}
