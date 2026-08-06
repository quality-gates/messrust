//! naming rules through the injectable CLI entry.
//!
//! Seam: `messrust::run`. Each test asserts the exit code and the user-visible
//! text (location line, rule name, and full message). Length boundaries,
//! exception lists, prefix/suffix trimming, and convention properties are
//! exercised so a mutation of a comparison, continue, or default cannot escape.

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

// --- ShortClassName ---------------------------------------------------------

#[test]
fn short_class_name_reports_exact_message_for_short_struct() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "short.rs", "struct Fo;\n");
    let (code, out, err) = run_only(&path, "ShortClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "ShortClassName",
        "Avoid types with short names like Fo. Configured minimum length is 3.",
    );
}

#[test]
fn short_class_name_boundary_allows_exact_minimum_reports_below() {
    let dir = TempDir::new().unwrap();
    // Foo length 3 == default minimum → allowed; Ab length 2 → reported.
    let path = write_file(dir.path(), "bound.rs", "struct Foo;\nstruct Ab;\n");
    let (code, out, err) = run_only(&path, "ShortClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("like Foo."),
        "exact minimum must pass: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        2,
        "ShortClassName",
        "Avoid types with short names like Ab. Configured minimum length is 3.",
    );
}

#[test]
fn short_class_name_keeps_scanning_after_long_enough_type() {
    let dir = TempDir::new().unwrap();
    // First type is long enough (continue). Second is short. break would miss Ab.
    let path = write_file(dir.path(), "scan.rs", "struct LongEnough;\nstruct Ab;\n");
    let (code, out, err) = run_only(&path, "ShortClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "ShortClassName",
        "Avoid types with short names like Ab. Configured minimum length is 3.",
    );
}

#[test]
fn short_class_name_exception_skips_listed_name_but_reports_later() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "ex.rs", "struct Id;\nstruct Ab;\n");
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
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("like Id."),
        "exception Id must stay quiet: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        2,
        "ShortClassName",
        "Avoid types with short names like Ab. Configured minimum length is 3.",
    );
}

#[test]
fn short_class_name_honours_each_entry_in_exceptions_list() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "ex2.rs", "struct Id;\nstruct Io;\nstruct Ab;\n");
    let xml = dir.path().join("sc.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="sc">
  <rule ref="naming/ShortClassName">
    <properties>
      <property name="exceptions" value="Id, Io"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(!out.contains("like Id."), "Id quiet: stdout={out:?}");
    assert!(!out.contains("like Io."), "Io quiet: stdout={out:?}");
    assert_finding(
        &out,
        &path,
        3,
        "ShortClassName",
        "Avoid types with short names like Ab. Configured minimum length is 3.",
    );
}

#[test]
fn short_class_name_reports_short_trait_and_enum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "te.rs", "trait Ab {}\nenum X { A }\n");
    let (code, out, err) = run_only(&path, "ShortClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "ShortClassName",
        "Avoid types with short names like Ab. Configured minimum length is 3.",
    );
    assert_finding(
        &out,
        &path,
        2,
        "ShortClassName",
        "Avoid types with short names like X. Configured minimum length is 3.",
    );
}

#[test]
fn short_class_name_reports_short_union() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "u.rs", "union Ab { x: u32 }\n");
    let (code, out, err) = run_only(&path, "ShortClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "ShortClassName",
        "Avoid types with short names like Ab. Configured minimum length is 3.",
    );
}

#[test]
fn short_class_name_honours_custom_minimum() {
    let dir = TempDir::new().unwrap();
    // With minimum 5, Abcd (4) fails and Abcde (5) passes.
    let path = write_file(dir.path(), "min.rs", "struct Abcde;\nstruct Abcd;\n");
    let xml = dir.path().join("sc.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="sc">
  <rule ref="naming/ShortClassName">
    <properties>
      <property name="minimum" value="5"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("like Abcde."),
        "length 5 at minimum 5 must pass: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        2,
        "ShortClassName",
        "Avoid types with short names like Abcd. Configured minimum length is 5.",
    );
}

// --- LongClassName ----------------------------------------------------------

#[test]
fn long_class_name_reports_exact_message_and_boundary() {
    let dir = TempDir::new().unwrap();
    // Exact length 20 allowed; length 21 reported. First OK so continue≠break.
    let ok = format!("A{}", "x".repeat(19)); // 20 chars
    let bad = format!("A{}", "x".repeat(20)); // 21 chars
    assert_eq!(ok.len(), 20);
    assert_eq!(bad.len(), 21);
    let path = write_file(
        dir.path(),
        "long.rs",
        &format!("struct {ok};\nstruct {bad};\n"),
    );
    let xml = dir.path().join("lc.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="lc">
  <rule ref="naming/LongClassName">
    <properties>
      <property name="maximum" value="20"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains(&format!("like {ok}.")),
        "exact maximum must pass: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        2,
        "LongClassName",
        &format!(
            "Avoid excessively long type names like {bad}. Keep type name length under 20."
        ),
    );
}

#[test]
fn long_class_name_subtracts_prefix_and_suffix() {
    let dir = TempDir::new().unwrap();
    // After stripping Abstract (8), SomeLongishNameX is 15 → under 20.
    // After stripping Suffix, PrefixTooLongClassNm is still long.
    // ATooLongClassNameThatHintsAtADesignProblem has no trim → reported.
    let path = write_file(
        dir.path(),
        "trim.rs",
        "struct AbstractSomeLongishNameX;\nstruct PrefixTooLongClassNmSuffix;\nstruct ATooLongClassNameThatHintsAtADesignProblem;\n",
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
      <property name="subtract-suffixes" value="Suffix"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("AbstractSomeLongishNameX"),
        "prefix trim must keep it quiet: stdout={out:?}"
    );
    assert!(
        !out.contains("PrefixTooLongClassNmSuffix"),
        "suffix trim must keep it quiet: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        3,
        "LongClassName",
        "Avoid excessively long type names like ATooLongClassNameThatHintsAtADesignProblem. Keep type name length under 20.",
    );
}

#[test]
fn long_class_name_default_maximum_allows_forty_char_name() {
    let dir = TempDir::new().unwrap();
    // Default maximum is 40. A 40-char name must pass; 41 must fail.
    // 40 chars: FortyCharTypeNameThatIsExactlyOk!!
    // Count carefully with known literals.
    let ok = "T".repeat(40);
    let bad = "T".repeat(41);
    let path = write_file(
        dir.path(),
        "def.rs",
        &format!("struct {ok};\nstruct {bad};\n"),
    );
    let (code, out, err) = run_only(&path, "LongClassName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains(&format!("like {ok}.")),
        "default maximum 40 must allow length 40: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        2,
        "LongClassName",
        &format!(
            "Avoid excessively long type names like {bad}. Keep type name length under 40."
        ),
    );
}

// --- ShortVariable ----------------------------------------------------------

#[test]
fn short_variable_reports_field_param_local_with_exact_messages() {
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
    let _ = (r, xs);
}
"#,
    );
    let (code, out, err) = run_only(&path, "ShortVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "ShortVariable",
        "Avoid variables with short names like q. Configured minimum length is 3.",
    );
    assert_finding(
        &out,
        &path,
        5,
        "ShortVariable",
        "Avoid variables with short names like xs. Configured minimum length is 3.",
    );
    assert_finding(
        &out,
        &path,
        6,
        "ShortVariable",
        "Avoid variables with short names like r. Configured minimum length is 3.",
    );
}

#[test]
fn short_variable_skips_loop_binders_and_keeps_scanning() {
    let dir = TempDir::new().unwrap();
    // Loop binders first (continue). Short local after. break would miss `ab`.
    let path = write_file(
        dir.path(),
        "loop.rs",
        r#"
fn main() {
    for i in 0..10 {
        let _ = i;
    }
    while let Some(j) = Some(1) {
        let _ = j;
        break;
    }
    let ab = 1;
    let _ = ab;
}
"#,
    );
    let (code, out, err) = run_only(&path, "ShortVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(!out.contains("like i."), "for binder quiet: stdout={out:?}");
    assert!(
        !out.contains("like j."),
        "while-let binder quiet: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        10,
        "ShortVariable",
        "Avoid variables with short names like ab. Configured minimum length is 3.",
    );
}

#[test]
fn short_variable_boundary_allows_exact_minimum() {
    let dir = TempDir::new().unwrap();
    // `abc` length 3 == minimum → allowed; `ab` length 2 → reported after.
    let path = write_file(
        dir.path(),
        "bound.rs",
        "fn main() {\n    let abc = 1;\n    let ab = 2;\n    let _ = (abc, ab);\n}\n",
    );
    let (code, out, err) = run_only(&path, "ShortVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("like abc."),
        "exact minimum must pass: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        3,
        "ShortVariable",
        "Avoid variables with short names like ab. Configured minimum length is 3.",
    );
}

#[test]
fn short_variable_exception_skips_listed_name_but_reports_later() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ex.rs",
        "struct S {\n    q: i32,\n    r: i32,\n}\n",
    );
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
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(!out.contains("like q."), "exception quiet: stdout={out:?}");
    assert_finding(
        &out,
        &path,
        3,
        "ShortVariable",
        "Avoid variables with short names like r. Configured minimum length is 3.",
    );
}

// --- LongVariable -----------------------------------------------------------

#[test]
fn long_variable_reports_exact_message_and_boundary() {
    let dir = TempDir::new().unwrap();
    // Exact length 20 allowed first; length 21 reported after (continue≠break).
    let ok = format!("a{}", "x".repeat(19)); // 20 chars
    let bad = format!("a{}", "x".repeat(20)); // 21 chars
    assert_eq!(ok.len(), 20);
    assert_eq!(bad.len(), 21);
    let path = write_file(
        dir.path(),
        "lv.rs",
        &format!("fn main() {{\n    let {ok} = 1;\n    let {bad} = 2;\n    let _ = ({ok}, {bad});\n}}\n"),
    );
    let xml = dir.path().join("lv.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="lv">
  <rule ref="naming/LongVariable">
    <properties>
      <property name="maximum" value="20"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains(&format!("like {ok}.")),
        "exact maximum must pass: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        3,
        "LongVariable",
        &format!(
            "Avoid excessively long variable names like {bad}. Keep variable name length under 20."
        ),
    );
}

#[test]
fn long_variable_subtracts_prefix_and_suffix_and_reports_field_param_local() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "lv.rs",
        r#"
struct Something {
    really_long_int_name_var: i32,
    x_really_long_name: i32,
    really_long_name_sfx: i32,
}
fn main(interesting_arguments_list: i32) {
    let other_really_long_name = -5;
    let _ = (
        interesting_arguments_list,
        other_really_long_name,
        Something {
            really_long_int_name_var: 0,
            x_really_long_name: 0,
            really_long_name_sfx: 0,
        },
    );
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
      <property name="subtract-suffixes" value="_sfx"/>
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
        "LongVariable",
        "Avoid excessively long variable names like really_long_int_name_var. Keep variable name length under 20.",
    );
    assert!(
        !out.contains("x_really_long_name"),
        "prefix trim quiet: stdout={out:?}"
    );
    assert!(
        !out.contains("really_long_name_sfx"),
        "suffix trim quiet: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        7,
        "LongVariable",
        "Avoid excessively long variable names like interesting_arguments_list. Keep variable name length under 20.",
    );
    assert_finding(
        &out,
        &path,
        8,
        "LongVariable",
        "Avoid excessively long variable names like other_really_long_name. Keep variable name length under 20.",
    );
}

#[test]
fn long_variable_skips_loop_binder_and_keeps_scanning() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "loop.rs",
        r#"
fn main() {
    for interesting_arguments_list in 0..1 {
        let _ = interesting_arguments_list;
    }
    let other_really_long_name = 1;
    let _ = other_really_long_name;
}
"#,
    );
    let (code, out, err) = run_only(&path, "LongVariable");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("interesting_arguments_list"),
        "for binder quiet: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        6,
        "LongVariable",
        "Avoid excessively long variable names like other_really_long_name. Keep variable name length under 20.",
    );
}

// --- ShortMethodName --------------------------------------------------------

#[test]
fn short_method_name_reports_method_and_free_fn_with_exact_messages() {
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
    assert_finding(
        &out,
        &path,
        4,
        "ShortMethodName",
        "Avoid using short method names like ShortMethod::a(). The configured minimum method name length is 3.",
    );
    assert_finding(
        &out,
        &path,
        8,
        "ShortMethodName",
        "Avoid using short method names like ::go(). The configured minimum method name length is 3.",
    );
}

#[test]
fn short_method_name_boundary_allows_exact_minimum_and_keeps_scanning() {
    let dir = TempDir::new().unwrap();
    // `abc` length 3 == minimum → continue; `ab` after must still report.
    let path = write_file(dir.path(), "bound.rs", "fn abc() {}\nfn ab() {}\n");
    let (code, out, err) = run_only(&path, "ShortMethodName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("::abc()"),
        "exact minimum must pass: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        2,
        "ShortMethodName",
        "Avoid using short method names like ::ab(). The configured minimum method name length is 3.",
    );
}

#[test]
fn short_method_name_exception_skips_listed_name_but_reports_later() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "sme.rs", "fn go() {}\nfn ab() {}\n");
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
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(!out.contains("::go()"), "exception quiet: stdout={out:?}");
    assert_finding(
        &out,
        &path,
        2,
        "ShortMethodName",
        "Avoid using short method names like ::ab(). The configured minimum method name length is 3.",
    );
}

// --- ConstantNamingConventions ----------------------------------------------

#[test]
fn constant_naming_default_upper_reports_exact_message() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "cn.rs",
        "const MY_NUM: i32 = 0;\nconst badName: i32 = 1;\nstatic ALSO_OK: i32 = 2;\n",
    );
    let (code, out, err) = run_only(&path, "ConstantNamingConventions");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("MY_NUM should"),
        "upper const quiet: stdout={out:?}"
    );
    assert!(
        !out.contains("ALSO_OK should"),
        "upper static quiet: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        2,
        "ConstantNamingConventions",
        "Constant badName should be defined in SCREAMING_SNAKE_CASE",
    );
}

#[test]
fn constant_naming_pascal_option_reports_exact_message() {
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
    assert!(
        !out.contains("MyNum should"),
        "PascalCase quiet: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        2,
        "ConstantNamingConventions",
        "Constant MY_NUM should be defined in PascalCase",
    );
}

#[test]
fn constant_naming_checks_trait_associated_const_with_exact_message() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "tc.rs",
        "trait T { const bad_name: i32; }\n",
    );
    let (code, out, err) = run_only(&path, "ConstantNamingConventions");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "ConstantNamingConventions",
        "Constant bad_name should be defined in SCREAMING_SNAKE_CASE",
    );
}

#[test]
fn constant_naming_accepts_case_insensitive_pascal_convention() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "ci.rs", "const MY_NUM: i32 = 1;\n");
    let xml = dir.path().join("cn.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="cn">
  <rule ref="naming/ConstantNamingConventions">
    <properties>
      <property name="convention" value="PASCAL"/>
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
        1,
        "ConstantNamingConventions",
        "Constant MY_NUM should be defined in PascalCase",
    );
}

// --- BooleanGetMethodName ---------------------------------------------------

#[test]
fn boolean_get_method_name_reports_get_bool_without_params_exact_message() {
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
    assert_finding(
        &out,
        &path,
        4,
        "BooleanGetMethodName",
        "The 'get_foo()' method which returns a boolean should be named 'is_...()' or 'has_...()'",
    );
    assert!(!out.contains("is_foo"), "is_foo quiet: stdout={out:?}");
    assert!(
        !out.contains("get_bar"),
        "parameterized quiet by default: stdout={out:?}"
    );
    assert!(
        !out.contains("get_count"),
        "non-bool quiet: stdout={out:?}"
    );
}

#[test]
fn boolean_get_method_name_default_skips_parameterized_without_property() {
    // Kill default-false → true: define the rule inline with no property so the
    // Rust default is the only source of the false value (a catalog ref would
    // inherit checkParameterizedMethods=false from naming.xml).
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "def.rs",
        "fn get_ready(flag: bool) -> bool { flag }\nfn get_ok() -> bool { true }\n",
    );
    let xml = dir.path().join("bg.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="bg">
  <rule name="BooleanGetMethodName"
          message="The '{0}()' method which returns a boolean should be named 'is_...()' or 'has_...()'"
          class="PHPMD\Rule\Naming\BooleanGetMethodName"
          externalInfoUrl="https://phpmd.org/rules/naming.html#booleangetmethodname">
    <priority>4</priority>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("get_ready"),
        "default must skip parameterized: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        2,
        "BooleanGetMethodName",
        "The 'get_ok()' method which returns a boolean should be named 'is_...()' or 'has_...()'",
    );
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
    assert_finding(
        &out,
        &path,
        1,
        "BooleanGetMethodName",
        "The 'get_ready()' method which returns a boolean should be named 'is_...()' or 'has_...()'",
    );
}

#[test]
fn boolean_get_method_name_keeps_scanning_after_non_match_and_parameterized() {
    let dir = TempDir::new().unwrap();
    // Non-getter and parameterized getter continue; final no-arg getter must report.
    let path = write_file(
        dir.path(),
        "scan.rs",
        r#"
fn is_ready() -> bool { true }
fn get_ready(flag: bool) -> bool { flag }
fn get_flag() -> bool { true }
"#,
    );
    let (code, out, err) = run_only(&path, "BooleanGetMethodName");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(!out.contains("is_ready"), "non-get quiet: stdout={out:?}");
    assert!(
        !out.contains("get_ready"),
        "parameterized quiet: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        4,
        "BooleanGetMethodName",
        "The 'get_flag()' method which returns a boolean should be named 'is_...()' or 'has_...()'",
    );
}
