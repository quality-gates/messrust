//! Shared analysis helpers through the injectable CLI entry.
//!
//! Seam: `messrust::run`. These cases cover helpers that no rule-family ticket
//! owns alone: length trimming with several prefixes, getter-name predicates,
//! SCREAMING_SNAKE digit rules, snake_case digit rules, PHPMD regex compile
//! offsets, class LOC that adds each method span, and `--ignore-tests` range
//! boundaries.

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

// --- length_without: first matching prefix/suffix only ----------------------

#[test]
fn long_class_name_applies_only_first_matching_prefix() {
    let dir = TempDir::new().unwrap();
    // Name length 24. Duplicate prefix Pre,Pre.
    // First match only → effective 21 (long for max 20).
    // If break became continue, a second Pre strip → effective 18 (quiet).
    let path = write_file(dir.path(), "pref.rs", "struct PrePreVeryLongClassNames;\n");
    let xml = dir.path().join("lc.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="lc">
  <rule ref="naming/LongClassName">
    <properties>
      <property name="maximum" value="20"/>
      <property name="subtract-prefixes" value="Pre,Pre"/>
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
        "LongClassName",
        "Avoid excessively long type names like PrePreVeryLongClassNames. Keep type name length under 20.",
    );
}

#[test]
fn long_class_name_applies_only_first_matching_suffix() {
    let dir = TempDir::new().unwrap();
    // Name length 24. Duplicate suffix End,End.
    // First match only → effective 21 (long for max 20).
    // If break became continue, a second End strip → effective 18 (quiet).
    let path = write_file(dir.path(), "suf.rs", "struct VeryLongClassNamesEndEnd;\n");
    let xml = dir.path().join("lc.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="lc">
  <rule ref="naming/LongClassName">
    <properties>
      <property name="maximum" value="20"/>
      <property name="subtract-suffixes" value="End,End"/>
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
        "LongClassName",
        "Avoid excessively long type names like VeryLongClassNamesEndEnd. Keep type name length under 20.",
    );
}

// --- is_getter_name via BooleanGetMethodName --------------------------------

#[test]
fn boolean_get_method_name_treats_exact_get_as_getter() {
    let dir = TempDir::new().unwrap();
    // Length 3 boundary: "get" must match (>= 3), and "ge" must not (>= 2 mutant).
    // Letter checks: set_* must not match without the leading 'g'; gea_* without the 't'.
    let path = write_file(
        dir.path(),
        "get.rs",
        "fn get() -> bool { true }\nfn ge() -> bool { true }\nfn getx() -> bool { true }\nfn gat_flag() -> bool { true }\nfn gxt_flag() -> bool { true }\nfn geT_ok() -> bool { true }\nfn set_flag() -> bool { true }\nfn gea_flag() -> bool { true }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "BooleanGetMethodName",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        1,
        "BooleanGetMethodName",
        "The 'get()' method which returns a boolean should be named 'is_...()' or 'has_...()'",
    );
    assert_finding(
        &out,
        &path,
        3,
        "BooleanGetMethodName",
        "The 'getx()' method which returns a boolean should be named 'is_...()' or 'has_...()'",
    );
    assert_finding(
        &out,
        &path,
        6,
        "BooleanGetMethodName",
        "The 'geT_ok()' method which returns a boolean should be named 'is_...()' or 'has_...()'",
    );
    assert!(!out.contains("'ge()'"), "ge must not be a getter: stdout={out:?}");
    assert!(
        !out.contains("gat_flag"),
        "gat_flag must not be a getter: stdout={out:?}"
    );
    assert!(
        !out.contains("gxt_flag"),
        "gxt_flag must not be a getter: stdout={out:?}"
    );
    assert!(
        !out.contains("set_flag"),
        "set_flag must not be a getter: stdout={out:?}"
    );
    assert!(
        !out.contains("gea_flag"),
        "gea_flag must not be a getter: stdout={out:?}"
    );
}

// --- is_upper_case via ConstantNamingConventions ----------------------------

#[test]
fn constant_naming_allows_digits_and_rejects_underscore_only() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "const.rs",
        "const OK_VALUE_1: i32 = 1;\nconst _OK: i32 = 2;\nconst ___: i32 = 3;\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "ConstantNamingConventions",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("OK_VALUE_1"),
        "digits must stay SCREAMING_SNAKE: stdout={out:?}"
    );
    assert!(
        !out.contains("_OK"),
        "leading underscore then letters must stay upper: stdout={out:?}"
    );
    assert_finding(
        &out,
        &path,
        3,
        "ConstantNamingConventions",
        "Constant ___ should be defined in SCREAMING_SNAKE_CASE",
    );
}

// --- is_snake_case digit handling -------------------------------------------

#[test]
fn camel_case_method_name_allows_digit_then_letter() {
    let dir = TempDir::new().unwrap();
    // Digit before any letter: continue-on-digit must keep scanning.
    // A break-on-digit mutant stops with saw_letter=false and reports the name.
    let path = write_file(dir.path(), "dig.rs", "fn _2x_load() {}\nfn BadName() {}\n");
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "controversial",
        "--only",
        "CamelCaseMethodName",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("_2x_load"),
        "digit-then-letter must stay snake_case: stdout={out:?}"
    );
    assert!(out.contains("BadName"), "stdout={out:?}");
}

#[test]
fn camel_case_method_name_allows_letter_digit_letter() {
    let dir = TempDir::new().unwrap();
    // Removing the digit `continue` makes digits fall through to `return false`.
    let path = write_file(dir.path(), "mid.rs", "fn item_2b() {}\n");
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "controversial",
        "--only",
        "CamelCaseMethodName",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

// --- is_pascal_case_no_abbrev windows(2) ------------------------------------

#[test]
fn camel_case_class_name_abbreviations_reject_two_char_all_caps() {
    let dir = TempDir::new().unwrap();
    // windows(2) sees ['A','B']; windows(3) yields no window and would allow AB.
    let path = write_file(dir.path(), "ab.rs", "struct AB;\n");
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
    assert_finding(
        &out,
        &path,
        1,
        "CamelCaseClassName",
        "The type AB is not named in PascalCase.",
    );
}

// --- compile_phpmd_regex body/flag offsets ----------------------------------

#[test]
fn boolean_argument_flag_ignorepattern_body_and_flags_must_parse() {
    let dir = TempDir::new().unwrap();
    // Body starts at index 1 (`^create`); flags at close+1 (`i`).
    // Off-by-one on the body drops `^`, so `recreate_*` would be ignored by mistake.
    let path = write_file(
        dir.path(),
        "ign.rs",
        "fn CreateThing(enabled: bool) {}\nfn recreate_thing(enabled: bool) {}\nfn other_thing(enabled: bool) {}\n",
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
        !out.contains("CreateThing"),
        "ignorepattern must stay case-insensitive anchored: stdout={out:?}"
    );
    assert!(
        out.contains("recreate_thing"),
        "anchor must not ignore recreate_*: stdout={out:?}"
    );
    assert!(out.contains("other_thing"), "stdout={out:?}");
}

// --- type_loc method span addend (no ignore-whitespace) ---------------------

#[test]
fn excessive_class_length_adds_each_impl_method_line_span() {
    let dir = TempDir::new().unwrap();
    // struct line 1 + one single-line method → raw LOC 2.
    // saturating_add(0) → 1 (quiet at minimum 2); saturating_add(2) → 3 (still fires).
    // Use exact message count, then a second case at minimum 3 that must stay quiet
    // unless the addend becomes 2.
    let path = write_file(
        dir.path(),
        "ecl.rs",
        "struct Short;\nimpl Short {\n    fn only() {}\n}\n",
    );
    let xml = write_file(
        dir.path(),
        "ecl.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="ecl">
  <rule name="ExcessiveClassLength"
        message="The class {0} has {1} lines of code. Current threshold is {2}. Avoid really long classes."
        class="PHPMD\Rule\Design\LongClass">
    <priority>3</priority>
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
        "The class Short has 2 lines of code. Current threshold is 2. Avoid really long classes.",
    );

    let xml3 = write_file(
        dir.path(),
        "ecl3.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="ecl3">
  <rule name="ExcessiveClassLength"
        message="The class {0} has {1} lines of code. Current threshold is {2}. Avoid really long classes."
        class="PHPMD\Rule\Design\LongClass">
    <priority>3</priority>
    <properties>
      <property name="minimum" value="3"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml3.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

// --- format_message with more than one argument -----------------------------

#[test]
fn analyze_sorts_violations_by_file_then_line_across_rules() {
    let dir = TempDir::new().unwrap();
    // ShortVariable (line 1) is applied after ShortClassName (line 3) in RULE_HANDLERS.
    // Without the final sort, the class finding would appear first.
    let path = write_file(
        dir.path(),
        "order.rs",
        "fn work() { let x = 1; let _ = x; }\n\nstruct Ab;\n",
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "naming"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    let var_at = out.find("ShortVariable").expect("ShortVariable");
    let class_at = out.find("ShortClassName").expect("ShortClassName");
    assert!(
        var_at < class_at,
        "violations must sort by begin line: stdout={out:?}"
    );
}

#[test]
fn short_method_name_message_includes_parent_name_and_minimum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "sm.rs",
        "struct Owner;\nimpl Owner {\n    fn go() {}\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "ShortMethodName",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "ShortMethodName",
        "Avoid using short method names like Owner::go(). The configured minimum method name length is 3.",
    );
}

// --- --ignore-tests range scan ----------------------------------------------

#[test]
fn boolean_argument_flag_scans_past_receiver_and_non_bool() {
    let dir = TempDir::new().unwrap();
    // continue-on-receiver / continue-on-non-bool must keep scanning to `flag`.
    let path = write_file(
        dir.path(),
        "baf.rs",
        "struct Host;\nimpl Host {\n    fn configure(&self, count: i32, flag: bool) { let _ = (count, flag); }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "cleancode",
        "--only",
        "BooleanArgumentFlag",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        3,
        "BooleanArgumentFlag",
        "The method Host::configure has a boolean flag argument flag, which is a certain sign of a Single Responsibility Principle violation.",
    );
}

#[test]
fn too_many_fields_unit_struct_stays_quiet_at_zero_max() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "unit.rs", "struct Empty;\n");
    let xml = write_file(
        dir.path(),
        "tmf.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="tmf">
  <rule ref="codesize/TooManyFields">
    <properties>
      <property name="maxfields" value="0"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn excessive_public_count_unit_struct_stays_quiet_at_one() {
    let dir = TempDir::new().unwrap();
    // Unit structs contribute (0, 0) field stats. A mutant that sets public_fields to 1
    // would fire ExcessivePublicCount at minimum 1.
    let path = write_file(dir.path(), "unit_pub.rs", "struct Empty;\n");
    let xml = write_file(
        dir.path(),
        "epc.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="epc">
  <rule ref="codesize/ExcessivePublicCount">
    <properties>
      <property name="minimum" value="1"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn boolean_argument_flag_collects_ref_pattern() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "pats.rs",
        "fn ref_flag(ref flag: bool) { let _ = flag; }\nfn plain(flag: bool) { let _ = flag; }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "cleancode",
        "--only",
        "BooleanArgumentFlag",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ref_flag"), "stdout={out:?}");
    assert!(out.contains("plain"), "stdout={out:?}");
}


#[test]
fn duplicated_array_key_still_runs_duplicate_collector() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "dup.rs",
        "struct Point { x: i32, y: i32 }\nfn main() {\n    let _ = Point { x: 1, x: 2, y: 3 };\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "cleancode",
        "--only",
        "DuplicatedArrayKey",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("DuplicatedArrayKey"), "stdout={out:?}");
    assert!(out.contains("Duplicated array key x"), "stdout={out:?}");
}

#[test]
fn ignore_tests_range_uses_inclusive_module_span_boundaries() {
    let dir = TempDir::new().unwrap();
    // Violation on the first and last lines inside the cfg(test) module must drop.
    // A range that uses exclusive end, or start+1, would leave a finding.
    let path = write_file(
        dir.path(),
        "ranges.rs",
        r#"fn production(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32, p6: i32, p7: i32, p8: i32, p9: i32, p10: i32) {}
#[cfg(test)]
mod tests {
fn first_line(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32, p6: i32, p7: i32, p8: i32, p9: i32, p10: i32) {}
fn last_line(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32, p6: i32, p7: i32, p8: i32, p9: i32, p10: i32) {}
}
"#,
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--only",
        "ExcessiveParameterList",
        "--ignore-tests",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("production"), "stdout={out:?}");
    assert!(
        !out.contains("first_line"),
        "inclusive start must drop first_line: stdout={out:?}"
    );
    assert!(
        !out.contains("last_line"),
        "inclusive end must drop last_line: stdout={out:?}"
    );
}

#[test]
fn nested_module_items_are_analyzed() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "nested_mod.rs",
        "mod outer {\n    struct Ab;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "ShortClassName",
    ]);
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
fn impl_without_struct_still_builds_type_for_method_rules() {
    let dir = TempDir::new().unwrap();
    // attach_impl inserts a synthetic type when no prior struct exists.
    let path = write_file(
        dir.path(),
        "orphan.rs",
        "impl Orphan {\n    fn go() {}\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "ShortMethodName",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "ShortMethodName",
        "Avoid using short method names like Orphan::go(). The configured minimum method name length is 3.",
    );
}

#[test]
fn trait_methods_count_for_too_many_methods() {
    let dir = TempDir::new().unwrap();
    let methods: String = (0..26).map(|i| format!("    fn m{i}(&self);\n")).collect();
    let path = write_file(
        dir.path(),
        "tr.rs",
        &format!("pub trait Busy {{\n{methods}}}\n"),
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--only",
        "TooManyMethods",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("TooManyMethods"), "stdout={out:?}");
    assert!(out.contains("Busy"), "stdout={out:?}");
}

#[test]
fn enum_public_fields_stay_zero_for_public_count() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "en.rs", "enum Kind { A, B }\n");
    let xml = write_file(
        dir.path(),
        "epc.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="epc">
  <rule ref="codesize/ExcessivePublicCount">
    <properties>
      <property name="minimum" value="1"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn coupling_includes_return_type_dependencies() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "cbo.rs",
        "struct Other;\nstruct Host;\nimpl Host {\n    fn make(&self) -> Other { Other }\n}\n",
    );
    let xml = write_file(
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
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("CouplingBetweenObjects"), "stdout={out:?}");
    assert!(out.contains("Host"), "stdout={out:?}");
}

#[test]
fn upsert_type_updates_fields_when_struct_follows_impl() {
    let dir = TempDir::new().unwrap();
    // Impl first creates a synthetic type; struct upsert must replace field_count.
    let path = write_file(
        dir.path(),
        "upsert.rs",
        "impl Bag {\n    fn touch() {}\n}\nstruct Bag { a: i32, b: i32, c: i32 }\n",
    );
    let xml = write_file(
        dir.path(),
        "tmf.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="tmf">
  <rule ref="codesize/TooManyFields">
    <properties>
      <property name="maxfields" value="2"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("TooManyFields"), "stdout={out:?}");
    assert!(out.contains("Bag"), "stdout={out:?}");
}

#[test]
fn constant_naming_covers_static_impl_and_trait_consts() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "consts.rs",
        "static bad_static: i32 = 1;\nstruct Host;\nimpl Host { const bad_impl: i32 = 2; }\ntrait Marker { const bad_trait: i32; }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "ConstantNamingConventions",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("bad_static"), "stdout={out:?}");
    assert!(out.contains("bad_impl"), "stdout={out:?}");
    assert!(out.contains("bad_trait"), "stdout={out:?}");
}

#[test]
fn short_variable_skips_while_let_binder_and_reads_body() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "while_let.rs",
        "fn work(mut items: impl Iterator<Item = i32>) {\n    while let Some(x) = items.next() {\n        let y = x;\n        let _ = y;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "ShortVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        !out.contains("argument x") && !out.contains("like x."),
        "while-let binder quiet: stdout={out:?}"
    );
    assert!(out.contains("like y."), "body short var must report: stdout={out:?}");
}

#[test]
fn global_variable_tracks_assign_and_compound_assign() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "glob.rs",
        "static mut COUNTER: i32 = 0;\nstatic mut OTHER: i32 = 0;\nfn bump() {\n    unsafe {\n        COUNTER = 1;\n        OTHER += 1;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "design",
        "--only",
        "GlobalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("COUNTER"), "stdout={out:?}");
    assert!(out.contains("OTHER"), "stdout={out:?}");
}

#[test]
fn ignore_tests_scans_nested_cfg_test_modules() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "nested.rs",
        r#"fn production(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32, p6: i32, p7: i32, p8: i32, p9: i32, p10: i32) {}
mod outer {
#[cfg(test)]
mod inner {
fn nested_only(p0: i32, p1: i32, p2: i32, p3: i32, p4: i32, p5: i32, p6: i32, p7: i32, p8: i32, p9: i32, p10: i32) {}
}
}
"#,
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--only",
        "ExcessiveParameterList",
        "--ignore-tests",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("production"), "stdout={out:?}");
    assert!(
        !out.contains("nested_only"),
        "nested cfg(test) must drop: stdout={out:?}"
    );
}

#[test]
fn upsert_type_updates_public_fields_and_begin_line() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "upsert_pub.rs",
        "impl Bag {\n    fn touch() {}\n}\nstruct Bag { pub a: i32, pub b: i32 }\n",
    );
    let xml = write_file(
        dir.path(),
        "epc.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="epc">
  <rule ref="codesize/ExcessivePublicCount">
    <properties>
      <property name="minimum" value="2"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessivePublicCount"), "stdout={out:?}");
    // begin_line must be the struct line (4), not the earlier impl line (1).
    assert_finding(
        &out,
        &path,
        4,
        "ExcessivePublicCount",
        "The struct Bag has 2 public methods and attributes. Consider reducing the number of public items to less than 2.",
    );
}

#[test]
fn global_variable_does_not_mark_rhs_static_as_mutated() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "rhs.rs",
        "static mut DEST: i32 = 0;\nstatic mut SRC: i32 = 0;\nfn copy() {\n    unsafe { DEST = SRC; }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "design",
        "--only",
        "GlobalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("DEST"), "stdout={out:?}");
    assert!(!out.contains("SRC"), "rhs must stay quiet: stdout={out:?}");
}

#[test]
fn global_variable_tracks_nested_assign_on_rhs() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "nested_as.rs",
        "static mut A: i32 = 0;\nstatic mut B: i32 = 0;\nfn work() {\n    unsafe { A = { B = 1; 0 }; }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "design",
        "--only",
        "GlobalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("A"), "stdout={out:?}");
    assert!(out.contains("B"), "nested rhs assign must count: stdout={out:?}");
}

#[test]
fn upsert_type_replaces_empty_fields_from_synthetic_impl() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "upsert_fields.rs",
        "impl Bag {\n    fn touch(&self) {}\n}\nstruct Bag { unused_item: i32 }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("unused_item"), "stdout={out:?}");
}

#[test]
fn upsert_type_field_types_feed_coupling() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "upsert_cbo.rs",
        "impl Bag { fn touch() {} }\nstruct Other;\nstruct Bag { other: Other }\n",
    );
    let xml = write_file(
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
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("Bag"), "stdout={out:?}");
}

#[test]
fn upsert_type_updates_node_type_to_enum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "upsert_enum.rs",
        "impl Kind { fn touch() {} }\nenum Kind { A, B }\n",
    );
    let xml = write_file(
        dir.path(),
        "epc.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="epc">
  <rule ref="codesize/ExcessivePublicCount">
    <properties>
      <property name="minimum" value="1"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    // Enum has 0 public fields; only quiet if node_type/public_fields stay consistent.
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn short_variable_inside_for_body_is_recorded() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "for_body.rs",
        "fn work() {\n    for _ in 0..1 {\n        let z = 1;\n        let _ = z;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "ShortVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("like z."), "stdout={out:?}");
}

#[test]
fn short_variable_inside_for_iterable_is_recorded() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "for_iter.rs",
        "fn work() {\n    for _ in {\n        let w = 1;\n        0..w\n    } {}\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "ShortVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("like w."), "stdout={out:?}");
}

#[test]
fn global_variable_compound_assign_ignores_rhs_and_nests() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "compound.rs",
        "static mut DEST: i32 = 0;\nstatic mut SRC: i32 = 0;\nstatic mut NEST: i32 = 0;\nfn bump() {\n    unsafe {\n        DEST += SRC;\n        DEST += { NEST = 2; 1 };\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "design",
        "--only",
        "GlobalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("DEST"), "stdout={out:?}");
    assert!(out.contains("NEST"), "stdout={out:?}");
    assert!(!out.contains("SRC"), "compound rhs quiet: stdout={out:?}");
}

#[test]
fn union_fields_count_for_too_many_fields() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "un.rs",
        "union Pair { a: i32, b: i32, c: i32 }\n",
    );
    let xml = write_file(
        dir.path(),
        "tmf.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="tmf">
  <rule ref="codesize/TooManyFields">
    <properties>
      <property name="maxfields" value="2"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("union"), "stdout={out:?}");
    assert!(out.contains("Pair"), "stdout={out:?}");
}

#[test]
fn short_variable_inside_while_condition_is_recorded() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "while_cond.rs",
        "fn work() {\n    while {\n        let v = false;\n        v\n    } {}\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "ShortVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("like v."), "stdout={out:?}");
}

#[test]
fn while_let_scrutinee_locals_are_recorded() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "while_scrut.rs",
        "fn work() {\n    while let Some(item) = {\n        let v = Some(1);\n        v\n    } {\n        let _ = item;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "naming",
        "--only",
        "ShortVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("like v."), "stdout={out:?}");
}

#[test]
fn upsert_type_end_line_feeds_class_length() {
    let dir = TempDir::new().unwrap();
    // Impl first, then a multi-line struct so end_line must update from the struct span.
    let path = write_file(
        dir.path(),
        "upsert_ecl.rs",
        "impl Bag { fn touch() {} }\nstruct Bag {\n    a: i32,\n}\n",
    );
    let xml = write_file(
        dir.path(),
        "ecl.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="ecl">
  <rule name="ExcessiveClassLength"
        message="The class {0} has {1} lines of code. Current threshold is {2}. Avoid really long classes."
        class="PHPMD\Rule\Design\LongClass">
    <priority>3</priority>
    <properties>
      <property name="minimum" value="3"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    // struct lines 2-4 = 3, plus touch method 1 line => 4
    assert!(out.contains("has 4 lines of code"), "stdout={out:?}");
}

#[test]
fn unused_local_from_tuple_destructuring_assignment() {
    let dir = TempDir::new().unwrap();
    // (a, b) = (1, 2) should not count a/b as reads; unread locals stay unused.
    let path = write_file(
        dir.path(),
        "tuple_as.rs",
        "fn work() {\n    let mut a = 0;\n    let mut b = 0;\n    (a, b) = (1, 2);\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("a") || out.contains("b"), "stdout={out:?}");
}

#[test]
fn unused_local_index_base_is_a_read() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "idx.rs",
        "fn work(mut items: [i32; 1]) {\n    let i = 0;\n    items[i] = 1;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_counts_macro_dot_field() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "mac_field.rs",
        "struct Host { used_field: i32, dead_field: i32 }\nfn show(h: &Host) {\n    println!(\"{}\", h.used_field);\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead_field"), "stdout={out:?}");
    assert!(!out.contains("used_field"), "stdout={out:?}");
}

#[test]
fn unused_private_field_skips_serialize_derive() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "ser.rs",
        "#[derive(Serialize)]\nstruct Host { only_field: i32 }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_skips_deserialize_derive() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "de3.rs",
        "#[derive(Deserialize)]\nstruct Host { only_field: i32 }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_not_misclassified_as_parameter_after_bindings() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "mode.rs",
        "fn work(used: i32) {\n    let dead = 1;\n    let _ = used;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_finding(
        &out,
        &path,
        2,
        "UnusedLocalVariable",
        "Avoid unused local variables such as 'dead'.",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedFormalParameter",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn upsert_type_node_type_appears_in_public_count_message() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "upsert_msg.rs",
        "impl Kind {\n    pub fn a() {}\n    pub fn b() {}\n}\nenum Kind { X }\n",
    );
    let xml = write_file(
        dir.path(),
        "epc.xml",
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="epc">
  <rule ref="codesize/ExcessivePublicCount">
    <properties>
      <property name="minimum" value="2"/>
    </properties>
  </rule>
</ruleset>
"#,
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(
        out.contains("The enum Kind has 2 public methods"),
        "node_type must update to enum: stdout={out:?}"
    );
}

#[test]
fn unused_local_does_not_record_closure_params_via_leaked_binding_mode() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "closure_mode.rs",
        "fn work() {\n    let f = |z| 1;\n    let _ = f;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    // `f` is used; closure param `z` must not be recorded as a local via leaked mode.
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_counts_nested_format_capture_group() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "fmt_group.rs",
        "fn work() {\n    let name = 1;\n    println!(\"{}\", { name });\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_derive_debug_does_not_count_as_serde() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "debug_derive.rs",
        "#[derive(Debug)]\nstruct Host { only_field: i32 }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("only_field"), "stdout={out:?}");
}

#[test]
fn unused_private_field_not_kept_alive_by_leading_macro_ident() {
    let dir = TempDir::new().unwrap();
    // If after_dot starts true, the first macro ident `name` is stored as a field read.
    let path = write_file(
        dir.path(),
        "mac_lead.rs",
        "struct Host { name: i32 }\nfn show() {\n    println!(\"{}\", name);\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("name"), "stdout={out:?}");
}

#[test]
fn unused_private_field_counts_field_inside_macro_group() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "mac_group.rs",
        "struct Host { used_field: i32, dead_field: i32 }\nfn show(h: &Host) {\n    println!(\"{}\", (h.used_field));\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead_field"), "stdout={out:?}");
    assert!(!out.contains("used_field"), "stdout={out:?}");
}

#[test]
fn unused_private_field_not_marked_used_by_leading_macro_ident() {
    let dir = TempDir::new().unwrap();
    // If after_dot starts true, the first macro ident is wrongly stored as a field read.
    let path = write_file(
        dir.path(),
        "mac_lead.rs",
        "struct Host { value: i32 }\nfn show(h: &Host) {\n    let value = 1;\n    dbg!(value);\n    let _ = h;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("value"), "stdout={out:?}");
}

#[test]
fn unused_private_field_requires_dot_reset_between_macro_idents() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "mac_reset.rs",
        "struct Host { live: i32, dead: i32 }\nfn show(h: &Host) {\n    println!(\"{:?}\", h.live);\n    let dead = 1;\n    dbg!(dead);\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead"), "stdout={out:?}");
    assert!(!out.contains("'live'"), "stdout={out:?}");
}

#[test]
fn unused_local_field_assign_counts_base_as_read() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "field_as.rs",
        "struct Wrapper { inner: i32 }\nfn work() {\n    let mut host = Wrapper { inner: 0 };\n    host.inner = 1;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_tuple_field_assign_counts_bases_as_reads() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "tuple_field_as.rs",
        "struct Pair { a: i32, b: i32 }\nfn work() {\n    let mut left = Pair { a: 0, b: 0 };\n    let mut right = Pair { a: 0, b: 0 };\n    (left.a, right.b) = (1, 2);\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_format_nested_group_capture() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "fmt_nested.rs",
        "fn work() {\n    let name = 1;\n    assert_eq!(name, name);\n    let other = 2;\n    println!(\"{}\", { other });\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_macro_resets_after_dot_before_next_ident() {
    let dir = TempDir::new().unwrap();
    // stringify!(h.live dead) — after `live`, after_dot must clear so `dead` is not a field read.
    let path = write_file(
        dir.path(),
        "mac_stringify.rs",
        "struct Host { live: i32, dead: i32 }\nfn show(h: &Host) {\n    let _ = stringify!(h.live dead);\n    let _ = h;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead"), "stdout={out:?}");
    assert!(!out.contains("'live'"), "stdout={out:?}");
}

#[test]
fn unused_private_field_macro_resets_after_group() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "mac_group.rs",
        "struct Host { live: i32, dead: i32 }\nfn show(h: &Host) {\n    let _ = stringify!((h.live) dead);\n    let _ = h;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead"), "stdout={out:?}");
}

#[test]
fn unused_local_index_expr_base_is_a_read() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "idx_base.rs",
        "fn work() {\n    let mut items = [0];\n    let i = 0;\n    items[i] = 1;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_formal_parameter_on_inherent_impl_method() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "impl_param.rs",
        "struct Host;\nimpl Host {\n    fn work(&self, unused_arg: i32) {}\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedFormalParameter",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("unused_arg"), "stdout={out:?}");
}

#[test]
fn unused_formal_parameter_on_trait_default_method() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "trait_param.rs",
        "trait Host {\n    fn work(&self, unused_arg: i32) {}\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedFormalParameter",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("unused_arg"), "stdout={out:?}");
}

#[test]
fn unused_private_method_after_trait_impl_restores_flag() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "trait_then_inherent.rs",
        "trait Touch { fn touch(&self); }\nstruct Host;\nimpl Touch for Host {\n    fn touch(&self) {}\n}\nimpl Host {\n    fn dead_method(&self) {}\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateMethod",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead_method"), "stdout={out:?}");
}

#[test]
fn match_guard_local_read_counts() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "guard.rs",
        "fn work(flag: bool) {\n    let guard = true;\n    match flag {\n        true if guard => {}\n        _ => {}\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn if_let_else_branch_local_read_counts() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "if_else.rs",
        "fn work(value: Option<i32>) {\n    let fallback = 1;\n    if let Some(x) = value {\n        let _ = x;\n    } else {\n        let _ = fallback;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn self_path_is_not_counted_as_ident_read_for_unused_local() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "self_read.rs",
        "struct Host { value: i32 }\nimpl Host {\n    fn work(&self) {\n        let dead = 1;\n        let _ = self.value;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead"), "stdout={out:?}");
}

#[test]
fn unused_private_field_macro_literal_resets_after_dot() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "mac_lit.rs",
        "struct Host { dead: i32 }\nfn show(h: &Host) {\n    let _ = stringify!(h . \"lit\" dead);\n    let _ = h;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead"), "stdout={out:?}");
}

#[test]
fn unused_local_format_capture_inside_token_group() {
    let dir = TempDir::new().unwrap();
    // Group-wrapped format string must still yield the capture name.
    let path = write_file(
        dir.path(),
        "fmt_grp_lit.rs",
        "fn work() {\n    let name = 1;\n    panic!((\"{name}\"));\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_array_field_assign_counts_bases_as_reads() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "array_field_as.rs",
        "struct Pair { a: i32, b: i32 }\nfn work() {\n    let mut left = Pair { a: 0, b: 0 };\n    let mut right = Pair { a: 0, b: 0 };\n    [left.a, right.b] = [1, 2];\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_struct_field_assign_counts_bases_as_reads() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "struct_field_as.rs",
        "struct Point { x: i32, y: i32 }\nstruct Host { inner: i32 }\nfn work() {\n    let mut left = Host { inner: 0 };\n    let mut right = Host { inner: 0 };\n    let src = Point { x: 1, y: 2 };\n    Point { x: left.inner, y: right.inner } = src;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_local_paren_field_assign_counts_base_as_read() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "paren_field_as.rs",
        "struct Wrapper { inner: i32 }\nfn work() {\n    let mut host = Wrapper { inner: 0 };\n    (host.inner) = 1;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn if_let_unused_binder_is_reported() {
    let dir = TempDir::new().unwrap();
    // Binder must be recorded; dropping visit_pat on if-let would hide it.
    let path = write_file(
        dir.path(),
        "if_let_unused.rs",
        "fn work(value: Option<i32>) {\n    if let Some(x) = value {}\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("x"), "stdout={out:?}");
}

#[test]
fn if_let_scrutinee_counts_as_param_read() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "if_let_scrut.rs",
        "fn work(value: Option<i32>) {\n    if let Some(x) = value {\n        let _ = x;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedFormalParameter",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn plain_if_condition_local_read_counts() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "plain_if.rs",
        "fn work() {\n    let flag = true;\n    if flag {\n        let _ = 1;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn while_let_unused_binder_is_reported() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "while_let_unused.rs",
        "fn work(mut items: impl Iterator<Item = i32>) {\n    while let Some(x) = items.next() {}\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("x"), "stdout={out:?}");
}

#[test]
fn while_let_scrutinee_counts_as_param_read() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "while_let_scrut.rs",
        "fn work(value: Option<i32>) {\n    while let Some(x) = value {\n        let _ = x;\n        break;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedFormalParameter",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn while_body_local_read_counts() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "while_body.rs",
        "fn work() {\n    let body = 1;\n    while false {\n        let _ = body;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn plain_while_condition_local_read_counts() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "plain_while.rs",
        "fn work() {\n    let flag = false;\n    while flag {\n        let _ = 1;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn if_then_branch_local_read_counts() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "if_then.rs",
        "fn work() {\n    let then_val = 1;\n    if true {\n        let _ = then_val;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn for_loop_unused_binder_is_reported() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "for_unused.rs",
        "fn work() {\n    for x in 0..1 {}\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("x"), "stdout={out:?}");
}

#[test]
fn for_loop_iterable_param_read_counts() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "for_iter.rs",
        "fn work(items: [i32; 1]) {\n    for x in items {\n        let _ = x;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedFormalParameter",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn for_loop_body_local_read_counts() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "for_body.rs",
        "fn work() {\n    let body = 1;\n    for _ in 0..0 {\n        let _ = body;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn match_arm_unused_binder_is_reported() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "match_unused.rs",
        "fn work(value: Option<i32>) {\n    match value {\n        Some(x) => {}\n        None => {}\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("x"), "stdout={out:?}");
}

#[test]
fn unused_private_field_on_union_without_derive() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "union_dead.rs",
        "union Host { dead: i32 }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead"), "stdout={out:?}");
}

#[test]
fn unused_private_field_skips_serialize_on_union() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "union_ser.rs",
        "#[derive(Serialize)]\nunion Host { only_field: i32 }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn unused_private_field_on_enum_variant_without_derive() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "enum_dead.rs",
        "enum Host { Point { dead: i32 } }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead"), "stdout={out:?}");
}

#[test]
fn unused_private_field_skips_serialize_on_enum() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "enum_ser.rs",
        "#[derive(Serialize)]\nenum Host { Point { only_field: i32 } }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}

#[test]
fn derive_flag_restores_after_serialize_before_union() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "derive_restore_union.rs",
        "#[derive(Serialize)]\nstruct Outer { live: i32 }\nunion Host { dead: i32 }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead"), "stdout={out:?}");
    assert!(!out.contains("live"), "stdout={out:?}");
}

#[test]
fn derive_flag_restores_after_serialize_union_before_struct() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "derive_restore_after_union.rs",
        "#[derive(Serialize)]\nunion Outer { live: i32 }\nstruct Host { dead: i32 }\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedPrivateField",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead"), "stdout={out:?}");
    assert!(!out.contains("live"), "stdout={out:?}");
}

#[test]
fn unused_local_in_trait_default_method_body() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "trait_default_local.rs",
        "trait Host {\n    fn work(&self) {\n        let dead = 1;\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("dead"), "stdout={out:?}");
}

#[test]
fn unused_local_deref_assign_counts_pointer_as_read() {
    let dir = TempDir::new().unwrap();
    let path = write_file(
        dir.path(),
        "deref_as.rs",
        "fn work() {\n    let mut value = 0;\n    let ptr = &mut value;\n    *ptr = 1;\n}\n",
    );
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "unusedcode",
        "--only",
        "UnusedLocalVariable",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?} stdout={out:?}");
}
