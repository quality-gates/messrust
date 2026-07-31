//! Report model, text renderer, and exit-code mapping.

use std::io::Write;
use std::path::PathBuf;

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
    pub rule_name: String,
    pub description: String,
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

pub fn render_text(
    report: &Report,
    target: WriteTarget,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    match target {
        WriteTarget::Stdout => {
            write_text(report, stdout).map_err(|e| e.to_string())?;
        }
        WriteTarget::File(path) => {
            let mut f = std::fs::File::create(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            write_text(report, &mut f).map_err(|e| e.to_string())?;
            // Leave stdout empty.
        }
    }
    Ok(())
}

fn write_text(report: &Report, out: &mut dyn Write) -> std::io::Result<()> {
    const SPACING: usize = 2;

    let locations: Vec<String> = report
        .violations
        .iter()
        .map(|v| format!("{}:{}", v.file, v.begin_line))
        .collect();
    let loc_width = locations.iter().map(|s| s.len()).max().unwrap_or(0);
    let rule_width = report
        .violations
        .iter()
        .map(|v| v.rule_name.len())
        .max()
        .unwrap_or(0);

    for (v, loc) in report.violations.iter().zip(locations.iter()) {
        let pad1 = " ".repeat(loc_width.saturating_sub(loc.len()) + SPACING);
        let pad2 = " ".repeat(rule_width.saturating_sub(v.rule_name.len()) + SPACING);
        writeln!(
            out,
            "{loc}{pad1}{}{pad2}{}",
            v.rule_name, v.description
        )?;
    }

    for err in &report.errors {
        writeln!(out, "{}\t-\t{}", err.file, err.message)?;
    }
    Ok(())
}
