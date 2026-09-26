# Exploratory testing report: messrust

Date: 2026-09-26

Checkout: `85d607b5c7eb4b55f216116f4dae7198177f01e5` on `main`.

Binary: messrust 0.1.13.

Environment: `messrust-dev`, Rust 1.85.1, Docker capped at 2 CPUs and 2 GB
RAM. [run.sh](run.sh) runs the built binary in the container, with this
directory mounted at `/et`. The worktree had an untracked `.serena/` directory.
That directory was not changed.

Evidence: [evidence](evidence), [fixtures](fixtures), and [rulesets](rulesets).

## User journeys

### 1. Run code-quality checks, apply filters, and generate machine reports

Goal: scan source files, apply command-line options and filters, and generate
reports in various machine formats for CI.

The normal run on [service.rs](fixtures/service.rs) with the `rust` ruleset
returned exit code `2` and found 4 violations: `LackOfCohesionOfMethods`,
`DevelopmentCodeFragment`, `ExcessiveParameterList`, and `UnusedLocalVariable`
([j1-default-service.txt](evidence/j1-default-service.txt)). Running with
`rust,opinionated` surfaced 16 findings, including `BooleanArgumentFlag`,
`ElseExpression`, `StaticAccess`, and `ShortVariable`
([j1-opinionated-service.txt](evidence/j1-opinionated-service.txt)).

Variations:

- `--reportfile` wrote report files and kept stdout empty for `json`, `sarif`,
  `gitlab`, `checkstyle`, `html`, `xml`, `ansi`, and `github` formats
  ([evidence](evidence)).
- `--ignore-violations-on-exit` returned exit code `0` while keeping all findings
  in the generated report file
  ([j1-ignore-violations.json](evidence/j1-ignore-violations.json)).
- Priority filters `--minimumpriority 2` and `--maximumpriority 3` selected the
  expected priority subsets ([j1-filters.txt](evidence/j1-filters.txt)).
- `--only ExcessiveParameterList` isolated that rule.
- `--disable LackOfCohesionOfMethods,ExcessiveParameterList` removed those rules.
- `--exclude service` excluded matching files.
- In-source suppression with `// messrust-disable-next-line` and
  `// messrust-disable` / `// messrust-enable` hid findings by default.
  `--strict` showed the suppressed findings marked `[suppressed]`
  ([j1-suppress.txt](evidence/j1-suppress.txt)).

### 2. Compose XML rulesets, override thresholds, and manage ruleset inheritance

Goal: write custom XML policies to configure rule thresholds and exclusions, and
combine them with built-in rulesets.

[custom-params.xml](rulesets/custom-params.xml) configured `minimum="4"` for
`ExcessiveParameterList`. Running on [params_test.rs](fixtures/params_test.rs)
correctly flagged a 5-parameter function and ignored a 3-parameter function
([j2-composition.txt](evidence/j2-composition.txt)).

Variations:

- `custom-params.xml,codesize` resolved the later built-in ruleset to restore
  the default threshold of 10, exiting `0` without findings.
- `codesize,custom-params.xml` resolved the later custom ruleset to override the
  threshold to 4, exiting `2` with a finding.
- [exclude-rule.xml](rulesets/exclude-rule.xml) used `<exclude name="..."/>` to
  turn off `DevelopmentCodeFragment` while keeping the rest of `rust`.
- [invalid-rule.xml](rulesets/invalid-rule.xml) with an unresolvable rule name
  exited `1` with diagnostic message `error: no rules were loaded from the
  specified rulesets`. This journey passed.

### 3. Modern Rust syntax, abstract trait signatures, and static state tracking

Goal: analyze idiomatic Rust syntax forms (such as `use` imports, abstract trait
declarations, and naming conventions) and verify rule precision.

The following confirmed regressions were identified and replayed twice:

- [#188: GlobalVariable misses mutations to static mut items accessed via use imports](https://github.com/quality-gates/messrust/issues/188).
  Mutating a `static mut` item across module boundaries via `use` imports
  in the same file is missed, returning exit code `0`
  ([bug1-replay.txt](evidence/bug1-replay.txt)). A control with qualified paths
  returns exit code `2`.
- [#189: UnusedFormalParameter emits false positives for abstract trait method parameters](https://github.com/quality-gates/messrust/issues/189).
  In trait definitions without default method bodies, parameter names are
  falsely reported as unused, returning exit code `2`
  ([bug2-replay.txt](evidence/bug2-replay.txt)).
- [#190: BooleanGetMethodName falsely flags non-getter methods whose names start with get](https://github.com/quality-gates/messrust/issues/190).
  Methods named with English words starting with `get` (such as `getting_started`
  or `getter`) returning `bool` are falsely reported as getters
  ([bug3-replay.txt](evidence/bug3-replay.txt)).

## Confirmed bugs

All three bugs were replayed 2/2 from known fixtures:

### Bug 1: #188 GlobalVariable misses mutations through use imports

User impact: `static mut` mutations imported into a submodule or into root are
dropped. The gate fails to warn about mutable global state in idiomatic code.

Replay: `./run.sh fixtures/bug1_global_variable_use.rs text design --only GlobalVariable`.

Expected: findings on lines 4 and 17, exit code `2`.
Actual: exit code `0`, no findings.

Cause: `StaticMutCollector` in `src/analyze/model/build.rs` does not track `use`
imports. Local path resolution prefixes the local module scope, failing to match
the declared static's key.

### Bug 2: #189 UnusedFormalParameter emits false positives for abstract trait method parameters

User impact: abstract trait declarations (such as `fn save(&self, user: &User)`)
falsely report their parameters as unused code. Developers must either add
redundant underscores to parameter names or disable the rule.

Replay: `./run.sh fixtures/bug2_trait_param.rs text unusedcode --only UnusedFormalParameter`.

Expected: exit code `0`, no findings for abstract trait method declarations.
Actual: exit code `2`, `id`, `user`, and `ctx` are reported as unused.

Cause: in `src/analyze/model/use_def.rs`, `visit_trait_item_fn` calls
`record_params_from_sig` even when `node.default` is `None`.

### Bug 3: #190 BooleanGetMethodName falsely flags non-getter methods starting with get

User impact: ordinary methods like `getting_started()` or `getter()` that return
`bool` are incorrectly flagged as boolean getter naming violations.

Replay: `./run.sh fixtures/bug3_boolean_get_prefix.rs text naming --only BooleanGetMethodName`.

Expected: only `get_ready()` and `get_status()` are reported.
Actual: `getting_started()`, `getter()`, and `gets_updated()` are also reported.

Cause: `is_getter_name` in `src/analyze/helpers.rs` checks only for the `get`
character prefix without checking for an underscore `_` or PascalCase boundary.

## Candidate classification

Confirmed: #188, #189, and #190. Each failure repeated 2/2 and violated a
grounded rule expectation.

Rejected, documented behavior:

- `// messrust-disable-next-line` placed above a function signature does not
  suppress findings inside the function body. `docs/usage.md` specifies that
  this suppression applies only to the immediate physical line.
- `eprint!` is not detected by `DevelopmentCodeFragment`. Both `docs/design.md`
  and `rulesets/design.xml` explicitly document the default list as `println`,
  `print`, `eprintln`, and `dbg`.

Unresolved: none from the journeys above.

## Usability observations

- Observation: `eprint!` is omitted from `DevelopmentCodeFragment` while
  `print!` and `eprintln!` are included. Adding `eprint` to the default list
  would provide consistent coverage for stderr prints.
- Observation: `--reportfile` fails when parent directories do not exist.
  Creating parent directories automatically would improve CI usability.
- Observation: `BooleanGetMethodName` flags bare `get()` and suggests
  renaming to `is_...()` or `has_...()`.

## Issue filing

All three confirmed bugs were filed in GitHub Issues with labels `bug` and
`needs-triage`:

- [#188](https://github.com/quality-gates/messrust/issues/188): `[bug] GlobalVariable misses mutations to static mut items accessed via use imports`
- [#189](https://github.com/quality-gates/messrust/issues/189): `[bug] UnusedFormalParameter emits false positives for abstract trait method parameters`
- [#190](https://github.com/quality-gates/messrust/issues/190): `[bug] BooleanGetMethodName falsely flags non-getter methods whose names start with get (e.g. getting_started, getter)`

## Limitations and cleanup

Analysis was performed in the `messrust-dev` Docker container capped at 2 CPUs
and 2 GB RAM. The test binary was built from checkout `85d607b`.

Docker volumes `messrust-et-target-20260926` and `messrust-et-cargo-20260926`
will be removed during post-pass cleanup. The report, [run.sh](run.sh),
fixtures, rulesets, and evidence are preserved in this directory.
