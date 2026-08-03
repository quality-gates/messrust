//! Exact report-format documents through `messrust::run`.
//!
//! A format is what a user reads. These tests assert the full document shape
//! for text, json, sarif, github, and the other family formats — including
//! the empty-finding case and `--reportfile` output.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use messrust::{run, EXIT_ERROR, EXIT_SUCCESS, EXIT_VIOLATION};
use serde_json::json;
use tempfile::TempDir;

fn run_cli(args: &[&str]) -> (i32, String, String) {
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run(&args, &mut out, &mut err);
    (
        code,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

fn write_file(dir: &Path, rel: &str, contents: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, contents).unwrap();
    path
}

fn fn_with_n_params(name: &str, n: usize) -> String {
    let params: Vec<String> = (0..n).map(|i| format!("param_{i}: i32")).collect();
    format!("fn {name}({}) {{}}\n", params.join(", "))
}

fn param_list_message(name: &str, n: usize) -> String {
    format!(
        "The function {name} has {n} parameters. Consider reducing the number of parameters to less than 10."
    )
}

fn priority_ruleset(dir: &Path, priority: u8) -> PathBuf {
    let path = dir.join(format!("prio_{priority}.xml"));
    fs::write(
        &path,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="Prio">
  <description>priority override</description>
  <rule ref="codesize/ExcessiveParameterList">
    <priority>{priority}</priority>
  </rule>
</ruleset>
"#
        ),
    )
    .unwrap();
    path
}

fn assert_recent_timestamp(ts: &str) {
    let parts: Vec<&str> = ts.split(|c| c == '-' || c == 'T' || c == ':' || c == 'Z').collect();
    assert_eq!(
        parts.len(),
        7,
        "timestamp must be YYYY-MM-DDTHH:MM:SSZ, got {ts:?}"
    );
    assert!(
        parts.iter().take(6).all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
        "timestamp digits: {ts:?}"
    );
    assert_eq!(parts[0].len(), 4, "year width: {ts:?}");
    assert_eq!(parts[1].len(), 2, "month width: {ts:?}");
    assert_eq!(parts[2].len(), 2, "day width: {ts:?}");
    assert_eq!(parts[3].len(), 2, "hour width: {ts:?}");
    assert_eq!(parts[4].len(), 2, "minute width: {ts:?}");
    assert_eq!(parts[5].len(), 2, "second width: {ts:?}");
    assert!(ts.ends_with('Z'), "timestamp={ts:?}");

    let y: i32 = parts[0].parse().unwrap();
    let m: u32 = parts[1].parse().unwrap();
    let d: u32 = parts[2].parse().unwrap();
    let hh: u32 = parts[3].parse().unwrap();
    let mm: u32 = parts[4].parse().unwrap();
    let ss: u32 = parts[5].parse().unwrap();
    assert!((1..=12).contains(&m), "month={m}");
    assert!((1..=31).contains(&d), "day={d}");
    assert!(hh < 24, "hour={hh}");
    assert!(mm < 60, "minute={mm}");
    assert!(ss < 60, "second={ss}");

    // Civil date back to days-since-epoch (inverse of Howard Hinnant), then compare
    // to wall clock so a broken calendar conversion cannot hide as a valid stamp.
    let days = days_from_civil(y, m, d);
    let stamp_secs = days * 86400 + i64::from(hh * 3600 + mm * 60 + ss);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(
        (now - stamp_secs).abs() <= 120,
        "timestamp {ts} (unix {stamp_secs}) not within 120s of now ({now})"
    );
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let mut y = y;
    let m = m as i32;
    let d = d as i32;
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u32;
    era as i64 * 146097 + doe as i64 - 719468
}

fn assert_four_space_json(out: &str) {
    // messgo re-indent doubles serde's 2-space pretty print to 4 spaces per level.
    assert!(
        out.lines().any(|l| l.starts_with("    \"") || l.starts_with("        \"")),
        "expected 4-space indent: {out}"
    );
    assert!(
        !out.lines().any(|l| {
            let spaces = l.len() - l.trim_start_matches(' ').len();
            spaces == 2 && l.trim_start().starts_with('"')
        }),
        "unexpected 2-space indent: {out}"
    );
}

fn assert_two_space_json(out: &str) {
    assert!(
        out.lines().any(|l| l.starts_with("  \"")),
        "expected 2-space indent: {out}"
    );
}

fn gitlab_fingerprint(file: &str, line: usize, rule: &str) -> String {
    let raw = format!("{file}:{line}:{rule}");
    raw.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

// ----- text / ansi / color ------------------------------------------------

#[test]
fn text_format_prints_exact_aligned_violation_line() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();
    let (code, out, err) = run_cli(&[file, "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let expected = format!(
        "{file}:1  ExcessiveParameterList  {}\n",
        param_list_message("entry_point", 11)
    );
    assert_eq!(out, expected);
}

#[test]
fn text_format_empty_findings_prints_nothing_and_exits_zero() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fn_with_n_params("ok", 0));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    assert_eq!(out, "");
}

#[test]
fn text_format_pads_columns_across_unequal_path_lengths() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "a.rs", &fn_with_n_params("a", 11));
    write_file(dir.path(), "longer_name.rs", &fn_with_n_params("longer", 12));
    let (code, out, err) = run_cli(&[dir.path().to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");

    let short = dir.path().join("a.rs");
    let long = dir.path().join("longer_name.rs");
    let short_loc = format!("{}:1", short.display());
    let long_loc = format!("{}:1", long.display());
    let loc_width = short_loc.len().max(long_loc.len());
    let rule = "ExcessiveParameterList";
    let pad = |loc: &str| " ".repeat(loc_width - loc.len() + 2);
    let rule_pad = "  ";
    let expected = format!(
        "{short_loc}{p1}{rule}{rule_pad}{}\n{long_loc}{p2}{rule}{rule_pad}{}\n",
        param_list_message("a", 11),
        param_list_message("longer", 12),
        p1 = pad(&short_loc),
        p2 = pad(&long_loc),
    );
    assert_eq!(out, expected);
}

#[test]
fn text_format_marks_suppressed_rules_and_prints_errors_with_tabs() {
    let dir = TempDir::new().unwrap();
    let suppressed = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}",
        fn_with_n_params("entry_point", 11)
    );
    write_file(dir.path(), "sup.rs", &suppressed);
    write_file(dir.path(), "bad.rs", "fn broken( {\n");
    let (code, out, err) = run_cli(&[
        dir.path().to_str().unwrap(),
        "text",
        "codesize",
        "--strict",
    ]);
    assert_eq!(code, EXIT_ERROR, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    assert!(
        out.contains("ExcessiveParameterList [suppressed]"),
        "stdout={out:?}"
    );
    let bad = dir.path().join("bad.rs");
    assert!(
        out.contains(&format!(
            "{}\t-\tcannot parse string into token stream",
            bad.display()
        )),
        "stdout={out:?}"
    );
}

#[test]
fn ansi_format_uses_exact_color_codes_around_rule_and_message() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();
    let (code, out, err) = run_cli(&[file, "ansi", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let expected = format!(
        "{file}:1  \u{1b}[33mExcessiveParameterList\u{1b}[0m  \u{1b}[31m{}\u{1b}[0m\n",
        param_list_message("entry_point", 11)
    );
    assert_eq!(out, expected);
}

#[test]
fn color_flag_on_text_matches_ansi_document() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();
    let (code_ansi, out_ansi, err_ansi) = run_cli(&[file, "ansi", "codesize"]);
    let (code_color, out_color, err_color) = run_cli(&[file, "text", "codesize", "--color"]);
    assert_eq!(code_ansi, EXIT_VIOLATION, "stderr={err_ansi:?}");
    assert_eq!(code_color, EXIT_VIOLATION, "stderr={err_color:?}");
    assert!(err_ansi.is_empty() && err_color.is_empty());
    assert_eq!(out_color, out_ansi);
    assert!(!run_cli(&[file, "text", "codesize"]).1.contains('\u{1b}'));
}

// ----- github -------------------------------------------------------------

#[test]
fn github_format_prints_exact_warning_annotation() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();
    let (code, out, err) = run_cli(&[file, "github", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let expected = format!(
        "::warning file={file},line=1,col=1::{} (ExcessiveParameterList)\n",
        param_list_message("entry_point", 11)
    );
    assert_eq!(out, expected);
}

#[test]
fn github_format_empty_findings_prints_nothing() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fn_with_n_params("ok", 0));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "github", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert_eq!(out, "");
    assert!(err.is_empty(), "stderr={err:?}");
}

#[test]
fn github_format_marks_suppressed_and_emits_error_annotations() {
    let dir = TempDir::new().unwrap();
    let suppressed = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}",
        fn_with_n_params("entry_point", 11)
    );
    let path = write_file(dir.path(), "sup.rs", &suppressed);
    let bad = write_file(dir.path(), "bad.rs", "fn broken( {\n");
    let (code, out, err) = run_cli(&[
        dir.path().to_str().unwrap(),
        "github",
        "codesize",
        "--strict",
    ]);
    assert_eq!(code, EXIT_ERROR, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let warning = format!(
        "::warning file={},line=2,col=1::{} (ExcessiveParameterList, suppressed)\n",
        path.display(),
        param_list_message("entry_point", 11)
    );
    let error = format!(
        "::error file={}::cannot parse string into token stream\n",
        bad.display()
    );
    assert!(out.contains(&warning), "stdout={out:?}");
    assert!(out.contains(&error), "stdout={out:?}");
}

// ----- json ---------------------------------------------------------------

#[test]
fn json_format_prints_exact_document_for_one_violation() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();
    let (code, out, err) = run_cli(&[file, "json", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    assert_four_space_json(&out);
    assert!(out.ends_with('\n'), "json must end with newline");

    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["version"], "0.1.0");
    assert_eq!(v["package"], "messrust");
    assert_recent_timestamp(v["timestamp"].as_str().unwrap());
    assert_eq!(
        v["files"],
        json!([{
            "file": file,
            "violations": [{
                "beginLine": 1,
                "endLine": 1,
                "package": "",
                "function": "entry_point",
                "class": "",
                "method": "",
                "description": param_list_message("entry_point", 11),
                "rule": "ExcessiveParameterList",
                "ruleSet": "Code Size Rules",
                "externalInfoUrl": "",
                "priority": 3,
                "suppressed": false
            }]
        }])
    );
    assert!(v.get("errors").is_none(), "empty errors must be omitted: {out}");
}

#[test]
fn json_format_empty_findings_is_skeleton_with_empty_files() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fn_with_n_params("ok", 0));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "json", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    assert_four_space_json(&out);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["version"], "0.1.0");
    assert_eq!(v["package"], "messrust");
    assert_recent_timestamp(v["timestamp"].as_str().unwrap());
    assert_eq!(v["files"], json!([]));
    assert!(v.get("errors").is_none(), "report={out}");
}

#[test]
fn json_format_includes_errors_and_groups_files_in_first_seen_order() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "z.rs", &fn_with_n_params("late", 11));
    write_file(dir.path(), "a.rs", &fn_with_n_params("early", 12));
    write_file(dir.path(), "bad.rs", "fn broken( {\n");
    let (code, out, err) = run_cli(&[dir.path().to_str().unwrap(), "json", "codesize"]);
    assert_eq!(code, EXIT_ERROR, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0]["file"].as_str().unwrap().ends_with("a.rs"));
    assert!(files[1]["file"].as_str().unwrap().ends_with("z.rs"));
    let errors = v["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0]["fileName"].as_str().unwrap().ends_with("bad.rs"));
    assert_eq!(
        errors[0]["message"],
        "cannot parse string into token stream"
    );
}

#[test]
fn json_format_groups_two_violations_in_the_same_file() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "fixture.rs",
        &(fn_with_n_params("first", 11) + &fn_with_n_params("second", 12)),
    );
    let file = path.to_str().unwrap();
    let (code, out, err) = run_cli(&[file, "json", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "same file must be one entry: {out}");
    assert_eq!(files[0]["file"], file);
    assert_eq!(files[0]["violations"].as_array().unwrap().len(), 2);
    assert_eq!(files[0]["violations"][0]["function"], "first");
    assert_eq!(files[0]["violations"][1]["function"], "second");
}

#[test]
fn color_flag_does_not_rewrite_non_text_formats() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();

    let (code, out, err) = run_cli(&[file, "github", "codesize", "--color"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    assert_eq!(
        out,
        format!(
            "::warning file={file},line=1,col=1::{} (ExcessiveParameterList)\n",
            param_list_message("entry_point", 11)
        )
    );

    let (code, out, err) = run_cli(&[file, "json", "codesize", "--color"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    assert!(!out.contains('\u{1b}'), "json must stay uncolored: {out}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["package"], "messrust");
    assert_eq!(v["files"][0]["violations"][0]["rule"], "ExcessiveParameterList");
}

#[test]
fn json_format_marks_suppressed_true_under_strict() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}",
        fn_with_n_params("entry_point", 11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "json", "codesize", "--strict"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["files"][0]["violations"][0]["suppressed"], true);
}

#[test]
fn json_format_carries_class_and_method_for_impl_methods() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "fixture.rs",
        "struct S;\nimpl S {\n  fn m(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32, p6: i32, p7: i32, p8: i32, p9: i32, p10: i32) {}\n}\n",
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "json", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let violation = &v["files"][0]["violations"][0];
    assert_eq!(violation["class"], "S");
    assert_eq!(violation["method"], "m");
    assert_eq!(violation["function"], "");
}

// ----- sarif --------------------------------------------------------------

#[test]
fn sarif_format_prints_exact_document_for_one_violation() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();
    let (code, out, err) = run_cli(&[file, "sarif", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    assert_two_space_json(&out);
    assert!(out.ends_with('\n'));

    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v["$schema"],
        "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json"
    );
    assert_eq!(v["version"], "2.1.0");
    let run = &v["runs"][0];
    assert_eq!(run["tool"]["driver"]["name"], "messrust");
    assert_eq!(run["tool"]["driver"]["version"], "0.1.0");
    assert_eq!(
        run["tool"]["driver"]["rules"],
        json!([{
            "id": "ExcessiveParameterList",
            "name": "ExcessiveParameterList",
            "shortDescription": { "text": "ExcessiveParameterList" }
        }])
    );
    assert_eq!(
        run["results"],
        json!([{
            "ruleId": "ExcessiveParameterList",
            "level": "warning",
            "message": { "text": param_list_message("entry_point", 11) },
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": { "uri": file },
                    "region": { "startLine": 1, "endLine": 1 }
                }
            }],
            "properties": { "priority": 3, "suppressed": false }
        }])
    );
}

#[test]
fn sarif_format_empty_findings_has_empty_rules_and_results() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fn_with_n_params("ok", 0));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "sarif", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["version"], "2.1.0");
    assert_eq!(v["runs"][0]["results"], json!([]));
    assert_eq!(v["runs"][0]["tool"]["driver"]["rules"], json!([]));
    assert_eq!(v["runs"][0]["tool"]["driver"]["name"], "messrust");
}

#[test]
fn sarif_format_adds_suppressions_block_when_strict() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}",
        fn_with_n_params("entry_point", 11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "sarif", "codesize", "--strict"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let result = &v["runs"][0]["results"][0];
    assert_eq!(result["properties"]["suppressed"], true);
    assert_eq!(result["suppressions"], json!([{ "kind": "inSource" }]));
}

#[test]
fn sarif_level_is_error_for_priority_one_and_two() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    for priority in [1_u8, 2] {
        let rules = priority_ruleset(dir.path(), priority);
        let (code, out, err) = run_cli(&[
            path.to_str().unwrap(),
            "sarif",
            rules.to_str().unwrap(),
        ]);
        assert_eq!(code, EXIT_VIOLATION, "p={priority} stderr={err:?}");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["runs"][0]["results"][0]["level"],
            "error",
            "priority={priority}"
        );
        assert_eq!(
            v["runs"][0]["results"][0]["properties"]["priority"],
            priority
        );
    }
}

#[test]
fn sarif_dedupes_rule_metadata_across_two_findings_of_same_rule() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "fixture.rs",
        &(fn_with_n_params("first", 11) + &fn_with_n_params("second", 12)),
    );
    let (code, out, err) = run_cli(&[dir.path().join("fixture.rs").to_str().unwrap(), "sarif", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["runs"][0]["tool"]["driver"]["rules"].as_array().unwrap().len(), 1);
    assert_eq!(v["runs"][0]["results"].as_array().unwrap().len(), 2);
}

// ----- reportfile ---------------------------------------------------------

#[test]
fn reportfile_writes_exact_text_document_and_leaves_stdout_empty() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let report = dir.path().join("out.txt");
    let file = path.to_str().unwrap();
    let (code, out, err) = run_cli(&[
        file,
        "text",
        "codesize",
        "--reportfile",
        report.to_str().unwrap(),
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_eq!(out, "");
    assert!(err.is_empty(), "stderr={err:?}");
    let body = fs::read_to_string(&report).unwrap();
    let expected = format!(
        "{file}:1  ExcessiveParameterList  {}\n",
        param_list_message("entry_point", 11)
    );
    assert_eq!(body, expected);
}

#[test]
fn reportfile_writes_exact_json_sarif_and_github_documents() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();

    for format in ["json", "sarif", "github"] {
        let report = dir.path().join(format!("out.{format}"));
        let (code, out, err) = run_cli(&[
            file,
            format,
            "codesize",
            "--reportfile",
            report.to_str().unwrap(),
        ]);
        assert_eq!(code, EXIT_VIOLATION, "format={format} stderr={err:?}");
        assert_eq!(out, "", "format={format}");
        assert!(err.is_empty(), "format={format} stderr={err:?}");
        let body = fs::read_to_string(&report).unwrap();
        let (code2, stdout_body, _) = run_cli(&[file, format, "codesize"]);
        assert_eq!(code2, EXIT_VIOLATION);
        if format == "json" {
            // Timestamps may differ by a second; compare structure.
            let a: serde_json::Value = serde_json::from_str(&body).unwrap();
            let b: serde_json::Value = serde_json::from_str(&stdout_body).unwrap();
            assert_eq!(a["files"], b["files"]);
            assert_eq!(a["package"], "messrust");
            assert_recent_timestamp(a["timestamp"].as_str().unwrap());
        } else {
            assert_eq!(body, stdout_body, "format={format}");
        }
    }
}

#[test]
fn reportfile_missing_parent_directory_exits_one_on_stderr() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let missing = dir.path().join("no/such/dir/out.txt");
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--reportfile",
        missing.to_str().unwrap(),
    ]);
    assert_eq!(code, EXIT_ERROR);
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        err.contains("error:") && err.contains("no/such/dir/out.txt"),
        "stderr={err:?}"
    );
}

// ----- xml / html / gitlab / checkstyle -----------------------------------

#[test]
fn xml_format_prints_pmd_document_with_escaped_paths() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "a&b<>\".rs",
        &fn_with_n_params("f", 11),
    );
    // Also cover apostrophe escape.
    let path2 = write_file(dir.path(), "o'brien.rs", &fn_with_n_params("g", 11));
    let (code, out, err) = run_cli(&[dir.path().to_str().unwrap(), "xml", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n"));
    assert!(out.contains("tool=\"messrust\""));
    assert!(out.contains(&format!("version=\"{}\"", env!("CARGO_PKG_VERSION"))));
    let ts_start = out.find("timestamp=\"").unwrap() + "timestamp=\"".len();
    let ts_end = out[ts_start..].find('"').unwrap() + ts_start;
    assert_recent_timestamp(&out[ts_start..ts_end]);
    assert!(out.contains(&format!(
        "<file name=\"{}\">",
        path.to_str().unwrap().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
    )));
    assert!(out.contains(&format!(
        "<file name=\"{}\">",
        path2.to_str().unwrap().replace('\'', "&#039;")
    )));
    assert!(out.contains("beginline=\"1\""));
    assert!(out.contains("endline=\"1\""));
    assert!(out.contains("rule=\"ExcessiveParameterList\""));
    assert!(out.contains("ruleset=\"Code Size Rules\""));
    assert!(out.contains("function=\"f\""));
    assert!(out.contains("function=\"g\""));
    assert!(out.contains("priority=\"3\""));
    assert!(out.contains("suppressed=\"false\""));
    assert!(!out.contains(" package="), "empty package must be omitted");
    assert!(!out.contains(" externalInfoUrl="), "empty url must be omitted");
    assert!(out.ends_with("</pmd>\n"));
}

#[test]
fn xml_format_closes_each_file_element_in_a_multi_file_report() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "a.rs", &fn_with_n_params("a", 11));
    write_file(dir.path(), "b.rs", &fn_with_n_params("b", 12));
    let (code, out, err) = run_cli(&[dir.path().to_str().unwrap(), "xml", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let a = dir.path().join("a.rs");
    let b = dir.path().join("b.rs");
    let expected_files = format!(
        "  <file name=\"{}\">\n    <violation beginline=\"1\" endline=\"1\" rule=\"ExcessiveParameterList\" ruleset=\"Code Size Rules\" function=\"a\" priority=\"3\" suppressed=\"false\">\n      {}\n    </violation>\n  </file>\n  <file name=\"{}\">\n    <violation beginline=\"1\" endline=\"1\" rule=\"ExcessiveParameterList\" ruleset=\"Code Size Rules\" function=\"b\" priority=\"3\" suppressed=\"false\">\n      {}\n    </violation>\n  </file>\n</pmd>\n",
        a.display(),
        param_list_message("a", 11),
        b.display(),
        param_list_message("b", 12),
    );
    assert!(
        out.ends_with(&expected_files),
        "multi-file xml must close each file element:\n{out}"
    );
    assert_eq!(out.matches("</file>").count(), 2, "out={out}");
    assert_eq!(out.matches("<file ").count(), 2, "out={out}");
}

#[test]
fn xml_format_empty_findings_is_shell_only() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fn_with_n_params("ok", 0));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "xml", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n"));
    assert!(out.contains("<pmd version=\"0.1.0\" tool=\"messrust\" timestamp=\""));
    assert!(out.ends_with("\">\n</pmd>\n") || out.contains("\">\n</pmd>\n"));
    assert!(!out.contains("<file "));
    assert!(!out.contains("<error "));
}

#[test]
fn xml_format_emits_error_elements_and_method_attrs() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "bad.rs", "fn broken( {\n");
    write_file(
        dir.path(),
        "method.rs",
        "struct S;\nimpl S {\n  fn m(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32, p6: i32, p7: i32, p8: i32, p9: i32, p10: i32) {}\n}\n",
    );
    let (code, out, err) = run_cli(&[dir.path().to_str().unwrap(), "xml", "codesize"]);
    assert_eq!(code, EXIT_ERROR, "stderr={err:?}");
    assert!(out.contains("class=\"S\""));
    assert!(out.contains("method=\"m\""));
    assert!(out.contains("<error filename=\""));
    assert!(out.contains("msg=\"cannot parse string into token stream\""));
}

#[test]
fn html_format_prints_exact_table_document() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();
    let (code, out, err) = run_cli(&[file, "html", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let expected = format!(
        "<!DOCTYPE html>\n\
<html><head><meta charset=\"utf-8\"><title>messrust report</title></head><body>\n\
<h1>messrust report</h1>\n\
<h2>{file}</h2>\n\
<table border=\"1\" cellspacing=\"0\" cellpadding=\"3\">\n\
<tr><th>Line</th><th>Rule</th><th>Suppressed</th><th>Description</th></tr>\n\
<tr><td>1</td><td>ExcessiveParameterList</td><td>false</td><td>{}</td></tr>\n\
</table>\n\
</body></html>\n",
        param_list_message("entry_point", 11)
    );
    assert_eq!(out, expected);
}

#[test]
fn html_format_empty_findings_omits_table() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fn_with_n_params("ok", 0));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "html", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    let expected = "\
<!DOCTYPE html>\n\
<html><head><meta charset=\"utf-8\"><title>messrust report</title></head><body>\n\
<h1>messrust report</h1>\n\
</body></html>\n";
    assert_eq!(out, expected);
}

#[test]
fn html_format_escapes_special_characters_in_path() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "a&b.rs", &fn_with_n_params("f", 11));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "html", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    let escaped = path.to_str().unwrap().replace('&', "&amp;");
    assert!(out.contains(&format!("<h2>{escaped}</h2>")), "out={out}");
    assert!(
        !out.contains(&format!("<h2>{}</h2>", path.display())),
        "raw ampersand must not appear in h2: {out}"
    );
}

#[test]
fn gitlab_format_prints_exact_issue_array_with_fingerprint() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();
    let (code, out, err) = run_cli(&[file, "gitlab", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    assert_four_space_json(&out);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v,
        json!([{
            "type": "issue",
            "check_name": "ExcessiveParameterList",
            "description": param_list_message("entry_point", 11),
            "fingerprint": gitlab_fingerprint(file, 1, "ExcessiveParameterList"),
            "severity": "major",
            "suppressed": false,
            "location": {
                "path": file,
                "lines": { "begin": 1 }
            }
        }])
    );
}

#[test]
fn gitlab_format_empty_findings_is_empty_array() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fn_with_n_params("ok", 0));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "gitlab", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert_eq!(out, "[]\n");
}

#[test]
fn gitlab_severity_maps_each_priority() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let cases = [
        (1_u8, "blocker"),
        (2, "critical"),
        (3, "major"),
        (4, "minor"),
        (5, "info"),
    ];
    for (priority, severity) in cases {
        let rules = priority_ruleset(dir.path(), priority);
        let (code, out, err) = run_cli(&[
            path.to_str().unwrap(),
            "gitlab",
            rules.to_str().unwrap(),
        ]);
        assert_eq!(code, EXIT_VIOLATION, "p={priority} stderr={err:?}");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v[0]["severity"], severity, "priority={priority}");
    }
}

#[test]
fn checkstyle_format_prints_exact_document() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let file = path.to_str().unwrap();
    let (code, out, err) = run_cli(&[file, "checkstyle", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let msg = param_list_message("entry_point", 11);
    let expected = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<checkstyle version=\"0.1.0\">\n  <file name=\"{file}\">\n    <error line=\"1\" column=\"1\" severity=\"warning\" message=\"{msg}\" source=\"Code Size Rules/ExcessiveParameterList\"/>\n  </file>\n</checkstyle>\n"
    );
    assert_eq!(out, expected);
}

#[test]
fn checkstyle_format_empty_findings_is_shell_only() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fn_with_n_params("ok", 0));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "checkstyle", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert_eq!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<checkstyle version=\"0.1.0\">\n\
</checkstyle>\n"
    );
}

#[test]
fn checkstyle_severity_and_suppressed_message_suffix() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fn_with_n_params("entry_point", 11));
    let cases = [(1_u8, "error"), (2, "error"), (3, "warning"), (4, "info"), (5, "info")];
    for (priority, severity) in cases {
        let rules = priority_ruleset(dir.path(), priority);
        let (code, out, err) = run_cli(&[
            path.to_str().unwrap(),
            "checkstyle",
            rules.to_str().unwrap(),
        ]);
        assert_eq!(code, EXIT_VIOLATION, "p={priority} stderr={err:?}");
        assert!(
            out.contains(&format!("severity=\"{severity}\"")),
            "priority={priority} out={out}"
        );
    }

    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}",
        fn_with_n_params("entry_point", 11)
    );
    let path = write_file(dir.path(), "sup.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "checkstyle", "codesize", "--strict"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        out.contains(&format!(
            "message=\"{} [suppressed]\"",
            param_list_message("entry_point", 11)
        )),
        "out={out}"
    );
}

#[test]
fn checkstyle_and_xml_escape_ampersand_in_file_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "x&y.rs", &fn_with_n_params("f", 11));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "checkstyle", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("&amp;"));
    assert!(!out.contains(&format!("name=\"{}\"", path.display())));
}
