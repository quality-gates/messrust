# Exploratory testing report: messrust

Date: 2026-10-03

Checkout: `461a2011f21ad1f0eeaaeccdd72bf012b7487650` on `main`.

Binary: messrust 0.1.16.

Environment: `messrust-dev`, Rust 1.85.1, Docker capped at 2 CPUs and 2 GB
RAM. [run.sh](run.sh) runs the built binary in the container, with this
directory mounted at `/et`. The worktree had untracked `.serena/` and
`docs/exploratory-testing/2026-09-12-messrust/` directories. Those directories
were not changed.

Evidence: [evidence](evidence), [fixtures](fixtures), and [rulesets](rulesets).

## User journeys

### 1. Run code-quality checks, apply filters, and generate machine reports

Goal: scan source files, apply command-line options and filters, and generate
reports in various machine formats for CI.

The normal run on [service.rs](fixtures/service.rs) with the `rust` ruleset
returned exit code `2` and found 6 violations: `UnusedPrivateField` (2),
`DevelopmentCodeFragment`, `UnusedLocalVariable` (2), and `ExcessiveParameterList`
([j1-default-service.txt](evidence/j1-default-service.txt)). Running with
`rust,opinionated` surfaced 22 findings, including `ShortVariable`,
`BooleanArgumentFlag`, `ElseExpression`, and `StaticAccess`
([j1-opinionated-service.txt](evidence/j1-opinionated-service.txt)).

Variations:

- `--reportfile` wrote report files and kept stdout empty for `json`, `xml`,
  `sarif`, `gitlab`, `checkstyle`, `html`, `github`, and `ansi` formats
  ([evidence](evidence)).
- `--ignore-violations-on-exit` returned exit code `0` while keeping all findings
  in the generated report file
  ([j1-ignore-violations.json](evidence/j1-ignore-violations.json)).
- Priority filters `--minimumpriority 2` and `--maximumpriority 3` selected the
  expected priority subsets ([j1-filters.txt](evidence/j1-filters.txt)).
- In-source suppression with `// messrust-disable-next-line` and
  `// messrust-disable` / `// messrust-enable` hid findings by default
  ([j1-suppress-default.txt](evidence/j1-suppress-default.txt)).
  `--strict` showed the suppressed findings marked `[suppressed]`
  ([j1-suppress-strict.txt](evidence/j1-suppress-strict.txt)).

### 2. Compose XML rulesets, override thresholds, and manage ruleset inheritance

Goal: write custom XML policies to configure rule thresholds and exclusions, and
combine them with built-in rulesets.

[custom-params.xml](rulesets/custom-params.xml) configured `minimum="4"` for
`ExcessiveParameterList`. Running on [params_test.rs](fixtures/params_test.rs)
correctly flagged a 5-parameter function and ignored a 3-parameter function
([j2-custom-params.txt](evidence/j2-custom-params.txt)).

Variations:

- `custom-params.xml,codesize` resolved the later built-in ruleset to restore
  the default threshold of 10, exiting `0` without findings
  ([j2-override-builtin-wins.txt](evidence/j2-override-builtin-wins.txt)).
- `codesize,custom-params.xml` resolved the later custom ruleset to override the
  threshold to 4, exiting `2` with a finding
  ([j2-override-custom-wins.txt](evidence/j2-override-custom-wins.txt)).
- [exclude-rule.xml](rulesets/exclude-rule.xml) used `<exclude name="..."/>` to
  turn off `DevelopmentCodeFragment` while keeping the rest of `rust`
  ([j2-exclude-rule.txt](evidence/j2-exclude-rule.txt)).
- [invalid-rule.xml](rulesets/invalid-rule.xml) with an unresolvable rule name
  exited `1` with diagnostic message `error: no rules were loaded from the
  specified rulesets` ([j2-invalid-rule.txt](evidence/j2-invalid-rule.txt)).

### 3. Modern Rust syntax, static analysis edge cases, and raw identifiers

Goal: analyze idiomatic Rust syntax forms (such as raw identifiers `r#`, qualified
receiver paths `<Type>::method()`, `let-else`, slice patterns, and raw address
operations `&raw mut`) and verify rule precision.

The following confirmed regressions were identified and replayed twice:

- [#216: GlobalVariable misses static mut writes using the raw address operator (*&raw mut STATIC = ...)](https://github.com/quality-gates/messrust/issues/216).
  Mutating a `static mut` item using `*&raw mut STATIC = val` or
  `*(&raw mut STATIC) = val` is missed, returning exit code `0`
  ([bug1-replay1.txt](evidence/bug1-replay1.txt), [bug1-replay2.txt](evidence/bug1-replay2.txt)).
  A control with direct assignment returns exit code `2`
  ([bug1-control.txt](evidence/bug1-control.txt)).
- [#217: StaticAccess misses static method calls on raw identifier types (r#Type::method() and <r#Type>::method())](https://github.com/quality-gates/messrust/issues/217).
  Associated function calls on types named with raw identifiers (e.g. `r#Service::run()`
  or `<r#Helper>::run()`) are missed, returning exit code `0`
  ([bug2-replay1.txt](evidence/bug2-replay1.txt), [bug2-replay2.txt](evidence/bug2-replay2.txt)).
  A control with standard PascalCase types returns exit code `2`
  ([bug2-control.txt](evidence/bug2-control.txt)).
- [#218: ImplicitInput and ImplicitOutput miss qualified associated function calls (<Type>::method())](https://github.com/quality-gates/messrust/issues/218).
  Calling standard I/O and process functions via qualified syntax
  (such as `<File>::open(...)`, `<Instant>::now()`, `<File>::create(...)`, or
  `<Command>::new(...)`) is missed, returning exit code `0`
  ([bug3-replay1.txt](evidence/bug3-replay1.txt), [bug3-replay2.txt](evidence/bug3-replay2.txt)).
  A control with unqualified calls returns exit code `2`
  ([bug3-control.txt](evidence/bug3-control.txt)).

## Confirmed bugs

All three bugs were replayed 2/2 from known fixtures:

### Bug 1: #216 GlobalVariable misses static mut writes through raw address operator

User impact: in Rust 1.82+ and Rust 2024 edition, direct reference or assignment
to `static mut` is discouraged or rejected by the compiler. Idiomatic code writes
through raw pointers using the raw address operator `*&raw mut STATIC = val`.
Because `collect_mutated_static_place` in `src/analyze/model/build.rs` does not
handle `syn::Expr::RawAddr`, all mutations using `&raw mut` are ignored, and
`GlobalVariable` fails to report mutable global state.

Replay: `./run.sh fixtures/bug1_raw_addr_static.rs text design --only GlobalVariable`.

Expected: findings on lines 5 and 11, exit code `2`.
Actual: exit code `0`, no findings.

Cause: `collect_mutated_static_place` unwraps `syn::Expr::Unary` dereferences,
but does not match `syn::Expr::RawAddr`. The unwrapping terminates without
resolving the static identifier.

### Bug 2: #217 StaticAccess misses static method calls on raw identifier types

User impact: types whose names collide with keywords use raw identifiers (e.g.
`pub struct r#Service;` or `pub struct r#Type;`). When external code invokes
associated functions on these types (`r#Service::run()` or `<r#Helper>::run()`),
`StaticAccess` fails to flag the dependency.

Replay: `./run.sh fixtures/bug2_raw_static_access.rs text cleancode --only StaticAccess`.

Expected: findings on lines 5 and 9, exit code `2`.
Actual: exit code `0`, no findings.

Cause: `static_receiver_type` and `static_call_receiver` in
`src/analyze/rules/cleancode.rs` check `name.chars().next().is_some_and(|c| c.is_ascii_uppercase())`
without stripping the `r#` prefix. For raw identifiers, the first character is
`'r'`, so the type receiver is dropped.

### Bug 3: #218 ImplicitInput and ImplicitOutput miss qualified associated function calls

User impact: developers or macros using qualified type call syntax
(`<File>::open(...)`, `<Instant>::now()`, `<File>::create(...)`, `<Command>::new(...)`)
bypass explicitness tracking. Hidden side effects and inputs are not reported.

Replay: `./run.sh fixtures/bug3_qualified_explicitness.rs text explicitness`.

Expected: 2 implicit input findings and 2 implicit output findings, exit code `2`.
Actual: exit code `0`, no findings.

Cause: in `src/analyze/rules/explicitness.rs`, `visit_expr_call` passes `&path.path`
to `note_call`, which inspects `last_two_segments`. Qualified calls `<Type>::method()`
store `Type` in `path.qself`, leaving only `method` in `path.path`. `last_two_segments`
produces `"open"` or `"create"` instead of `"File::open"` or `"File::create"`.

## Candidate classification

Confirmed: #216, #217, and #218. Each failure repeated 2/2 and violated a
grounded rule expectation.

Rejected, documented behavior:

- `// messrust-disable-next-line` placed above a function signature does not
  suppress findings inside the function body. `docs/usage.md` specifies that
  this suppression applies only to the immediate physical line.
- `println!("{r#type}")` is rejected by `rustc` (`error: invalid format string:
  raw identifiers are not supported`). Modern Rust format strings use
  `println!("{type}")`, which `messrust` already supports as a read.

Unresolved: none from the journeys above.

## Usability observations

- Observation: in `src/analyze/rules/naming.rs`, `ConstantNamingConventions`
  includes the `r#` prefix in the message (`Constant r#bad should be defined in
  SCREAMING_SNAKE_CASE`), whereas other naming rules strip `r#` in violation
  messages. Stripping `r#` in constant messages would maintain consistency.
- Observation: in `src/analyze/model/use_def.rs`, `is_format_macro` supports
  standard library format macros, but omitting popular logging and tracing
  macros (`tracing::info!`, `log::info!`) means format captures in logging calls
  can cause false positive `UnusedFormalParameter` findings if parameters are
  only logged.

## Issue filing

All three confirmed bugs were filed in GitHub Issues with labels `bug` and
`needs-triage`:

- [#216](https://github.com/quality-gates/messrust/issues/216): `[bug] GlobalVariable misses static mut writes using the raw address operator (*&raw mut STATIC = ...)`
- [#217](https://github.com/quality-gates/messrust/issues/217): `[bug] StaticAccess misses static method calls on raw identifier types (r#Type::method() and <r#Type>::method())`
- [#218](https://github.com/quality-gates/messrust/issues/218): `[bug] ImplicitInput and ImplicitOutput miss qualified associated function calls (<Type>::method())`

## Limitations and cleanup

Analysis was performed in the `messrust-dev` Docker container capped at 2 CPUs
and 2 GB RAM. The test binary was built from checkout `461a201`.

Docker volumes `messrust-et-target-20261003` and `messrust-et-cargo-20261003`
will be removed during post-pass cleanup. The report, [run.sh](run.sh),
fixtures, rulesets, and evidence are preserved in this directory.
