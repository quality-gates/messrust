//! controversial rules through the injectable CLI entry.

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
    run_cli(&[
        path.to_str().unwrap(),
        "text",
        "controversial",
        "--only",
        rule,
    ])
}

#[test]
fn all_controversial_rules_load_without_verbose_skips() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", "fn clean_entry() {}\n");
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "controversial",
        "--verbose",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        !err.contains("Skipping unimplemented rule"),
        "stderr={err:?}"
    );
}

#[test]
fn camel_case_class_name_reports_snake_case_type() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "t.rs", "struct bad_name;\n");
    let (code, out, err) = run_only(&path, "CamelCaseClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("CamelCaseClassName"), "stdout={out:?}");
    assert!(out.contains("bad_name"), "stdout={out:?}");
    assert!(out.contains("PascalCase"), "stdout={out:?}");
}

#[test]
fn camel_case_class_name_allows_pascal_case_types() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        "struct GoodName;\nenum Status { Ready }\ntrait Handler {}\nunion Bits { x: u32 }\n",
    );
    let (code, out, err) = run_only(&path, "CamelCaseClassName");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn camel_case_class_name_abbreviations_reject_consecutive_caps() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "abbr.rs", "struct HTTPClient;\nstruct HttpClient;\n");
    let xml = dir.path().join("abbr.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="abbr">
  <rule ref="controversial/CamelCaseClassName">
    <properties>
      <property name="camelcase-abbreviations" value="true"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("HTTPClient"), "stdout={out:?}");
    assert!(!out.contains("HttpClient"), "stdout={out:?}");
}

#[test]
fn camel_case_method_name_reports_pascal_case_fn() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "m.rs",
        "fn GetName() {}\nimpl S { fn AlsoBad() {} }\nstruct S;\n",
    );
    let (code, out, err) = run_only(&path, "CamelCaseMethodName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("GetName"), "stdout={out:?}");
    assert!(out.contains("AlsoBad"), "stdout={out:?}");
    assert!(out.contains("snake_case"), "stdout={out:?}");
}

#[test]
fn camel_case_method_name_allows_snake_case() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ok.rs",
        "fn get_name() {}\nfn _unused() {}\nimpl S { fn also_good() {} }\nstruct S;\n",
    );
    let (code, out, err) = run_only(&path, "CamelCaseMethodName");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn camel_case_property_name_reports_camel_case_field() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "p.rs",
        "struct S { badField: i32, good_field: i32 }\n",
    );
    let (code, out, err) = run_only(&path, "CamelCasePropertyName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("badField"), "stdout={out:?}");
    assert!(!out.contains("good_field"), "stdout={out:?}");
}

#[test]
fn camel_case_parameter_name_reports_camel_case_param() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "par.rs",
        "fn work(userName: i32, user_id: i32) { let _ = (userName, user_id); }\n",
    );
    let (code, out, err) = run_only(&path, "CamelCaseParameterName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("userName"), "stdout={out:?}");
    assert!(!out.contains("user_id"), "stdout={out:?}");
}

#[test]
fn camel_case_variable_name_reports_camel_case_local() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "v.rs",
        "fn work() {\n    let dataModule = 1;\n    let data_module = 2;\n    let _ = (dataModule, data_module);\n}\n",
    );
    let (code, out, err) = run_only(&path, "CamelCaseVariableName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dataModule"), "stdout={out:?}");
    assert!(!out.contains("data_module"), "stdout={out:?}");
}

#[test]
fn camel_case_variable_name_skips_blank_ident() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "blank.rs", "fn work() { let _ = 1; }\n");
    let (code, out, err) = run_only(&path, "CamelCaseVariableName");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn camel_case_class_name_rejects_pascal_case_with_underscore() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "us.rs", "struct Bad_Name;\n");
    let (code, out, err) = run_only(&path, "CamelCaseClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("Bad_Name"), "stdout={out:?}");
}

#[test]
fn camel_case_class_name_default_allows_consecutive_caps() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "http.rs", "struct HTTPClient;\n");
    let xml = dir.path().join("bare.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="bare">
  <rule name="CamelCaseClassName"
         message="The type {0} is not named in PascalCase."
         class="PHPMD\Rule\Controversial\CamelCaseClassName">
    <priority>1</priority>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn camel_case_class_name_reports_bad_type_after_good_type() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "order.rs", "struct GoodName;\nstruct bad_name;\n");
    let (code, out, err) = run_only(&path, "CamelCaseClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("bad_name"), "stdout={out:?}");
    assert!(!out.contains("GoodName"), "stdout={out:?}");
}

#[test]
fn camel_case_class_name_abbreviations_allow_single_capital_word() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "abbr_ok.rs", "struct HttpClient;\n");
    let xml = dir.path().join("abbr_ok.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="abbr_ok">
  <rule ref="controversial/CamelCaseClassName">
    <properties>
      <property name="camelcase-abbreviations" value="true"/>
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
fn camel_case_method_name_reports_underscore_digit_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "digit.rs", "fn _1() {}\n");
    let (code, out, err) = run_only(&path, "CamelCaseMethodName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("_1"), "stdout={out:?}");
}

#[test]
fn camel_case_method_name_rejects_mixed_case_with_underscore() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "mixed.rs", "fn user_Name() {}\n");
    let (code, out, err) = run_only(&path, "CamelCaseMethodName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("user_Name"), "stdout={out:?}");
}

#[test]
fn camel_case_method_name_reports_bad_fn_after_good_fn() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "order.rs", "fn good_name() {}\nfn BadName() {}\n");
    let (code, out, err) = run_only(&path, "CamelCaseMethodName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("BadName"), "stdout={out:?}");
    assert!(!out.contains("good_name"), "stdout={out:?}");
}

#[test]
fn camel_case_property_name_skips_tuple_field_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "tuple.rs", "struct Point(i32, i32);\n");
    let (code, out, err) = run_only(&path, "CamelCasePropertyName");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn camel_case_property_name_skips_blank_ident() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "blank_field.rs", "struct S { _: i32 }\n");
    let (code, out, err) = run_only(&path, "CamelCasePropertyName");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn camel_case_property_name_reports_bad_field_after_good_field() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "order.rs",
        "struct S { good_field: i32, badField: i32 }\n",
    );
    let (code, out, err) = run_only(&path, "CamelCasePropertyName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("badField"), "stdout={out:?}");
    assert!(!out.contains("good_field"), "stdout={out:?}");
}

#[test]
fn camel_case_parameter_name_skips_blank_ident() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "blank_param.rs", "fn work(_: i32) {}\n");
    let (code, out, err) = run_only(&path, "CamelCaseParameterName");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn camel_case_parameter_name_reports_underscore_digit_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "par_digit.rs", "fn work(_1: i32) {}\n");
    let (code, out, err) = run_only(&path, "CamelCaseParameterName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("_1"), "stdout={out:?}");
}

#[test]
fn camel_case_parameter_name_reports_bad_param_after_good_param() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "order.rs",
        "fn work(good_name: i32, badName: i32) { let _ = (good_name, badName); }\n",
    );
    let (code, out, err) = run_only(&path, "CamelCaseParameterName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("badName"), "stdout={out:?}");
    assert!(!out.contains("good_name"), "stdout={out:?}");
}

#[test]
fn camel_case_variable_name_reports_underscore_digit_name() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "var_digit.rs", "fn work() { let _1 = 1; let _ = _1; }\n");
    let (code, out, err) = run_only(&path, "CamelCaseVariableName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("_1"), "stdout={out:?}");
}

#[test]
fn camel_case_variable_name_rejects_mixed_case_with_underscore() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "var_mixed.rs",
        "fn work() { let user_Id = 1; let _ = user_Id; }\n",
    );
    let (code, out, err) = run_only(&path, "CamelCaseVariableName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("user_Id"), "stdout={out:?}");
}

#[test]
fn camel_case_variable_name_reports_bad_local_after_good_local() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "order.rs",
        "fn work() {\n    let good_name = 1;\n    let badName = 2;\n    let _ = (good_name, badName);\n}\n",
    );
    let (code, out, err) = run_only(&path, "CamelCaseVariableName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("badName"), "stdout={out:?}");
    assert!(!out.contains("good_name"), "stdout={out:?}");
}

#[test]
fn clean_idiomatic_rust_passes_full_controversial_set() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "clean.rs",
        r#"
struct Widget {
    item_count: i32,
}
impl Widget {
    fn new(item_count: i32) -> Self {
        let local_value = item_count;
        Self { item_count: local_value }
    }
}
fn helper(user_name: &str) -> usize {
    user_name.len()
}
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "controversial"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}
