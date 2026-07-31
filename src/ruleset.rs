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
    let (xml, _) = match read_ruleset(&base) {
        Ok(v) => v,
        Err(_) => {
            warn(format!("Cannot resolve ref: {}", xr.ref_path));
            return Ok(());
        }
    };
    let src = parse_ruleset(&xml)?;
    let excluded: HashSet<&str> = xr.excludes.iter().map(String::as_str).collect();
    let src_name = if src.name.is_empty() {
        base
    } else {
        src.name.clone()
    };
    for sr in &src.rules {
        if sr.class.is_empty() {
            continue;
        }
        if !rule_name.is_empty() {
            if sr.name == rule_name {
                if let Some(rule) = build_rule(&src_name, sr, xr, opts, warn) {
                    out.push(rule);
                }
            }
            continue;
        }
        if excluded.contains(sr.name.as_str()) {
            continue;
        }
        if let Some(rule) = build_rule(&src_name, sr, sr, opts, warn) {
            out.push(rule);
        }
    }
    Ok(())
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
    match class {
        "PHPMD\\Rule\\CyclomaticComplexity" => Some(RuleKind::CyclomaticComplexity),
        "PHPMD\\Rule\\Design\\NpathComplexity" => Some(RuleKind::NPathComplexity),
        "PHPMD\\Rule\\Design\\LongMethod" => Some(RuleKind::ExcessiveMethodLength),
        "PHPMD\\Rule\\Design\\LongClass" => Some(RuleKind::ExcessiveClassLength),
        "PHPMD\\Rule\\Design\\LongParameterList" => Some(RuleKind::ExcessiveParameterList),
        "PHPMD\\Rule\\ExcessivePublicCount" => Some(RuleKind::ExcessivePublicCount),
        "PHPMD\\Rule\\Design\\TooManyFields" => Some(RuleKind::TooManyFields),
        "PHPMD\\Rule\\Design\\TooManyMethods" => Some(RuleKind::TooManyMethods),
        "PHPMD\\Rule\\Design\\TooManyPublicMethods" => Some(RuleKind::TooManyPublicMethods),
        "PHPMD\\Rule\\Design\\WeightedMethodCount" => Some(RuleKind::ExcessiveClassComplexity),
        "PHPMD\\Rule\\Naming\\ShortClassName" => Some(RuleKind::ShortClassName),
        "PHPMD\\Rule\\Naming\\LongClassName" => Some(RuleKind::LongClassName),
        "PHPMD\\Rule\\Naming\\ShortVariable" => Some(RuleKind::ShortVariable),
        "PHPMD\\Rule\\Naming\\LongVariable" => Some(RuleKind::LongVariable),
        "PHPMD\\Rule\\Naming\\ShortMethodName" => Some(RuleKind::ShortMethodName),
        "PHPMD\\Rule\\Naming\\ConstantNamingConventions" => {
            Some(RuleKind::ConstantNamingConventions)
        }
        "PHPMD\\Rule\\Naming\\BooleanGetMethodName" => Some(RuleKind::BooleanGetMethodName),
        "PHPMD\\Rule\\UnusedPrivateField" => Some(RuleKind::UnusedPrivateField),
        "PHPMD\\Rule\\UnusedLocalVariable" => Some(RuleKind::UnusedLocalVariable),
        "PHPMD\\Rule\\UnusedPrivateMethod" => Some(RuleKind::UnusedPrivateMethod),
        "PHPMD\\Rule\\UnusedFormalParameter" => Some(RuleKind::UnusedFormalParameter),
        "PHPMD\\Rule\\CleanCode\\BooleanArgumentFlag" => Some(RuleKind::BooleanArgumentFlag),
        "PHPMD\\Rule\\CleanCode\\ElseExpression" => Some(RuleKind::ElseExpression),
        "PHPMD\\Rule\\CleanCode\\IfStatementAssignment" => Some(RuleKind::IfStatementAssignment),
        "PHPMD\\Rule\\CleanCode\\DuplicatedArrayKey" => Some(RuleKind::DuplicatedArrayKey),
        "PHPMD\\Rule\\CleanCode\\StaticAccess" => Some(RuleKind::StaticAccess),
        _ => None,
    }
}

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
        "codesize" | "naming" | "unusedcode" | "cleancode" | "design" | "controversial" => {
            Some(stem.to_string())
        }
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
        match child.tag_name().name() {
            "priority" => {
                if let Ok(p) = child.text().unwrap_or("").trim().parse::<u8>() {
                    rule.priority = Some(p);
                }
            }
            "properties" => {
                for prop in child.children().filter(|n| n.is_element()) {
                    if prop.tag_name().name() != "property" {
                        continue;
                    }
                    let pname = prop.attribute("name").unwrap_or("").to_string();
                    let mut value = prop.attribute("value").unwrap_or("").to_string();
                    if value.is_empty() {
                        for v in prop.children().filter(|n| n.is_element()) {
                            if v.tag_name().name() == "value" {
                                value = v.text().unwrap_or("").trim().to_string();
                            }
                        }
                    }
                    if !pname.is_empty() {
                        rule.properties.insert(pname, value);
                    }
                }
            }
            "exclude" => {
                if let Some(n) = child.attribute("name") {
                    rule.excludes.push(n.to_string());
                }
            }
            _ => {}
        }
    }
    rule
}
