//! explicitness rules through the injectable CLI entry.

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
    fs::write(&path, contents).unwrap();
    path
}

/// Each text finding as `(line, rule, message)`.
fn findings(out: &str, path: &Path) -> Vec<(usize, String, String)> {
    let prefix = format!("{}:", path.display());
    out.lines()
        .map(|line| {
            let rest = line.strip_prefix(&prefix).expect(line);
            let (number, rest) = rest.split_once(' ').unwrap();
            let (rule, message) = rest.trim_start().split_once(' ').unwrap();
            (
                number.parse().unwrap(),
                rule.to_string(),
                message.trim_start().to_string(),
            )
        })
        .collect()
}

fn expect(line: usize, rule: &str, message: &str) -> (usize, String, String) {
    (line, rule.to_string(), message.to_string())
}

fn run_ruleset(source: &str, ruleset: &str) -> (i32, Vec<(usize, String, String)>) {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "ex.rs", source);
    let ruleset_arg = if ruleset.starts_with('<') {
        let xml = write_file(dir.path(), "strict.xml", ruleset);
        xml.to_str().unwrap().to_string()
    } else {
        ruleset.to_string()
    };
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "text", &ruleset_arg]);
    assert!(err.is_empty(), "stderr={err:?}");
    (code, findings(&out, &path))
}

const STRICT: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="strict">
  <rule ref="explicitness/ImplicitInput">
    <properties><property name="include-self" value="true"/></properties>
  </rule>
  <rule ref="explicitness/ImplicitOutput">
    <properties><property name="include-self" value="true"/></properties>
  </rule>
</ruleset>
"#;

#[test]
fn pure_function_with_constants_is_clean() {
    let source = "const LIMIT: u32 = 3;\nstatic NAME: &str = \"x\";\n\
fn add(a: u32, b: u32) -> u32 {\n    a + b + LIMIT + NAME.len() as u32\n}\n";
    let (code, found) = run_ruleset(source, "explicitness");
    assert_eq!(code, EXIT_SUCCESS, "found={found:?}");
    assert!(found.is_empty(), "found={found:?}");
}

#[test]
fn reads_of_shared_statics_and_ambient_calls_are_implicit_inputs() {
    let source = "use std::sync::atomic::{AtomicUsize, Ordering};\n\
static HITS: AtomicUsize = AtomicUsize::new(0);\n\
static mut COUNT: u32 = 0;\n\
fn read() -> usize {\n\
    let now = std::time::SystemTime::now();\n\
    let _ = now;\n\
    let home = std::env::var(\"HOME\").ok();\n\
    let _ = unsafe { COUNT };\n\
    HITS.load(Ordering::Relaxed) + home.map_or(0, |h| h.len())\n\
}\n";
    let (code, found) = run_ruleset(source, "explicitness");
    assert_eq!(code, EXIT_VIOLATION);
    let input = "ImplicitInput";
    assert_eq!(
        found,
        vec![
            expect(
                5,
                input,
                "The function 'read' has an implicit input: it calls 'SystemTime::now'."
            ),
            expect(
                7,
                input,
                "The function 'read' has an implicit input: it calls 'env::var'."
            ),
            expect(
                8,
                input,
                "The function 'read' has an implicit input: it reads the global 'COUNT'."
            ),
            expect(
                9,
                input,
                "The function 'read' has an implicit input: it reads the global 'HITS'."
            ),
        ]
    );
}

#[test]
fn writes_prints_and_mut_params_are_implicit_outputs() {
    let source = "use std::cell::RefCell;\n\
static mut COUNT: u32 = 0;\n\
thread_local! {\n    static CACHE: RefCell<Vec<u32>> = RefCell::new(Vec::new());\n}\n\
fn write(out: &mut Vec<u32>) {\n\
    unsafe { COUNT = 1; }\n\
    println!(\"hi\");\n\
    CACHE.with_borrow_mut(|c| c.push(1));\n\
    std::fs::write(\"a\", \"b\").unwrap();\n\
    out.push(1);\n\
}\n";
    let (code, found) = run_ruleset(source, "explicitness");
    assert_eq!(code, EXIT_VIOLATION);
    let output = "ImplicitOutput";
    assert_eq!(
        found,
        vec![
            expect(6, output, "The function 'write' has an implicit output: it writes through the '&mut' parameter 'out'."),
            expect(7, output, "The function 'write' has an implicit output: it writes the global 'COUNT'."),
            expect(8, output, "The function 'write' has an implicit output: it uses 'println!'."),
            expect(9, output, "The function 'write' has an implicit output: it writes the global 'CACHE'."),
            expect(10, output, "The function 'write' has an implicit output: it calls 'fs::write'."),
        ]
    );
}

#[test]
fn repeated_site_is_reported_once_at_first_line() {
    let source = "static mut COUNT: u32 = 0;\n\
fn bump() {\n\
    unsafe { COUNT += 1; }\n\
    unsafe { COUNT += 1; }\n\
}\n";
    let (_, found) = run_ruleset(source, "explicitness");
    assert_eq!(
        found,
        vec![expect(
            3,
            "ImplicitOutput",
            "The function 'bump' has an implicit output: it writes the global 'COUNT'."
        )]
    );
}

#[test]
fn format_capture_of_shared_static_is_an_implicit_input() {
    let source = "static mut COUNT: u32 = 0;\n\
fn show() -> String {\n\
    unsafe { format!(\"{COUNT}\") }\n\
}\n";
    let (_, found) = run_ruleset(source, "explicitness");
    assert_eq!(
        found,
        vec![expect(
            3,
            "ImplicitInput",
            "The function 'show' has an implicit input: it reads the global 'COUNT'."
        )]
    );
}

const CART: &str = "struct Cart {\n    items: Vec<u32>,\n}\n\
impl Cart {\n\
    fn total(&self) -> u32 {\n        self.items.iter().sum()\n    }\n\
    fn clear(&mut self) {\n        self.items = Vec::new();\n    }\n\
}\n";

#[test]
fn method_state_is_not_reported_by_default() {
    let (code, found) = run_ruleset(CART, "explicitness");
    assert_eq!(code, EXIT_SUCCESS, "found={found:?}");
    assert!(found.is_empty(), "found={found:?}");
}

#[test]
fn include_self_reports_method_state_reads_and_changes() {
    let (code, found) = run_ruleset(CART, STRICT);
    assert_eq!(code, EXIT_VIOLATION);
    assert_eq!(
        found,
        vec![
            expect(
                6,
                "ImplicitInput",
                "The method 'Cart::total' has an implicit input: it reads the state of 'self'."
            ),
            expect(
                8,
                "ImplicitOutput",
                "The method 'Cart::clear' has an implicit output: it changes the state of 'self'."
            ),
        ]
    );
}

#[test]
fn trait_impl_skips_signature_and_self_but_not_body_effects() {
    let source = "struct Cart {\n    items: Vec<u32>,\n}\n\
impl std::fmt::Display for Cart {\n\
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n\
        println!(\"debug\");\n\
        write!(f, \"{}\", self.items.len())\n\
    }\n\
}\n";
    let (_, found) = run_ruleset(source, STRICT);
    assert_eq!(
        found,
        vec![expect(
            6,
            "ImplicitOutput",
            "The method 'Cart::fmt' has an implicit output: it uses 'println!'."
        )]
    );
}

#[test]
fn nested_function_effects_belong_to_the_nested_function() {
    let source = "fn outer() -> u32 {\n\
    fn inner() {\n        println!(\"x\");\n    }\n\
    inner();\n\
    1\n\
}\n";
    let (_, found) = run_ruleset(source, "explicitness");
    assert!(
        found
            .iter()
            .all(|(_, _, message)| !message.contains("'outer'")),
        "found={found:?}"
    );
}

#[test]
fn write_through_lazy_lock_mutex_static_is_an_implicit_output() {
    let source = "use std::sync::{LazyLock, Mutex};\n\
static AUDIT: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));\n\
fn audit(entry: &str) {\n\
    AUDIT.lock().unwrap().push(entry.to_string());\n\
}\n";
    let (_, found) = run_ruleset(source, "explicitness");
    assert_eq!(
        found,
        vec![expect(
            4,
            "ImplicitOutput",
            "The function 'audit' has an implicit output: it writes the global 'AUDIT'."
        )]
    );
}

#[test]
fn write_through_thread_local_with_closure_is_an_implicit_output() {
    let source = "use std::cell::RefCell;\n\
thread_local! {\n    static SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::new());\n}\n\
fn push(byte: u8) {\n\
    SCRATCH.with(|s| s.borrow_mut().push(byte));\n\
}\n";
    let (_, found) = run_ruleset(source, "explicitness");
    assert_eq!(
        found,
        vec![expect(
            6,
            "ImplicitOutput",
            "The function 'push' has an implicit output: it writes the global 'SCRATCH'."
        )]
    );
}

#[test]
fn write_through_qualified_thread_local_key_is_an_implicit_output() {
    let source = "use std::cell::Cell;\n\
thread_local! {\n    static COUNTER: Cell<u32> = Cell::new(0);\n}\n\
fn bump() {\n\
    self::COUNTER.with(|c| c.set(1));\n\
}\n";
    let (_, found) = run_ruleset(source, "explicitness");
    assert_eq!(
        found,
        vec![expect(
            6,
            "ImplicitOutput",
            "The function 'bump' has an implicit output: it writes the global 'COUNTER'."
        )]
    );
}

#[test]
fn write_through_typed_thread_local_closure_parameter_is_an_implicit_output() {
    let source = "use std::cell::{Cell, RefCell};\n\
thread_local! {\n    static COUNTER: Cell<u32> = Cell::new(0);\n}\n\
thread_local! {\n    static LOG: RefCell<Vec<i32>> = RefCell::new(Vec::new());\n}\n\
fn bump() {\n\
    COUNTER.with(|c: &Cell<u32>| c.set(1));\n\
}\n\
fn log() {\n\
    LOG.with(|v: &RefCell<Vec<i32>>| v.borrow_mut().push(1));\n\
}\n";
    let (_, found) = run_ruleset(source, "explicitness");
    assert_eq!(
        found,
        vec![
            expect(
                9,
                "ImplicitOutput",
                "The function 'bump' has an implicit output: it writes the global 'COUNTER'."
            ),
            expect(
                12,
                "ImplicitOutput",
                "The function 'log' has an implicit output: it writes the global 'LOG'."
            ),
        ]
    );
}

#[test]
fn read_through_typed_qualified_thread_local_key_stays_an_implicit_input() {
    let source = "use std::cell::Cell;\n\
thread_local! {\n    static COUNTER: Cell<u32> = Cell::new(0);\n}\n\
fn peek() -> u32 {\n\
    self::COUNTER.with(|c: &Cell<u32>| c.get())\n\
}\n";
    let (_, found) = run_ruleset(source, "explicitness");
    assert_eq!(
        found,
        vec![expect(
            6,
            "ImplicitInput",
            "The function 'peek' has an implicit input: it reads the global 'COUNTER'."
        )]
    );
}
