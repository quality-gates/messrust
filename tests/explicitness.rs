use std::fs;
use std::path::{Path, PathBuf};

use messrust::{run, EXIT_SUCCESS, EXIT_VIOLATION};
use serde_json::Value;
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

fn run_json(source: &str, ruleset: &str) -> (i32, Value) {
    let dir = TempDir::new().unwrap();
    let path = write_file(dir.path(), "ex.rs", source);
    let (code, out, err) = run_cli(&[path.to_str().unwrap(), "json", ruleset]);
    assert!(err.is_empty(), "stderr={err:?}");
    (code, serde_json::from_str(&out).unwrap())
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
            expect(7, output, "The function 'write' has an implicit output: it writes the global 'COUNT'."),
            expect(8, output, "The function 'write' has an implicit output: it uses 'println!'."),
            expect(9, output, "The function 'write' has an implicit output: it writes the global 'CACHE'."),
            expect(10, output, "The function 'write' has an implicit output: it calls 'fs::write'."),
            expect(11, output, "The function 'write' has an implicit output: it writes through the borrowed parameter 'out'."),
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
        vec![
            expect(
                3,
                "ImplicitInput",
                "The function 'bump' has an implicit input: it reads the global 'COUNT'."
            ),
            expect(
                3,
                "ImplicitOutput",
                "The function 'bump' has an implicit output: it writes the global 'COUNT'."
            ),
        ]
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
fn borrowed_method_state_writes_are_outputs_by_default() {
    let (code, found) = run_ruleset(CART, "explicitness");
    assert_eq!(code, EXIT_VIOLATION);
    assert_eq!(
        found,
        vec![expect(
            9,
            "ImplicitOutput",
            "The method 'Cart::clear' has an implicit output: it changes the state of 'self'."
        )]
    );
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
                9,
                "ImplicitOutput",
                "The method 'Cart::clear' has an implicit output: it changes the state of 'self'."
            ),
        ]
    );
}

#[test]
fn trait_impl_retains_effects_and_receiver_reads() {
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
        vec![
            expect(6, "ImplicitOutput", "The method 'Cart::fmt' has an implicit output: it uses 'println!'."),
            expect(7, "ImplicitInput", "The method 'Cart::fmt' has an implicit input: it reads the state of 'self'."),
            expect(7, "ImplicitOutput", "The method 'Cart::fmt' has an implicit output: it writes through the borrowed parameter 'f'."),
        ]
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
    assert_eq!(
        found,
        vec![
            expect(
                3,
                "ImplicitOutput",
                "The function 'inner' has an implicit output: it uses 'println!'."
            ),
            expect(
                5,
                "ImplicitOutput",
                "The function 'outer' has an implicit output: it uses 'println!'."
            ),
        ]
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

#[test]
fn qualified_self_calls_are_implicit_inputs_and_outputs() {
    let source = "use std::fs::File;\n\
use std::time::Instant;\n\
use std::process::Command;\n\
\n\
pub fn test_inputs() {\n\
    let _ = <File>::open(\"input.txt\");\n\
    let _ = <Instant>::now();\n\
}\n\
\n\
pub fn test_outputs() {\n\
    let _ = <File>::create(\"output.txt\");\n\
    let _ = <Command>::new(\"ls\");\n\
}\n";
    let (code, found) = run_ruleset(source, "explicitness");
    assert_eq!(code, EXIT_VIOLATION);
    let input = "ImplicitInput";
    let output = "ImplicitOutput";
    assert_eq!(
        found,
        vec![
            expect(
                6,
                input,
                "The function 'test_inputs' has an implicit input: it calls 'File::open'."
            ),
            expect(
                7,
                input,
                "The function 'test_inputs' has an implicit input: it calls 'Instant::now'."
            ),
            expect(
                11,
                output,
                "The function 'test_outputs' has an implicit output: it calls 'File::create'."
            ),
            expect(
                12,
                output,
                "The function 'test_outputs' has an implicit output: it calls 'Command::new'."
            ),
        ]
    );
}

#[test]
fn qualified_self_call_with_full_type_path_uses_the_type_name() {
    let source = "fn open() {\n    let _ = <std::fs::File>::open(\"input.txt\");\n}\n";
    let (_, found) = run_ruleset(source, "explicitness");
    assert_eq!(
        found,
        vec![expect(
            2,
            "ImplicitInput",
            "The function 'open' has an implicit input: it calls 'File::open'."
        )]
    );
}

#[test]
fn mutable_borrow_capability_does_not_establish_an_action() {
    let (code, found) = run_ruleset(
        "fn inspect(values: &mut Vec<i32>) -> bool { values.is_empty() }",
        "explicitness",
    );
    assert_eq!((code, found), (EXIT_SUCCESS, vec![]));
}

#[test]
fn writes_follow_borrowed_aliases_and_trait_implementations() {
    let (_, found) = run_ruleset(
        "fn change(value: &mut i32) { let alias = value; *alias = 1; }\ntrait Edit { fn change(&self, value: &mut i32); }\nstruct T;\nimpl Edit for T { fn change(&self, value: &mut i32) { *value = 1; } }\n",
        "explicitness",
    );
    assert_eq!(found, vec![
        expect(1, "ImplicitOutput", "The function 'change' has an implicit output: it writes through the borrowed parameter 'value'."),
        expect(4, "ImplicitOutput", "The method 'T::change' has an implicit output: it writes through the borrowed parameter 'value'."),
    ]);
}

#[test]
fn transitive_and_recursive_effects_propagate_to_callers() {
    let (_, found) = run_ruleset(
        "fn leaf(value: i32) { println!(\"{}\", value); }\nfn middle(value: i32) { leaf(value); }\nfn entry(value: i32) { middle(value); }\nfn first(value: i32) { second(value); }\nfn second(value: i32) { println!(\"{}\", value); first(value); }\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![
            expect(
                1,
                "ImplicitOutput",
                "The function 'leaf' has an implicit output: it uses 'println!'."
            ),
            expect(
                2,
                "ImplicitOutput",
                "The function 'middle' has an implicit output: it uses 'println!'."
            ),
            expect(
                3,
                "ImplicitOutput",
                "The function 'entry' has an implicit output: it uses 'println!'."
            ),
            expect(
                4,
                "ImplicitOutput",
                "The function 'first' has an implicit output: it uses 'println!'."
            ),
            expect(
                5,
                "ImplicitOutput",
                "The function 'second' has an implicit output: it uses 'println!'."
            ),
        ]
    );
}

#[test]
fn unknown_effects_propagate_without_fabricating_inputs_or_outputs() {
    let (_, found) = run_ruleset(
        "fn leaf(value: i32, notify: fn(i32) -> i32) -> i32 { notify(value) }\nfn wrapper(value: i32, notify: fn(i32) -> i32) -> i32 { leaf(value, notify) }\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![
            expect(
                1,
                "UnknownEffect",
                "The function 'leaf' has an unknown effect: it calls unresolved 'notify'."
            ),
            expect(
                2,
                "UnknownEffect",
                "The function 'wrapper' has an unknown effect: it calls unresolved 'notify'."
            ),
        ]
    );
}

#[test]
fn known_action_and_unknown_callback_coexist() {
    let (_, found) = run_ruleset(
        "fn send(value: i32, notify: fn(i32)) {\n    println!(\"{}\", value);\n    notify(value);\n}\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![
            expect(
                2,
                "ImplicitOutput",
                "The function 'send' has an implicit output: it uses 'println!'."
            ),
            expect(
                3,
                "UnknownEffect",
                "The function 'send' has an unknown effect: it calls unresolved 'notify'."
            ),
        ]
    );
}

#[test]
fn callback_creation_and_immediate_invocation_have_separate_owners() {
    let (_, found) = run_ruleset(
        "fn build() -> impl Fn() {\n    || println!(\"later\")\n}\nfn call_now() {\n    let action = || println!(\"now\");\n    action();\n}\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![
            expect(
                2,
                "ImplicitOutput",
                "The function '<lambda>' has an implicit output: it uses 'println!'."
            ),
            expect(
                5,
                "ImplicitOutput",
                "The function '<lambda>' has an implicit output: it uses 'println!'."
            ),
            expect(
                6,
                "ImplicitOutput",
                "The function 'call_now' has an implicit output: it uses 'println!'."
            ),
        ]
    );
}

#[test]
fn pure_fixed_capture_callback_factory_is_clean() {
    let (code, found) = run_ruleset(
        "fn make_adder(start: i32) -> impl Fn(i32) -> i32 {\n    move |value| start + value\n}\n",
        "explicitness",
    );
    assert_eq!((code, found), (EXIT_SUCCESS, vec![]));
}

#[test]
fn top_level_private_mutation_is_clean_but_shallow_shared_elements_are_not() {
    let (_, found) = run_ruleset(
        "use std::cell::RefCell;\nuse std::rc::Rc;\nfn private() {\n    let mut values = Vec::new();\n    values.push(1);\n}\nfn shared(values: &[Rc<RefCell<i32>>]) {\n    let next = values.to_vec();\n    *next[0].borrow_mut() = 1;\n}\n",
        "explicitness",
    );
    assert_eq!(found, vec![
        expect(9, "ImplicitOutput", "The function 'shared' has an implicit output: it writes through the borrowed parameter 'values'."),
    ]);
}

#[test]
fn aliases_rebinding_and_conditional_copies_keep_operation_site_storage() {
    let (_, found) = run_ruleset(
        "use std::cell::RefCell;\nuse std::rc::Rc;\nfn rebind(values: &mut Vec<i32>) {\n    let alias = values;\n    alias.push(1);\n    let mut values = Vec::new();\n    values.push(2);\n}\nfn conditional(values: &[Rc<RefCell<i32>>], flag: bool) {\n    let mut next = values.to_vec();\n    if flag {\n        next = Vec::new();\n    }\n    *next[0].borrow_mut() = 1;\n}\n",
        "explicitness",
    );
    assert_eq!(found, vec![
        expect(5, "ImplicitOutput", "The function 'rebind' has an implicit output: it writes through the borrowed parameter 'values'."),
        expect(14, "ImplicitOutput", "The function 'conditional' has an implicit output: it writes through the borrowed parameter 'values'."),
    ]);
}

#[test]
fn literal_false_branches_do_not_execute_known_or_unknown_effects() {
    let (_, found) = run_ruleset(
        "fn dead_print(value: i32) {\n    if false {\n        println!(\"{}\", value);\n    }\n}\nfn live_print(value: i32) {\n    if true {\n        println!(\"{}\", value);\n    }\n}\nfn dead_callback(value: i32, notify: fn(i32)) {\n    if false {\n        notify(value);\n    }\n}\nfn live_callback(value: i32, notify: fn(i32)) {\n    if true {\n        notify(value);\n    }\n}\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![
            expect(
                8,
                "ImplicitOutput",
                "The function 'live_print' has an implicit output: it uses 'println!'."
            ),
            expect(
                18,
                "UnknownEffect",
                "The function 'live_callback' has an unknown effect: it calls unresolved 'notify'."
            ),
        ]
    );
}

#[test]
fn literal_false_branches_do_not_join_dead_storage_aliases() {
    let (_, found) = run_ruleset(
        "use std::cell::RefCell;\nuse std::rc::Rc;\nfn dead_storage(values: &[Rc<RefCell<i32>>]) {\n    let mut next: Vec<Rc<RefCell<i32>>> = Vec::new();\n    if false {\n        next = values.to_vec();\n    }\n    *next[0].borrow_mut() = 1;\n}\nfn live_storage(values: &[Rc<RefCell<i32>>]) {\n    let mut next: Vec<Rc<RefCell<i32>>> = Vec::new();\n    if true {\n        next = values.to_vec();\n    }\n    *next[0].borrow_mut() = 1;\n}\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![expect(
            15,
            "ImplicitOutput",
            "The function 'live_storage' has an implicit output: it writes through the borrowed parameter 'values'."
        ),]
    );
}

#[test]
fn lexical_shadowing_blocks_global_callable_resolution() {
    let (_, found) = run_ruleset(
        "fn helper(value: i32) -> i32 { value + 1 }\nfn send(value: i32, helper: fn(i32) -> i32) -> i32 { helper(value) }\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![expect(
            2,
            "UnknownEffect",
            "The function 'send' has an unknown effect: it calls unresolved 'helper'."
        ),]
    );
}

#[test]
fn shadowed_standard_macro_is_unknown_not_the_standard_effect() {
    let (_, found) = run_ruleset(
        "macro_rules! println { ($value:expr) => { $value }; }\nfn send(value: i32) { let _ = println!(value); }\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![expect(
            2,
            "UnknownEffect",
            "The function 'send' has an unknown effect: it uses local macro 'println!'."
        )]
    );
}

#[test]
fn custom_method_names_do_not_certify_purity() {
    let (_, found) = run_ruleset(
        "struct Worker;\nimpl Worker {\n    fn len(&self) {\n        println!(\"work\");\n    }\n}\nfn call(worker: Worker) {\n    worker.len();\n}\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![
            expect(
                4,
                "ImplicitOutput",
                "The method 'Worker::len' has an implicit output: it uses 'println!'."
            ),
            expect(
                8,
                "ImplicitOutput",
                "The function 'call' has an implicit output: it uses 'println!'."
            ),
        ]
    );
}

#[test]
fn helper_mutation_is_private_after_fresh_argument_substitution() {
    let (_, found) = run_ruleset(
        "fn leaf(values: &mut Vec<i32>) {\n    values.push(1);\n}\nfn wrapper() -> Vec<i32> {\n    let mut next = Vec::new();\n    leaf(&mut next);\n    next\n}\n",
        "explicitness",
    );
    assert_eq!(found, vec![
        expect(2, "ImplicitOutput", "The function 'leaf' has an implicit output: it writes through the borrowed parameter 'values'."),
    ]);
}

#[test]
fn returned_argument_alias_mutation_changes_caller_storage() {
    let (_, found) = run_ruleset(
        "fn alias(values: &mut Vec<i32>) -> &mut Vec<i32> {\n    values\n}\nfn change(values: &mut Vec<i32>) {\n    alias(values).push(1);\n}\n",
        "explicitness",
    );
    assert_eq!(found, vec![
        expect(5, "ImplicitOutput", "The function 'change' has an implicit output: it writes through the borrowed parameter 'values'."),
    ]);
}

#[test]
fn callback_json_metadata_uses_the_execution_owner() {
    let (code, json) = run_json(
        "fn build() -> impl Fn() {\n    || println!(\"later\")\n}\n",
        "explicitness",
    );
    assert_eq!(code, EXIT_VIOLATION);
    let violation = &json["files"][0]["violations"][0];
    assert_eq!(violation["function"], "<lambda>");
    assert_eq!(violation["class"], "");
    assert_eq!(violation["method"], "");
    assert_eq!(violation["beginLine"], 2);
    assert_eq!(violation["rule"], "ImplicitOutput");
}

#[test]
fn imported_environment_reads_keep_the_standard_effect() {
    let (_, found) = run_ruleset(
        "use std::env::var_os as setting;\nfn read() -> bool {\n    setting(\"VALUE\").is_some()\n}\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![expect(
            3,
            "ImplicitInput",
            "The function 'read' has an implicit input: it calls 'env::var_os'.",
        )]
    );
}

#[test]
fn mutable_self_writes_are_borrowed_parameter_outputs() {
    let (_, found) = run_ruleset(
        "struct Counter { value: i32 }\nimpl Counter {\n    fn add(&mut self) {\n        self.value += 1;\n    }\n    fn read(&self) -> i32 { self.value }\n}\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![expect(
            4,
            "ImplicitOutput",
            "The method 'Counter::add' has an implicit output: it changes the state of 'self'.",
        )]
    );
}

#[test]
fn futures_execute_on_await_and_keep_the_force_line() {
    let (_, found) = run_ruleset(
        "async fn fetch() {\n    println!(\"later\");\n}\nfn start() {\n    let _pending = fetch();\n}\nasync fn drive() {\n    let pending = fetch();\n    pending.await;\n}\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![
            expect(
                2,
                "ImplicitOutput",
                "The function 'fetch' has an implicit output: it uses 'println!'."
            ),
            expect(
                9,
                "ImplicitOutput",
                "The function 'drive' has an implicit output: it uses 'println!'."
            ),
        ]
    );
}

#[test]
fn iterator_callbacks_execute_only_when_consumed() {
    let (_, found) = run_ruleset(
        "fn build(values: &[i32]) -> impl Iterator<Item = i32> + '_ {\n    values.iter().map(|value| {\n        println!(\"{}\", value);\n        *value + 1\n    })\n}\nfn consume(values: &[i32]) -> Vec<i32> {\n    let later = build(values);\n    later.collect()\n}\n",
        "explicitness",
    );
    assert_eq!(
        found,
        vec![
            expect(
                3,
                "ImplicitOutput",
                "The function '<lambda>' has an implicit output: it uses 'println!'."
            ),
            expect(
                9,
                "ImplicitOutput",
                "The function 'consume' has an implicit output: it uses 'println!'."
            ),
        ]
    );
}

#[test]
fn captured_private_mutable_storage_belongs_to_the_closure() {
    let (_, found) = run_ruleset(
        "use std::cell::RefCell;\nfn make() -> impl Fn(i32) -> usize {\n    let fresh = RefCell::new(Vec::new());\n    move |value| {\n        let seen = fresh.borrow().len();\n        fresh.borrow_mut().push(value);\n        seen\n    }\n}\n",
        "explicitness",
    );
    assert_eq!(found, vec![
        expect(5, "ImplicitInput", "The function '<lambda>' has an implicit input: it reads captured storage 'fresh'."),
        expect(6, "ImplicitOutput", "The function '<lambda>' has an implicit output: it changes captured storage 'fresh'."),
    ]);
}

#[test]
fn renamed_callbacks_keep_private_arguments_at_the_forwarder_boundary() {
    let (_, found) = run_ruleset(
        "fn apply(value: &mut Vec<i32>, f: fn(&mut Vec<i32>)) { f(value); }\nfn forward(_value: &mut Vec<i32>, f: fn(&mut Vec<i32>)) { let mut fresh = Vec::new(); apply(&mut fresh, f); }\nfn mutator(payload: &mut Vec<i32>) { payload.push(1); }\nfn wrapper(items: &mut Vec<i32>) { forward(items, mutator); }\n",
        "explicitness",
    );
    assert_eq!(found, vec![
        expect(1, "UnknownEffect", "The function 'apply' has an unknown effect: it calls unresolved 'f'."),
        expect(2, "UnknownEffect", "The function 'forward' has an unknown effect: it calls unresolved 'f'."),
        expect(3, "ImplicitOutput", "The function 'mutator' has an implicit output: it writes through the borrowed parameter 'payload'."),
    ]);
}

#[test]
fn conditional_pattern_aliases_retain_the_parameter_owner() {
    let (_, found) = run_ruleset(
        "fn conditional(values: Option<&mut Vec<i32>>) { if let Some(target) = values { target.push(1); } }\nfn matched(values: Option<&mut Vec<i32>>) { match values { Some(target) => target.push(1), None => {} } }\nfn joined(values: &mut Vec<i32>, flag: bool) { let mut local = Vec::new(); let mut target = &mut local; if flag { target = values; } target.push(1); }\n",
        "explicitness",
    );
    assert_eq!(found, vec![
        expect(1, "ImplicitOutput", "The function 'conditional' has an implicit output: it writes through the borrowed parameter 'values'."),
        expect(2, "ImplicitOutput", "The function 'matched' has an implicit output: it writes through the borrowed parameter 'values'."),
        expect(3, "ImplicitOutput", "The function 'joined' has an implicit output: it writes through the borrowed parameter 'values'."),
    ]);
}

#[test]
fn finite_callable_alternatives_keep_pure_and_unknown_branches() {
    let (code, found) = run_ruleset(
        "fn first(value: i32) -> i32 { value + 1 }\nfn second(value: i32) -> i32 { value + 2 }\nfn choose(value: i32, flag: bool) -> i32 { let selected = if flag { first } else { second }; selected(value) }\n",
        "explicitness",
    );
    assert_eq!(code, EXIT_SUCCESS, "found={found:?}");
    assert_eq!(found, vec![]);
    let (_, found) = run_ruleset(
        "fn first(value: i32) -> i32 { value + 1 }\nfn choose(value: i32, flag: bool) -> i32 { let selected = if flag { first } else { outside }; selected(value) }\n",
        "explicitness",
    );
    assert_eq!(found, vec![expect(2, "UnknownEffect", "The function 'choose' has an unknown effect: it invokes an unresolved callable alternative.")]);
}

#[test]
fn external_names_do_not_inherit_standard_effects() {
    let (_, found) = run_ruleset(
        "use replacement::env::var_os;\nfn read() { var_os(\"VALUE\"); }\n",
        "explicitness",
    );
    assert_eq!(found, vec![expect(2, "UnknownEffect", "The function 'read' has an unknown effect: it calls unresolved 'replacement::env::var_os'.")]);
}

#[test]
fn qualified_calls_preserve_standard_library_identity() {
    let (_, found) = run_ruleset(
        "use std::fs::File as InputFile; use std::vec::Vec as Values;\nfn opaque(path: &str) { let _ = <replacement::fs::File>::open(path); }\nfn trusted(path: &str) { let _ = <std::fs::File>::open(path); }\nfn renamed(path: &str) { let _ = <InputFile>::open(path); }\nfn plain() -> Values<i32> { <Values<i32>>::new() }\n",
        "explicitness",
    );
    assert_eq!(found, vec![
        expect(2, "UnknownEffect", "The function 'opaque' has an unknown effect: it calls unresolved 'replacement::fs::File::open'."),
        expect(3, "ImplicitInput", "The function 'trusted' has an implicit input: it calls 'File::open'."),
        expect(4, "ImplicitInput", "The function 'renamed' has an implicit input: it calls 'File::open'."),
    ]);
}

#[test]
fn owned_custom_destructors_keep_uncertainty_at_scope_exit() {
    let (_, found) = run_ruleset(
        "struct Guard;\nimpl Drop for Guard { fn drop(&mut self) { println!(\"drop\"); } }\nfn consume(_guard: Guard) {\n}\nfn borrow(_guard: &Guard) {}\nstruct Plain;\nfn plain(_value: Plain) {}\n",
        "explicitness",
    );
    assert_eq!(found, vec![
        expect(2, "ImplicitOutput", "The method 'Guard::drop' has an implicit output: it uses 'println!'."),
        expect(4, "UnknownEffect", "The function 'consume' has an unknown effect: it can run an unresolved destructor."),
    ]);
}
