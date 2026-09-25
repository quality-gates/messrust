//! Report model and family format renderers (messgo / PHPMD shapes).

use std::collections::BTreeMap;
use std::io::Write;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub(crate) enum ReportFormat {
    Text,
    Ansi,
    Json,
    Xml,
    Html,
    Github,
    Gitlab,
    Checkstyle,
    Sarif,
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

impl ReportFormat {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|format| format.name() == name)
    }

    pub(crate) fn all() -> &'static [Self] {
        const ALL: &[ReportFormat] = &[
            ReportFormat::Text,
            ReportFormat::Xml,
            ReportFormat::Json,
            ReportFormat::Html,
            ReportFormat::Ansi,
            ReportFormat::Github,
            ReportFormat::Gitlab,
            ReportFormat::Checkstyle,
            ReportFormat::Sarif,
        ];
        ALL
    }

    pub(crate) fn name(&self) -> &'static str {
        const NAMES: [&str; 9] = [
            "text",
            "ansi",
            "json",
            "xml",
            "html",
            "github",
            "gitlab",
            "checkstyle",
            "sarif",
        ];
        NAMES[*self as usize]
    }

    pub(crate) fn render(
        &self,
        report: &Report,
        color: bool,
        out: &mut dyn Write,
    ) -> std::io::Result<()> {
        self.render_with_color(report, self.uses_color(color), out)
    }

    fn uses_color(&self, color: bool) -> bool {
        match self {
            Self::Ansi => true,
            Self::Text => color,
            _ => false,
        }
    }

    fn render_with_color(
        &self,
        report: &Report,
        colored: bool,
        out: &mut dyn Write,
    ) -> std::io::Result<()> {
        match *self {
            Self::Text | Self::Ansi => write_text(report, colored, out),
            Self::Json => write_json(report, out),
            Self::Xml => write_xml(report, out),
            Self::Html => write_html(report, out),
            Self::Github => write_github(report, out),
            Self::Gitlab => write_gitlab(report, out),
            Self::Checkstyle => write_checkstyle(report, out),
            Self::Sarif => write_sarif(report, out),
        }
    }
}

fn write_text(report: &Report, colored: bool, out: &mut dyn Write) -> std::io::Result<()> {
    const SPACING: usize = 2;

    if !report.violations.is_empty() {
        let locations: Vec<String> = report
            .violations
            .iter()
            .map(|v| format!("{}:{}", v.file, v.begin_line))
            .collect();
        let loc_width = locations.iter().map(|s| s.len()).max().unwrap();
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
        let rule_width = rule_labels.iter().map(String::len).max().unwrap();

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
    use std::mem::MaybeUninit;
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    // Delegate civil UTC conversion to libc. Hand-rolled calendar arithmetic
    // is not reachable with distinct observable outputs through the command
    // seam (wall-clock inputs make most mutants equivalent).
    let mut tm = MaybeUninit::<libc::tm>::uninit();
    let ok = unsafe { !libc::gmtime_r(&secs, tm.as_mut_ptr()).is_null() };
    if !ok {
        return "1970-01-01T00:00:00Z".to_string();
    }
    let tm = unsafe { tm.assume_init() };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
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
    let mut out = String::new();
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

fn github_escape_property(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

fn github_escape_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn write_github(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    for v in &report.violations {
        writeln!(
            out,
            "::warning file={},line={},col=1::{} ({}{})",
            github_escape_property(&v.file),
            v.begin_line,
            github_escape_data(&v.description),
            github_escape_data(&v.rule_name),
            if v.suppressed { ", suppressed" } else { "" }
        )?;
    }
    for e in &report.errors {
        writeln!(
            out,
            "::error file={}::{}",
            github_escape_property(&e.file),
            github_escape_data(&e.message)
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod github_tests {
    use super::*;

    #[test]
    fn escapes_annotation_properties_and_data() {
        let report = Report {
            violations: vec![Violation {
                file: "fixture,colon:%\r\n.rs".to_string(),
                begin_line: 1,
                end_line: 1,
                rule_name: "Rule%".to_string(),
                ruleset_name: String::new(),
                description: "message%\r\n".to_string(),
                priority: 1,
                package: String::new(),
                function: String::new(),
                class: String::new(),
                method: String::new(),
                external_info_url: String::new(),
                suppressed: false,
            }],
            errors: vec![ProcessingError {
                file: "error,file:%.rs".to_string(),
                message: "error%\r\n".to_string(),
            }],
        };
        let mut out = Vec::new();

        write_github(&report, &mut out).unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "::warning file=fixture%2Ccolon%3A%25%0D%0A.rs,line=1,col=1::message%25%0D%0A (Rule%25)\n::error file=error%2Cfile%3A%25.rs::error%25%0D%0A\n"
        );
    }
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
    let mut seen = std::collections::BTreeSet::new();
    let mut rules = Vec::new();
    let mut results = Vec::new();
    for v in &report.violations {
        if seen.insert(v.rule_name.clone()) {
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

#[cfg(test)]
mod report_format_tests {
    use super::{ProcessingError, Report, ReportFormat, Violation};

    fn synthetic_report() -> Report {
        Report {
            violations: vec![Violation {
                file: "fixture.rs".to_string(),
                begin_line: 4,
                end_line: 4,
                rule_name: "SyntheticRule".to_string(),
                ruleset_name: "synthetic".to_string(),
                description: "A synthetic finding".to_string(),
                priority: 1,
                package: "fixture".to_string(),
                function: "run".to_string(),
                class: String::new(),
                method: String::new(),
                external_info_url: String::new(),
                suppressed: false,
            }],
            errors: vec![ProcessingError {
                file: "broken.rs".to_string(),
                message: "A synthetic processing error".to_string(),
            }],
        }
    }

    fn render_to_string(format: ReportFormat) -> String {
        let mut output = Vec::new();
        format
            .render(&synthetic_report(), false, &mut output)
            .unwrap();
        String::from_utf8(output).unwrap()
    }

    fn assert_renders_to_memory(format: ReportFormat) {
        let output = render_to_string(format);
        assert!(output.contains("fixture.rs"), "format={format:?}: {output}");
    }

    #[test]
    fn every_format_parses_from_its_name() {
        for format in ReportFormat::all() {
            assert_eq!(ReportFormat::parse(format.name()), Some(*format));
        }
        assert_eq!(ReportFormat::parse("unknown"), None);
    }

    #[test]
    fn text_renders_to_memory() {
        assert_renders_to_memory(ReportFormat::Text);
    }

    #[test]
    fn ansi_renders_to_memory() {
        assert_renders_to_memory(ReportFormat::Ansi);
    }

    #[test]
    fn json_renders_a_valid_document_to_memory() {
        let output = render_to_string(ReportFormat::Json);
        let document: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(document["package"], "messrust");
        assert_eq!(document["files"][0]["file"], "fixture.rs");
        assert_eq!(
            document["files"][0]["violations"][0]["rule"],
            "SyntheticRule"
        );
        assert_eq!(document["files"][0]["violations"][0]["beginLine"], 4);
        assert_eq!(document["errors"][0]["fileName"], "broken.rs");
        assert_eq!(
            document["errors"][0]["message"],
            "A synthetic processing error"
        );
    }

    #[test]
    fn xml_renders_a_valid_document_to_memory() {
        let output = render_to_string(ReportFormat::Xml);
        let document = roxmltree::Document::parse(&output).unwrap();
        let root = document.root_element();
        let violation = root
            .descendants()
            .find(|node| node.has_tag_name("violation"))
            .unwrap();
        let error = root
            .descendants()
            .find(|node| node.has_tag_name("error"))
            .unwrap();

        assert_eq!(root.tag_name().name(), "pmd");
        let file = violation
            .ancestors()
            .find(|node| node.has_tag_name("file"))
            .unwrap();
        assert_eq!(file.attribute("name"), Some("fixture.rs"));
        assert_eq!(violation.attribute("rule"), Some("SyntheticRule"));
        assert_eq!(violation.attribute("beginline"), Some("4"));
        assert_eq!(error.attribute("filename"), Some("broken.rs"));
        assert_eq!(error.attribute("msg"), Some("A synthetic processing error"));
    }

    #[test]
    fn html_renders_to_memory() {
        assert_renders_to_memory(ReportFormat::Html);
    }

    #[test]
    fn github_renders_to_memory() {
        assert_renders_to_memory(ReportFormat::Github);
    }

    #[test]
    fn gitlab_renders_to_memory() {
        assert_renders_to_memory(ReportFormat::Gitlab);
    }

    #[test]
    fn checkstyle_renders_to_memory() {
        assert_renders_to_memory(ReportFormat::Checkstyle);
    }

    #[test]
    fn sarif_renders_a_valid_document_to_memory() {
        let output = render_to_string(ReportFormat::Sarif);
        let document: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(document["version"], "2.1.0");
        assert_eq!(document["runs"][0]["results"][0]["ruleId"], "SyntheticRule");
        assert_eq!(
            document["runs"][0]["results"][0]["message"]["text"],
            "A synthetic finding"
        );
        assert_eq!(
            document["runs"][0]["results"][0]["locations"][0]["physicalLocation"]
                ["artifactLocation"]["uri"],
            "fixture.rs"
        );
        assert_eq!(
            document["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]
                ["startLine"],
            4
        );
    }
}
