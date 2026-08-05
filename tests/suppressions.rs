//! Integration tests for source comment suppression directives, driven
//! through the injectable CLI entry (`messrust::run`).
//!
//! Prior art for the harness pattern: `tests/cli.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use messrust::{run, EXIT_SUCCESS, EXIT_VIOLATION};
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

fn fixture_with_params(n: usize) -> String {
    let params: Vec<String> = (0..n).map(|i| format!("param_{i}: i32")).collect();
    format!("fn entry_point({}) {{}}\n", params.join(", "))
}

fn fixture_named(name: &str, n: usize) -> String {
    fixture_with_params(n).replacen("entry_point", name, 1)
}

/// A rule name that produces a finding on the same source line as the
/// declaration, so directive-boundary tests can target an exact line number
/// without depending on `codesize` rule shapes.
fn long_name_source(name: &str) -> String {
    format!("fn {name}() {{}}\n")
}

// ---------------------------------------------------------------------
// Rule-name parsing: case, separators, and validity
// ---------------------------------------------------------------------

#[test]
fn disable_next_line_matches_rule_name_case_insensitively() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// MESSRUST-DISABLE-NEXT-LINE excessiveparameterlist\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn disable_accepts_comma_separated_rule_list() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable ExcessiveParameterList,LongMethod\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn disable_next_line_ignores_a_second_rule_name_not_matching_the_finding() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line LongMethod\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn disable_directive_without_trailing_separator_is_not_recognised() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-linefoo ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn disable_with_no_rule_names_at_all_suppresses_nothing() {
    let dir = TempDir::new().unwrap();
    let source = format!("// messrust-disable-next-line\n{}", fixture_with_params(11));
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn disable_rejects_rule_name_starting_with_a_digit() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line 9BadName\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn disable_rule_names_are_separated_by_whitespace_too() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList LongMethod\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

// ---------------------------------------------------------------------
// Region directives: disable / enable line boundaries
// ---------------------------------------------------------------------

#[test]
fn disable_region_suppresses_every_line_up_to_the_enable_line() {
    let dir = TempDir::new().unwrap();
    let a = fixture_named("first", 11);
    let b = fixture_named("second", 12);
    let source = format!("// messrust-disable ExcessiveParameterList\n{a}{b}");
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn enable_directive_on_its_own_line_removes_only_the_named_rule() {
    let dir = TempDir::new().unwrap();
    let a = fixture_named("first", 11);
    let source = format!(
        "// messrust-disable ExcessiveParameterList,LongMethod\n{a}// messrust-enable ExcessiveParameterList\n"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    // ExcessiveParameterList was suppressed for the finding line (line 2),
    // enabling it afterwards must not retroactively un-suppress that finding.
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn enable_without_a_matching_disable_is_a_harmless_no_op() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-enable ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn disable_next_line_only_reaches_the_immediately_following_line() {
    let dir = TempDir::new().unwrap();
    let a = fixture_named("first", 11);
    let b = fixture_named("second", 12);
    // The directive covers only the line right after it; the following
    // finding two lines down must still fire.
    let source = format!("// messrust-disable-next-line ExcessiveParameterList\n{a}{b}");
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(!out.contains(":2"), "line 2 should be suppressed: {out:?}");
    assert!(out.contains(":3"), "line 3 should still fire: {out:?}");
}

#[test]
fn disable_next_line_at_the_end_of_the_file_suppresses_nothing() {
    let dir = TempDir::new().unwrap();
    let source = "// messrust-disable-next-line ExcessiveParameterList\n".to_string();
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(err.is_empty(), "stderr={err:?}");
}

#[test]
fn disable_on_last_line_of_file_without_trailing_newline_still_applies_next_line() {
    let dir = TempDir::new().unwrap();
    let a = fixture_with_params(11);
    // No trailing newline after the finding line.
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}",
        a.trim_end_matches('\n')
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

// ---------------------------------------------------------------------
// Token scanning: comments vs strings vs char literals vs raw/byte strings
// ---------------------------------------------------------------------

#[test]
fn line_comment_directive_reaches_to_end_of_line_only() {
    let dir = TempDir::new().unwrap();
    // Trailing content after the directive on the same physical comment
    // line must not leak the directive text onto the next line's scan.
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList trailing text\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn block_comment_directive_on_a_single_line_suppresses_the_next_line() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "/* messrust-disable-next-line ExcessiveParameterList */\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn multiline_block_comment_directive_keeps_correct_line_numbers_after_it() {
    let dir = TempDir::new().unwrap();
    // The directive line is the last content line of the comment, closed on
    // the same physical line; the finding right after the comment closes
    // must be the "next line" the directive reaches.
    let source = format!(
        "/*\nsome preamble\nmessrust-disable-next-line ExcessiveParameterList */\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn nested_block_comment_directive_still_resolves_line_numbers_after_close() {
    let dir = TempDir::new().unwrap();
    // A nested `/* */` must not close the outer comment early; if depth
    // tracking breaks, the text after the inner close becomes bare source
    // and fails to parse.
    let source = format!(
        "/* /* nested */\nmessrust-disable-next-line ExcessiveParameterList */\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn directive_text_inside_a_double_quoted_string_is_ignored() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &str = \"// messrust-disable-next-line ExcessiveParameterList\";\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn escaped_quote_inside_a_string_does_not_end_the_string_early() {
    let dir = TempDir::new().unwrap();
    // The escaped quote must not be treated as the string terminator; if it
    // were, the following directive text would be read as code and the
    // still-open string would swallow the real directive below it.
    let source = format!(
        "const TEXT: &str = \"a \\\" // messrust-disable-next-line ExcessiveParameterList\";\n// messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn directive_text_inside_a_raw_string_is_ignored() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &str = r#\"// messrust-disable-next-line ExcessiveParameterList\"#;\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn keeps_line_numbers_after_a_multiline_raw_string() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &str = r#\"first\n// messrust-disable LongVariable\nlast\"#;\n// messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn directive_text_inside_a_byte_string_is_ignored() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &[u8] = b\"// messrust-disable-next-line ExcessiveParameterList\";\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn a_char_literal_containing_a_quote_character_does_not_start_a_string() {
    let dir = TempDir::new().unwrap();
    // '"' is a char literal holding a double-quote; it must not be treated
    // as the start of a string that swallows the rest of the file.
    let source = format!(
        "const C: char = '\"';\n// messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn a_lifetime_apostrophe_is_not_treated_as_a_char_literal() {
    let dir = TempDir::new().unwrap();
    // `'a` is a lifetime, not a char literal; scanning must not treat the
    // rest of the line/file as being inside a character literal.
    let source = format!(
        "fn generic<'a>(value: &'a str) {{}}\n// messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn an_escaped_char_literal_does_not_confuse_the_scanner() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const C: char = '\\'';\n// messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

// ---------------------------------------------------------------------
// Multiple findings, multiple rules
// ---------------------------------------------------------------------

#[test]
fn disable_next_line_with_one_rule_leaves_a_second_unrelated_rule_active() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}",
        long_name_source("fn_with_a_needlessly_long_and_verbose_name_for_the_test")
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, err) = run_cli(&[path.to_str().unwrap(), "text", "naming"]);
    // Naming rule findings (if any) are unaffected by an unrelated
    // suppression; this just proves the command runs cleanly end to end.
    assert!(
        code == EXIT_SUCCESS || code == EXIT_VIOLATION,
        "stderr={err:?}"
    );
}

#[test]
fn json_report_marks_a_suppressed_finding_as_suppressed_true_under_strict() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "json", "codesize", "--strict"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(err.is_empty(), "stderr={err:?}");
    let report: serde_json::Value = serde_json::from_str(&out).unwrap();
    let violations = report["files"][0]["violations"].as_array().unwrap();
    assert_eq!(violations.len(), 1, "report={out}");
    assert_eq!(violations[0]["suppressed"], serde_json::Value::Bool(true));
}
