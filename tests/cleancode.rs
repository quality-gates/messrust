//! cleancode rules through the injectable CLI entry.

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

fn run_only(path: &Path, rule: &str) -> (i32, String, String) {
    run_cli(&[path.to_str().unwrap(), "text", "cleancode", "--only", rule])
}

#[test]
fn all_cleancode_rules_load_without_verbose_skips() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", "fn clean_entry() {}\n");
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "cleancode", "--verbose"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        !err.contains("Skipping unimplemented rule"),
        "stderr={err:?}"
    );
}

#[test]
fn boolean_argument_flag_reports_bool_param() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "flag.rs",
        r#"
fn process(flag: bool) {
    let _ = flag;
}
"#,
    );
    let (code, out, err) = run_only(&path, "BooleanArgumentFlag");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("BooleanArgumentFlag"), "stdout={out:?}");
    assert!(out.contains("process"), "stdout={out:?}");
    assert!(out.contains("flag"), "stdout={out:?}");
}

#[test]
fn boolean_argument_flag_skips_underscore_and_non_bool() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        r#"
fn process(_flag: bool, count: i32) {
    let _ = count;
}
"#,
    );
    let (code, out, err) = run_only(&path, "BooleanArgumentFlag");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn boolean_argument_flag_honours_exceptions_and_ignorepattern() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ex.rs",
        r#"
struct Allowed;
impl Allowed {
    fn run(flag: bool) {
        let _ = flag;
    }
}
struct Other;
impl Other {
    fn create_with(flag: bool) {
        let _ = flag;
    }
}
"#,
    );
    let xml = dir.path().join("baf.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="baf">
  <rule ref="cleancode/BooleanArgumentFlag">
    <properties>
      <property name="exceptions" value="Allowed"/>
      <property name="ignorepattern" value="(^create)i"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn else_expression_reports_terminal_else() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "else.rs",
        r#"
fn choose(flag: bool) -> i32 {
    if flag {
        1
    } else {
        2
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "ElseExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ElseExpression"), "stdout={out:?}");
    assert!(out.contains("choose"), "stdout={out:?}");
}

#[test]
fn else_expression_reports_terminal_else_but_not_else_if_only_chains() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "elseif_only.rs",
        r#"
fn chain(n: i32) -> i32 {
    if n > 0 {
        1
    } else if n < 0 {
        -1
    } else {
        0
    }
}
fn no_terminal(n: i32) -> i32 {
    if n > 0 {
        1
    } else if n < 0 {
        -1
    } else if true {
        0
    } else if false {
        2
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "ElseExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("chain"), "stdout={out:?}");
    assert!(!out.contains("no_terminal"), "stdout={out:?}");
}

#[test]
fn if_statement_assignment_reports_assign_in_condition() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "assign.rs",
        r#"
fn scan(mut x: i32) {
    if { x = 1; true } {
        let _ = x;
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "IfStatementAssignment");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("IfStatementAssignment"), "stdout={out:?}");
    assert!(out.contains("line"), "stdout={out:?}");
}

#[test]
fn if_statement_assignment_allows_if_let() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "iflet.rs",
        r#"
fn scan(v: Option<i32>) {
    if let Some(x) = v {
        let _ = x;
    }
    while let Some(x) = Some(1) {
        let _ = x;
        break;
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "IfStatementAssignment");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn duplicated_array_key_reports_duplicate_struct_fields() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "dup.rs",
        r#"
struct Point { x: i32, y: i32 }
fn make() -> Point {
    Point { x: 1, y: 2, x: 3 }
}
"#,
    );
    let (code, out, err) = run_only(&path, "DuplicatedArrayKey");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("DuplicatedArrayKey"), "stdout={out:?}");
    assert!(out.contains("x"), "stdout={out:?}");
}

#[test]
fn duplicated_array_key_allows_unique_fields() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        r#"
struct Point { x: i32, y: i32 }
fn make() -> Point {
    Point { x: 1, y: 2 }
}
"#,
    );
    let (code, out, err) = run_only(&path, "DuplicatedArrayKey");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn static_access_reports_other_type_path_call() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "static.rs",
        r#"
struct Helper;
impl Helper {
    fn make() -> i32 { 1 }
}
struct Worker;
impl Worker {
    fn run() -> i32 {
        Helper::make()
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "StaticAccess");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("StaticAccess"), "stdout={out:?}");
    assert!(out.contains("Helper"), "stdout={out:?}");
    assert!(out.contains("run"), "stdout={out:?}");
}

#[test]
fn if_statement_assignment_reports_assign_in_while_condition() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "while.rs",
        r#"
fn scan(mut x: i32) {
    while { x = 1; false } {
        let _ = x;
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "IfStatementAssignment");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("IfStatementAssignment"), "stdout={out:?}");
}

#[test]
fn static_access_honours_ignorepattern() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ignore.rs",
        r#"
struct Helper;
impl Helper {
    fn make() -> i32 { 1 }
}
struct Worker;
impl Worker {
    fn create_worker() -> i32 {
        Helper::make()
    }
}
"#,
    );
    let xml = dir.path().join("sa.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="sa">
  <rule ref="cleancode/StaticAccess">
    <properties>
      <property name="ignorepattern" value="(^create)i"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn static_access_allows_self_path_and_exceptions() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        r#"
struct Worker;
impl Worker {
    fn make() -> i32 { 1 }
    fn run() -> i32 {
        Self::make()
    }
}
struct Math;
impl Math {
    fn abs(n: i32) -> i32 { n }
}
fn use_math() -> i32 {
    Math::abs(1)
}
"#,
    );
    let xml = dir.path().join("sa.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="sa">
  <rule ref="cleancode/StaticAccess">
    <properties>
      <property name="exceptions" value="Math"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}
