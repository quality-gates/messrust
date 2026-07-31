//! design rules through the injectable CLI entry.

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
    run_cli(&[path.to_str().unwrap(), "text", "design", "--only", rule])
}

#[test]
fn all_design_rules_load_without_verbose_skips() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", "fn clean_entry() {}\n");
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "design", "--verbose"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        !err.contains("Skipping unimplemented rule"),
        "stderr={err:?}"
    );
}

#[test]
fn goto_statement_stays_quiet_on_ordinary_control_flow() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "flow.rs",
        r#"
fn choose(flag: bool) -> i32 {
    if flag {
        return 1;
    }
    loop {
        break;
    }
    0
}
"#,
    );
    let (code, out, err) = run_only(&path, "GotoStatement");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
    assert!(!out.contains("GotoStatement"), "stdout={out:?}");
}

#[test]
fn exit_expression_reports_process_exit() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "exit.rs",
        r#"
fn bail(code: i32) {
    if code != 0 {
        std::process::exit(code);
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "ExitExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExitExpression"), "stdout={out:?}");
    assert!(out.contains("bail"), "stdout={out:?}");
}

#[test]
fn exit_expression_reports_process_abort() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "abort.rs",
        r#"
fn panic_out() {
    std::process::abort();
}
"#,
    );
    let (code, out, err) = run_only(&path, "ExitExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExitExpression"), "stdout={out:?}");
    assert!(out.contains("panic_out"), "stdout={out:?}");
}

#[test]
fn exit_expression_allows_ordinary_calls() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        r#"
fn work() -> i32 {
    helper(1)
}
fn helper(n: i32) -> i32 { n }
"#,
    );
    let (code, out, err) = run_only(&path, "ExitExpression");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn count_in_loop_expression_reports_len_in_while() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "loop.rs",
        r#"
fn scan(items: &[i32]) {
    let mut i = 0;
    while i < items.len() {
        i += 1;
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "CountInLoopExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("CountInLoopExpression"), "stdout={out:?}");
    assert!(out.contains("len"), "stdout={out:?}");
}

#[test]
fn count_in_loop_expression_reports_len_in_for_range() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "forlen.rs",
        r#"
fn scan(items: &[i32]) {
    for i in 0..items.len() {
        let _ = items[i];
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "CountInLoopExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("CountInLoopExpression"), "stdout={out:?}");
    assert!(out.contains("len"), "stdout={out:?}");
}

#[test]
fn count_in_loop_expression_allows_cached_len() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        r#"
fn scan(items: &[i32]) {
    let n = items.len();
    let mut i = 0;
    while i < n {
        i += 1;
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "CountInLoopExpression");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn development_code_fragment_reports_println_macro() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "debug.rs",
        r#"
fn work(items: &[i32]) {
    for item in items {
        println!("{item}");
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "DevelopmentCodeFragment");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("DevelopmentCodeFragment"), "stdout={out:?}");
    assert!(out.contains("println"), "stdout={out:?}");
}

#[test]
fn development_code_fragment_honours_unwanted_functions() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "trace.rs",
        r#"
fn work() {
    tracing_debug("hi");
}
fn tracing_debug(_msg: &str) {}
"#,
    );
    let xml = dir.path().join("dev.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="dev">
  <rule ref="design/DevelopmentCodeFragment">
    <properties>
      <property name="unwanted-functions" value="tracing_debug"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("DevelopmentCodeFragment"), "stdout={out:?}");
    assert!(out.contains("tracing_debug"), "stdout={out:?}");
}

#[test]
fn empty_catch_block_reports_empty_if_let_err() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "swallow.rs",
        r#"
fn work() {
    if let Err(_e) = might_fail() {}
}
fn might_fail() -> Result<(), String> { Ok(()) }
"#,
    );
    let (code, out, err) = run_only(&path, "EmptyCatchBlock");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("EmptyCatchBlock"), "stdout={out:?}");
    assert!(out.contains("work"), "stdout={out:?}");
}

#[test]
fn empty_catch_block_reports_empty_err_match_arm() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "match.rs",
        r#"
fn work() {
    match might_fail() {
        Ok(()) => {}
        Err(_e) => {}
    }
}
fn might_fail() -> Result<(), String> { Ok(()) }
"#,
    );
    let (code, out, err) = run_only(&path, "EmptyCatchBlock");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("EmptyCatchBlock"), "stdout={out:?}");
    assert!(out.contains("work"), "stdout={out:?}");
}

#[test]
fn empty_catch_block_allows_handled_err() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        r#"
fn work() {
    if let Err(e) = might_fail() {
        let _ = e;
    }
}
fn might_fail() -> Result<(), String> { Ok(()) }
"#,
    );
    let (code, out, err) = run_only(&path, "EmptyCatchBlock");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn coupling_between_objects_reports_high_dependency_count() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "coupled.rs",
        r#"
struct A;
struct B;
struct C;
struct D;
struct E;
struct F;
struct G;
struct H;
struct I;
struct J;
struct K;
struct L;
struct M;
struct N;
struct Foo {
    a: A, b: B, c: C, d: D, e: E, f: F, g: G,
    h: H, i: I, j: J, k: K, l: L, m: M, n: N,
}
"#,
    );
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("CouplingBetweenObjects"), "stdout={out:?}");
    assert!(out.contains("Foo"), "stdout={out:?}");
}

#[test]
fn coupling_between_objects_allows_few_dependencies() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        r#"
struct Bar;
struct Foo {
    bar: Bar,
    count: i32,
}
"#,
    );
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn global_variable_reports_mutated_static_mut() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "glob.rs",
        r#"
static mut COUNTER: i32 = 0;
fn bump() {
    unsafe { COUNTER += 1; }
}
"#,
    );
    let (code, out, err) = run_only(&path, "GlobalVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("GlobalVariable"), "stdout={out:?}");
    assert!(out.contains("COUNTER"), "stdout={out:?}");
}

#[test]
fn global_variable_allows_immutable_static_and_unmutated_static_mut() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        r#"
static MAX: i32 = 10;
static mut UNUSED: i32 = 0;
fn read_max() -> i32 { MAX }
"#,
    );
    let (code, out, err) = run_only(&path, "GlobalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn global_variable_report_immutable_flags_unmutated_static_mut() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "imm.rs",
        r#"
static mut UNUSED: i32 = 0;
fn read() -> i32 { unsafe { UNUSED } }
"#,
    );
    let xml = dir.path().join("gv.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="gv">
  <rule ref="design/GlobalVariable">
    <properties>
      <property name="report-immutable" value="true"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("UNUSED"), "stdout={out:?}");
}

#[test]
fn lack_of_cohesion_reports_disjoint_method_groups() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "lcom.rs",
        r#"
struct Server {
    conns: i32,
    stats: i32,
}
impl Server {
    fn accept(&mut self) { self.conns += 1; }
    fn record(&mut self) { self.stats += 1; }
}
"#,
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("LackOfCohesionOfMethods"), "stdout={out:?}");
    assert!(out.contains("Server"), "stdout={out:?}");
}

#[test]
fn lack_of_cohesion_allows_shared_field_methods() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        r#"
struct Counter {
    value: i32,
}
impl Counter {
    fn bump(&mut self) { self.value += 1; }
    fn get(&self) -> i32 { self.value }
}
"#,
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}
