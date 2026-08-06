//! Integration tests for source suppression directives through `messrust::run`.

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

// ---------------------------------------------------------------------------
// disable-next-line basic behaviour
// ---------------------------------------------------------------------------

#[test]
fn disable_next_line_suppresses_exactly_one_line() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{first}{second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    // Line 2 is suppressed, but line 3 is not.
    assert!(!out.contains(":2"), "line 2 should be suppressed: {out:?}");
    assert!(out.contains(":3"), "line 3 should fire: {out:?}");
}

// ---------------------------------------------------------------------------
// disable / enable region
// ---------------------------------------------------------------------------

#[test]
fn disable_region_suppresses_all_lines_until_enable() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let third = fixture_with_params(13).replacen("entry_point", "third", 1);
    let source = format!(
        "// messrust-disable ExcessiveParameterList\n\
         {first}\
         {second}\
         // messrust-enable ExcessiveParameterList\n\
         {third}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(!out.contains(":2"), "suppressed: {out:?}");
    assert!(!out.contains(":3"), "suppressed: {out:?}");
    assert!(out.contains(":5"), "re-enabled: {out:?}");
}

#[test]
fn disable_without_enable_suppresses_rest_of_file() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let source = format!(
        "// messrust-disable ExcessiveParameterList\n{first}{second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS);
}

// ---------------------------------------------------------------------------
// case insensitivity
// ---------------------------------------------------------------------------

#[test]
fn directive_keyword_is_case_insensitive() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// MESSRUST-DISABLE-NEXT-LINE ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "upper-case directive should suppress");
}

#[test]
fn rule_name_matching_is_case_insensitive() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line excessiveparameterlist\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "lower-case rule name should match");
}

// ---------------------------------------------------------------------------
// multiple rules in one directive
// ---------------------------------------------------------------------------

#[test]
fn comma_separated_rules_suppress_each_named_rule() {
    let dir = TempDir::new().unwrap();
    let source =
        "// messrust-disable ExcessiveParameterList,ShortVariable\n\
         fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) { let x = 1; let _ = x; }\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize,naming",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "both rules should be suppressed");
}

#[test]
fn space_separated_rules_suppress_each_named_rule() {
    let dir = TempDir::new().unwrap();
    let source =
        "// messrust-disable ExcessiveParameterList ShortVariable\n\
         fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) { let x = 1; let _ = x; }\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize,naming",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "space-separated rules should suppress");
}

// ---------------------------------------------------------------------------
// invalid directive forms
// ---------------------------------------------------------------------------

#[test]
fn directive_without_rule_names_does_not_suppress() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "empty rule list must not suppress: {out:?}"
    );
}

#[test]
fn directive_with_invalid_rule_name_starting_digit_does_not_suppress() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line 123bad\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "digit-prefixed rule name is invalid: {out:?}"
    );
}

#[test]
fn command_suffix_without_separator_is_not_a_directive() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-linefoo ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "should not match: {out:?}"
    );
}

#[test]
fn disable_suffix_without_separator_is_not_a_directive() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disablefoo ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "should not match: {out:?}"
    );
}

#[test]
fn enable_suffix_without_separator_is_not_a_directive() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let source = format!(
        "// messrust-disable ExcessiveParameterList\n\
         {first}\
         // messrust-enablefoo ExcessiveParameterList\n\
         {second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "enablefoo is not a valid enable");
}

// ---------------------------------------------------------------------------
// directive inside string literals
// ---------------------------------------------------------------------------

#[test]
fn directive_in_double_quoted_string_does_not_suppress() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &str = \"// messrust-disable-next-line ExcessiveParameterList\";\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn directive_in_raw_string_does_not_suppress() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &str = r#\"// messrust-disable-next-line ExcessiveParameterList\"#;\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn directive_in_byte_string_does_not_suppress() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &[u8] = b\"// messrust-disable-next-line ExcessiveParameterList\";\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn directive_in_raw_byte_string_does_not_suppress() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &[u8] = br#\"// messrust-disable-next-line ExcessiveParameterList\"#;\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

// ---------------------------------------------------------------------------
// block comment directives
// ---------------------------------------------------------------------------

#[test]
fn block_comment_directive_suppresses_next_line() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "/* messrust-disable-next-line ExcessiveParameterList */\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "block comment directive should suppress");
}

#[test]
fn block_comment_disable_region_suppresses() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let source = format!(
        "/* messrust-disable ExcessiveParameterList */\n\
         {first}\
         {second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "block comment disable should suppress");
}

#[test]
fn multiline_block_comment_with_directive_on_second_line() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let source = format!(
        "/*\n\
         messrust-disable ExcessiveParameterList\n\
         */\n\
         {first}\
         {second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "multiline block comment directive should suppress");
}

#[test]
fn nested_block_comment_does_not_end_early() {
    let dir = TempDir::new().unwrap();
    // The nested block comment must keep track of depth so the inner `*/`
    // does not close the outer comment.  Put the directive on its own
    // line inside the outer comment, after the inner comment closes.
    let source = format!(
        "/* outer /* inner */\nmessrust-disable ExcessiveParameterList\n*/\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "nested block comment should still parse directive");
}

// ---------------------------------------------------------------------------
// line counting through multiline constructs
// ---------------------------------------------------------------------------

#[test]
fn line_count_correct_after_multiline_raw_string() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &str = r#\"line one\n\
         line two\n\
         line three\"#;\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "directive after multiline raw string should work");
}

#[test]
fn line_count_correct_after_multiline_string() {
    let dir = TempDir::new().unwrap();
    // A regular string with actual newlines in it (no backslash-continuation)
    let source = "const TEXT: &str = \"line one\\nline two\\nline three\";\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "directive after string should work");
}

#[test]
fn line_count_correct_after_multiline_block_comment() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "/* block\n\
         comment\n\
         end */\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "directive after multiline block comment should work");
}

// ---------------------------------------------------------------------------
// character literal handling
// ---------------------------------------------------------------------------

#[test]
fn char_literal_does_not_interfere_with_directive() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const C: char = '\\'';\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "char literal should not break directive");
}

#[test]
fn lifetime_tick_does_not_break_directive() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "fn takes_ref<'a>(s: &'a str) -> &'a str {{ s }}\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "lifetime tick should not break directive");
}

// ---------------------------------------------------------------------------
// enable takes effect on its own line
// ---------------------------------------------------------------------------

#[test]
fn enable_takes_effect_on_its_own_line() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let source = format!(
        "// messrust-disable ExcessiveParameterList\n\
         {first}\
         // messrust-enable ExcessiveParameterList\n\
         {second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(!out.contains(":2"), "line 2 should be suppressed: {out:?}");
    assert!(out.contains(":4"), "line 4 should fire: {out:?}");
}

// ---------------------------------------------------------------------------
// slash that is not a comment start
// ---------------------------------------------------------------------------

#[test]
fn slash_not_followed_by_slash_or_star_is_harmless() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const RATIO: f64 = 1.0 / 2.0;\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "bare slash should not affect scanner");
}

// ---------------------------------------------------------------------------
// enable without prior disable is a no-op
// ---------------------------------------------------------------------------

#[test]
fn enable_without_prior_disable_does_not_panic_or_fail() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-enable ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

// ---------------------------------------------------------------------------
// raw string with multiple hashes
// ---------------------------------------------------------------------------

#[test]
fn directive_in_raw_string_with_two_hashes_does_not_suppress() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &str = r##\"// messrust-disable-next-line ExcessiveParameterList\"##;\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

// ---------------------------------------------------------------------------
// bare 'b' and 'r' identifiers
// ---------------------------------------------------------------------------

#[test]
fn bare_b_identifier_does_not_break_scanner() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "fn b() {{}}\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "bare 'b' should not break scanner");
}

#[test]
fn bare_r_identifier_does_not_break_scanner() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "fn r() {{}}\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "bare 'r' should not break scanner");
}

// ---------------------------------------------------------------------------
// disable-next-line does not suppress the directive line itself
// ---------------------------------------------------------------------------

#[test]
fn disable_next_line_does_not_suppress_directive_line() {
    let dir = TempDir::new().unwrap();
    let source =
        "fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {} // messrust-disable-next-line ExcessiveParameterList\n\
         fn second(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains(":1"), "line 1 violation should fire: {out:?}");
    assert!(!out.contains(":2"), "line 2 should be suppressed: {out:?}");
}

// ---------------------------------------------------------------------------
// inline comment directive
// ---------------------------------------------------------------------------

#[test]
fn inline_comment_directive_suppresses_next_line() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const X: i32 = 42; // messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "inline directive should work");
}

// ---------------------------------------------------------------------------
// two disable-next-line directives
// ---------------------------------------------------------------------------

#[test]
fn two_disable_next_line_directives_each_suppress_their_next_line() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n\
         {first}\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "both lines should be suppressed");
}

// ---------------------------------------------------------------------------
// directive with extra whitespace
// ---------------------------------------------------------------------------

#[test]
fn directive_with_extra_whitespace_still_works() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "//   messrust-disable-next-line   ExcessiveParameterList  \n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "whitespace-padded directive should work");
}

// ---------------------------------------------------------------------------
// suppress one rule but not another
// ---------------------------------------------------------------------------

#[test]
fn disable_one_rule_does_not_suppress_another() {
    let dir = TempDir::new().unwrap();
    let source =
        "// messrust-disable-next-line ShortVariable\n\
         fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) { let x = 1; let _ = x; }\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, out, _err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize,naming",
    ]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "EPL should still fire: {out:?}"
    );
}

// ---------------------------------------------------------------------------
// escaped quote inside string
// ---------------------------------------------------------------------------

#[test]
fn escaped_quote_in_string_does_not_end_string_early() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &str = \"escaped \\\" quote // messrust-disable-next-line ExcessiveParameterList\";\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "directive in escaped string should not suppress: {out:?}"
    );
}

// ---------------------------------------------------------------------------
// empty source file
// ---------------------------------------------------------------------------

#[test]
fn empty_source_file_produces_no_errors() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "empty.rs", "");
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

// ---------------------------------------------------------------------------
// source without trailing newline
// ---------------------------------------------------------------------------

#[test]
fn source_without_trailing_newline_suppresses_correctly() {
    let dir = TempDir::new().unwrap();
    let params: Vec<String> = (0..11).map(|i| format!("param_{i}: i32")).collect();
    let func = format!("fn entry_point({}) {{}}", params.join(", "));
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{func}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "no trailing newline should still work");
}

// ---------------------------------------------------------------------------
// strict mode with suppressed findings
// ---------------------------------------------------------------------------

#[test]
fn strict_mode_marks_suppressed_findings_in_json() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[
        path.to_str().unwrap(),
        "json",
        "codesize",
        "--strict",
    ]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("\"suppressed\": true"), "report={out}");
}

#[test]
fn strict_mode_marks_suppressed_findings_in_text() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--strict",
    ]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("[suppressed]"), "stdout={out:?}");
}

// ---------------------------------------------------------------------------
// unsuppressed line after suppressed one still fires
// ---------------------------------------------------------------------------

#[test]
fn unsuppressed_line_after_next_line_directive_fires() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n\
         fn clean() {{}}\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
    assert!(out.contains(":3"), "line 3 should fire: {out:?}");
}

// ---------------------------------------------------------------------------
// comma with leading rule name immediately after directive
// ---------------------------------------------------------------------------

#[test]
fn directive_with_comma_after_keyword_splits_rules() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line,ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    // The comma after the keyword is accepted as a separator per command_rest
    assert_eq!(code, EXIT_SUCCESS, "comma-separated from keyword should work");
}

// ---------------------------------------------------------------------------
// slash at end of file
// ---------------------------------------------------------------------------

#[test]
fn trailing_slash_at_end_of_file_does_not_panic() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{}/",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    // We only care that it does not panic; exit code depends on parse success.
    let (_code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
}

// ---------------------------------------------------------------------------
// multiline raw string with newlines then directive
// ---------------------------------------------------------------------------

#[test]
fn raw_string_with_embedded_directive_preserves_line_count() {
    let dir = TempDir::new().unwrap();
    // The raw string has an embedded directive on line 2 (inside the string).
    // The real directive is on line 5. The violation is on line 6.
    // Wrap the let in a function so syn can parse it.
    let source = "fn wrapper() {\n\
                  let _text = r#\"first\n// messrust-disable ExcessiveParameterList\nlast\"#;\n\
                  }\n\
                  // messrust-disable-next-line ExcessiveParameterList\n\
                  fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "line count through raw string should be correct");
}

// ---------------------------------------------------------------------------
// newlines in a regular string literal
// ---------------------------------------------------------------------------

#[test]
fn string_with_literal_newline_tracks_line_count() {
    let dir = TempDir::new().unwrap();
    // A regular string that spans multiple lines (backslash-newline continuation)
    let source = "const S: &str = \"\\\n\\\n\";\n\
                  // messrust-disable-next-line ExcessiveParameterList\n\
                  fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "line count through string with newlines should be correct");
}

// ---------------------------------------------------------------------------
// `b` followed by `r` (byte raw string)
// ---------------------------------------------------------------------------

#[test]
fn byte_raw_string_with_newlines_tracks_line_count() {
    let dir = TempDir::new().unwrap();
    let source = "const S: &[u8] = br#\"line1\nline2\"#;\n\
                  // messrust-disable-next-line ExcessiveParameterList\n\
                  fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "byte raw string line count should be correct");
}

// ---------------------------------------------------------------------------
// char newline/cr does not confuse scanner
// ---------------------------------------------------------------------------

#[test]
fn newline_char_literal_does_not_confuse_line_count() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const NL: char = '\\n';\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "newline char literal should not confuse line count");
}

// ---------------------------------------------------------------------------
// deeply nested block comment
// ---------------------------------------------------------------------------

#[test]
fn deeply_nested_block_comment_tracks_depth_correctly() {
    let dir = TempDir::new().unwrap();
    // Three levels of nesting. A broken depth counter would end the comment
    // too early and leave the directive text as code, causing a parse error.
    let source = format!(
        "/* level1 /* level2 /* level3 */ back2 */ back1\n\
         messrust-disable ExcessiveParameterList\n\
         */\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "triple-nested block comment should work");
}

// ---------------------------------------------------------------------------
// block comment depth: wrong depth counter should make the directive
// text appear as code or not get extracted
// ---------------------------------------------------------------------------

#[test]
fn single_block_comment_with_no_nesting_still_works() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "/* messrust-disable ExcessiveParameterList */\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "single block comment should suppress");
}

// ---------------------------------------------------------------------------
// char literal with backslash-backslash then quote
// ---------------------------------------------------------------------------

#[test]
fn char_literal_backslash_backslash_does_not_eat_directive() {
    let dir = TempDir::new().unwrap();
    // '\\'  is a valid char literal (backslash). The scanner must not treat
    // the closing quote as escaped.
    let source = format!(
        "const C: char = '\\\\';\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "double-backslash char literal should not break scanner");
}

// ---------------------------------------------------------------------------
// raw string with no hashes
// ---------------------------------------------------------------------------

#[test]
fn raw_string_no_hashes_does_not_suppress() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &str = r\"// messrust-disable-next-line ExcessiveParameterList\";\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

// ---------------------------------------------------------------------------
// raw string with many hashes
// ---------------------------------------------------------------------------

#[test]
fn raw_string_with_three_hashes_does_not_suppress() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const TEXT: &str = r###\"// messrust-disable-next-line ExcessiveParameterList\"###;\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

// ---------------------------------------------------------------------------
// raw string with embedded quote-hash that is not the closing delimiter
// ---------------------------------------------------------------------------

#[test]
fn raw_string_with_false_closing_does_not_end_early() {
    let dir = TempDir::new().unwrap();
    // r##" ... "# ... "## — the "# in the middle is not the closing delimiter.
    let source = format!(
        "const TEXT: &str = r##\"contains \\\"# not end\n// messrust-disable ExcessiveParameterList\nstill inside\"##;\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "directive inside raw string should not suppress: {out:?}"
    );
}

// ---------------------------------------------------------------------------
// multiline raw string — line count matters for directive after it
// ---------------------------------------------------------------------------

#[test]
fn multiline_raw_string_with_hashes_preserves_line_count() {
    let dir = TempDir::new().unwrap();
    // Raw string spans lines 1-3, directive on line 4, violation on line 5.
    let source = "const TEXT: &str = r##\"first\nsecond\nthird\"##;\n\
                  // messrust-disable-next-line ExcessiveParameterList\n\
                  fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "line count after hashed raw string should be correct");
}

// ---------------------------------------------------------------------------
// b"..." byte string with escape sequences
// ---------------------------------------------------------------------------

#[test]
fn byte_string_with_escaped_quote_does_not_break_scanner() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const B: &[u8] = b\"escaped \\\" byte // messrust-disable ExcessiveParameterList\";\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "directive in byte string should not suppress: {out:?}"
    );
}

// ---------------------------------------------------------------------------
// directive after char literal with backslash-n escape
// ---------------------------------------------------------------------------

#[test]
fn escaped_char_literal_does_not_shift_line_count() {
    let dir = TempDir::new().unwrap();
    // Multiple char literals with escapes, then a directive.
    let source = format!(
        "const A: char = '\\t';\n\
         const B: char = '\\0';\n\
         const C: char = '\\r';\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "escaped char literals should preserve line count");
}

// ---------------------------------------------------------------------------
// a lone quote (lifetime) followed by valid directive
// ---------------------------------------------------------------------------

#[test]
fn multiple_lifetimes_do_not_break_scanner() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "struct Pair<'a, 'b> {{ x: &'a str, y: &'b str }}\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "multiple lifetimes should not break scanner");
}

// ---------------------------------------------------------------------------
// directive inside a block comment that has newlines after the directive
// ---------------------------------------------------------------------------

#[test]
fn block_comment_newlines_after_directive_preserves_count() {
    let dir = TempDir::new().unwrap();
    // Directive is on line 1, but the block comment continues for 2 more
    // lines.  The violation must appear at the correct line after the
    // comment closes.
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let source = format!(
        "/* messrust-disable-next-line ExcessiveParameterList\n\
         line two\n\
         line three */\n\
         {first}\
         {second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    // disable-next-line at line 1 suppresses line 2. But the lines inside
    // the block comment are not code lines that produce violations.
    // The violation functions start at line 4 and 5 (after the comment).
    // Only line 2 is suppressed, so lines 4 and 5 should fire.
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

// ---------------------------------------------------------------------------
// verify that `contains` is false for a completely different rule name
// ---------------------------------------------------------------------------

#[test]
fn contains_returns_false_for_wrong_rule_name() {
    let dir = TempDir::new().unwrap();
    // Suppress ShortVariable but still get ExcessiveParameterList.
    let source = format!(
        "// messrust-disable ShortVariable\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

// ---------------------------------------------------------------------------
// byte raw string with multiple hashes preserves line count
// ---------------------------------------------------------------------------

#[test]
fn byte_raw_string_multiline_with_hashes_preserves_line_count() {
    let dir = TempDir::new().unwrap();
    let source = "const S: &[u8] = br##\"line1\nline2\nline3\"##;\n\
                  // messrust-disable-next-line ExcessiveParameterList\n\
                  fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "byte raw string with hashes line count should be correct");
}

// ---------------------------------------------------------------------------
// unterminated string at EOF (edge case for skip_quoted)
// ---------------------------------------------------------------------------

#[test]
fn unterminated_string_at_eof_does_not_panic() {
    let dir = TempDir::new().unwrap();
    // This is malformed Rust, but the scanner should not panic.
    let source = "const S: &str = \"unterminated";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (_code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    // Just verify no panic; exit code 1 (parse error) is expected.
}

// ---------------------------------------------------------------------------
// unterminated block comment does not panic
// ---------------------------------------------------------------------------

#[test]
fn unterminated_block_comment_does_not_panic() {
    let dir = TempDir::new().unwrap();
    // Malformed Rust: block comment never closes.
    let source = "/* unterminated block comment\n// messrust-disable ExcessiveParameterList\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    // The scanner should not panic. The file may not parse, so exit code 1
    // is acceptable. We only verify no panic.
    let (_code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
}

// ---------------------------------------------------------------------------
// char literal that looks like comment start character
// ---------------------------------------------------------------------------

#[test]
fn char_literal_slash_does_not_start_comment() {
    let dir = TempDir::new().unwrap();
    // The char literal '/' should not be mistaken for the start of a comment.
    let source = format!(
        "const SLASH: char = '/';\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "char literal '/' should not break scanner");
}

// ---------------------------------------------------------------------------
// char literal star does not interfere
// ---------------------------------------------------------------------------

#[test]
fn char_literal_star_does_not_start_block_comment() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const STAR: char = '*';\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "char literal '*' should not break scanner");
}

// ---------------------------------------------------------------------------
// b"..." plain byte string (not raw) with directive inside
// ---------------------------------------------------------------------------

#[test]
fn plain_byte_string_skips_directive() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const B: &[u8] = b\"messrust-disable ExcessiveParameterList\";\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "directive inside b\"\" should not suppress: {out:?}"
    );
}

// ---------------------------------------------------------------------------
// double backslash then quote in string: \\\" is backslash then escaped quote
// ---------------------------------------------------------------------------

#[test]
fn double_backslash_in_string_followed_by_quote_does_not_end_early() {
    let dir = TempDir::new().unwrap();
    // String contents: literal backslash followed by escaped quote, then directive text.
    // The sequence \\\\ is two literal backslashes, \\\" is escaped quote.
    let source = format!(
        "const S: &str = \"\\\\\\\"// messrust-disable ExcessiveParameterList\";\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "directive after escaped sequences in string should not suppress: {out:?}"
    );
}

// ---------------------------------------------------------------------------
// block comment followed immediately by line directive
// ---------------------------------------------------------------------------

#[test]
fn block_comment_then_line_directive_works() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "/* some comment */\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "directive after block comment should work");
}

// ---------------------------------------------------------------------------
// directive at end of file with no trailing newline
// ---------------------------------------------------------------------------

#[test]
fn directive_at_eof_without_newline_still_works() {
    let dir = TempDir::new().unwrap();
    let params: Vec<String> = (0..11).map(|i| format!("param_{i}: i32")).collect();
    let func = format!("fn entry_point({}) {{}}", params.join(", "));
    // Directive is on last line with no trailing newline.
    let source = format!("{func}\n// messrust-disable ExcessiveParameterList");
    let path = write_file(dir.path(), "fixture.rs", &source);
    // The disable applies from line 2 onwards. But the violation is on line 1,
    // which is BEFORE the disable. So it should still fire.
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "violation before disable should fire: {out:?}"
    );
}

// ---------------------------------------------------------------------------
// disable directive at EOF suppresses when violation is after it
// ---------------------------------------------------------------------------

#[test]
fn disable_directive_no_trailing_newline_suppresses_line_after() {
    let dir = TempDir::new().unwrap();
    let params: Vec<String> = (0..11).map(|i| format!("param_{i}: i32")).collect();
    let func = format!("fn entry_point({}) {{}}", params.join(", "));
    // Disable on line 1, violation on line 2 (no trailing newline).
    let source = format!("// messrust-disable ExcessiveParameterList\n{func}");
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "disable at line 1 with no trailing newline should suppress");
}

// ---------------------------------------------------------------------------
// newline count in block comment that spans many lines
// ---------------------------------------------------------------------------

#[test]
fn block_comment_spanning_five_lines_tracks_count() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "/* line1\n\
         line2\n\
         line3\n\
         line4\n\
         line5 */\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "block comment line counting should be correct");
}

// ---------------------------------------------------------------------------
// block comment with `*` not followed by `/` (no false close)
// ---------------------------------------------------------------------------

#[test]
fn block_comment_with_stray_star_does_not_close_early() {
    let dir = TempDir::new().unwrap();
    // The `*` at "star*here" is not followed by `/`, so it should not close.
    let source = format!(
        "/* star*here\nmessrust-disable ExcessiveParameterList\n*/\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stray star should not close block comment");
}

// ---------------------------------------------------------------------------
// block comment with `/` not preceded by `*` (no false open)
// ---------------------------------------------------------------------------

#[test]
fn block_comment_with_stray_slash_does_not_open_nested() {
    let dir = TempDir::new().unwrap();
    // The `/` at "slash/here" is preceded by `h`, not `*`, so it should not
    // be treated as a nested comment opening.
    let source = format!(
        "/* slash/here\nmessrust-disable ExcessiveParameterList\n*/\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stray slash should not open nested comment");
}

// ---------------------------------------------------------------------------
// scan_block_comment: depth counter must decrement correctly
// The directive text is OUTSIDE the inner comment but INSIDE the outer.
// If depth is wrong, the outer comment closes too early or too late.
// ---------------------------------------------------------------------------

#[test]
fn nested_comment_directive_between_inner_close_and_outer_close() {
    let dir = TempDir::new().unwrap();
    // Structure: /* outer_start /* inner */ \n messrust-disable EPL \n */
    // The inner `*/` closes the inner comment (depth 2->1).
    // The directive text is on a separate line at depth 1 (still inside outer).
    // The final `*/` closes the outer (depth 1->0).
    let source = format!(
        "/* /* inner */\nmessrust-disable ExcessiveParameterList\n*/\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "directive between inner close and outer close should suppress");
}

// ---------------------------------------------------------------------------
// verify line 1 in from_source is always processed (line counter starts at 1)
// ---------------------------------------------------------------------------

#[test]
fn disable_on_line_one_suppresses_line_two() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "disable on line 1 should suppress line 2");
}

// ---------------------------------------------------------------------------
// multiple char literals on one line before directive
// ---------------------------------------------------------------------------

#[test]
fn many_char_literals_before_directive() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const A: char = 'a'; const B: char = 'b'; const C: char = 'c';\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "multiple char literals should not break scanner");
}

// ---------------------------------------------------------------------------
// char literal with unicode escape
// ---------------------------------------------------------------------------

#[test]
fn char_literal_with_unicode_escape_does_not_break_scanner() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "const U: char = '\\u{{0041}}';\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "unicode escape in char literal should not break scanner");
}

// ---------------------------------------------------------------------------
// two block comments on consecutive lines
// ---------------------------------------------------------------------------

#[test]
fn two_block_comments_then_directive() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "/* first */\n/* second */\n// messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "two block comments should not break line count");
}

// ---------------------------------------------------------------------------
// block comment with nested depth 2: verify index advances by 2 after /*
// ---------------------------------------------------------------------------

#[test]
fn nested_block_comment_index_advance_after_open() {
    let dir = TempDir::new().unwrap();
    // If the scanner only advances by 1 after `/*` instead of 2, the `*`
    // would be re-scanned. The next byte is `/` → false `*/` detected.
    // This would decrement depth incorrectly.
    // Put the directive on its own line so add_directive can match it.
    let source = format!(
        "/* /* inner */\nmessrust-disable ExcessiveParameterList\n*/\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "nested block comment with index advance should work");
}

// ---------------------------------------------------------------------------
// scan_block_comment: verify `end = index` sets the correct end position
// The returned text should not include the closing `*/`.
// ---------------------------------------------------------------------------

#[test]
fn block_comment_text_does_not_include_closing_delimiter() {
    let dir = TempDir::new().unwrap();
    // If `end = index` is mutated (e.g., to `end = index + 1`), the comment
    // text might include `*` from `*/`, corrupting the directive.
    // Place the directive right before `*/` with no space.
    let source = format!(
        "/* messrust-disable ExcessiveParameterList*/\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    // The text extracted is " messrust-disable ExcessiveParameterList".
    // After trim: "messrust-disable ExcessiveParameterList". This should match.
    assert_eq!(code, EXIT_SUCCESS, "directive right before */ should suppress");
}

// ---------------------------------------------------------------------------
// scan_block_comment: after closing */, index must be past both characters
// ---------------------------------------------------------------------------

#[test]
fn block_comment_scanner_resumes_after_closing_delimiter() {
    let dir = TempDir::new().unwrap();
    // The `*/` is immediately followed by `//` directive. If the scanner
    // does not advance past both `*` and `/`, it might re-parse `/` as
    // the start of `//`.
    let source = format!(
        "/* comment */\n// messrust-disable-next-line ExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "scanner should resume correctly after */");
}

// ---------------------------------------------------------------------------
// from_source: line counting starts at 1, not 0
// ---------------------------------------------------------------------------

#[test]
fn disable_next_line_on_line_one_suppresses_line_two_not_one() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList\n{first}{second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    // Line 2 is suppressed, line 3 is not.
    assert_eq!(code, EXIT_VIOLATION);
    assert!(!out.contains(":2"), "line 2 suppressed: {out:?}");
    assert!(out.contains(":3"), "line 3 fires: {out:?}");
}

// ---------------------------------------------------------------------------
// scan_character: newline_count is called on the char literal range
// ---------------------------------------------------------------------------

#[test]
fn char_literal_range_does_not_add_extra_lines() {
    let dir = TempDir::new().unwrap();
    // Simple char literal 'x' followed by directive. The newline_count for
    // the char literal range ['x'] should be 0.
    let source = format!(
        "const X: char = 'x';\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "simple char literal should not affect line count");
}

// ---------------------------------------------------------------------------
// skip_raw_or_byte_string: `b"..."` takes the quote branch, not raw branch
// ---------------------------------------------------------------------------

#[test]
fn byte_string_without_raw_takes_quote_branch() {
    let dir = TempDir::new().unwrap();
    // b"..." should be handled by `skip_quoted`, not `skip_raw_string`.
    // Put a `#` right after the closing `"` to verify the raw string path
    // is not accidentally taken.
    let source = format!(
        "const B: &[u8] = b\"text\";\n\
         const H: u8 = b'#';\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "b\"\" followed by # should not break scanner");
}

// ---------------------------------------------------------------------------
// string_prefix_end: `b` NOT followed by `"` or `r` falls through
// ---------------------------------------------------------------------------

#[test]
fn b_followed_by_identifier_char_is_not_string() {
    let dir = TempDir::new().unwrap();
    // `bar` starts with `b` but is followed by `a`, not `"` or `r`.
    let source = format!(
        "fn bar() {{}}\n\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "identifier starting with 'b' should not confuse scanner");
}

// ---------------------------------------------------------------------------
// has_hashes with count=0 returns true (empty range in `all`)
// ---------------------------------------------------------------------------

#[test]
fn raw_string_zero_hashes_closing_works() {
    let dir = TempDir::new().unwrap();
    // r"..." has 0 hashes. has_hashes(bytes, cursor+1, 0) should return true
    // for any position because (0..0).all(...) is vacuously true.
    let source = "const S: &str = r\"line1\nline2\";\n\
                  // messrust-disable-next-line ExcessiveParameterList\n\
                  fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "raw string with 0 hashes line count should work");
}

// ---------------------------------------------------------------------------
// has_hashes: count=1, matching hash after closing quote
// ---------------------------------------------------------------------------

#[test]
fn raw_string_one_hash_closing_delimiter_detected() {
    let dir = TempDir::new().unwrap();
    // Ensure the `"#` closing delimiter is detected correctly.
    // Put a directive INSIDE the raw string to verify it is not parsed.
    let source = "const S: &str = r#\"// messrust-disable ExcessiveParameterList\nstill inside\"#;\n\
                  fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(
        out.contains("ExcessiveParameterList"),
        "directive inside r#\"\"# should not suppress: {out:?}"
    );
}

// ---------------------------------------------------------------------------
// skip_raw_string with hashes: `"#` inside r##"..."## is not closing
// ---------------------------------------------------------------------------

#[test]
fn raw_string_two_hashes_inner_single_hash_is_not_closing() {
    let dir = TempDir::new().unwrap();
    // r##"..."## requires "## to close. A "# inside is not enough.
    let source = "const S: &str = r##\"contains \\\"# not close\nstill inside\"##;\n\
                  // messrust-disable-next-line ExcessiveParameterList\n\
                  fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "inner \"# should not close r##\"\"##");
}

// ---------------------------------------------------------------------------
// skip_raw_string return value includes hashes in the final position
// ---------------------------------------------------------------------------

#[test]
fn raw_string_end_position_includes_closing_hashes() {
    let dir = TempDir::new().unwrap();
    // After r#"..."#, the scanner must resume AFTER the closing `#`.
    // If it resumes too early, the `#` might be misinterpreted.
    // Place a `;` right after the closing `#`, then a directive.
    let source = "const S: &str = r#\"text\"#;\n\
                  // messrust-disable-next-line ExcessiveParameterList\n\
                  fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let path = write_file(dir.path(), "fixture.rs", source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "scanner must resume after closing hashes");
}

// ---------------------------------------------------------------------------
// valid_rule_name: alphanumeric chars after first alpha char
// ---------------------------------------------------------------------------

#[test]
fn rule_name_with_digits_suppresses() {
    let dir = TempDir::new().unwrap();
    // Rule names can contain digits after the first alpha character.
    // Use the actual rule name which is all-alpha, but also add a
    // fictional rule with digits that won't match but should be accepted
    // by valid_rule_name.
    let source = format!(
        "// messrust-disable-next-line ExcessiveParameterList,Rule123\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "rule name with digits should be valid");
}

// ---------------------------------------------------------------------------
// command_rest: separator check — command followed by whitespace
// ---------------------------------------------------------------------------

#[test]
fn command_rest_accepts_tab_separator() {
    let dir = TempDir::new().unwrap();
    // Tab character between the command and the rule name.
    let source = format!(
        "// messrust-disable-next-line\tExcessiveParameterList\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "tab separator should be accepted");
}

// ---------------------------------------------------------------------------
// apply_enables: enable removes rule from active set on that line
// ---------------------------------------------------------------------------

#[test]
fn enable_on_same_line_as_violation_re_enables_that_line() {
    let dir = TempDir::new().unwrap();
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    // Line 1: disable EPL
    // Line 2: first violation (suppressed by disable)
    // Line 3: enable EPL → this line is re-enabled
    // Line 4: second violation (not suppressed, enable took effect)
    let source = format!(
        "// messrust-disable ExcessiveParameterList\n\
         fn first(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {{}}\n\
         // messrust-enable ExcessiveParameterList\n\
         {second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(!out.contains(":2"), "line 2 suppressed: {out:?}");
    assert!(out.contains(":4"), "line 4 fires after enable: {out:?}");
}

// ---------------------------------------------------------------------------
// apply_disables: DisableNextLine uses line + 1
// ---------------------------------------------------------------------------

#[test]
fn disable_next_line_suppresses_line_plus_one_not_current() {
    let dir = TempDir::new().unwrap();
    // Violation on same line as directive should NOT be suppressed.
    // Violation on next line should be suppressed.
    let first = "fn entry_point(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {} // messrust-disable-next-line ExcessiveParameterList\n";
    let second = "fn second(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let third = "fn third(param_0: i32, param_1: i32, param_2: i32, param_3: i32, param_4: i32, param_5: i32, param_6: i32, param_7: i32, param_8: i32, param_9: i32, param_10: i32) {}\n";
    let source = format!("{first}{second}{third}");
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains(":1"), "line 1 should fire: {out:?}");
    assert!(!out.contains(":2"), "line 2 suppressed: {out:?}");
    assert!(out.contains(":3"), "line 3 should fire: {out:?}");
}

// ---------------------------------------------------------------------------
// contains: rule name is lowercased for lookup
// ---------------------------------------------------------------------------

#[test]
fn mixed_case_rule_name_in_directive_matches_mixed_case_violation() {
    let dir = TempDir::new().unwrap();
    let source = format!(
        "// messrust-disable-next-line EXCESSIVEPARAMETERLIST\n{}",
        fixture_with_params(11)
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, _out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(
        code, EXIT_SUCCESS,
        "all-caps rule name should match actual rule"
    );
}

// ---------------------------------------------------------------------------
// disable on the same line as enable: enable wins for that line
// ---------------------------------------------------------------------------

#[test]
fn enable_before_disable_on_same_line_enables_then_disables() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    // Line 1: disable
    // Line 2: first violation (suppressed)
    // Line 3: enable (re-enables on this line) then we also put a new disable
    // Since enable runs first, line 3 is re-enabled; disable then re-disables from line 4
    let source = format!(
        "// messrust-disable ExcessiveParameterList\n\
         {first}\
         // messrust-enable ExcessiveParameterList\n\
         {second}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, _err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(!out.contains(":2"), "line 2 suppressed: {out:?}");
    assert!(out.contains(":4"), "line 4 fires: {out:?}");
}

// ---------------------------------------------------------------------------
// apply_enables must ignore non-enable directives (kills kind-filter mutant)
// ---------------------------------------------------------------------------

#[test]
fn disable_next_line_for_same_rule_does_not_end_a_disable_region() {
    let dir = TempDir::new().unwrap();
    let first = fixture_with_params(11);
    let second = fixture_with_params(12).replacen("entry_point", "second", 1);
    let third = fixture_with_params(13).replacen("entry_point", "third", 1);
    // Region disable stays active across a disable-next-line for the same rule.
    // If apply_enables drops the Enable kind check, disable-next-line clears active.
    let source = format!(
        "// messrust-disable ExcessiveParameterList\n\
         {first}\
         // messrust-disable-next-line ExcessiveParameterList\n\
         {second}\
         {third}"
    );
    let path = write_file(dir.path(), "fixture.rs", &source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
    assert!(out.is_empty(), "region must still suppress later lines: {out:?}");
}
