//! design rules through the injectable CLI entry.
//!
//! Seam: `messrust::run`. Each test asserts the exit code and the user-visible
//! text (location line, rule name, and message). Threshold rules show the value
//! at the maximum and above it.

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

fn write_ruleset(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    path
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
        "fn choose(flag: bool) -> i32 {\n    if flag {\n        return 1;\n    }\n    loop {\n        break;\n    }\n    0\n}\n",
    );
    let (code, out, err) = run_only(&path, "GotoStatement");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
    assert!(!out.contains("GotoStatement"), "stdout={out:?}");
}

#[test]
fn exit_expression_reports_process_exit_at_call_line() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "exit.rs",
        "fn bail(code: i32) {\n    if code != 0 {\n        std::process::exit(code);\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "ExitExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "ExitExpression",
        "The function bail() contains an exit expression.",
    );
}

#[test]
fn exit_expression_reports_process_abort_at_call_line() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "abort.rs",
        "fn panic_out() {\n    std::process::abort();\n}\n",
    );
    let (code, out, err) = run_only(&path, "ExitExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "ExitExpression",
        "The function panic_out() contains an exit expression.",
    );
}

#[test]
fn exit_expression_reports_method_with_function_kind_label() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "method_exit.rs",
        "struct Worker;\nimpl Worker {\n    fn bail(&self) {\n        std::process::exit(1);\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "ExitExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        4,
        "ExitExpression",
        "The method bail() contains an exit expression.",
    );
}

#[test]
fn exit_expression_skips_nested_function_then_reports_outer() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "nested_exit.rs",
        "fn outer() {\n    fn nested() {\n        std::process::exit(1);\n    }\n    std::process::abort();\n}\n",
    );
    let (code, out, err) = run_only(&path, "ExitExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        5,
        "ExitExpression",
        "The function outer() contains an exit expression.",
    );
    assert!(
        !out.contains(":3"),
        "nested exit must stay quiet: stdout={out:?}"
    );
}

#[test]
fn exit_expression_allows_ordinary_calls() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        "fn work() -> i32 {\n    helper(1)\n}\nfn helper(n: i32) -> i32 { n }\n",
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
        "fn scan(items: &[i32]) {\n    let mut i = 0;\n    while i < items.len() {\n        i += 1;\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "CountInLoopExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "CountInLoopExpression",
        "Avoid using len in while loops.",
    );
}

#[test]
fn count_in_loop_expression_reports_capacity_in_while() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "cap_while.rs",
        "fn scan(items: Vec<i32>) {\n    while items.capacity() > 0 {\n        break;\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "CountInLoopExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "CountInLoopExpression",
        "Avoid using capacity in while loops.",
    );
}

#[test]
fn count_in_loop_expression_reports_len_in_for_range() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "forlen.rs",
        "fn scan(items: &[i32]) {\n    for i in 0..items.len() {\n        let _ = items[i];\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "CountInLoopExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "CountInLoopExpression",
        "Avoid using len in for loops.",
    );
}

#[test]
fn count_in_loop_expression_reports_bare_capacity_call_in_for() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "forcap.rs",
        "fn scan() {\n    for i in 0..capacity() {\n        let _ = i;\n    }\n}\nfn capacity() -> usize { 3 }\n",
    );
    let (code, out, err) = run_only(&path, "CountInLoopExpression");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "CountInLoopExpression",
        "Avoid using capacity in for loops.",
    );
}

#[test]
fn count_in_loop_expression_allows_cached_len() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        "fn scan(items: &[i32]) {\n    let n = items.len();\n    let mut i = 0;\n    while i < n {\n        i += 1;\n    }\n}\n",
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
        "fn work(items: &[i32]) {\n    for item in items {\n        println!(\"{item}\");\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "DevelopmentCodeFragment");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "DevelopmentCodeFragment",
        "The function work() calls the typical debug function println() which is mostly only used during development.",
    );
}

#[test]
fn development_code_fragment_reports_print_eprintln_and_dbg() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "macros.rs",
        "fn work() {\n    print!(\"x\");\n    eprintln!(\"y\");\n    dbg!(1);\n}\n",
    );
    let (code, out, err) = run_only(&path, "DevelopmentCodeFragment");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "DevelopmentCodeFragment",
        "The function work() calls the typical debug function print() which is mostly only used during development.",
    );
    assert_finding(
        &out,
        &path,
        3,
        "DevelopmentCodeFragment",
        "The function work() calls the typical debug function eprintln() which is mostly only used during development.",
    );
    assert_finding(
        &out,
        &path,
        4,
        "DevelopmentCodeFragment",
        "The function work() calls the typical debug function dbg() which is mostly only used during development.",
    );
}

#[test]
fn development_code_fragment_reports_method_with_parent_image() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "method_dbg.rs",
        "struct Worker;\nimpl Worker {\n    fn run(&self) {\n        println!(\"hi\");\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "DevelopmentCodeFragment");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        4,
        "DevelopmentCodeFragment",
        "The method Worker::run() calls the typical debug function println() which is mostly only used during development.",
    );
}

#[test]
fn development_code_fragment_reports_function_call_form() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "call.rs",
        "fn work() {\n    dbg(1);\n}\nfn dbg(_n: i32) {}\n",
    );
    let (code, out, err) = run_only(&path, "DevelopmentCodeFragment");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "DevelopmentCodeFragment",
        "The function work() calls the typical debug function dbg() which is mostly only used during development.",
    );
}

#[test]
fn development_code_fragment_honours_unwanted_functions() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "trace.rs",
        "fn work() {\n    tracing_debug(\"hi\");\n}\nfn tracing_debug(_msg: &str) {}\n",
    );
    let xml = write_ruleset(
        dir.path(),
        "dev.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="dev">
  <rule ref="design/DevelopmentCodeFragment">
    <properties>
      <property name="unwanted-functions" value="tracing_debug"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "DevelopmentCodeFragment",
        "The function work() calls the typical debug function tracing_debug() which is mostly only used during development.",
    );
}

#[test]
fn development_code_fragment_ignores_empty_unwanted_entries() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", "fn work() { helper(); }\nfn helper() {}\n");
    let xml = write_ruleset(
        dir.path(),
        "dev.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="dev">
  <rule ref="design/DevelopmentCodeFragment">
    <properties>
      <property name="unwanted-functions" value=", ,"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn empty_catch_block_reports_empty_if_let_err() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "swallow.rs",
        "fn work() {\n    if let Err(_e) = might_fail() {}\n}\nfn might_fail() -> Result<(), String> { Ok(()) }\n",
    );
    let (code, out, err) = run_only(&path, "EmptyCatchBlock");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "EmptyCatchBlock",
        "Avoid using empty catch blocks in work.",
    );
}

#[test]
fn empty_catch_block_reports_empty_err_match_arm() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "match.rs",
        "fn work() {\n    match might_fail() {\n        Ok(()) => {}\n        Err(_e) => {}\n    }\n}\nfn might_fail() -> Result<(), String> { Ok(()) }\n",
    );
    let (code, out, err) = run_only(&path, "EmptyCatchBlock");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        4,
        "EmptyCatchBlock",
        "Avoid using empty catch blocks in work.",
    );
}

#[test]
fn empty_catch_block_reports_empty_err_unit_arm() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "unit.rs",
        "fn work() {\n    match might_fail() {\n        Ok(()) => {}\n        Err(_e) => ()\n    }\n}\nfn might_fail() -> Result<(), String> { Ok(()) }\n",
    );
    let (code, out, err) = run_only(&path, "EmptyCatchBlock");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        4,
        "EmptyCatchBlock",
        "Avoid using empty catch blocks in work.",
    );
}

#[test]
fn empty_catch_block_reports_or_pattern_err_arm() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "orpat.rs",
        "fn work(v: Result<(), &'static str>) {\n    match v {\n        Ok(()) => {}\n        Err(\"a\") | Err(\"b\") => {}\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "EmptyCatchBlock");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        4,
        "EmptyCatchBlock",
        "Avoid using empty catch blocks in work.",
    );
}

#[test]
fn empty_catch_block_allows_handled_err() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        "fn work() {\n    if let Err(e) = might_fail() {\n        let _ = e;\n    }\n}\nfn might_fail() -> Result<(), String> { Ok(()) }\n",
    );
    let (code, out, err) = run_only(&path, "EmptyCatchBlock");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

fn thirteen_dep_struct(name: &str) -> String {
    format!(
        "struct A; struct B; struct C; struct D; struct E; struct F; struct G;\n\
         struct H; struct I; struct J; struct K; struct L; struct M;\n\
         struct {name} {{\n\
             a: A, b: B, c: C, d: D, e: E, f: F, g: G,\n\
             h: H, i: I, j: J, k: K, l: L, m: M,\n\
         }}\n"
    )
}

#[test]
fn coupling_between_objects_reports_at_exact_maximum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "at_max.rs", &thirteen_dep_struct("Foo"));
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "CouplingBetweenObjects",
        "The class Foo has a coupling between objects value of 13. Consider to reduce the number of dependencies under 13.",
    );
}

#[test]
fn coupling_between_objects_reports_above_maximum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "above.rs",
        "struct A; struct B; struct C; struct D; struct E; struct F; struct G;\n\
         struct H; struct I; struct J; struct K; struct L; struct M; struct N;\n\
         struct Foo {\n\
             a: A, b: B, c: C, d: D, e: E, f: F, g: G,\n\
             h: H, i: I, j: J, k: K, l: L, m: M, n: N,\n\
         }\n",
    );
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "CouplingBetweenObjects",
        "The class Foo has a coupling between objects value of 14. Consider to reduce the number of dependencies under 13.",
    );
}

#[test]
fn coupling_between_objects_allows_below_maximum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "below.rs",
        "struct A; struct B; struct C; struct D; struct E; struct F; struct G;\n\
         struct H; struct I; struct J; struct K; struct L;\n\
         struct Foo {\n\
             a: A, b: B, c: C, d: D, e: E, f: F, g: G,\n\
             h: H, i: I, j: J, k: K, l: L,\n\
         }\n",
    );
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn coupling_between_objects_allows_few_dependencies() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        "struct Bar;\nstruct Foo {\n    bar: Bar,\n    count: i32,\n}\n",
    );
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn coupling_between_objects_reports_enum_at_maximum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "enum_cbo.rs",
        "struct A; struct B; struct C; struct D; struct E; struct F; struct G;\n\
         struct H; struct I; struct J; struct K; struct L; struct M;\n\
         enum Foo {\n\
             V(A, B, C, D, E, F, G, H, I, J, K, L, M),\n\
         }\n",
    );
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "CouplingBetweenObjects",
        "The class Foo has a coupling between objects value of 13. Consider to reduce the number of dependencies under 13.",
    );
}

#[test]
fn coupling_between_objects_reports_union_at_maximum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "union_cbo.rs",
        "struct A; struct B; struct C; struct D; struct E; struct F; struct G;\n\
         struct H; struct I; struct J; struct K; struct L; struct M;\n\
         union Foo {\n\
             a: A, b: B, c: C, d: D, e: E, f: F, g: G,\n\
             h: H, i: I, j: J, k: K, l: L, m: M,\n\
         }\n",
    );
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "CouplingBetweenObjects",
        "The class Foo has a coupling between objects value of 13. Consider to reduce the number of dependencies under 13.",
    );
}

#[test]
fn coupling_between_objects_skips_trait_then_reports_later_struct() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "trait_then.rs",
        &format!(
            "trait Skip {{\n    fn visit(&self);\n}}\n{}",
            thirteen_dep_struct("Foo")
        ),
    );
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("Foo"), "stdout={out:?}");
    assert!(
        !out.contains("Skip"),
        "trait must stay quiet: stdout={out:?}"
    );
}

#[test]
fn coupling_between_objects_ignores_required_trait_method_signatures() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "visitor.rs",
        "struct A; struct B; struct C; struct D; struct E; struct F; struct G;\n\
         struct H; struct I; struct J; struct K; struct L; struct M; struct N;\n\
         trait Visit {\n\
             fn visit(&mut self, a: A, b: B, c: C, d: D, e: E, f: F, g: G,\n\
                      h: H, i: I, j: J, k: K, l: L, m: M, n: N);\n\
         }\n\
         struct Collector;\n\
         impl Visit for Collector {\n\
             fn visit(&mut self, _a: A, _b: B, _c: C, _d: D, _e: E, _f: F, _g: G,\n\
                      _h: H, _i: I, _j: J, _k: K, _l: L, _m: M, _n: N) {}\n\
         }\n",
    );
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn coupling_between_objects_counts_inherent_method_deps_after_free_fn() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "method_deps.rs",
        "struct A; struct B; struct C; struct D; struct E; struct F; struct G;\n\
         struct H; struct I; struct J; struct K; struct L; struct M;\n\
         struct Foo;\n\
         fn free(_a: A, _b: B, _c: C, _d: D, _e: E, _f: F, _g: G,\n\
                 _h: H, _i: I, _j: J, _k: K, _l: L, _m: M) {}\n\
         impl Foo {\n\
             fn touch(&self, _a: A, _b: B, _c: C, _d: D, _e: E, _f: F, _g: G,\n\
                      _h: H, _i: I, _j: J, _k: K, _l: L, _m: M) {}\n\
         }\n",
    );
    let (code, out, err) = run_only(&path, "CouplingBetweenObjects");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "CouplingBetweenObjects",
        "The class Foo has a coupling between objects value of 13. Consider to reduce the number of dependencies under 13.",
    );
}

#[test]
fn coupling_between_objects_excludes_builtins_and_self_owner() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "builtins.rs",
        "struct Foo {\n    count: i32,\n    flag: bool,\n    me: Self,\n}\n",
    );
    let xml = write_ruleset(
        dir.path(),
        "cbo.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="cbo">
  <rule ref="design/CouplingBetweenObjects">
    <properties>
      <property name="maximum" value="1"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn coupling_between_objects_excludes_owner_type_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "owner.rs",
        "struct Node {\n    next: Node,\n    mark: i32,\n}\n",
    );
    let xml = write_ruleset(
        dir.path(),
        "cbo.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="cbo">
  <rule ref="design/CouplingBetweenObjects">
    <properties>
      <property name="maximum" value="1"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn coupling_between_objects_honours_custom_maximum_boundary() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "custom.rs",
        "struct A; struct B;\nstruct Foo { a: A, b: B }\n",
    );
    let xml = write_ruleset(
        dir.path(),
        "cbo.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="cbo">
  <rule ref="design/CouplingBetweenObjects">
    <properties>
      <property name="maximum" value="2"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "CouplingBetweenObjects",
        "The class Foo has a coupling between objects value of 2. Consider to reduce the number of dependencies under 2.",
    );
}

#[test]
fn global_variable_reports_mutated_static_mut() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "glob.rs",
        "static mut COUNTER: i32 = 0;\nfn bump() {\n    unsafe { COUNTER += 1; }\n}\n",
    );
    let (code, out, err) = run_only(&path, "GlobalVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "GlobalVariable",
        "Avoid using static mutable state: COUNTER.",
    );
}

#[test]
fn global_variable_allows_immutable_static_and_unmutated_static_mut() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        "static MAX: i32 = 10;\nstatic mut UNUSED: i32 = 0;\nfn read_max() -> i32 { MAX }\n",
    );
    let (code, out, err) = run_only(&path, "GlobalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn global_variable_default_report_immutable_stays_false() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "imm.rs",
        "static mut UNUSED: i32 = 0;\nfn read() -> i32 { unsafe { UNUSED } }\n",
    );
    let xml = write_ruleset(
        dir.path(),
        "gv.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="gv">
  <rule ref="design/GlobalVariable"/>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn global_variable_report_immutable_flags_unmutated_static_mut() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "imm.rs",
        "static mut UNUSED: i32 = 0;\nfn read() -> i32 { unsafe { UNUSED } }\n",
    );
    let xml = write_ruleset(
        dir.path(),
        "gv.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="gv">
  <rule ref="design/GlobalVariable">
    <properties>
      <property name="report-immutable" value="true"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "GlobalVariable",
        "Avoid using static mutable state: UNUSED.",
    );
}

#[test]
fn lack_of_cohesion_reports_disjoint_method_groups() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "lcom.rs",
        "struct Server {\n    conns: i32,\n    stats: i32,\n}\nimpl Server {\n    fn accept(&mut self) { self.conns += 1; }\n    fn record(&mut self) { self.stats += 1; }\n}\n",
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "LackOfCohesionOfMethods",
        "The Server has a Lack of Cohesion Of Methods (LCOM4) value of 2. Consider to split this class into 2 smaller classes.",
    );
}

#[test]
fn lack_of_cohesion_allows_shared_field_methods() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        "struct Counter {\n    value: i32,\n}\nimpl Counter {\n    fn bump(&mut self) { self.value += 1; }\n    fn get(&self) -> i32 { self.value }\n}\n",
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn lack_of_cohesion_allows_accessor_only_data_carrier() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "accessors.rs",
        "struct Pair {\n    left: i32,\n    right: i32,\n}\nimpl Pair {\n    fn left(&self) -> i32 { self.left }\n    fn right(&self) -> i32 { self.right }\n    fn set_left(&mut self, v: i32) { self.left = v; }\n}\n",
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn lack_of_cohesion_links_through_getter_calls() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "via_get.rs",
        "struct S {\n    a: i32,\n    b: i32,\n}\nimpl S {\n    fn get_a(&self) -> i32 { self.a }\n    fn via(&mut self) { let _ = self.get_a(); }\n    fn bump_b(&mut self) { self.b += 1; }\n}\n",
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "LackOfCohesionOfMethods",
        "The S has a Lack of Cohesion Of Methods (LCOM4) value of 2. Consider to split this class into 2 smaller classes.",
    );
}

#[test]
fn lack_of_cohesion_does_not_treat_foreign_field_access_as_accessor() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "foreign.rs",
        "struct S {\n    a: i32,\n    b: i32,\n}\nimpl S {\n    fn get(other: &S) -> i32 { other.a }\n    fn via(&mut self) { let _ = self.get(self); }\n    fn bump_a(&mut self) { self.a += 1; }\n    fn bump_b(&mut self) { self.b += 1; }\n}\n",
    );
    let xml = write_ruleset(
        dir.path(),
        "lcom.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="lcom">
  <rule ref="design/LackOfCohesionOfMethods">
    <properties>
      <property name="maximum" value="2"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "LackOfCohesionOfMethods",
        "The S has a Lack of Cohesion Of Methods (LCOM4) value of 3. Consider to split this class into 3 smaller classes.",
    );
}

#[test]
fn lack_of_cohesion_skips_multi_statement_bodies_as_accessors() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "multi.rs",
        "struct S {\n    a: i32,\n    b: i32,\n}\nimpl S {\n    fn get_a(&self) -> i32 {\n        let x = self.a;\n        x\n    }\n    fn bump_a(&mut self) { self.a += 1; }\n    fn bump_b(&mut self) { self.b += 1; }\n}\n",
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "LackOfCohesionOfMethods",
        "The S has a Lack of Cohesion Of Methods (LCOM4) value of 2. Consider to split this class into 2 smaller classes.",
    );
}

#[test]
fn lack_of_cohesion_reports_after_quiet_enum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "enum_then_struct.rs",
        "enum Skip {\n    A { left: i32, right: i32 },\n}\nimpl Skip {\n    fn left(&self) -> i32 {\n        match self {\n            Skip::A { left, .. } => *left,\n        }\n    }\n    fn right(&self) -> i32 {\n        match self {\n            Skip::A { right, .. } => *right,\n        }\n    }\n}\nstruct Server {\n    conns: i32,\n    stats: i32,\n}\nimpl Server {\n    fn accept(&mut self) { self.conns += 1; }\n    fn record(&mut self) { self.stats += 1; }\n}\n",
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        16,
        "LackOfCohesionOfMethods",
        "The Server has a Lack of Cohesion Of Methods (LCOM4) value of 2. Consider to split this class into 2 smaller classes.",
    );
    assert!(
        !out.contains("Skip"),
        "quiet enum must not appear: stdout={out:?}"
    );
}

#[test]
fn lack_of_cohesion_reports_union_with_disjoint_methods() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "union_lcom.rs",
        "union Pair {\n    left: i32,\n    right: i32,\n}\nimpl Pair {\n    unsafe fn bump_left(&mut self) { self.left += 1; }\n    unsafe fn bump_right(&mut self) { self.right += 1; }\n}\n",
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "LackOfCohesionOfMethods",
        "The Pair has a Lack of Cohesion Of Methods (LCOM4) value of 2. Consider to split this class into 2 smaller classes.",
    );
}

#[test]
fn lack_of_cohesion_skips_trait_then_reports_later_struct() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "trait_lcom.rs",
        "trait Skip {\n    fn visit_left(&mut self);\n    fn visit_right(&mut self);\n}\nstruct Server {\n    conns: i32,\n    stats: i32,\n}\nimpl Server {\n    fn accept(&mut self) { self.conns += 1; }\n    fn record(&mut self) { self.stats += 1; }\n}\n",
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        5,
        "LackOfCohesionOfMethods",
        "The Server has a Lack of Cohesion Of Methods (LCOM4) value of 2. Consider to split this class into 2 smaller classes.",
    );
}

#[test]
fn lack_of_cohesion_ignores_required_trait_methods() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "visitor.rs",
        "trait Visit {\n    fn visit_left(&mut self);\n    fn visit_right(&mut self);\n}\nstruct Collector {\n    left: i32,\n    right: i32,\n}\nimpl Visit for Collector {\n    fn visit_left(&mut self) { self.left += 1; }\n    fn visit_right(&mut self) { self.right += 1; }\n}\n",
    );
    let (code, out, err) = run_only(&path, "LackOfCohesionOfMethods");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn lack_of_cohesion_honours_custom_maximum_at_boundary() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "custom_lcom.rs",
        "struct Server {\n    conns: i32,\n    stats: i32,\n}\nimpl Server {\n    fn accept(&mut self) { self.conns += 1; }\n    fn record(&mut self) { self.stats += 1; }\n}\n",
    );
    let xml_ok = write_ruleset(
        dir.path(),
        "ok.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="lcom">
  <rule ref="design/LackOfCohesionOfMethods">
    <properties>
      <property name="maximum" value="2"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml_ok.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");

    let path3 = write_file(
        dir.path(),
        "three.rs",
        "struct Server {\n    a: i32,\n    b: i32,\n    c: i32,\n}\nimpl Server {\n    fn fa(&mut self) { self.a += 1; }\n    fn fb(&mut self) { self.b += 1; }\n    fn fc(&mut self) { self.c += 1; }\n}\n",
    );
    let (code, out, err) = run_cli(&[path3.to_str().unwrap(), "text", xml_ok.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path3,
        1,
        "LackOfCohesionOfMethods",
        "The Server has a Lack of Cohesion Of Methods (LCOM4) value of 3. Consider to split this class into 3 smaller classes.",
    );
}
