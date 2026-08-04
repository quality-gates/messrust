//! cleancode rules through the injectable CLI entry.
//!
//! Seam: `messrust::run`. Each test asserts the exit code and the user-visible
//! text (location line, rule name, and message). Properties such as
//! `exceptions` and `ignorepattern` are exercised as separate cases.

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

fn assert_finding(out: &str, path: &Path, line: usize, rule: &str, message: &str) {
    let loc = format!("{}:{line}", path.display());
    assert!(
        out.contains(&loc),
        "missing location {loc} in stdout={out:?}"
    );
    assert!(out.contains(rule), "missing rule {rule} in stdout={out:?}");
    assert!(
        out.contains(message),
        "missing message {message:?} in stdout={out:?}"
    );
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
fn boolean_argument_flag_reports_bool_param_at_param_line() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "flag.rs",
        "fn process(flag: bool) {\n    let _ = flag;\n}\n",
    );
    let (code, out, err) = run_only(&path, "BooleanArgumentFlag");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "BooleanArgumentFlag",
        "The method process has a boolean flag argument flag, which is a certain sign of a Single Responsibility Principle violation.",
    );
}

#[test]
fn boolean_argument_flag_reports_method_with_enclosing_type_image() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "method.rs",
        "struct Worker;\nimpl Worker {\n    fn run(flag: bool) {\n        let _ = flag;\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "BooleanArgumentFlag");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "BooleanArgumentFlag",
        "The method Worker::run has a boolean flag argument flag, which is a certain sign of a Single Responsibility Principle violation.",
    );
}

#[test]
fn boolean_argument_flag_skips_underscore_and_non_bool() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        "fn process(_flag: bool, count: i32) {\n    let _ = count;\n}\n",
    );
    let (code, out, err) = run_only(&path, "BooleanArgumentFlag");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn boolean_argument_flag_reports_later_bool_after_underscore_param() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "mixed.rs",
        "fn process(_skip: bool, flag: bool) {\n    let _ = (_skip, flag);\n}\n",
    );
    let (code, out, err) = run_only(&path, "BooleanArgumentFlag");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "BooleanArgumentFlag",
        "The method process has a boolean flag argument flag, which is a certain sign of a Single Responsibility Principle violation.",
    );
    assert!(
        !out.contains("argument _skip"),
        "underscore param must stay quiet: stdout={out:?}"
    );
}

#[test]
fn boolean_argument_flag_honours_exceptions_on_enclosing_type() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ex.rs",
        "struct Allowed;\nimpl Allowed {\n    fn run(flag: bool) {\n        let _ = flag;\n    }\n}\nstruct Other;\nimpl Other {\n    fn run(flag: bool) {\n        let _ = flag;\n    }\n}\n",
    );
    let xml = dir.path().join("baf.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="baf">
  <rule ref="cleancode/BooleanArgumentFlag">
    <properties>
      <property name="exceptions" value="Allowed"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("Allowed::run"),
        "exceptions must skip Allowed: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        9,
        "BooleanArgumentFlag",
        "The method Other::run has a boolean flag argument flag, which is a certain sign of a Single Responsibility Principle violation.",
    );
}

#[test]
fn boolean_argument_flag_honours_ignorepattern_on_method_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ign.rs",
        "fn create_with(flag: bool) {\n    let _ = flag;\n}\nfn process(flag: bool) {\n    let _ = flag;\n}\n",
    );
    let xml = dir.path().join("baf.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="baf">
  <rule ref="cleancode/BooleanArgumentFlag">
    <properties>
      <property name="ignorepattern" value="(^create)i"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("create_with"),
        "ignorepattern must skip create_with: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        4,
        "BooleanArgumentFlag",
        "The method process has a boolean flag argument flag, which is a certain sign of a Single Responsibility Principle violation.",
    );
}

#[test]
fn else_expression_reports_terminal_else_at_else_line() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "else.rs",
        "fn choose(flag: bool) -> i32 {\n    if flag {\n        1\n    } else {\n        2\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "ElseExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        4,
        "ElseExpression",
        "The method choose uses an else expression. Else clauses are basically not necessary and you can simplify the code by not using them.",
    );
}

#[test]
fn else_expression_reports_terminal_else_but_not_else_if_only_chains() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "elseif_only.rs",
        "fn chain(n: i32) -> i32 {\n    if n > 0 {\n        1\n    } else if n < 0 {\n        -1\n    } else {\n        0\n    }\n}\nfn no_terminal(n: i32) -> i32 {\n    if n > 0 {\n        1\n    } else if n < 0 {\n        -1\n    } else if true {\n        0\n    } else if false {\n        2\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "ElseExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        6,
        "ElseExpression",
        "The method chain uses an else expression. Else clauses are basically not necessary and you can simplify the code by not using them.",
    );
    assert!(
        !out.contains("no_terminal"),
        "else-if-only chain must stay quiet: stdout={out:?}"
    );
}

#[test]
fn else_expression_skips_nested_fn_bodies() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "nested.rs",
        "fn outer(flag: bool) -> i32 {\n    fn inner(flag: bool) -> i32 {\n        if flag {\n            1\n        } else {\n            2\n        }\n    }\n    inner(flag)\n}\n",
    );
    let (code, out, err) = run_only(&path, "ElseExpression");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn if_statement_assignment_reports_line_and_column_in_if_condition() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "assign.rs",
        "fn scan(mut x: i32) {\n    if { x = 1; true } {\n        let _ = x;\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "IfStatementAssignment");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "IfStatementAssignment",
        "Avoid assigning values to variables in if clauses and the like (line '2', column '10').",
    );
}

#[test]
fn if_statement_assignment_reports_line_and_column_in_while_condition() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "while.rs",
        "fn scan(mut x: i32) {\n    while { x = 1; false } {\n        let _ = x;\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "IfStatementAssignment");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "IfStatementAssignment",
        "Avoid assigning values to variables in if clauses and the like (line '2', column '13').",
    );
}

#[test]
fn if_statement_assignment_allows_if_let_and_while_let() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "iflet.rs",
        "fn scan(v: Option<i32>) {\n    if let Some(x) = v {\n        let _ = x;\n    }\n    while let Some(x) = Some(1) {\n        let _ = x;\n        break;\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "IfStatementAssignment");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn if_statement_assignment_skips_nested_fn_bodies() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "nested.rs",
        "fn outer(mut x: i32) {\n    fn inner(mut x: i32) {\n        if { x = 1; true } {\n            let _ = x;\n        }\n    }\n    inner(x);\n}\n",
    );
    let (code, out, err) = run_only(&path, "IfStatementAssignment");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn duplicated_array_key_reports_duplicate_field_with_first_line() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "dup.rs",
        "struct Point { x: i32, y: i32 }\nfn make() -> Point {\n    Point { x: 1, y: 2, x: 3 }\n}\n",
    );
    let (code, out, err) = run_only(&path, "DuplicatedArrayKey");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "DuplicatedArrayKey",
        "Duplicated array key x, first declared at line 3.",
    );
}

#[test]
fn duplicated_array_key_reports_duplicate_across_different_lines() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "dup_lines.rs",
        "struct Point { x: i32, y: i32 }\nfn make() -> Point {\n    Point {\n        x: 1,\n        y: 2,\n        x: 3,\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "DuplicatedArrayKey");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        6,
        "DuplicatedArrayKey",
        "Duplicated array key x, first declared at line 4.",
    );
}

#[test]
fn duplicated_array_key_allows_unique_fields() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        "struct Point { x: i32, y: i32 }\nfn make() -> Point {\n    Point { x: 1, y: 2 }\n}\n",
    );
    let (code, out, err) = run_only(&path, "DuplicatedArrayKey");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn static_access_reports_other_type_path_call_at_call_line() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "static.rs",
        "struct Helper;\nimpl Helper {\n    fn make() -> i32 { 1 }\n}\nstruct Worker;\nimpl Worker {\n    fn run() -> i32 {\n        Helper::make()\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "StaticAccess");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        8,
        "StaticAccess",
        "Avoid using static access to class 'Helper' in method 'run'.",
    );
}

#[test]
fn static_access_allows_self_path_and_same_enclosing_type() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "self.rs",
        "struct Worker;\nimpl Worker {\n    fn make() -> i32 { 1 }\n    fn run() -> i32 {\n        Self::make()\n    }\n    fn again() -> i32 {\n        Worker::make()\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "StaticAccess");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn static_access_skips_snake_case_module_paths() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "modpath.rs",
        "mod helper {\n    pub fn make() -> i32 { 1 }\n}\nfn run() -> i32 {\n    helper::make()\n}\n",
    );
    let (code, out, err) = run_only(&path, "StaticAccess");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn static_access_skips_bare_path_without_call() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "bare.rs",
        "struct Helper;\nimpl Helper {\n    const VALUE: i32 = 1;\n}\nfn run() -> i32 {\n    let _ = Helper::VALUE;\n    0\n}\n",
    );
    let (code, out, err) = run_only(&path, "StaticAccess");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn static_access_honours_exceptions() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ex.rs",
        "struct Math;\nimpl Math {\n    fn abs(n: i32) -> i32 { n }\n}\nstruct Helper;\nimpl Helper {\n    fn make() -> i32 { 1 }\n}\nfn use_both() -> i32 {\n    Math::abs(Helper::make())\n}\n",
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
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("'Math'"),
        "exceptions must skip Math: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        10,
        "StaticAccess",
        "Avoid using static access to class 'Helper' in method 'use_both'.",
    );
}

#[test]
fn static_access_honours_ignorepattern() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ignore.rs",
        "struct Helper;\nimpl Helper {\n    fn make() -> i32 { 1 }\n}\nstruct Worker;\nimpl Worker {\n    fn create_worker() -> i32 {\n        Helper::make()\n    }\n    fn run() -> i32 {\n        Helper::make()\n    }\n}\n",
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
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("create_worker"),
        "ignorepattern must skip create_worker: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        11,
        "StaticAccess",
        "Avoid using static access to class 'Helper' in method 'run'.",
    );
}

#[test]
fn static_access_skips_nested_fn_bodies() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "nested.rs",
        "struct Helper;\nimpl Helper {\n    fn make() -> i32 { 1 }\n}\nfn outer() -> i32 {\n    fn inner() -> i32 {\n        Helper::make()\n    }\n    inner()\n}\n",
    );
    let (code, out, err) = run_only(&path, "StaticAccess");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn static_access_prefers_rightmost_pascal_receiver() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "path.rs",
        "mod outer {\n    pub struct Helper;\n    impl Helper {\n        pub fn make() -> i32 { 1 }\n    }\n}\nfn run() -> i32 {\n    outer::Helper::make()\n}\n",
    );
    let (code, out, err) = run_only(&path, "StaticAccess");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        8,
        "StaticAccess",
        "Avoid using static access to class 'Helper' in method 'run'.",
    );
}
