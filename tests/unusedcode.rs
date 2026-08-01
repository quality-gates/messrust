//! unusedcode rules through the injectable CLI entry.

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
    assert!(out.contains("UnusedLocalVariable"), "stdout={out:?}");
    assert!(out.contains("dead_local"), "stdout={out:?}");
    assert!(!out.contains("kept_local"), "stdout={out:?}");
}

#[test]
fn unused_local_variable_skips_underscore_names() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "us.rs",
        "fn f() {\n    let _ = 1;\n    let _ignored = 2;\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedLocalVariable");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
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
    assert!(out.contains("left"), "stdout={out:?}");
    assert!(out.contains("right"), "stdout={out:?}");
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
fn unused_formal_parameter_reports_unread_param() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "param.rs",
        "fn f(dead_param: i32, kept_param: i32) -> i32 {\n    kept_param\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedFormalParameter");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("UnusedFormalParameter"), "stdout={out:?}");
    assert!(out.contains("dead_param"), "stdout={out:?}");
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
    assert!(out.contains("UnusedPrivateField"), "stdout={out:?}");
    assert!(out.contains("dead_field"), "stdout={out:?}");
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
fn unused_private_method_reports_uncalled_private_method() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "meth.rs",
        "struct S;\nimpl S {\n    fn dead_method(&self) {}\n    fn kept_method(&self) {}\n    pub fn entry(&self) {\n        self.kept_method();\n    }\n}\n",
    );
    let (code, out, err) = run_only(&path, "UnusedPrivateMethod");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("UnusedPrivateMethod"), "stdout={out:?}");
    assert!(out.contains("dead_method"), "stdout={out:?}");
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
