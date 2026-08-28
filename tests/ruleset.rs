//! Ruleset loading and filtering through the injectable CLI entry.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use messrust::{run, EXIT_ERROR, EXIT_SUCCESS, EXIT_VIOLATION};
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

fn fixture_with_params(n: usize) -> String {
    let params: Vec<String> = (0..n).map(|i| format!("param_{i}: i32")).collect();
    format!("fn entry_point({}) {{}}\n", params.join(", "))
}

#[test]
fn direct_class_rule_without_priority_defaults_to_priority_three() {
    // A rule defined by class (not ref), with no <priority> element, must
    // fall back to priority 3 and use the message attribute given on the
    // rule itself.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let xml = dir.path().join("direct.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="Direct">
  <rule name="ExcessiveParameterList"
        message="Too many parameters: {0}"
        class="PHPMD\Rule\Design\LongParameterList"/>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "json", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("\"priority\": 3"), "stdout={out:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn rule_without_class_or_ref_is_skipped_silently() {
    // append_rule returns early when both class and ref are empty: no
    // error, no warning, no rule loaded from that entry.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fixture_with_params(0));
    let xml = dir.path().join("noop.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="Noop">
  <rule name="NeitherClassNorRef"/>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        xml.to_str().unwrap(),
        "--verbose",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(err.is_empty(), "stderr={err:?}");
}

#[test]
fn named_ref_to_missing_rule_in_source_ruleset_yields_no_rule_and_no_error() {
    // add_named_rule: the ref names a rule that does not exist in the
    // resolved source ruleset. No rule is added, and the load itself does
    // not error.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let xml = dir.path().join("missing.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="Missing">
  <rule ref="codesize/NotARealRule"/>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(err.is_empty(), "stderr={err:?}");
}

#[test]
fn unresolvable_ref_warns_cannot_resolve_and_does_not_error() {
    // read_referenced_ruleset: the ref base is neither a builtin name nor
    // a file. The load must warn (in verbose mode) and continue rather
    // than fail.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fixture_with_params(0));
    let xml = dir.path().join("badref.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="BadRef">
  <rule ref="doesnotexist/Something"/>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        xml.to_str().unwrap(),
        "--verbose",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        err.contains("warning: Cannot resolve ref: doesnotexist/Something"),
        "stderr={err:?}"
    );
}

#[test]
fn property_value_reads_nested_value_element_when_attribute_absent() {
    // property_value falls back to a nested <value> element's text when
    // the property has no `value` attribute.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(6));
    let xml = dir.path().join("nested.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="Nested">
  <rule ref="codesize/ExcessiveParameterList">
    <properties>
      <property name="minimum">
        <value>5</value>
      </property>
    </properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn minimumpriority_boundary_equal_to_rule_priority_keeps_the_rule() {
    // ExcessiveParameterList has priority 3. minimumpriority 3 keeps
    // priority <= 3, so the boundary value must not drop it.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--minimumpriority",
        "3",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn maximumpriority_boundary_equal_to_rule_priority_keeps_the_rule() {
    // ExcessiveParameterList has priority 3. maximumpriority 3 keeps
    // priority >= 3, so the boundary value must not drop it.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--maximumpriority",
        "3",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn builtin_name_matches_are_case_insensitive() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "CodeSize"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn builtin_name_with_xml_suffix_resolves_to_the_builtin_ruleset() {
    // A spec that looks like a filename but is not present on disk still
    // resolves to the builtin ruleset when its stem matches a known name.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "codesize.xml"]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn unnamed_ruleset_uses_file_stem_as_ruleset_name_in_xml_output() {
    // parse_ruleset: an empty name="" attribute falls back to the file
    // stem for the display/ruleset name. Use a direct class rule (no ref)
    // so the top-level ruleset's own name is what reaches the report,
    // not a referenced ruleset's name.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let xml = dir.path().join("stem_name.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset>
  <rule name="ExcessiveParameterList"
        message="Too many parameters: {0}"
        class="PHPMD\Rule\Design\LongParameterList"/>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "xml", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ruleset=\"stem_name\""), "stdout={out:?}");
}

#[test]
fn named_ruleset_attribute_overrides_file_stem_in_xml_output() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let xml = dir.path().join("stem_name.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="Explicit Name">
  <rule name="ExcessiveParameterList"
        message="Too many parameters: {0}"
        class="PHPMD\Rule\Design\LongParameterList"/>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "xml", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ruleset=\"Explicit Name\""), "stdout={out:?}");
    assert!(!out.contains("ruleset=\"stem_name\""), "stdout={out:?}");
}

#[test]
fn invalid_ruleset_xml_reports_parse_error_on_stderr() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fixture_with_params(0));
    let xml = dir.path().join("broken.xml");
    fs::write(&xml, "not xml at all <").unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_ERROR);
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(err.contains("invalid ruleset XML"), "stderr={err:?}");
}

#[test]
fn ruleset_xml_with_wrong_root_element_reports_error() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fixture_with_params(0));
    let xml = dir.path().join("wrongroot.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<notaruleset name="Wrong">
  <rule ref="codesize/ExcessiveParameterList"/>
</notaruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_ERROR);
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        err.contains("ruleset XML root must be <ruleset>"),
        "stderr={err:?}"
    );
}

#[test]
fn ref_to_a_ruleset_with_an_exclude_missing_name_attribute_is_ignored() {
    // parse_rule_child: an <exclude> without a name attribute must not
    // panic and must not exclude anything.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let xml = dir.path().join("bare_exclude.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="BareExclude">
  <rule ref="codesize/ExcessiveParameterList">
    <exclude/>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn full_ruleset_reference_keeps_named_excludes() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let xml = dir.path().join("exclude.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="Exclude">
  <rule ref="codesize">
    <exclude name="ExcessiveParameterList"/>
  </rule>
</ruleset>
"#,
    )
    .unwrap();

    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);

    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(!out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn unknown_ruleset_name_reports_a_clear_error() {
    // read_ruleset: an identifier that matches neither a builtin name nor a
    // file on disk must fail with a clear message naming the bad spec.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fixture_with_params(0));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", "not-a-real-ruleset"]);
    assert_eq!(code, EXIT_ERROR, "stdout={out:?} stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        err.contains("unknown ruleset or file: not-a-real-ruleset"),
        "stderr={err:?}"
    );
}

#[test]
fn only_filter_naming_an_unloaded_rule_reports_a_clear_error() {
    // apply_name_filters: --only naming a rule absent from the loaded
    // rulesets must fail with a clear message naming the bad rule.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--only",
        "NotALoadedRule",
    ]);
    assert_eq!(code, EXIT_ERROR, "stdout={out:?} stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(
        err.contains("rule 'NotALoadedRule' is not present in the loaded rulesets"),
        "stderr={err:?}"
    );
}

#[test]
fn only_filter_keeps_the_named_rule_and_drops_the_rest() {
    // apply_name_filters: --only retains just the named rule even though
    // the codesize ruleset loads more than one rule.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--only",
        "ExcessiveParameterList",
    ]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn disable_filter_drops_the_named_rule() {
    // apply_name_filters: --disable removes the named rule, so a fixture
    // that would otherwise violate it produces no finding.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let (code, out, err) = run_cli(&[
        path.to_str().unwrap(),
        "text",
        "codesize",
        "--disable",
        "ExcessiveParameterList",
    ]);
    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(!out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn empty_ruleset_spec_reports_no_rulesets_specified() {
    // load_and_filter: an empty ruleset positional splits to zero specs,
    // which must fail clearly rather than silently loading nothing.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "clean.rs", &fixture_with_params(0));
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", ""]);
    assert_eq!(code, EXIT_ERROR, "stdout={out:?} stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert!(err.contains("no rulesets specified"), "stderr={err:?}");
}

#[test]
fn bare_ruleset_ref_pulls_in_every_rule_from_the_referenced_ruleset() {
    // add_ruleset_rules: a <rule ref="codesize"/> with no rule name in the
    // ref path pulls in every non-empty-class rule from the whole
    // referenced ruleset, not just one named rule.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let xml = dir.path().join("bare_ruleset_ref.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="BareRulesetRef">
  <rule ref="codesize"/>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn duplicate_rule_names_across_specs_are_deduplicated() {
    // load_and_filter: seen.insert(...) dedups a rule name loaded twice
    // (e.g. two comma-separated rulesets that reference the same rule).
    // Assert only one finding is printed, not two.
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let first = dir.path().join("first.xml");
    fs::write(
        &first,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="First">
  <rule ref="codesize/ExcessiveParameterList"/>
</ruleset>
"#,
    )
    .unwrap();
    let second = dir.path().join("second.xml");
    fs::write(
        &second,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="Second">
  <rule ref="codesize/ExcessiveParameterList"/>
</ruleset>
"#,
    )
    .unwrap();
    let spec = format!("{},{}", first.to_str().unwrap(), second.to_str().unwrap());
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", &spec]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert_eq!(
        out.matches("ExcessiveParameterList").count(),
        1,
        "stdout={out:?}"
    );
}

#[test]
fn custom_ruleset_referencing_rust_ruleset_pulls_in_rules() {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "fixture.rs", &fixture_with_params(11));
    let xml = dir.path().join("ref_rust.xml");
    fs::write(
        &xml,
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="RefRust">
  <rule ref="rust"/>
</ruleset>
"#,
    )
    .unwrap();
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml.to_str().unwrap()]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn self_referencing_ruleset_reports_the_reference_chain() {
    let dir = TempDir::new().unwrap();
    let source = write_file(dir.path(), "clean.rs", "fn entry_point() {}\n");
    let ruleset = dir.path().join("self.xml");
    fs::write(
        &ruleset,
        format!(
            r#"<ruleset name="Self">
  <rule ref="{}"/>
</ruleset>
"#,
            ruleset.display()
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_messrust"))
        .args([source.to_str().unwrap(), "text", ruleset.to_str().unwrap()])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(EXIT_ERROR), "stderr={stderr:?}");
    assert!(
        stderr.contains("ruleset reference cycle"),
        "stderr={stderr:?}"
    );
    assert!(
        stderr.matches(ruleset.to_str().unwrap()).count() >= 2,
        "stderr={stderr:?}"
    );
}

#[test]
fn two_file_ruleset_cycle_reports_the_reference_chain() {
    let dir = TempDir::new().unwrap();
    let source = write_file(dir.path(), "clean.rs", "fn entry_point() {}\n");
    let first = dir.path().join("first.xml");
    let second = dir.path().join("second.xml");
    fs::write(
        &first,
        format!(
            r#"<ruleset name="First">
  <rule ref="{}"/>
</ruleset>
"#,
            second.display()
        ),
    )
    .unwrap();
    fs::write(
        &second,
        format!(
            r#"<ruleset name="Second">
  <rule ref="{}"/>
</ruleset>
"#,
            first.display()
        ),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_messrust"))
        .args([source.to_str().unwrap(), "text", first.to_str().unwrap()])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(EXIT_ERROR), "stderr={stderr:?}");
    assert!(
        stderr.contains("ruleset reference cycle"),
        "stderr={stderr:?}"
    );
    assert!(
        stderr.contains(first.to_str().unwrap()),
        "stderr={stderr:?}"
    );
    assert!(
        stderr.contains(second.to_str().unwrap()),
        "stderr={stderr:?}"
    );
}

#[test]
fn self_named_reference_to_a_direct_rule_is_valid() {
    let dir = TempDir::new().unwrap();
    let source = write_file(dir.path(), "fixture.rs", &fixture_with_params(6));
    let ruleset = dir.path().join("self_named.xml");
    fs::write(
        &ruleset,
        format!(
            r#"<ruleset name="SelfNamed">
  <rule ref="{}/ExcessiveParameterList">
    <properties>
      <property name="minimum" value="5"/>
    </properties>
  </rule>
  <rule name="ExcessiveParameterList" class="PHPMD\Rule\Design\LongParameterList"/>
</ruleset>
"#,
            ruleset.display()
        ),
    )
    .unwrap();

    let (code, out, err) = run_cli(&[source.to_str().unwrap(), "text", ruleset.to_str().unwrap()]);

    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("ExcessiveParameterList"), "stdout={out:?}");
}

#[test]
fn nested_named_references_keep_override_precedence() {
    let dir = TempDir::new().unwrap();
    let source = write_file(dir.path(), "fixture.rs", &fixture_with_params(6));
    let middle = dir.path().join("middle.xml");
    let outer = dir.path().join("outer.xml");
    fs::write(
        &middle,
        r#"<ruleset name="Middle">
  <rule ref="codesize/ExcessiveParameterList" message="Middle {0}">
    <priority>2</priority>
    <properties><property name="minimum" value="7"/></properties>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    fs::write(
        &outer,
        format!(
            r#"<ruleset name="Outer">
  <rule ref="{}/ExcessiveParameterList" message="Outer {{2}}">
    <priority>1</priority>
    <properties><property name="minimum" value="5"/></properties>
  </rule>
</ruleset>
"#,
            middle.display()
        ),
    )
    .unwrap();

    let (code, out, err) = run_cli(&[source.to_str().unwrap(), "json", outer.to_str().unwrap()]);

    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("\"priority\": 2"), "stdout={out:?}");
    assert!(out.contains("Outer 6"), "stdout={out:?}");
}

fn write_repeated_diamond(dir: &Path, depth: usize, leaf: &str) -> PathBuf {
    for index in 0..depth {
        let current = dir.join(format!("node-{index}.xml"));
        let next = if index + 1 == depth {
            leaf.to_string()
        } else {
            dir.join(format!("node-{}.xml", index + 1))
                .display()
                .to_string()
        };
        fs::write(
            current,
            format!(
                r#"<ruleset name="Node {index}">
  <rule ref="missing-ruleset-{index}"/>
  <rule ref="{next}"><exclude name="DummyLeft{index}"/></rule>
  <rule ref="{next}"><exclude name="DummyRight{index}"/></rule>
</ruleset>
"#,
            ),
        )
        .unwrap();
    }

    dir.join("node-0.xml")
}

fn write_relevant_diamond(dir: &Path, depth: usize, leaf: &str, override_xml: &str) -> PathBuf {
    for index in 0..depth {
        let current = dir.join(format!("relevant-{index}.xml"));
        let next = if index + 1 == depth {
            leaf.to_string()
        } else {
            dir.join(format!("relevant-{}.xml", index + 1))
                .display()
                .to_string()
        };
        fs::write(
            current,
            format!(
                "<ruleset name=\"Relevant {index}\">\n\
                 <rule ref=\"missing-relevant-{index}\"/>\n\
                 <rule ref=\"{next}\">{override_xml}</rule>\n\
                 <rule ref=\"{next}\">{override_xml}</rule>\n\
                 </ruleset>\n"
            ),
        )
        .unwrap();
    }
    dir.join("relevant-0.xml")
}

#[test]
fn repeated_excluded_diamond_expands_each_ruleset_once() {
    let dir = TempDir::new().unwrap();
    let source = write_file(dir.path(), "clean.rs", "fn entry_point() {}\n");
    let depth = 9;
    let first = write_repeated_diamond(dir.path(), depth, "codesize");

    let root = dir.path().join("root.xml");
    fs::write(
        &root,
        format!(
            r#"<ruleset name="Root">
  <rule ref="{}">
    <exclude name="ExcessiveParameterList"/>
  </rule>
</ruleset>
"#,
            first.display()
        ),
    )
    .unwrap();

    let (code, _out, err) = run_cli(&[
        source.to_str().unwrap(),
        "text",
        root.to_str().unwrap(),
        "--verbose",
    ]);

    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert_eq!(
        err.matches("Cannot resolve ref").count(),
        depth,
        "stderr={err:?}"
    );
}

#[test]
fn repeated_priority_filtered_diamond_expands_each_ruleset_once() {
    let dir = TempDir::new().unwrap();
    let source = write_file(dir.path(), "clean.rs", "fn entry_point() {}\n");
    let depth = 9;
    let first = write_repeated_diamond(dir.path(), depth, "codesize/ExcessiveParameterList");
    let root = dir.path().join("root.xml");
    fs::write(
        &root,
        format!(
            r#"<ruleset name="Root">
  <rule ref="{}"><priority>5</priority></rule>
</ruleset>
"#,
            first.display()
        ),
    )
    .unwrap();

    let (code, _out, err) = run_cli(&[
        source.to_str().unwrap(),
        "text",
        root.to_str().unwrap(),
        "--minimumpriority",
        "3",
        "--verbose",
    ]);

    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert_eq!(
        err.matches("Cannot resolve ref").count(),
        depth,
        "stderr={err:?}"
    );
}

#[test]
fn relevant_exclusion_diamond_expands_each_ruleset_once() {
    let dir = TempDir::new().unwrap();
    let source = write_file(dir.path(), "clean.rs", "fn entry_point() {}\n");
    let leaf = write_file(
        dir.path(),
        "blocked.xml",
        "<ruleset name=\"Blocked\"><rule name=\"Blocked\" class=\"PHPMD\\Rule\\Design\\LongParameterList\"/></ruleset>\n",
    );
    let depth = 14;
    let first = write_relevant_diamond(
        dir.path(),
        depth,
        leaf.to_str().unwrap(),
        "<exclude name=\"Blocked\"/>",
    );

    let (code, out, err) = run_cli(&[
        source.to_str().unwrap(),
        "text",
        first.to_str().unwrap(),
        "--verbose",
    ]);

    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert_eq!(err.matches("Cannot resolve ref").count(), depth);
}

#[test]
fn relevant_priority_diamond_expands_each_ruleset_once() {
    let dir = TempDir::new().unwrap();
    let source = write_file(dir.path(), "clean.rs", "fn entry_point() {}\n");
    let depth = 14;
    let first = write_relevant_diamond(
        dir.path(),
        depth,
        "codesize/ExcessiveParameterList",
        "<priority>5</priority>",
    );

    let (code, out, err) = run_cli(&[
        source.to_str().unwrap(),
        "text",
        first.to_str().unwrap(),
        "--minimumpriority",
        "3",
        "--verbose",
    ]);

    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
    assert_eq!(err.matches("Cannot resolve ref").count(), depth);
}

#[test]
fn deep_chain_shares_many_blocked_rules() {
    let dir = TempDir::new().unwrap();
    let source = write_file(dir.path(), "clean.rs", "fn entry_point() {}\n");
    let rule_count = 200;
    let mut leaf_xml = String::from("<ruleset name=\"Leaf\">\n");
    for index in 0..rule_count {
        leaf_xml.push_str(&format!(
            "  <rule name=\"Blocked{index}\" class=\"PHPMD\\Rule\\Design\\LongParameterList\"/>\n"
        ));
    }
    leaf_xml.push_str("</ruleset>\n");
    let leaf = write_file(dir.path(), "leaf.xml", &leaf_xml);

    let depth = 40;
    let mut next = leaf;
    for index in (0..depth).rev() {
        let current = dir.path().join(format!("chain-{index}.xml"));
        fs::write(
            &current,
            format!(
                "<ruleset name=\"Chain {index}\">\
                 <rule name=\"Local{index}\" class=\"PHPMD\\Rule\\Design\\LongParameterList\"/>\
                 <rule ref=\"{}\"/>\
                 </ruleset>\n",
                next.display()
            ),
        )
        .unwrap();
        next = current;
    }

    let root = dir.path().join("root.xml");
    let mut root_xml = format!("<ruleset name=\"Root\"><rule ref=\"{}\">\n", next.display());
    for index in 0..rule_count {
        root_xml.push_str(&format!("  <exclude name=\"Blocked{index}\"/>\n"));
    }
    for index in 0..depth {
        root_xml.push_str(&format!("  <exclude name=\"Local{index}\"/>\n"));
    }
    root_xml.push_str("</rule></ruleset>\n");
    fs::write(&root, root_xml).unwrap();

    let (code, out, err) = run_cli(&[source.to_str().unwrap(), "text", root.to_str().unwrap()]);

    assert_eq!(code, EXIT_SUCCESS, "stderr={err:?}");
    assert!(out.is_empty(), "stdout={out:?}");
}

#[test]
fn nested_named_reference_uses_the_inner_exclusion_boundary() {
    let dir = TempDir::new().unwrap();
    let source = write_file(
        dir.path(),
        "fixture.rs",
        r#"struct Large {
    a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32, h: i32,
    i: i32, j: i32, k: i32, l: i32, m: i32, n: i32, o: i32, p: i32,
}
fn entry_point(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32,
    g: i32, h: i32, i: i32, j: i32, k: i32) {}
"#,
    );
    let middle = dir.path().join("middle.xml");
    let outer = dir.path().join("outer.xml");
    fs::write(
        &middle,
        r#"<ruleset name="Middle">
  <rule name="MiddleBundle" ref="codesize">
    <exclude name="ExcessiveParameterList"/>
  </rule>
</ruleset>
"#,
    )
    .unwrap();
    fs::write(
        &outer,
        format!(
            r#"<ruleset name="Outer">
  <rule ref="{}/MiddleBundle"><exclude name="TooManyFields"/></rule>
</ruleset>
"#,
            middle.display()
        ),
    )
    .unwrap();

    let (code, out, err) = run_cli(&[source.to_str().unwrap(), "text", outer.to_str().unwrap()]);

    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("TooManyFields"), "stdout={out:?}");
    assert!(!out.contains("ExcessiveParameterList"), "stdout={out:?}");
}
