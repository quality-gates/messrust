//! unusedcode rules through the injectable CLI entry.
//!
//! Seam: `messrust::run`. Each test asserts the exit code and the user-visible
//! text (location line, rule name, and message). Skip-then-report cases keep a
//! later unused name visible when an earlier name is skipped.

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
    run_cli(&[path.to_str().unwrap(), "text", "unusedcode", "--only", rule])
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
fn all_unusedcode_rules_load_without_verbose_skips() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", "fn clean_entry() {}\n");
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "unusedcode", "--verbose"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        !err.contains("Skipping unimplemented rule"),
        "stderr={err:?}"
    );
}

#[test]
fn unused_local_variable_reports_unread_let() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "local.rs",
        "fn f() {\n    let dead_local = 5;\n    let kept_local = 6;\n    let _ = kept_local;\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "UnusedLocalVariable",
        "Avoid unused local variables such as 'dead_local'.",
    );
    assert!(!out.contains("kept_local"), "stdout={out:?}");
}

#[test]
fn unused_local_variable_skips_underscore_names() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "underscore_locals.rs",
        "fn f() {\n    let _ = 1;\n    let _ignored = 2;\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_variable_reports_after_underscore_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "underscore_then_dead_local.rs",
        "fn f() {\n    let _ignored = 1;\n    let dead_local = 2;\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "UnusedLocalVariable",
        "Avoid unused local variables such as 'dead_local'.",
    );
    assert!(!out.contains("_ignored"), "stdout={out:?}");
}

#[test]
fn unused_local_variable_reports_after_used_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "used_then_dead.rs",
        "fn f() {\n    let kept_local = 1;\n    let dead_local = 2;\n    let _ = kept_local;\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "UnusedLocalVariable",
        "Avoid unused local variables such as 'dead_local'.",
    );
    assert!(!out.contains("kept_local"), "stdout={out:?}");
}

#[test]
fn unused_local_variable_does_not_treat_enum_variants_as_bindings() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "variant.rs",
        "fn f(value: Option<i32>) {\n    match value {\n        Some(number) => { let _ = number; }\n        None => {}\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_variable_counts_rust_format_capture_as_a_read() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "format_capture.rs",
        "fn f() -> String {\n    let error = \"bad\";\n    format!(\"failure: {error}\")\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_variable_counts_assignment_index_as_a_read() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "assignment_index.rs",
        "fn f(values: &mut [i32]) {\n    let index = 0;\n    values[index] = 1;\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_variable_does_not_count_destructuring_targets_as_reads() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "destructuring_assignment.rs",
        "fn f() {\n    let left;\n    let right;\n    (left, right) = (1, 2);\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?} stdout={out:?}");
    assert_finding(
        &out,
        &path,
        2,
        "UnusedLocalVariable",
        "Avoid unused local variables such as 'left'.",
    );
    assert_finding(
        &out,
        &path,
        3,
        "UnusedLocalVariable",
        "Avoid unused local variables such as 'right'.",
    );
}

#[test]
fn unused_local_variable_honours_exceptions() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "ex.rs", "fn f() {\n    let allowed = 1;\n}\n");
    let xml = dir.path().join("ul.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="ul">
  <rule ref="unusedcode/UnusedLocalVariable">
    <properties>
      <property name="exceptions" value="allowed"/>
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
fn unused_local_variable_reports_after_exception_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ex_then_dead.rs",
        "fn f() {\n    let allowed = 1;\n    let dead_local = 2;\n}\n",
    );
    let xml = dir.path().join("ul.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="ul">
  <rule ref="unusedcode/UnusedLocalVariable">
    <properties>
      <property name="exceptions" value="allowed"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "UnusedLocalVariable",
        "Avoid unused local variables such as 'dead_local'.",
    );
    assert!(!out.contains("allowed"), "stdout={out:?}");
}

#[test]
fn unused_local_variable_counts_use_in_nested_block() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "nested.rs",
        "fn f() {\n    let kept = 1;\n    {\n        let _ = kept;\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_variable_counts_use_in_closure() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "closure.rs",
        "fn f() -> i32 {\n    let kept = 1;\n    let c = || kept;\n    c()\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_variable_counts_use_in_macro_argument() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "macro_arg.rs",
        "fn f() {\n    let kept = 1;\n    println!(\"{}\", kept);\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_variable_reports_unread_in_other_function() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "other_fn.rs",
        "fn used_fn() {\n    let kept = 1;\n    let _ = kept;\n}\n\
         fn unread_fn() {\n    let dead_local = 2;\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        6,
        "UnusedLocalVariable",
        "Avoid unused local variables such as 'dead_local'.",
    );
    assert!(!out.contains("kept"), "stdout={out:?}");
}

#[test]
fn unused_formal_parameter_reports_unread_param() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "param.rs",
        "fn f(dead_param: i32, kept_param: i32) -> i32 {\n    kept_param\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedFormalParameter");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "UnusedFormalParameter",
        "Avoid unused parameters such as 'dead_param'.",
    );
    assert!(!out.contains("kept_param"), "stdout={out:?}");
}

#[test]
fn unused_formal_parameter_skips_underscore_and_self() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "selfp.rs",
        "struct S;\nimpl S {\n    fn m(&self, _skip: i32) {}\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedFormalParameter");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_formal_parameter_reports_after_underscore_param() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "underscore_then_dead_param.rs",
        "fn f(_skip: i32, dead_param: i32) {}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedFormalParameter");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "UnusedFormalParameter",
        "Avoid unused parameters such as 'dead_param'.",
    );
    assert!(!out.contains("_skip"), "stdout={out:?}");
}

#[test]
fn unused_formal_parameter_reports_after_used_param() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "used_then_dead_param.rs",
        "fn f(kept_param: i32, dead_param: i32) -> i32 {\n    kept_param\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedFormalParameter");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "UnusedFormalParameter",
        "Avoid unused parameters such as 'dead_param'.",
    );
    assert!(!out.contains("kept_param"), "stdout={out:?}");
}

#[test]
fn unused_formal_parameter_counts_use_in_nested_block() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "param_nested.rs",
        "fn f(kept: i32) {\n    {\n        let _ = kept;\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedFormalParameter");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_formal_parameter_counts_use_in_closure() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "param_closure.rs",
        "fn f(kept: i32) -> i32 {\n    let c = || kept;\n    c()\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedFormalParameter");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_formal_parameter_counts_use_in_macro_argument() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "param_macro.rs",
        "fn f(kept: i32) {\n    println!(\"{}\", kept);\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedFormalParameter");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_formal_parameter_reports_unread_in_other_function() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "param_other_fn.rs",
        "fn used_fn(kept: i32) -> i32 {\n    kept\n}\n\
         fn unread_fn(dead_param: i32) {}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedFormalParameter");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        4,
        "UnusedFormalParameter",
        "Avoid unused parameters such as 'dead_param'.",
    );
    assert!(!out.contains("kept"), "stdout={out:?}");
}

#[test]
fn unused_private_field_reports_unread_private_field() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "field.rs",
        "struct S {\n    dead_field: i32,\n    kept_field: i32,\n}\n\
         impl S {\n    fn read(&self) -> i32 {\n        self.kept_field\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateField");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "UnusedPrivateField",
        "Avoid unused private fields such as 'dead_field'.",
    );
    assert!(!out.contains("kept_field"), "stdout={out:?}");
}

#[test]
fn unused_private_field_skips_pub_fields() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "pubf.rs",
        "pub struct S {\n    pub exposed: i32,\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateField");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_skips_fields_used_by_serialization_derives() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "serialized.rs",
        "#[derive(Serialize)]\nstruct Report {\n    message: String,\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateField");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_skips_pub_crate_fields() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "cratef.rs",
        "pub struct S {\n    pub(crate) exposed: i32,\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateField");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_skips_underscore_names() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "underscore_field.rs",
        "struct S {\n    _ignored: i32,\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateField");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_reports_after_underscore_and_used_field() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "field_order.rs",
        "struct S {\n    _ignored: i32,\n    kept_field: i32,\n    dead_field: i32,\n}\n\
         impl S {\n    fn read(&self) -> i32 {\n        self.kept_field\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateField");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        4,
        "UnusedPrivateField",
        "Avoid unused private fields such as 'dead_field'.",
    );
    assert!(!out.contains("_ignored"), "stdout={out:?}");
    assert!(!out.contains("kept_field"), "stdout={out:?}");
}

#[test]
fn unused_private_field_counts_use_in_nested_block() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "field_nested.rs",
        "struct S {\n    kept_field: i32,\n}\n\
         impl S {\n    fn read(&self) -> i32 {\n        {\n            self.kept_field\n        }\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateField");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_counts_use_in_closure() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "field_closure.rs",
        "struct S {\n    kept_field: i32,\n}\n\
         impl S {\n    fn read(&self) -> i32 {\n        let c = || self.kept_field;\n        c()\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateField");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_counts_use_in_macro_argument() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "field_macro.rs",
        "struct S {\n    kept_field: i32,\n}\n\
         impl S {\n    fn read(&self) {\n        println!(\"{}\", self.kept_field);\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateField");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_counts_use_in_different_impl_item() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "field_other_item.rs",
        "struct S {\n    kept_field: i32,\n    dead_field: i32,\n}\n\
         impl S {\n    fn unused_reader(&self) {}\n    fn reader(&self) -> i32 {\n        self.kept_field\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateField");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "UnusedPrivateField",
        "Avoid unused private fields such as 'dead_field'.",
    );
    assert!(!out.contains("kept_field"), "stdout={out:?}");
}

#[test]
fn unused_private_method_reports_uncalled_private_method() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "meth.rs",
        "struct S;\nimpl S {\n    fn dead_method(&self) {}\n    fn kept_method(&self) {}\n    pub fn entry(&self) {\n        self.kept_method();\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "UnusedPrivateMethod",
        "Avoid unused private methods such as 'dead_method'.",
    );
    assert!(!out.contains("kept_method"), "stdout={out:?}");
}

#[test]
fn unused_private_method_skips_pub_methods() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "pubm.rs",
        "struct S;\nimpl S {\n    pub fn exposed(&self) {}\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_method_counts_path_call() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "pathcall.rs",
        "struct S;\nimpl S {\n    fn helper() {}\n    pub fn entry() {\n        S::helper();\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_method_skips_trait_impl_methods() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "trait_impl.rs",
        "trait T { fn required(&self); }\n\
         struct S;\n\
         impl T for S {\n    fn required(&self) {}\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_method_skips_underscore_names() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "underscore_method.rs",
        "struct S;\nimpl S {\n    fn _ignored(&self) {}\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_method_reports_after_underscore_and_called_method() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "method_order.rs",
        "struct S;\nimpl S {\n    fn _ignored(&self) {}\n    fn kept_method(&self) {}\n    fn dead_method(&self) {}\n\
         pub fn entry(&self) {\n        self.kept_method();\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        5,
        "UnusedPrivateMethod",
        "Avoid unused private methods such as 'dead_method'.",
    );
    assert!(!out.contains("_ignored"), "stdout={out:?}");
    assert!(!out.contains("kept_method"), "stdout={out:?}");
}

#[test]
fn unused_private_method_counts_ident_read_of_same_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "method_ident.rs",
        "struct S;\nimpl S {\n    fn helper(&self) {}\n    fn dead_method(&self) {}\n\
         pub fn entry(&self) {\n        let helper = 1;\n        let _ = helper;\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        4,
        "UnusedPrivateMethod",
        "Avoid unused private methods such as 'dead_method'.",
    );
    assert!(!out.contains("'helper'"), "stdout={out:?}");
}

#[test]
fn unused_private_method_counts_use_in_nested_block() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "method_nested.rs",
        "struct S;\nimpl S {\n    fn kept_method(&self) {}\n    pub fn entry(&self) {\n        {\n            self.kept_method();\n        }\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_method_counts_use_in_closure() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "method_closure.rs",
        "struct S;\nimpl S {\n    fn kept_method(&self) {}\n    pub fn entry(&self) {\n        let c = || self.kept_method();\n        c();\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_method_counts_use_in_macro_argument() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "method_macro.rs",
        "struct S;\nimpl S {\n    fn kept_method(&self) -> i32 { 1 }\n    pub fn entry(&self) {\n        println!(\"{}\", self.kept_method());\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_method_counts_call_from_different_impl_item() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "method_other_item.rs",
        "struct S;\nimpl S {\n    fn kept_method(&self) {}\n    fn dead_method(&self) {}\n\
         pub fn entry(&self) {\n        self.kept_method();\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        4,
        "UnusedPrivateMethod",
        "Avoid unused private methods such as 'dead_method'.",
    );
    assert!(!out.contains("kept_method"), "stdout={out:?}");
}

#[test]
fn unused_private_method_reports_private_method_in_inherent_impl_after_trait_impl() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "after_trait.rs",
        r#"
trait MyTrait {
    fn trait_fn(&self);
}
struct S;
impl MyTrait for S {
    fn trait_fn(&self) {}
}
struct Other;
impl Other {
    fn uncalled_private(&self) {}
}
"#,
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        11,
        "UnusedPrivateMethod",
        "Avoid unused private methods such as 'uncalled_private'.",
    );
}

#[test]
fn unused_local_variable_counts_todo_and_unreachable_format_capture() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "todo_capture.rs",
        r#"
fn work() {
    let a = 1;
    let b = 2;
    let c = 3;
    if a > 0 {
        todo!("{b}");
    } else {
        unreachable!("{c}");
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}
