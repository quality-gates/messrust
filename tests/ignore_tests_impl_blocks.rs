//! `--ignore-tests` and test-only `impl` blocks (#154).
//!
//! Seam: `messrust::run`. A `#[cfg(test)] impl` block, or a `#[cfg(test)]`
//! method inside a production `impl` block, must not add to a type metric when
//! `--ignore-tests` is set. Each case compares three runs of the real CLI:
//! the file with the test-only code and the flag, the same file with the
//! test-only code deleted, and the file with the test-only code and no flag.

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

fn ruleset(dir: &Path, name: &str, rule: &str, property_list: &[(&str, &str)]) -> PathBuf {
    let mut properties = String::new();
    for (key, value) in property_list {
        properties.push_str(&format!(
            "      <property name=\"{key}\" value=\"{value}\"/>\n"
        ));
    }
    write_file(
        dir,
        &format!("{name}.xml"),
        &format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n\
             <ruleset name=\"{name}\">\n  \
             <rule ref=\"{rule}\">\n    \
             <properties>\n{properties}    </properties>\n  </rule>\n</ruleset>\n"
        ),
    )
}

/// Analysis of `production` + `test_only` with the flag, of `production` alone
/// with the flag, and of `production` + `test_only` without the flag.
struct Outcomes {
    with_flag: (i32, String),
    production_only: (i32, String),
    without_flag: (i32, String),
}

fn outcomes(dir: &Path, production: &str, test_only: &str, xml: &Path) -> Outcomes {
    let combined = write_file(dir, "combined.rs", &format!("{production}{test_only}"));
    let production_only = write_file(dir, "production_only.rs", production);
    let xml = xml.to_str().unwrap();

    let (with_code, with_out, with_err) =
        run_cli(&[combined.to_str().unwrap(), "text", xml, "--ignore-tests"]);
    assert!(with_err.is_empty(), "stderr={with_err:?}");
    let (only_code, only_out, only_err) = run_cli(&[
        production_only.to_str().unwrap(),
        "text",
        xml,
        "--ignore-tests",
    ]);
    assert!(only_err.is_empty(), "stderr={only_err:?}");
    let (no_flag_code, no_flag_out, no_flag_err) =
        run_cli(&[combined.to_str().unwrap(), "text", xml]);
    assert!(no_flag_err.is_empty(), "stderr={no_flag_err:?}");

    Outcomes {
        with_flag: (with_code, with_out),
        production_only: (only_code, only_out),
        without_flag: (no_flag_code, no_flag_out),
    }
}

/// The flagged run of the combined file must agree with the production-only
/// run, and the unflagged run must still report the test-only code.
fn assert_test_impl_ignored(result: &Outcomes, rule: &str) {
    assert_eq!(
        result.with_flag.0, EXIT_SUCCESS,
        "{rule}: --ignore-tests must stay quiet, stdout={:?}",
        result.with_flag.1
    );
    assert!(
        result.with_flag.1.is_empty(),
        "{rule}: stdout={:?}",
        result.with_flag.1
    );
    assert_eq!(
        result.production_only.0, EXIT_SUCCESS,
        "{rule}: production code alone must stay quiet, stdout={:?}",
        result.production_only.1
    );
    assert_eq!(
        result.without_flag.0, EXIT_VIOLATION,
        "{rule}: no flag must keep the old count, stdout={:?}",
        result.without_flag.1
    );
    assert!(
        result.without_flag.1.contains(rule),
        "{rule}: stdout={:?}",
        result.without_flag.1
    );
}

const METHODS_PRODUCTION: &str =
    "struct S;\nimpl S {\n    fn m0(&self) {}\n    fn m1(&self) {}\n    fn m2(&self) {}\n}\n";
const METHODS_TEST_ONLY: &str =
    "#[cfg(test)]\nimpl S {\n    fn t0(&self) {}\n    fn t1(&self) {}\n    fn t2(&self) {}\n}\n";

#[test]
fn too_many_methods_drops_cfg_test_impl_methods() {
    let dir = TempDir::new().unwrap();
    let xml = ruleset(
        dir.path(),
        "tmm",
        "codesize/TooManyMethods",
        &[("maxmethods", "3"), ("ignorepattern", "")],
    );
    let result = outcomes(dir.path(), METHODS_PRODUCTION, METHODS_TEST_ONLY, &xml);
    assert_test_impl_ignored(&result, "TooManyMethods");
    assert!(
        result.without_flag.1.contains("has 6 non-getter"),
        "no flag must still count all six methods: stdout={:?}",
        result.without_flag.1
    );
}

#[test]
fn too_many_public_methods_drops_cfg_test_impl_methods() {
    let dir = TempDir::new().unwrap();
    let xml = ruleset(
        dir.path(),
        "tmpm",
        "codesize/TooManyPublicMethods",
        &[("maxmethods", "2"), ("ignorepattern", "")],
    );
    let result = outcomes(
        dir.path(),
        "struct S;\nimpl S {\n    pub fn m0(&self) {}\n    pub fn m1(&self) {}\n}\n",
        "#[cfg(test)]\nimpl S {\n    pub fn t0(&self) {}\n}\n",
        &xml,
    );
    assert_test_impl_ignored(&result, "TooManyPublicMethods");
}

#[test]
fn excessive_public_count_drops_cfg_test_impl_methods() {
    let dir = TempDir::new().unwrap();
    let xml = ruleset(
        dir.path(),
        "epc",
        "codesize/ExcessivePublicCount",
        &[("minimum", "3")],
    );
    let result = outcomes(
        dir.path(),
        "pub struct S {\n    pub a: i32,\n}\nimpl S {\n    pub fn m0(&self) {}\n}\n",
        "#[cfg(test)]\nimpl S {\n    pub fn t0(&self) {}\n}\n",
        &xml,
    );
    assert_test_impl_ignored(&result, "ExcessivePublicCount");
}

#[test]
fn excessive_class_complexity_drops_cfg_test_impl_methods() {
    let dir = TempDir::new().unwrap();
    let xml = ruleset(
        dir.path(),
        "ecc",
        "codesize/ExcessiveClassComplexity",
        &[("maximum", "5")],
    );
    // Each impl block holds cyclomatic complexity 3: one method, two branches.
    let branchy = |name: &str| {
        format!(
            "    fn {name}(&self, x: i32) -> i32 {{\n        if x > 0 {{ return 1; }}\n        if x < 0 {{ return 2; }}\n        0\n    }}\n"
        )
    };
    let result = outcomes(
        dir.path(),
        &format!("struct S;\nimpl S {{\n{}}}\n", branchy("production")),
        &format!("#[cfg(test)]\nimpl S {{\n{}}}\n", branchy("test_only")),
        &xml,
    );
    assert_test_impl_ignored(&result, "ExcessiveClassComplexity");
    assert!(
        result.without_flag.1.contains("overall complexity of 6"),
        "no flag must still sum both impl blocks: stdout={:?}",
        result.without_flag.1
    );
}

#[test]
fn excessive_class_length_drops_cfg_test_impl_lines() {
    let dir = TempDir::new().unwrap();
    let xml = ruleset(
        dir.path(),
        "ecl",
        "codesize/ExcessiveClassLength",
        &[("minimum", "8")],
    );
    let body = "        let _ = self;\n".repeat(4);
    let result = outcomes(
        dir.path(),
        &format!("struct S;\nimpl S {{\n    fn production(&self) {{\n{body}    }}\n}}\n"),
        &format!("#[cfg(test)]\nimpl S {{\n    fn test_only(&self) {{\n{body}    }}\n}}\n"),
        &xml,
    );
    assert_test_impl_ignored(&result, "ExcessiveClassLength");
}

#[test]
fn lack_of_cohesion_drops_cfg_test_impl_methods() {
    let dir = TempDir::new().unwrap();
    let xml = ruleset(
        dir.path(),
        "lcom",
        "design/LackOfCohesionOfMethods",
        &[("maximum", "1")],
    );
    // Both production methods read field `a`, so they form one component. The
    // test-only method reads field `b` and adds a second component.
    let result = outcomes(
        dir.path(),
        "struct S {\n    a: i32,\n    b: i32,\n}\nimpl S {\n    fn first(&self) -> i32 {\n        self.a + 1\n    }\n    fn second(&self) -> i32 {\n        self.a + 2\n    }\n}\n",
        "#[cfg(test)]\nimpl S {\n    fn test_only(&self) -> i32 {\n        self.b + 3\n    }\n}\n",
        &xml,
    );
    assert_test_impl_ignored(&result, "LackOfCohesionOfMethods");
}

#[test]
fn coupling_between_objects_drops_cfg_test_impl_dependencies() {
    let dir = TempDir::new().unwrap();
    let xml = ruleset(
        dir.path(),
        "cbo",
        "design/CouplingBetweenObjects",
        &[("maximum", "2")],
    );
    let result = outcomes(
        dir.path(),
        "struct Alpha;\nstruct Beta;\nstruct Gamma;\nstruct S;\nimpl S {\n    fn production(&self, alpha: Alpha) {\n        let _ = alpha;\n    }\n}\n",
        "#[cfg(test)]\nimpl S {\n    fn test_only(&self, beta: Beta, gamma: Gamma) {\n        let _ = (beta, gamma);\n    }\n}\n",
        &xml,
    );
    assert_test_impl_ignored(&result, "CouplingBetweenObjects");
}

#[test]
fn cfg_test_method_inside_production_impl_is_dropped() {
    let dir = TempDir::new().unwrap();
    let xml = ruleset(
        dir.path(),
        "tmm",
        "codesize/TooManyMethods",
        &[("maxmethods", "2"), ("ignorepattern", "")],
    );
    // The test-only method sits in the production impl block, so it cannot be
    // deleted as a whole block. Compare the flagged run with the unflagged run.
    let path = write_file(
        dir.path(),
        "single.rs",
        "struct S;\nimpl S {\n    fn m0(&self) {}\n    fn m1(&self) {}\n    #[cfg(test)]\n    fn t0(&self) {}\n}\n",
    );
    let xml = xml.to_str().unwrap();

    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml, "--ignore-tests"]);
    assert_eq!(code, EXIT_SUCCESS, "stdout={out:?} stderr={err:?}");

    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("has 3 non-getter"), "stdout={out:?}");
}

#[test]
fn cfg_not_test_and_feature_impl_blocks_still_count() {
    let dir = TempDir::new().unwrap();
    let xml = ruleset(
        dir.path(),
        "tmm",
        "codesize/TooManyMethods",
        &[("maxmethods", "2"), ("ignorepattern", "")],
    );
    let xml = xml.to_str().unwrap();
    // File names must not look like test paths: `--ignore-tests` skips those
    // before analysis and would hide the point of this case.
    for (name, guard) in [
        ("guard_not", "#[cfg(not(test))]"),
        ("guard_feature", "#[cfg(feature = \"x\")]"),
    ] {
        let path = write_file(
            dir.path(),
            &format!("{name}.rs"),
            &format!(
                "struct S;\nimpl S {{\n    fn m0(&self) {{}}\n    fn m1(&self) {{}}\n}}\n{guard}\nimpl S {{\n    fn m2(&self) {{}}\n}}\n"
            ),
        );
        let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml, "--ignore-tests"]);
        assert_eq!(code, EXIT_VIOLATION, "guard={guard} stderr={err:?}");
        assert!(
            out.contains("has 3 non-getter"),
            "guard={guard} stdout={out:?}"
        );
    }
}

#[test]
fn cfg_test_module_and_test_paths_keep_their_behaviour() {
    let dir = TempDir::new().unwrap();
    let xml = ruleset(
        dir.path(),
        "tmm",
        "codesize/TooManyMethods",
        &[("maxmethods", "2"), ("ignorepattern", "")],
    );
    let xml = xml.to_str().unwrap();
    // A type declared and filled inside a #[cfg(test)] module keeps dropping
    // through the finding filter, not through the model.
    let path = write_file(
        dir.path(),
        "with_mod.rs",
        "#[cfg(test)]\nmod tests {\n    struct S;\n    impl S {\n        fn m0(&self) {}\n        fn m1(&self) {}\n        fn m2(&self) {}\n    }\n}\n",
    );
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml, "--ignore-tests"]);
    assert_eq!(code, EXIT_SUCCESS, "stdout={out:?} stderr={err:?}");

    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", xml]);
    assert_eq!(code, EXIT_VIOLATION, "stderr={err:?}");
    assert!(out.contains("TooManyMethods"), "stdout={out:?}");
}
