//! codesize rules through the injectable CLI entry.
//!
//! Seam: `messrust::run`. Each rule test asserts the exit code and the
//! user-visible text. Threshold cases cover the finding at the boundary, no
//! finding on the quiet side of the boundary, and the exact message text.

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
        "codesize",
        "--only",
        rule,
    ])
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

fn fields_src(kind: &str, name: &str, n: usize) -> String {
    let fields: String = (0..n).map(|i| format!("    f{i}: i32,\n")).collect();
    format!("{kind} {name} {{\n{fields}}}\n")
}

fn methods_src(type_vis: &str, name: &str, method_line: impl Fn(usize) -> String, n: usize) -> String {
    let methods: String = (0..n).map(method_line).collect();
    format!("{type_vis}struct {name} {{}}\n\nimpl {name} {{\n{methods}}}\n")
}

/// Line-for-line Rust translation of the phpmd 2.15.0 / messgo reference
/// function (CCN=12, NPath=324). Match arms stand in for switch case labels;
/// the `_` arm is the default and must not add to CCN.
const REFERENCE_FN: &str = r#"
fn high_complexity(a: i32, b: i32, c: i32, d: i32, e: i32) -> i32 {
    let mut x = 0;
    if a > 0 && b > 0 {
        x += 1;
    }
    if a > 1 || b > 1 {
        x += 1;
    }
    for i in 0..a {
        if i % 2 == 0 {
            x += 1;
        }
    }
    // Non-exhaustive on purpose: mirrors the phpmd/messgo switch with no
    // default so NPath stays 324. syn still parses this.
    match c {
        1 => {
            x += 1;
        }
        2 => {
            x += 1;
        }
        3 => {
            x += 1;
        }
    }
    if d > 0 {
        x += 1;
    }
    if e > 0 {
        x += 1;
    }
    x
}
"#;

#[test]
fn all_codesize_rules_load_without_verbose_skips() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", "fn ok() {}\n");
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
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
fn cyclomatic_complexity_reference_reports_phpmd_ccn_12() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "ref.rs", REFERENCE_FN);
    let xml = dir.path().join("cc.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="cc">
  <rule ref="codesize/CyclomaticComplexity">
    <properties>
      <property name="reportLevel" value="10"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("CyclomaticComplexity"), "stdout={out:?}");
    assert!(
        out.contains("Cyclomatic Complexity of 12"),
        "stdout={out:?}"
    );
    assert!(
        out.contains("configured cyclomatic complexity threshold is 10"),
        "stdout={out:?}"
    );
}

#[test]
fn npath_complexity_reference_reports_phpmd_npath_324() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "ref.rs", REFERENCE_FN);
    let xml = dir.path().join("np.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="np">
  <rule ref="codesize/NPathComplexity">
    <properties>
      <property name="minimum" value="200"/>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPathComplexity"), "stdout={out:?}");
    assert!(out.contains("NPath complexity of 324"), "stdout={out:?}");
}

#[test]
fn npath_complexity_eight_ifs_fires_at_256() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "npath.rs",
        r#"
fn many_paths(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32, h: i32) {
    if a > 0 { let _x = 1; }
    if b > 0 { let _x = 2; }
    if c > 0 { let _x = 3; }
    if d > 0 { let _x = 4; }
    if e > 0 { let _x = 5; }
    if f > 0 { let _x = 6; }
    if g > 0 { let _x = 7; }
    if h > 0 { let _x = 8; }
}
"#,
    );
    let (code, out, err) = run_only(&path, "NPathComplexity");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPathComplexity"), "stdout={out:?}");
    assert!(out.contains("NPath complexity of 256"), "stdout={out:?}");
}

#[test]
fn excessive_method_length_fires_on_long_function() {
    let dir = TempDir::new().unwrap();
    let lines: String = (0..99).map(|i| format!("    let _x{i} = {i};\n")).collect();
    let src = format!("fn long_method() {{\n{lines}}}\n");
    let path = write_file(dir.path(), "long.rs", &src);
    let (code, out, err) = run_only(&path, "ExcessiveMethodLength");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveMethodLength"), "stdout={out:?}");
    assert!(out.contains("lines of code"), "stdout={out:?}");
}

#[test]
fn excessive_class_length_fires_on_long_struct_with_methods() {
    let dir = TempDir::new().unwrap();
    let fields: String = (0..20).map(|i| format!("    f{i}: i32,\n")).collect();
    let methods: String = (0..40)
        .map(|i| {
            let body: String = (0..30).map(|j| format!("        let _x{j} = {j};\n")).collect();
            format!("    fn m{i}(&self) {{\n{body}    }}\n")
        })
        .collect();
    let src = format!("struct Long {{\n{fields}}}\n\nimpl Long {{\n{methods}}}\n");
    let path = write_file(dir.path(), "long_type.rs", &src);
    let (code, out, err) = run_only(&path, "ExcessiveClassLength");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveClassLength"), "stdout={out:?}");
    assert!(out.contains("class Long"), "stdout={out:?}");
}

#[test]
fn too_many_fields_fires_above_fifteen() {
    let dir = TempDir::new().unwrap();
    let fields: String = (0..16).map(|i| format!("    f{i}: i32,\n")).collect();
    let path = write_file(
        dir.path(),
        "fields.rs",
        &format!("struct Big {{\n{fields}}}\n"),
    );
    let (code, out, err) = run_only(&path, "TooManyFields");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("TooManyFields"), "stdout={out:?}");
    assert!(out.contains("16 fields"), "stdout={out:?}");
}

#[test]
fn too_many_fields_does_not_fire_at_fifteen() {
    let dir = TempDir::new().unwrap();
    let fields: String = (0..15).map(|i| format!("    f{i}: i32,\n")).collect();
    let path = write_file(
        dir.path(),
        "fields.rs",
        &format!("struct Border {{\n{fields}}}\n"),
    );
    let (code, out, err) = run_only(&path, "TooManyFields");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(!out.contains("TooManyFields"), "stdout={out:?}");
}

#[test]
fn excessive_public_count_fires_at_threshold() {
    let dir = TempDir::new().unwrap();
    let fields: String = (0..30).map(|i| format!("    pub f{i}: i32,\n")).collect();
    let methods: String = (0..20)
        .map(|i| format!("    pub fn m{i}(&self) {{}}\n"))
        .collect();
    let src = format!("pub struct Wide {{\n{fields}}}\n\nimpl Wide {{\n{methods}}}\n");
    let path = write_file(dir.path(), "wide.rs", &src);
    let (code, out, err) = run_only(&path, "ExcessivePublicCount");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessivePublicCount"), "stdout={out:?}");
    assert!(out.contains("50 public methods and attributes"), "stdout={out:?}");
}

#[test]
fn too_many_methods_fires_above_twenty_five() {
    let dir = TempDir::new().unwrap();
    let methods: String = (0..26)
        .map(|i| format!("    fn work{i}(&self) {{}}\n"))
        .collect();
    let src = format!("struct Busy {{}}\n\nimpl Busy {{\n{methods}}}\n");
    let path = write_file(dir.path(), "busy.rs", &src);
    let (code, out, err) = run_only(&path, "TooManyMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("TooManyMethods"), "stdout={out:?}");
    assert!(out.contains("26 non-getter"), "stdout={out:?}");
}

#[test]
fn too_many_methods_ignores_get_set_prefix() {
    let dir = TempDir::new().unwrap();
    let methods: String = (0..26)
        .map(|i| format!("    fn get_value{i}(&self) {{}}\n"))
        .collect();
    let src = format!("struct Busy {{}}\n\nimpl Busy {{\n{methods}}}\n");
    let path = write_file(dir.path(), "busy.rs", &src);
    let (code, out, err) = run_only(&path, "TooManyMethods");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(!out.contains("TooManyMethods"), "stdout={out:?}");
}

#[test]
fn too_many_public_methods_fires_above_ten() {
    let dir = TempDir::new().unwrap();
    let methods: String = (0..11)
        .map(|i| format!("    pub fn work{i}(&self) {{}}\n"))
        .collect();
    let src = format!("pub struct Api {{}}\n\nimpl Api {{\n{methods}}}\n");
    let path = write_file(dir.path(), "api.rs", &src);
    let (code, out, err) = run_only(&path, "TooManyPublicMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("TooManyPublicMethods"), "stdout={out:?}");
    assert!(out.contains("11 public methods"), "stdout={out:?}");
}

#[test]
fn excessive_class_complexity_fires_on_high_wmc() {
    let dir = TempDir::new().unwrap();
    // Ten methods each with five `if`s => CCN 6 each => WMC 60 >= 50.
    let methods: String = (0..10)
        .map(|i| {
            format!(
                "    fn m{i}(&self, a: i32, b: i32, c: i32, d: i32, e: i32) {{\n\
        if a > 0 {{}}\n\
        if b > 0 {{}}\n\
        if c > 0 {{}}\n\
        if d > 0 {{}}\n\
        if e > 0 {{}}\n\
    }}\n"
            )
        })
        .collect();
    let src = format!("struct Heavy {{}}\n\nimpl Heavy {{\n{methods}}}\n");
    let path = write_file(dir.path(), "heavy.rs", &src);
    let (code, out, err) = run_only(&path, "ExcessiveClassComplexity");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveClassComplexity"), "stdout={out:?}");
    assert!(out.contains("overall complexity of 60"), "stdout={out:?}");
}

#[test]
fn codesize_fixture_fires_parameter_list_and_too_many_fields() {
    let dir = TempDir::new().unwrap();
    let fields: String = (0..16).map(|i| format!("    f{i}: i32,\n")).collect();
    let params: String = (0..11).map(|i| format!("p{i}: i32")).collect::<Vec<_>>().join(", ");
    let src = format!("fn many_params({params}) {{}}\n\nstruct Big {{\n{fields}}}\n");
    let path = write_file(dir.path(), "fixture.rs", &src);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
    assert!(out.contains("TooManyFields"), "stdout={out:?}");
}

// ----- src/metrics.rs: cyclomatic complexity, isolated by decision point --

fn cc_xml(dir: &Path, name: &str, report_level: u32) -> PathBuf {
    write_file(
        dir,
        name,
        &format!(
            r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="cc">
  <rule ref="codesize/CyclomaticComplexity">
    <properties>
      <property name="reportLevel" value="{report_level}"/>
    </properties>
  </rule>
</ruleset>
"#
        ),
    )
}

fn np_xml(dir: &Path, name: &str, minimum: u32) -> PathBuf {
    write_file(
        dir,
        name,
        &format!(
            r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="np">
  <rule ref="codesize/NPathComplexity">
    <properties>
      <property name="minimum" value="{minimum}"/>
    </properties>
  </rule>
</ruleset>
"#
        ),
    )
}

#[test]
fn cyclomatic_complexity_bodyless_trait_method_is_base_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "trait_.rs", "trait Doer {\n    fn work(&self);\n}\n");
    let xml = cc_xml(dir.path(), "cc.xml", 1);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("Cyclomatic Complexity of 1"), "stdout={out:?}");
    assert!(out.contains("method work()"), "stdout={out:?}");
}

#[test]
fn cyclomatic_complexity_while_loop_adds_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "while_.rs", "fn f(a: bool) {\n    while a {\n    }\n}\n");
    let xml = cc_xml(dir.path(), "cc.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("Cyclomatic Complexity of 2"), "stdout={out:?}");
}

#[test]
fn cyclomatic_complexity_for_loop_adds_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "for_.rs", "fn f() {\n    for i in 0..3 {\n        let _ = i;\n    }\n}\n");
    let xml = cc_xml(dir.path(), "cc.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("Cyclomatic Complexity of 2"), "stdout={out:?}");
}

#[test]
fn cyclomatic_complexity_bare_loop_adds_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "loop_.rs", "fn f() {\n    loop {\n        break;\n    }\n}\n");
    let xml = cc_xml(dir.path(), "cc.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("Cyclomatic Complexity of 2"), "stdout={out:?}");
}

#[test]
fn cyclomatic_complexity_wildcard_arm_not_counted_but_guard_is() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "match_.rs",
        "fn f(x: i32) {\n    match x {\n        n if n > 0 => {}\n        _ => {}\n    }\n}\n",
    );
    let xml = cc_xml(dir.path(), "cc.xml", 3);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    // base 1 + guarded arm (1) + its guard (1) + wildcard arm (0) = 3.
    assert!(out.contains("Cyclomatic Complexity of 3"), "stdout={out:?}");
}

#[test]
fn cyclomatic_complexity_and_or_each_add_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "bools.rs",
        "fn f(a: bool, b: bool) {\n    if a && b || a {\n    }\n}\n",
    );
    let xml = cc_xml(dir.path(), "cc.xml", 4);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    // base 1 + if (1) + && (1) + || (1) = 4.
    assert!(out.contains("Cyclomatic Complexity of 4"), "stdout={out:?}");
}

// ----- src/metrics.rs: NPath complexity, isolated by statement form -------

#[test]
fn npath_complexity_bodyless_trait_method_is_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "trait_.rs", "trait Doer {\n    fn work(&self);\n}\n");
    let xml = np_xml(dir.path(), "np.xml", 1);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPath complexity of 1"), "stdout={out:?}");
}

#[test]
fn npath_complexity_while_loop_is_condition_plus_one_plus_body() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "while_.rs", "fn f(a: bool) {\n    while a {\n    }\n}\n");
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_for_loop_is_iter_plus_one_plus_body() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "for_.rs", "fn f() {\n    for i in 0..3 {\n        let _ = i;\n    }\n}\n");
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_bare_loop_is_one_plus_body() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "loop_.rs", "fn f() {\n    loop {\n        break;\n    }\n}\n");
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_empty_match_floors_to_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "empty_match.rs",
        "fn f(x: i32, a: bool) {\n    if a {\n    }\n    match x {}\n}\n",
    );
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    // if-stmt npath (2) times the empty match's floor of 1 = 2, not 0.
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_return_with_bool_expr_counts_and_or() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ret_bool.rs",
        "fn f(a: bool, b: bool) -> bool {\n    return a && b || a;\n}\n",
    );
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_bare_return_floors_to_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ret_bare.rs",
        "fn f(a: bool) {\n    if a {\n    }\n    return;\n}\n",
    );
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    // if-stmt npath (2) times the bare return's floor of 1 = 2, not 0.
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_bare_block_statement_descends_into_it() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "block_.rs",
        "fn f(a: bool) {\n    {\n        if a {\n        }\n    }\n}\n",
    );
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_async_block_statement_descends_into_it() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "async_.rs",
        "fn f(a: bool) {\n    async {\n        if a {\n        }\n    };\n}\n",
    );
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_try_block_statement_descends_into_it() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "try_.rs",
        "fn f(a: bool) {\n    try {\n        if a {\n        }\n    };\n}\n",
    );
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_unsafe_block_statement_descends_into_it() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "unsafe_.rs",
        "fn f(a: bool) {\n    unsafe {\n        if a {\n        }\n    }\n}\n",
    );
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_macro_statement_is_opaque_factor_of_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "macro_.rs",
        "fn f(a: bool) {\n    println!{\"x\"}\n    if a {\n    }\n}\n",
    );
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    // opaque macro factor (1) times the if-stmt (2) = 2.
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

#[test]
fn npath_complexity_nested_item_statement_is_opaque_factor_of_one() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "item_.rs",
        "fn f(a: bool) {\n    fn inner() {}\n    if a {\n    }\n}\n",
    );
    let xml = np_xml(dir.path(), "np.xml", 2);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    // opaque item factor (1) times the if-stmt (2) = 2.
    assert!(out.contains("NPath complexity of 2"), "stdout={out:?}");
}

// ----- src/metrics.rs: effective lines of code (comment/blank scanning) ---

fn eml_ignore_ws_xml(dir: &Path, name: &str, minimum: u32) -> PathBuf {
    write_file(
        dir,
        name,
        &format!(
            r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="eml">
  <rule ref="codesize/ExcessiveMethodLength">
    <properties>
      <property name="minimum" value="{minimum}"/>
      <property name="ignore-whitespace" value="true"/>
    </properties>
  </rule>
</ruleset>
"#
        ),
    )
}

#[test]
fn effective_lines_of_code_skips_a_comment_only_line() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "comment_line.rs",
        "fn f() {\n    let a = 1;\n    // just a comment\n    let b = 2;\n}\n",
    );
    let xml = eml_ignore_ws_xml(dir.path(), "eml.xml", 3);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    // 5 raw lines minus the comment-only line = 4 effective lines.
    assert!(out.contains("has 4 lines of code"), "stdout={out:?}");
}

#[test]
fn effective_lines_of_code_skips_a_multiline_block_comment() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "block_comment.rs",
        "fn g() {\n    let a = 1; /* start\n    still inside\n    end */ let b = 2;\n}\n",
    );
    let xml = eml_ignore_ws_xml(dir.path(), "eml.xml", 3);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    // 5 raw lines minus the fully-commented middle line = 4 effective lines.
    assert!(out.contains("has 4 lines of code"), "stdout={out:?}");
}

// ----- Threshold boundaries and exact messages (mutation gate) ------------

fn params_src(name: &str, n: usize) -> String {
    let params: String = (0..n)
        .map(|i| format!("p{i}: i32"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("fn {name}({params}) {{}}\n")
}

fn eml_raw_xml(dir: &Path, name: &str, minimum: u32) -> PathBuf {
    // Inline rule with no ignore-whitespace property so the default false is used.
    write_file(
        dir,
        name,
        &format!(
            r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="eml">
  <rule name="ExcessiveMethodLength"
        message="The {{0}} {{1}}() has {{2}} lines of code. Current threshold is set to {{3}}. Avoid really long methods."
        class="PHPMD\Rule\Design\LongMethod">
    <priority>3</priority>
    <properties>
      <property name="minimum" value="{minimum}"/>
    </properties>
  </rule>
</ruleset>
"#
        ),
    )
}

fn ecl_raw_xml(dir: &Path, name: &str, minimum: u32) -> PathBuf {
    write_file(
        dir,
        name,
        &format!(
            r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="ecl">
  <rule name="ExcessiveClassLength"
        message="The class {{0}} has {{1}} lines of code. Current threshold is {{2}}. Avoid really long classes."
        class="PHPMD\Rule\Design\LongClass">
    <priority>3</priority>
    <properties>
      <property name="minimum" value="{minimum}"/>
    </properties>
  </rule>
</ruleset>
"#
        ),
    )
}

#[test]
fn cyclomatic_complexity_fires_at_threshold_with_exact_message() {
    let dir = TempDir::new().unwrap();
    // base 1 + nine `if`s = CCN 10, the default reportLevel.
    let ifs: String = (0..9).map(|i| format!("    if a[{i}] {{}}\n")).collect();
    let path = write_file(
        dir.path(),
        "cc.rs",
        &format!("fn border(a: [bool; 9]) {{\n{ifs}}}\n"),
    );
    let (code, out, err) = run_only(&path, "CyclomaticComplexity");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "CyclomaticComplexity",
        "The function border() has a Cyclomatic Complexity of 10. The configured cyclomatic complexity threshold is 10.",
    );
}

#[test]
fn cyclomatic_complexity_quiet_below_threshold() {
    let dir = TempDir::new().unwrap();
    let ifs: String = (0..8).map(|i| format!("    if a[{i}] {{}}\n")).collect();
    let path = write_file(
        dir.path(),
        "cc.rs",
        &format!("fn under(a: [bool; 8]) {{\n{ifs}}}\n"),
    );
    let (code, out, err) = run_only(&path, "CyclomaticComplexity");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn npath_complexity_fires_at_threshold_with_exact_message() {
    let dir = TempDir::new().unwrap();
    // Eight sequential `if`s => NPath 256 with minimum lowered to 256.
    let ifs: String = (0..8).map(|i| format!("    if a[{i}] {{}}\n")).collect();
    let path = write_file(
        dir.path(),
        "np.rs",
        &format!("fn paths(a: [bool; 8]) {{\n{ifs}}}\n"),
    );
    let xml = np_xml(dir.path(), "np.xml", 256);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "NPathComplexity",
        "The function paths() has an NPath complexity of 256. The configured NPath complexity threshold is 256.",
    );
}

#[test]
fn npath_complexity_quiet_below_threshold() {
    let dir = TempDir::new().unwrap();
    let ifs: String = (0..7).map(|i| format!("    if a[{i}] {{}}\n")).collect();
    let path = write_file(
        dir.path(),
        "np.rs",
        &format!("fn paths(a: [bool; 7]) {{\n{ifs}}}\n"),
    );
    let xml = np_xml(dir.path(), "np.xml", 256);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn excessive_parameter_list_fires_at_ten_with_exact_message() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "params.rs", &params_src("many_params", 10));
    let (code, out, err) = run_only(&path, "ExcessiveParameterList");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "ExcessiveParameterList",
        "The function many_params has 10 parameters. Consider reducing the number of parameters to less than 10.",
    );
}

#[test]
fn excessive_parameter_list_quiet_at_nine() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "params.rs", &params_src("ok_params", 9));
    let (code, out, err) = run_only(&path, "ExcessiveParameterList");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn excessive_method_length_fires_at_hundred_with_exact_message() {
    let dir = TempDir::new().unwrap();
    // 98 body lines + braces = 100 lines of code.
    let lines: String = (0..98).map(|i| format!("    let _x{i} = {i};\n")).collect();
    let path = write_file(
        dir.path(),
        "long.rs",
        &format!("fn long_method() {{\n{lines}}}\n"),
    );
    let (code, out, err) = run_only(&path, "ExcessiveMethodLength");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "ExcessiveMethodLength",
        "The function long_method() has 100 lines of code. Current threshold is set to 100. Avoid really long methods.",
    );
}

#[test]
fn excessive_method_length_quiet_at_ninety_nine() {
    let dir = TempDir::new().unwrap();
    let lines: String = (0..97).map(|i| format!("    let _x{i} = {i};\n")).collect();
    let path = write_file(
        dir.path(),
        "long.rs",
        &format!("fn almost() {{\n{lines}}}\n"),
    );
    let (code, out, err) = run_only(&path, "ExcessiveMethodLength");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn excessive_method_length_default_ignore_whitespace_is_false() {
    let dir = TempDir::new().unwrap();
    // Raw LOC = 5 (with blank line). Effective LOC without blanks = 4.
    let path = write_file(
        dir.path(),
        "ws.rs",
        "fn spaced() {\n    let a = 1;\n\n    let b = 2;\n}\n",
    );
    let xml = eml_raw_xml(dir.path(), "eml.xml", 5);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "ExcessiveMethodLength",
        "The function spaced() has 5 lines of code. Current threshold is set to 5. Avoid really long methods.",
    );
}

#[test]
fn excessive_class_length_fires_at_threshold_with_exact_message() {
    let dir = TempDir::new().unwrap();
    // Two source lines for the type span.
    let path = write_file(dir.path(), "cls.rs", "struct Tiny {\n}\n");
    let xml = write_file(
        dir.path(),
        "ecl.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="ecl">
  <rule ref="codesize/ExcessiveClassLength">
    <properties>
      <property name="minimum" value="2"/>
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
        "ExcessiveClassLength",
        "The class Tiny has 2 lines of code. Current threshold is 2. Avoid really long classes.",
    );
}

#[test]
fn excessive_class_length_quiet_below_threshold() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "cls.rs", "struct Tiny {\n}\n");
    let xml = write_file(
        dir.path(),
        "ecl.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="ecl">
  <rule ref="codesize/ExcessiveClassLength">
    <properties>
      <property name="minimum" value="3"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn excessive_class_length_default_ignore_whitespace_is_false() {
    let dir = TempDir::new().unwrap();
    // Raw LOC = 4 with a blank line inside the struct; effective without blanks = 3.
    let path = write_file(dir.path(), "cls.rs", "struct Spaced {\n    a: i32,\n\n}\n");
    let xml = ecl_raw_xml(dir.path(), "ecl.xml", 4);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "ExcessiveClassLength",
        "The class Spaced has 4 lines of code. Current threshold is 4. Avoid really long classes.",
    );
}

#[test]
fn excessive_public_count_fires_at_forty_five_with_exact_message() {
    let dir = TempDir::new().unwrap();
    let fields: String = (0..45).map(|i| format!("    pub f{i}: i32,\n")).collect();
    let path = write_file(
        dir.path(),
        "wide.rs",
        &format!("pub struct Wide {{\n{fields}}}\n"),
    );
    let (code, out, err) = run_only(&path, "ExcessivePublicCount");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "ExcessivePublicCount",
        "The struct Wide has 45 public methods and attributes. Consider reducing the number of public items to less than 45.",
    );
}

#[test]
fn excessive_public_count_quiet_at_forty_four() {
    let dir = TempDir::new().unwrap();
    let fields: String = (0..44).map(|i| format!("    pub f{i}: i32,\n")).collect();
    let path = write_file(
        dir.path(),
        "wide.rs",
        &format!("pub struct Wide {{\n{fields}}}\n"),
    );
    let (code, out, err) = run_only(&path, "ExcessivePublicCount");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn too_many_fields_fires_on_union_above_fifteen_with_exact_message() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "u.rs", &fields_src("union", "Blob", 16));
    let (code, out, err) = run_only(&path, "TooManyFields");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "TooManyFields",
        "The union Blob has 16 fields. Consider redesigning Blob to keep the number of fields under 15.",
    );
}

#[test]
fn too_many_fields_skips_enum_variants() {
    let dir = TempDir::new().unwrap();
    let variants: String = (0..16).map(|i| format!("    V{i},\n")).collect();
    let path = write_file(
        dir.path(),
        "e.rs",
        &format!("enum Many {{\n{variants}}}\n"),
    );
    let (code, out, err) = run_only(&path, "TooManyFields");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn too_many_fields_continues_after_skipped_enum() {
    let dir = TempDir::new().unwrap();
    // Enum first (continue). A break mutant would stop the loop and miss Big.
    let variants: String = (0..4).map(|i| format!("    V{i},\n")).collect();
    let fields: String = (0..16).map(|i| format!("    f{i}: i32,\n")).collect();
    let path = write_file(
        dir.path(),
        "mix.rs",
        &format!("enum Skip {{\n{variants}}}\n\nstruct Big {{\n{fields}}}\n"),
    );
    let (code, out, err) = run_only(&path, "TooManyFields");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        8,
        "TooManyFields",
        "The struct Big has 16 fields. Consider redesigning Big to keep the number of fields under 15.",
    );
    assert!(
        !out.contains("Skip"),
        "enum must stay quiet: stdout={out:?}"
    );
}

#[test]
fn too_many_fields_struct_exact_message_above_fifteen() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "s.rs", &fields_src("struct", "Big", 16));
    let (code, out, err) = run_only(&path, "TooManyFields");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "TooManyFields",
        "The struct Big has 16 fields. Consider redesigning Big to keep the number of fields under 15.",
    );
}

#[test]
fn too_many_methods_quiet_at_twenty_five() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "busy.rs",
        &methods_src("", "Busy", |i| format!("    fn work{i}(&self) {{}}\n"), 25),
    );
    let (code, out, err) = run_only(&path, "TooManyMethods");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn too_many_methods_fires_at_twenty_six_with_exact_message() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "busy.rs",
        &methods_src("", "Busy", |i| format!("    fn work{i}(&self) {{}}\n"), 26),
    );
    let (code, out, err) = run_only(&path, "TooManyMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "TooManyMethods",
        "The struct Busy has 26 non-getter- and setter-methods. Consider refactoring Busy to keep number of methods under 25.",
    );
}

#[test]
fn too_many_public_methods_quiet_at_ten() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "api.rs",
        &methods_src("pub ", "Api", |i| format!("    pub fn work{i}(&self) {{}}\n"), 10),
    );
    let (code, out, err) = run_only(&path, "TooManyPublicMethods");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn too_many_public_methods_fires_at_eleven_with_exact_message() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "api.rs",
        &methods_src("pub ", "Api", |i| format!("    pub fn work{i}(&self) {{}}\n"), 11),
    );
    let (code, out, err) = run_only(&path, "TooManyPublicMethods");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "TooManyPublicMethods",
        "The struct Api has 11 public methods. Consider refactoring Api to keep number of public methods under 10.",
    );
}

#[test]
fn too_many_public_methods_ignores_private_methods() {
    let dir = TempDir::new().unwrap();
    // 9 public + 20 private non-ignored: must stay quiet (AND filter).
    // An OR / drop-is_public mutant would count the private methods and fire.
    let mut methods = String::new();
    for i in 0..9 {
        methods.push_str(&format!("    pub fn work{i}(&self) {{}}\n"));
    }
    for i in 0..20 {
        methods.push_str(&format!("    fn hidden{i}(&self) {{}}\n"));
    }
    let path = write_file(
        dir.path(),
        "api.rs",
        &format!("pub struct Api {{}}\n\nimpl Api {{\n{methods}}}\n"),
    );
    let (code, out, err) = run_only(&path, "TooManyPublicMethods");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn too_many_public_methods_ignores_get_set_prefix() {
    let dir = TempDir::new().unwrap();
    // 9 public work + 11 public get_*: ignore pattern keeps count at 9.
    let mut methods = String::new();
    for i in 0..9 {
        methods.push_str(&format!("    pub fn work{i}(&self) {{}}\n"));
    }
    for i in 0..11 {
        methods.push_str(&format!("    pub fn get_value{i}(&self) {{}}\n"));
    }
    let path = write_file(
        dir.path(),
        "api.rs",
        &format!("pub struct Api {{}}\n\nimpl Api {{\n{methods}}}\n"),
    );
    let (code, out, err) = run_only(&path, "TooManyPublicMethods");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn excessive_class_complexity_fires_at_fifty_with_exact_message() {
    let dir = TempDir::new().unwrap();
    // Ten methods each with four `if`s => CCN 5 each => WMC 50.
    let methods: String = (0..10)
        .map(|i| {
            format!(
                "    fn m{i}(&self, a: i32, b: i32, c: i32, d: i32) {{\n\
        if a > 0 {{}}\n\
        if b > 0 {{}}\n\
        if c > 0 {{}}\n\
        if d > 0 {{}}\n\
    }}\n"
            )
        })
        .collect();
    let path = write_file(
        dir.path(),
        "heavy.rs",
        &format!("struct Heavy {{}}\n\nimpl Heavy {{\n{methods}}}\n"),
    );
    let (code, out, err) = run_only(&path, "ExcessiveClassComplexity");
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "ExcessiveClassComplexity",
        "The class Heavy has an overall complexity of 50 which is very high. The configured complexity threshold is 50.",
    );
}

#[test]
fn excessive_class_complexity_quiet_at_forty_nine() {
    let dir = TempDir::new().unwrap();
    // Nine methods with four `if`s (CCN 5) + one with three `if`s (CCN 4) = 49.
    let mut methods = String::new();
    for i in 0..9 {
        methods.push_str(&format!(
            "    fn m{i}(&self, a: i32, b: i32, c: i32, d: i32) {{\n\
        if a > 0 {{}}\n\
        if b > 0 {{}}\n\
        if c > 0 {{}}\n\
        if d > 0 {{}}\n\
    }}\n"
        ));
    }
    methods.push_str(
        "    fn last(&self, a: i32, b: i32, c: i32) {\n\
        if a > 0 {}\n\
        if b > 0 {}\n\
        if c > 0 {}\n\
    }\n",
    );
    let path = write_file(
        dir.path(),
        "heavy.rs",
        &format!("struct Heavy {{}}\n\nimpl Heavy {{\n{methods}}}\n"),
    );
    let (code, out, err) = run_only(&path, "ExcessiveClassComplexity");
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}
