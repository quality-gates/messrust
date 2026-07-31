//! Report model and family format renderers (messgo / PHPMD shapes).
// messrust-disable UnusedLocalVariable,UnusedPrivateField

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::{EXIT_ERROR, EXIT_SUCCESS, EXIT_VIOLATION};

#[derive(Debug, Default)]
pub struct Report {
    pub violations: Vec<Violation>,
    pub errors: Vec<ProcessingError>,
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub file: String,
    pub begin_line: usize,
    pub end_line: usize,
    pub rule_name: String,
    pub ruleset_name: String,
    pub description: String,
    pub priority: u8,
    pub package: String,
    pub function: String,
    pub class: String,
    pub method: String,
    pub external_info_url: String,
    pub suppressed: bool,
}

#[derive(Debug, Clone)]
pub struct ProcessingError {
    pub file: String,
    pub message: String,
}

pub enum WriteTarget {
    Stdout,
    File(PathBuf),
}

pub fn exit_code_for(report: &Report, ignore_errors: bool, ignore_violations: bool) -> i32 {
    if !report.errors.is_empty() && !ignore_errors {
        return EXIT_ERROR;
    }
    if !report.violations.is_empty() && !ignore_violations {
        return EXIT_VIOLATION;
    }
    EXIT_SUCCESS
}

pub(crate) fn formats() -> &'static [&'static str] {
    &[
        "text",
        "xml",
        "json",
        "html",
        "ansi",
        "github",
        "gitlab",
        "checkstyle",
        "sarif",
    ]
}

pub(crate) fn is_known_format(format: &str) -> bool {
    formats().contains(&format)
}

/// Render a report. `color` colorizes `text` the same way as messgo (`--color`).
/// Format `ansi` always colorizes rule names and messages.
pub fn render(
    format: &str,
    report: &Report,
    color: bool,
    target: WriteTarget,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let colored = format == "ansi" || (format == "text" && color);
    let effective = if format == "text" && color {
        "ansi"
    } else {
        format
    };

    match target {
        WriteTarget::Stdout => {
            write_format(effective, colored, report, stdout).map_err(|e| e.to_string())?;
        }
        WriteTarget::File(path) => {
            let mut f = std::fs::File::create(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            write_format(effective, colored, report, &mut f).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn write_format(
    format: &str,
    colored: bool,
    report: &Report,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    match format {
        "text" | "ansi" => write_text(report, colored, out),
        "json" => write_json(report, out),
        "xml" => write_xml(report, out),
        "html" => write_html(report, out),
        "github" => write_github(report, out),
        "gitlab" => write_gitlab(report, out),
        "checkstyle" => write_checkstyle(report, out),
        "sarif" => write_sarif(report, out),
        _ => unreachable!("unknown format filtered by caller"),
    }
}

fn write_text(report: &Report, colored: bool, out: &mut dyn Write) -> std::io::Result<()> {
    const SPACING: usize = 2;

    let locations: Vec<String> = report
        .violations
        .iter()
        .map(|v| format!("{}:{}", v.file, v.begin_line))
        .collect();
    let loc_width = locations.iter().map(|s| s.len()).max().unwrap_or(0);
    let rule_labels: Vec<String> = report
        .violations
        .iter()
        .map(|v| {
            if v.suppressed {
                format!("{} [suppressed]", v.rule_name)
            } else {
                v.rule_name.clone()
            }
        })
        .collect();
    let rule_width = rule_labels.iter().map(String::len).max().unwrap_or(0);

    for ((v, loc), rule_label) in report
        .violations
        .iter()
        .zip(locations.iter())
        .zip(rule_labels.iter())
    {
        let pad1 = " ".repeat(loc_width.saturating_sub(loc.len()) + SPACING);
        let pad2 = " ".repeat(rule_width.saturating_sub(rule_label.len()) + SPACING);
        write!(out, "{loc}{pad1}")?;
        write!(out, "{}", colorize(rule_label, "33", colored))?;
        write!(out, "{pad2}")?;
        write!(out, "{}", colorize(&v.description, "31", colored))?;
        writeln!(out)?;
    }

    for err in &report.errors {
        writeln!(out, "{}\t-\t{}", err.file, err.message)?;
    }
    Ok(())
}

fn colorize(s: &str, code: &str, colored: bool) -> String {
    if colored {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#039;")
}

fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // RFC3339-ish UTC without pulling in a clock crate.
    let days = secs / 86400;
    let tod = secs % 86400;
    let (y, m, d) = civil_from_days(days as i64);
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Civil date from days since Unix epoch (Howard Hinnant algorithm).
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ----- JSON (PHPMD / messgo shape) ----------------------------------------

#[derive(Serialize)]
struct JsonReport {
    version: String,
    package: String,
    timestamp: String,
    files: Vec<JsonFile>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<JsonError>,
}

#[derive(Serialize)]
struct JsonFile {
    file: String,
    violations: Vec<JsonViolation>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonViolation {
    begin_line: usize,
    end_line: usize,
    package: String,
    function: String,
    class: String,
    method: String,
    description: String,
    rule: String,
    rule_set: String,
    external_info_url: String,
    priority: u8,
    suppressed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonError {
    file_name: String,
    message: String,
}

fn write_json(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    let mut files: Vec<JsonFile> = Vec::new();
    let mut idx: BTreeMap<&str, usize> = BTreeMap::new();
    // Preserve first-seen file order (violations already sorted by file).
    for v in &report.violations {
        let i = if let Some(&i) = idx.get(v.file.as_str()) {
            i
        } else {
            let i = files.len();
            idx.insert(&v.file, i);
            files.push(JsonFile {
                file: v.file.clone(),
                violations: Vec::new(),
            });
            i
        };
        files[i].violations.push(JsonViolation {
            begin_line: v.begin_line,
            end_line: v.end_line,
            package: v.package.clone(),
            function: v.function.clone(),
            class: v.class.clone(),
            method: v.method.clone(),
            description: v.description.clone(),
            rule: v.rule_name.clone(),
            rule_set: v.ruleset_name.clone(),
            external_info_url: v.external_info_url.clone(),
            priority: v.priority,
            suppressed: v.suppressed,
        });
    }
    let rep = JsonReport {
        version: version().to_string(),
        package: "messrust".to_string(),
        timestamp: timestamp(),
        files,
        errors: report
            .errors
            .iter()
            .map(|e| JsonError {
                file_name: e.file.clone(),
                message: e.message.clone(),
            })
            .collect(),
    };
    let mut body = serde_json::to_string_pretty(&rep).map_err(std::io::Error::other)?;
    // messgo uses 4-space indent; serde_json uses 2. Re-indent for family parity.
    body = reindent_json(&body);
    writeln!(out, "{body}")
}

fn reindent_json(s: &str) -> String {
    // Convert 2-space pretty JSON to 4-space like messgo's encoding/json.
    let mut out = String::with_capacity(s.len() * 2);
    for line in s.lines() {
        let trimmed = line.trim_start_matches(' ');
        let spaces = line.len() - trimmed.len();
        out.push_str(&" ".repeat(spaces * 2));
        out.push_str(trimmed);
        out.push('\n');
    }
    out.pop(); // trailing newline added by writeln
    out
}

// ----- XML ----------------------------------------------------------------

fn write_xml(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>")?;
    writeln!(
        out,
        "<pmd version=\"{}\" tool=\"messrust\" timestamp=\"{}\">",
        version(),
        xml_escape(&timestamp())
    )?;
    let mut cur_file = String::new();
    let mut open = false;
    for v in &report.violations {
        if v.file != cur_file {
            if open {
                writeln!(out, "  </file>")?;
            }
            cur_file = v.file.clone();
            writeln!(out, "  <file name=\"{}\">", xml_escape(&cur_file))?;
            open = true;
        }
        write!(out, "    <violation")?;
        write!(out, " beginline=\"{}\"", v.begin_line)?;
        write!(out, " endline=\"{}\"", v.end_line)?;
        write!(out, " rule=\"{}\"", xml_escape(&v.rule_name))?;
        write!(out, " ruleset=\"{}\"", xml_escape(&v.ruleset_name))?;
        maybe_attr(out, "package", &v.package)?;
        maybe_attr(out, "externalInfoUrl", &v.external_info_url)?;
        maybe_attr(out, "function", &v.function)?;
        maybe_attr(out, "class", &v.class)?;
        maybe_attr(out, "method", &v.method)?;
        write!(out, " priority=\"{}\"", v.priority)?;
        write!(out, " suppressed=\"{}\"", v.suppressed)?;
        writeln!(out, ">")?;
        writeln!(out, "      {}", xml_escape(&v.description))?;
        writeln!(out, "    </violation>")?;
    }
    if open {
        writeln!(out, "  </file>")?;
    }
    for e in &report.errors {
        writeln!(
            out,
            "  <error filename=\"{}\" msg=\"{}\" />",
            xml_escape(&e.file),
            xml_escape(&e.message)
        )?;
    }
    writeln!(out, "</pmd>")?;
    Ok(())
}

fn maybe_attr(out: &mut dyn Write, name: &str, val: &str) -> std::io::Result<()> {
    if !val.trim().is_empty() {
        write!(out, " {name}=\"{}\"", xml_escape(val))?;
    }
    Ok(())
}

// ----- HTML ---------------------------------------------------------------

fn write_html(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(out, "<!DOCTYPE html>")?;
    writeln!(
        out,
        "<html><head><meta charset=\"utf-8\"><title>messrust report</title></head><body>"
    )?;
    writeln!(out, "<h1>messrust report</h1>")?;
    let mut cur_file = String::new();
    let mut open = false;
    for v in &report.violations {
        if v.file != cur_file {
            if open {
                writeln!(out, "</table>")?;
            }
            cur_file = v.file.clone();
            writeln!(
                out,
                "<h2>{}</h2>\n<table border=\"1\" cellspacing=\"0\" cellpadding=\"3\">",
                xml_escape(&cur_file)
            )?;
            writeln!(
                out,
                "<tr><th>Line</th><th>Rule</th><th>Suppressed</th><th>Description</th></tr>"
            )?;
            open = true;
        }
        writeln!(
            out,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            v.begin_line,
            xml_escape(&v.rule_name),
            v.suppressed,
            xml_escape(&v.description)
        )?;
    }
    if open {
        writeln!(out, "</table>")?;
    }
    writeln!(out, "</body></html>")?;
    Ok(())
}

// ----- GitHub Actions -----------------------------------------------------

fn write_github(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    for v in &report.violations {
        writeln!(
            out,
            "::warning file={},line={},col=1::{} ({}{})",
            v.file,
            v.begin_line,
            v.description,
            v.rule_name,
            if v.suppressed { ", suppressed" } else { "" }
        )?;
    }
    for e in &report.errors {
        writeln!(out, "::error file={}::{}", e.file, e.message)?;
    }
    Ok(())
}

// ----- GitLab Code Quality ------------------------------------------------

#[derive(Serialize)]
struct GitlabEntry {
    #[serde(rename = "type")]
    entry_type: String,
    check_name: String,
    description: String,
    fingerprint: String,
    severity: String,
    suppressed: bool,
    location: GitlabLocation,
}

#[derive(Serialize)]
struct GitlabLocation {
    path: String,
    lines: GitlabLines,
}

#[derive(Serialize)]
struct GitlabLines {
    begin: usize,
}

fn write_gitlab(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    let entries: Vec<GitlabEntry> = report
        .violations
        .iter()
        .map(|v| GitlabEntry {
            entry_type: "issue".to_string(),
            check_name: v.rule_name.clone(),
            description: v.description.clone(),
            fingerprint: gitlab_fingerprint(v),
            severity: gitlab_severity(v.priority).to_string(),
            suppressed: v.suppressed,
            location: GitlabLocation {
                path: v.file.clone(),
                lines: GitlabLines {
                    begin: v.begin_line,
                },
            },
        })
        .collect();
    let body = serde_json::to_string_pretty(&entries).map_err(std::io::Error::other)?;
    writeln!(out, "{}", reindent_json(&body))
}

fn gitlab_severity(priority: u8) -> &'static str {
    match priority {
        1 => "blocker",
        2 => "critical",
        3 => "major",
        4 => "minor",
        _ => "info",
    }
}

fn gitlab_fingerprint(v: &Violation) -> String {
    // messgo: hex of UTF-8 bytes of "file:line:ruleName"
    let raw = format!("{}:{}:{}", v.file, v.begin_line, v.rule_name);
    raw.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

// ----- Checkstyle ---------------------------------------------------------

fn write_checkstyle(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>")?;
    writeln!(out, "<checkstyle version=\"{}\">", version())?;
    let mut cur_file = String::new();
    let mut open = false;
    for v in &report.violations {
        if v.file != cur_file {
            if open {
                writeln!(out, "  </file>")?;
            }
            cur_file = v.file.clone();
            writeln!(out, "  <file name=\"{}\">", xml_escape(&cur_file))?;
            open = true;
        }
        writeln!(
            out,
            "    <error line=\"{}\" column=\"1\" severity=\"{}\" message=\"{}\" source=\"{}\"/>",
            v.begin_line,
            checkstyle_severity(v.priority),
            xml_escape(&format_description(v)),
            xml_escape(&format!("{}/{}", v.ruleset_name, v.rule_name))
        )?;
    }
    if open {
        writeln!(out, "  </file>")?;
    }
    writeln!(out, "</checkstyle>")?;
    Ok(())
}

fn checkstyle_severity(priority: u8) -> &'static str {
    if priority <= 2 {
        "error"
    } else if priority == 3 {
        "warning"
    } else {
        "info"
    }
}

fn format_description(v: &Violation) -> String {
    if v.suppressed {
        format!("{} [suppressed]", v.description)
    } else {
        v.description.clone()
    }
}

// ----- SARIF 2.1.0 --------------------------------------------------------

fn write_sarif(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    let mut seen = BTreeMap::new();
    let mut rules = Vec::new();
    let mut results = Vec::new();
    for v in &report.violations {
        if !seen.contains_key(&v.rule_name) {
            seen.insert(v.rule_name.clone(), true);
            rules.push(serde_json::json!({
                "id": v.rule_name,
                "name": v.rule_name,
                "shortDescription": { "text": v.rule_name },
            }));
        }
        let mut result = serde_json::json!({
            "ruleId": v.rule_name,
            "level": sarif_level(v.priority),
            "message": { "text": v.description },
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": { "uri": v.file },
                    "region": {
                        "startLine": v.begin_line,
                        "endLine": v.end_line,
                    }
                }
            }],
            "properties": {
                "priority": v.priority,
                "suppressed": v.suppressed,
            }
        });
        if v.suppressed {
            result["suppressions"] = serde_json::json!([{"kind": "inSource"}]);
        }
        results.push(result);
    }
    let doc = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "messrust",
                    "version": version(),
                    "rules": rules,
                }
            },
            "results": results,
        }]
    });
    let body = serde_json::to_string_pretty(&doc).map_err(std::io::Error::other)?;
    // SARIF in messgo uses 2-space indent.
    writeln!(out, "{body}")
}

fn sarif_level(priority: u8) -> &'static str {
    if priority <= 2 {
        "error"
    } else {
        "warning"
    }
}
