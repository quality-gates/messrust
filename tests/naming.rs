//! naming rules through the injectable CLI entry.

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
    run_cli(&[path.to_str().unwrap(), "text", "naming", "--only", rule])
}

#[test]
fn all_naming_rules_load_without_verbose_skips() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", "fn clean_entry() {}\n");
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "naming", "--verbose"]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        !err.contains("Skipping unimplemented rule"),
        "stderr={err:?}"
    );
}

#[test]
fn short_class_name_reports_short_struct() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "short.rs", "struct Fo;\n");
    let (code, out, err) = run_only(&path, "ShortClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ShortClassName"), "stdout={out:?}");
    assert!(out.contains("Fo"), "stdout={out:?}");
    assert!(out.contains("3"), "stdout={out:?}");
}

#[test]
fn short_class_name_allows_long_enough_and_exceptions() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "ok.rs", "struct Foo;\nstruct Id;\n");
    let xml = dir.path().join("sc.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="sc">
  <rule ref="naming/ShortClassName">
    <properties>
      <property name="exceptions" value="Id"/>
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
fn short_class_name_reports_short_trait_and_enum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "te.rs", "trait Ab {}\nenum X { A }\n");
    let (code, out, err) = run_only(&path, "ShortClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("Ab"), "stdout={out:?}");
    assert!(out.contains("X"), "stdout={out:?}");
}

#[test]
fn long_class_name_reports_and_respects_subtract_prefix() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "long.rs",
        "struct ATooLongClassNameThatHintsAtADesignProblem;\nstruct AbstractSomeLongishName;\n",
    );
    let xml = dir.path().join("lc.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="lc">
  <rule ref="naming/LongClassName">
    <properties>
      <property name="maximum" value="20"/>
      <property name="subtract-prefixes" value="Abstract"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        out.contains("ATooLongClassNameThatHintsAtADesignProblem"),
        "stdout={out:?}"
    );
    assert!(
        !out.contains("AbstractSomeLongishName"),
        "stdout={out:?}"
    );
}

#[test]
fn short_variable_reports_field_param_local_but_skips_for_binder() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "sv.rs",
        r#"
struct Something {
    q: i32,
}
fn main(xs: i32) {
    let r = 20;
    for i in 0..10 {
        let _ = (i, r, xs);
    }
    while let Some(j) = Some(1) {
        let _ = j;
        break;
    }
}
"#,
    );
    let (code, out, err) = run_only(&path, "ShortVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("q"), "stdout={out:?}");
    assert!(out.contains("xs"), "stdout={out:?}");
    assert!(out.contains("r"), "stdout={out:?}");
    assert!(!out.contains("like i."), "stdout={out:?}");
    assert!(!out.contains("like j."), "stdout={out:?}");
}

#[test]
fn short_class_name_reports_short_union() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "u.rs", "union Ab { x: u32 }\n");
    let (code, out, err) = run_only(&path, "ShortClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("Ab"), "stdout={out:?}");
}

#[test]
fn constant_naming_checks_trait_associated_const() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "tc.rs",
        "trait T { const bad_name: i32; }\n",
    );
    let (code, out, err) = run_only(&path, "ConstantNamingConventions");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("bad_name"), "stdout={out:?}");
}

#[test]
fn short_variable_exception_suppresses() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "ex.rs", "struct S { q: i32 }\n");
    let xml = dir.path().join("sv.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="sv">
  <rule ref="naming/ShortVariable">
    <properties>
      <property name="exceptions" value="q"/>
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
fn long_variable_reports_and_subtracts_prefix() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "lv.rs",
        r#"
struct Something {
    really_long_int_name_var: i32,
    x_really_long_name: i32,
}
fn main(interesting_arguments_list: i32) {
    let other_really_long_name = -5;
    let _ = (interesting_arguments_list, other_really_long_name);
}
"#,
    );
    let xml = dir.path().join("lv.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="lv">
  <rule ref="naming/LongVariable">
    <properties>
      <property name="maximum" value="20"/>
      <property name="subtract-prefixes" value="x_"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("really_long_int_name_var"), "stdout={out:?}");
    assert!(out.contains("interesting_arguments_list"), "stdout={out:?}");
    assert!(out.contains("other_really_long_name"), "stdout={out:?}");
    assert!(!out.contains("x_really_long_name"), "stdout={out:?}");
}

#[test]
fn short_method_name_reports_short_fn_and_method() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "sm.rs",
        r#"
struct ShortMethod;
impl ShortMethod {
    fn a(&self, index: i32) {
        let _ = index;
    }
}
fn go() {}
"#,
    );
    let (code, out, err) = run_only(&path, "ShortMethodName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("a"), "stdout={out:?}");
    assert!(out.contains("go"), "stdout={out:?}");
    assert!(out.contains("ShortMethod"), "stdout={out:?}");
}

#[test]
fn short_method_name_exception_suppresses() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "sme.rs", "fn go() {}\n");
    let xml = dir.path().join("sm.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="sm">
  <rule ref="naming/ShortMethodName">
    <properties>
      <property name="exceptions" value="go"/>
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
fn constant_naming_default_upper_accepts_screaming_snake() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "cn.rs",
        "const MY_NUM: i32 = 0;\nconst badName: i32 = 1;\nstatic ALSO_OK: i32 = 2;\n",
    );
    let (code, out, err) = run_only(&path, "ConstantNamingConventions");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("badName"), "stdout={out:?}");
    assert!(!out.contains("like MY_NUM") && !out.contains("MY_NUM should"), "stdout={out:?}");
    assert!(!out.contains("ALSO_OK should"), "stdout={out:?}");
}

#[test]
fn constant_naming_pascal_option_accepts_pascal_case() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "cnp.rs",
        "const MyNum: i32 = 0;\nconst MY_NUM: i32 = 1;\n",
    );
    let xml = dir.path().join("cn.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="cn">
  <rule ref="naming/ConstantNamingConventions">
    <properties>
      <property name="convention" value="pascal"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("MY_NUM"), "stdout={out:?}");
    assert!(out.contains("PascalCase"), "stdout={out:?}");
    assert!(!out.contains("MyNum should"), "stdout={out:?}");
}

#[test]
fn boolean_get_method_name_reports_get_bool_without_params() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "bg.rs",
        r#"
struct Foo;
impl Foo {
    fn get_foo(&self) -> bool { true }
    fn is_foo(&self) -> bool { true }
    fn get_bar(&self, x: i32) -> bool { x > 0 }
    fn get_count(&self) -> i32 { 1 }
}
"#,
    );
    let (code, out, err) = run_only(&path, "BooleanGetMethodName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("get_foo"), "stdout={out:?}");
    assert!(!out.contains("is_foo"), "stdout={out:?}");
    assert!(!out.contains("get_bar"), "stdout={out:?}");
    assert!(!out.contains("get_count"), "stdout={out:?}");
}

#[test]
fn boolean_get_method_name_can_check_parameterized() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "bgp.rs",
        "fn get_ready(flag: bool) -> bool { flag }\n",
    );
    let xml = dir.path().join("bg.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="bg">
  <rule ref="naming/BooleanGetMethodName">
    <properties>
      <property name="checkParameterizedMethods" value="true"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("get_ready"), "stdout={out:?}");
}
