//! Integration tests through the injectable CLI entry.

use std::fs;
use std::path::{Path, PathBuf};

use messrust::{run, EXIT_ERROR, EXIT_SUCCESS, EXIT_VIOLATION};
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
    let params: Vec<String> = (0..n).map(|i| format!("p{i}: i32")).collect();
    format!("fn f({}) {{}}\n", params.join(", "))
}

#[test]
fn version_prints_package_version_and_exits_zero() {
    let (code, out, err) = run_cli(&["--version"]);
    assert_eq!(code, EXIT_SUCCESS);
    assert!(
        out.contains(env!("CARGO_PKG_VERSION")),
        "stdout={out:?}"
    );
    assert!(out.starts_with("messrust "), "stdout={out:?}");
    assert!(err.is_empty(), "stderr={err:?}");
}

#[test]
fn help_prints_usage_to_stdout_and_exits_zero() {
    for flag in ["--help", "-h"] {
        let (code, out, err) = run_cli(&[flag]);
        assert_eq!(code, EXIT_SUCCESS, "flag={flag}");
        assert!(out.contains("Usage:"), "stdout={out:?}");
        assert!(out.contains("messrust"), "stdout={out:?}");
        assert!(err.is_empty(), "stderr={err:?}");
    }
}

#[test]
fn no_args_prints_usage_to_stderr_and_exits_one() {
    let (code, out, err) = run_cli(&[]);
    assert_eq!(code, EXIT_ERROR);
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(err.contains("Usage:"), "stderr={err:?}");
}

#[test]
fn missing_positionals_exits_one() {
    let (code, _out, err) = run_cli(&["src"]);
    assert_eq!(code, EXIT_ERROR);
    assert!(err.contains("Usage:"), "stderr={err:?}");
}

#[test]
fn unknown_option_exits_one() {
    let (code, _out, err) = run_cli(&["src", "text", "codesize", "--not-a-real-flag"]);
    assert_eq!(code, EXIT_ERROR);
    assert!(err.contains("error:"), "stderr={err:?}");
}

#[test]
fn clean_fixture_exits_zero() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fixture_with_params(3));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(err.is_empty(), "stderr={err:?}");
}

#[test]
fn excessive_parameter_list_fires_in_text_and_exits_two() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        out.contains("ExcessiveParameterList"),
        "stdout={out:?}"
    );
    assert!(out.contains("11 parameters"), "stdout={out:?}");
    assert!(err.is_empty(), "stderr={err:?}");
}

#[test]
fn ignore_violations_on_exit_keeps_report_but_exits_zero() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--ignore-violations-on-exit",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(
        out.contains("ExcessiveParameterList"),
        "stdout={out:?}"
    );
}

#[test]
fn reportfile_writes_report_and_leaves_stdout_empty() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let report = dir.path().join("report.txt");
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--reportfile",
        report.to_str().unwrap(),
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    let body = fs::read_to_string(&report).unwrap();
    assert!(
        body.contains("ExcessiveParameterList"),
        "report={body:?}"
    );
}

#[test]
fn malformed_file_yields_processing_error_without_hiding_findings() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "bad.rs", "fn broken( {\n");
    write_file(dir.path(), "good.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[dir.path().to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_ERROR, "stderr={err:?}");
    assert!(
        out.contains("ExcessiveParameterList"),
        "stdout={out:?}"
    );
    assert!(out.contains('\t'), "expected error line with tab: {out:?}");
    assert!(out.contains("bad.rs"), "stdout={out:?}");
}

#[test]
fn ignore_errors_on_exit_keeps_error_report_but_exits_for_violations() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "bad.rs", "fn broken( {\n");
    write_file(dir.path(), "good.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[
        dir.path().to_str().unwrap(),
        "text",
        "codesize",
        "--ignore-errors-on-exit",
    ]);
    // Errors ignored → violations remain → exit 2
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
    assert!(out.contains("bad.rs"), "stdout={out:?}");
}

#[test]
fn discovery_skips_junk_dirs_and_honours_exclude_suffixes_ignore_tests() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "src/a.rs", &fixture_with_params(11));
    write_file(dir.path(), "src/b.txt", &fixture_with_params(11));
    write_file(dir.path(), "target/hidden.rs", &fixture_with_params(11));
    write_file(dir.path(), ".git/hidden.rs", &fixture_with_params(11));
    write_file(dir.path(), "node_modules/hidden.rs", &fixture_with_params(11));
    write_file(dir.path(), "vendor/skip_me.rs", &fixture_with_params(11));
    write_file(dir.path(), "src/foo_test.rs", &fixture_with_params(11));

    // Default: src/a.rs + src/foo_test.rs + vendor/skip_me.rs
    // (junk dirs skipped; .txt ignored; _test.rs kept without --ignore-tests)
    let (code, out, _) = run_cli(&[dir.path().to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("a.rs"), "stdout={out:?}");
    assert!(out.contains("skip_me.rs"), "stdout={out:?}");
    assert!(!out.contains("hidden.rs"), "stdout={out:?}");
    assert!(!out.contains("b.txt"), "stdout={out:?}");
    assert!(out.contains("foo_test.rs"), "stdout={out:?}");

    // --exclude vendor substring
    let (code, out, _) = run_cli(&[
        dir.path().to_str().unwrap(),
        "text",
        "codesize",
        "--exclude",
        "vendor",
    ]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("a.rs"), "stdout={out:?}");
    assert!(!out.contains("skip_me.rs"), "stdout={out:?}");

    // --ignore-tests drops *_test.rs
    let (code, out, _) = run_cli(&[
        dir.path().to_str().unwrap(),
        "text",
        "codesize",
        "--ignore-tests",
    ]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("a.rs"), "stdout={out:?}");
    assert!(!out.contains("foo_test.rs"), "stdout={out:?}");

    // --suffixes .txt only
    let (code, out, _) = run_cli(&[
        dir.path().to_str().unwrap(),
        "text",
        "codesize",
        "--suffixes",
        ".txt",
    ]);
    assert_eq!(code, EXIT_VIOLATION);
    assert!(out.contains("b.txt"), "stdout={out:?}");
    assert!(!out.contains("a.rs"), "stdout={out:?}");
}

#[test]
fn discovery_is_deterministic_path_order() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "z.rs", &fixture_with_params(11));
    write_file(dir.path(), "a.rs", &fixture_with_params(11));
    write_file(dir.path(), "m.rs", &fixture_with_params(11));
    let (code, out, _) = run_cli(&[dir.path().to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION);
    let a = out.find("a.rs").expect("a.rs");
    let m = out.find("m.rs").expect("m.rs");
    let z = out.find("z.rs").expect("z.rs");
    assert!(a < m && m < z, "order not sorted: {out:?}");
}

#[test]
fn both_ignore_flags_with_errors_and_violations_exits_zero() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "bad.rs", "fn broken( {\n");
    write_file(dir.path(), "good.rs", &fixture_with_params(11));
    let (code, out, _) = run_cli(&[
        dir.path().to_str().unwrap(),
        "text",
        "codesize",
        "--ignore-errors-on-exit",
        "--ignore-violations-on-exit",
    ]);
    assert_eq!(code, EXIT_SUCCESS);
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
    assert!(out.contains("bad.rs"), "stdout={out:?}");
}

#[test]
fn unknown_format_exits_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fixture_with_params(0));
    let (code, _out, err) = run_cli(&[path.to_str().unwrap(), "not-a-format", "codesize"]);
    assert_eq!(code, EXIT_ERROR);
    assert!(err.contains("error:"), "stderr={err:?}");
}

#[test]
fn unknown_ruleset_exits_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fixture_with_params(0));
    let (code, _out, err) = run_cli(&[path.to_str().unwrap(), "text", "nope"]);
    assert_eq!(code, EXIT_ERROR);
    assert!(err.contains("error:"), "stderr={err:?}");
}
