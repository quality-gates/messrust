//! Source discovery through the injectable CLI entry (`messrust::run`).
//!
//! These tests assert which files the command reads, the order of findings and
//! errors, and the skip rules for junk directories and conventional test paths.

use std::fs;
use std::os::unix::net::UnixListener;
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

fn hot(n: usize) -> String {
    let params: Vec<String> = (0..n).map(|i| format!("param_{i}: i32")).collect();
    format!("fn entry_point({}) {{}}\n", params.join(", "))
}

fn reported_basenames(stdout: &str) -> Vec<String> {
    let report: serde_json::Value = serde_json::from_str(stdout).unwrap();
    let mut names: Vec<String> = report["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            Path::new(f["file"].as_str().unwrap())
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn error_basenames(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| line.contains('\t'))
        .filter_map(|line| {
            let path = line.split('\t').next()?;
            Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .collect()
}

#[test]
fn missing_path_exits_one_and_names_the_path_on_stderr() {
    let missing = {
        let dir = TempDir::new().unwrap();
        dir.path().join("does-not-exist.rs")
    };
    let (code, out, err) = run_cli(&[missing.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_ERROR);
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        err.contains("does-not-exist.rs"),
        "stderr must name the missing path: {err:?}"
    );
    assert!(err.contains("error:"), "stderr={err:?}");
}

#[test]
fn single_rust_file_path_is_analyzed() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "only.rs", &hot(11));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "json", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_eq!(reported_basenames(&out), vec!["only.rs".to_string()]);
}

#[test]
fn directory_finds_nested_rs_files_in_sorted_order() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "z/z.rs", &hot(11));
    write_file(dir.path(), "a/a.rs", &hot(12));
    write_file(dir.path(), "m/m.rs", &hot(13));
    let (code, out, err) = run_cli(&[dir.path().to_str().unwrap(), "json", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    let report: serde_json::Value = serde_json::from_str(&out).unwrap();
    let names: Vec<&str> = report["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            Path::new(f["file"].as_str().unwrap())
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        })
        .collect();
    assert_eq!(names, vec!["a.rs", "m.rs", "z.rs"], "stdout={out}");
}

#[test]
fn discovery_sort_orders_processing_errors() {
    // Violations are re-sorted in analyze; errors keep discovery order.
    // Pass files in reverse alpha order so only an explicit sort yields a-then-z.
    let dir = TempDir::new().unwrap();
    let z = write_file(dir.path(), "z_bad.rs", "fn broken( {\n");
    let a = write_file(dir.path(), "a_bad.rs", "fn broken( {\n");
    let paths = format!("{},{}", z.to_str().unwrap(), a.to_str().unwrap());
    let (code, out, err) = run_cli(&[&paths, "text", "codesize"]);
    assert_eq!(code, EXIT_ERROR, "stderr={err:?}");
    let names = error_basenames(&out);
    assert_eq!(
        names,
        vec!["a_bad.rs".to_string(), "z_bad.rs".to_string()],
        "stdout={out:?}"
    );
}

#[test]
fn skips_target_git_and_node_modules_each_by_name() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "keep.rs", &hot(11));
    write_file(dir.path(), "target/from_target.rs", &hot(12));
    write_file(dir.path(), ".git/from_git.rs", &hot(13));
    write_file(dir.path(), "node_modules/from_nm.rs", &hot(14));
    let (code, out, err) = run_cli(&[dir.path().to_str().unwrap(), "json", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_eq!(reported_basenames(&out), vec!["keep.rs".to_string()]);
    assert!(!out.contains("from_target.rs"), "stdout={out}");
    assert!(!out.contains("from_git.rs"), "stdout={out}");
    assert!(!out.contains("from_nm.rs"), "stdout={out}");
}

#[test]
fn non_directory_entry_named_like_a_skip_dir_is_still_read() {
    // walk_entry_allowed must not apply directory skip names to files.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "pkg/.git", &hot(11));
    let (code, out, err) = run_cli(&[
        dir.path().to_str().unwrap(),
        "json",
        "codesize",
        "--suffixes",
        ".git",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?} path={path:?}");
    assert_eq!(reported_basenames(&out), vec![".git".to_string()]);
}

#[test]
fn directory_whose_name_ends_with_rs_is_not_analyzed_as_a_file() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "real.rs", &hot(11));
    fs::create_dir_all(dir.path().join("fake.rs")).unwrap();
    let (code, out, err) = run_cli(&[dir.path().to_str().unwrap(), "json", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_eq!(reported_basenames(&out), vec!["real.rs".to_string()]);
    assert!(
        !out.contains("fake.rs"),
        "directory fake.rs must not become a processing error: {out}"
    );
}

#[test]
fn ignore_tests_on_direct_file_path_skips_conventional_test_names() {
    let dir = TempDir::new().unwrap();
    let cases = [
        "test.rs",
        "tests.rs",
        "test_helpers.rs",
        "widget_test.rs",
    ];
    for name in cases {
        let path = write_file(dir.path(), name, &hot(11));
        let (code, out, err) = run_cli(&[path.to_str().unwrap(), "json", "codesize"]);
        assert_eq!(code, EXIT_VIOLATION, "without ignore-tests name={name} stderr={err:?}");
        assert_eq!(
            reported_basenames(&out),
            vec![name.to_string()],
            "without ignore-tests name={name}"
        );

        let (code, out, err) = run_cli(&[
            path.to_str().unwrap(),
            "json",
            "codesize",
            "--ignore-tests",
        ]);
        assert_eq!(code, EXIT_SUCCESS, "with ignore-tests name={name} stderr={err:?}");
        assert!(
            reported_basenames(&out).is_empty(),
            "with ignore-tests name={name} stdout={out}"
        );
    }
}

#[test]
fn ignore_tests_on_direct_path_skips_file_under_tests_directory() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "tests/integration.rs", &hot(11));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "json", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_eq!(reported_basenames(&out), vec!["integration.rs".to_string()]);

    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "json",
        "codesize",
        "--ignore-tests",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(reported_basenames(&out).is_empty(), "stdout={out}");
}

#[test]
fn ignore_tests_skips_files_under_test_tests_and_dunder_tests_dirs() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "src/lib.rs", &hot(11));
    write_file(dir.path(), "test/unit.rs", &hot(12));
    write_file(dir.path(), "tests/integration.rs", &hot(13));
    write_file(dir.path(), "__tests__/spec.rs", &hot(14));
    let (code, out, err) = run_cli(&[
        dir.path().to_str().unwrap(),
        "json",
        "codesize",
        "--ignore-tests",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_eq!(reported_basenames(&out), vec!["lib.rs".to_string()]);
}

#[test]
fn exclude_and_suffixes_select_exact_file_set() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "src/keep.rs", &hot(11));
    write_file(dir.path(), "src/notes.txt", &hot(12));
    write_file(dir.path(), "vendor/skip.rs", &hot(13));
    let (code, out, err) = run_cli(&[
        dir.path().to_str().unwrap(),
        "json",
        "codesize",
        "--exclude",
        "vendor",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_eq!(reported_basenames(&out), vec!["keep.rs".to_string()]);

    let (code, out, err) = run_cli(&[
        dir.path().to_str().unwrap(),
        "json",
        "codesize",
        "--suffixes",
        ".txt",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_eq!(reported_basenames(&out), vec!["notes.txt".to_string()]);
}

#[test]
fn duplicate_path_arguments_analyze_a_file_once() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "once.rs", &hot(11));
    let paths = format!("{},{}", path.to_str().unwrap(), path.to_str().unwrap());
    let (code, out, err) = run_cli(&[&paths, "json", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    let report: serde_json::Value = serde_json::from_str(&out).unwrap();
    let files = report["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "stdout={out}");
    assert_eq!(files[0]["violations"].as_array().unwrap().len(), 1);
}

#[test]
fn special_file_that_is_not_a_regular_file_or_directory_errors() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("socket.rs");
    let _listener = UnixListener::bind(&sock).unwrap();
    let (code, out, err) = run_cli(&[sock.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_ERROR, "stdout={out:?} stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        err.contains("not a file or directory"),
        "stderr={err:?}"
    );
}
