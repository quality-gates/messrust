# Exploratory testing report: explicitness ruleset

Date: 2026-09-23

Scope: the uncommitted `explicitness` diff on `feature/explicitness-ruleset`,
compared with `origin/main`.

Checkout: `9392b6cc1ae972c7a6b75dad250adb75e15497aa` plus the uncommitted
diff. The `origin/main` control build is `2a5d1a9da57f7b30648992e354883981c62a38e2`.

Binary: messrust 0.1.11.

Environment: `messrust-dev`, Rust 1.85.1, Docker capped at 2 CPUs and 2 GB
RAM. [run.sh](run.sh) runs the built binary read-only in the container, with
this directory mounted at `/et`. The worktree also had an untracked `.serena/`
directory. That directory was not changed.

Evidence: [evidence](evidence), [fixtures](fixtures), and [rulesets](rulesets).

## User journeys

### 1. Find the implicit inputs and outputs of ordinary code

Goal: run `messrust <path> text explicitness` on a realistic service module.
Each function that has an implicit input or output gets one finding at the
site. A pure function gets no finding. The expected result for each function is
in a comment in [service.rs](fixtures/app/service.rs).

Result before the fixes: 7 of 9 expected findings were correct, in
[j1-default-text.txt](evidence/j1-default-text.txt). The two missing findings
are C1 and C2 below. The JSON report
([j1-default.json](evidence/j1-default.json)) has the same findings.

Variations:

- `--ignore-tests` removes findings in `#[cfg(test)]` code
  ([tests_mod.rs](fixtures/tests_mod.rs)).
- `// messrust-disable-line` and region suppressions hide findings. `--strict`
  shows them as `[suppressed]` ([j1-suppress.txt](evidence/j1-suppress.txt)).

### 2. Use the strict method mode and compose rulesets

Goal: set `include-self=true` to report reads and changes of `self`, and use
`explicitness` with other rulesets and filters.

[strict.xml](rulesets/strict.xml) on [strict_methods.rs](fixtures/strict_methods.rs)
gave the expected findings. A trait impl kept only its body effects. A
constructor gave no finding ([j2-strict.txt](evidence/j2-strict.txt)).

Variations:

- `include-self` values `TRUE`, `1`, and `yes` turn the mode on. `on` and
  `bogus` leave it off. This is the existing shared `property_bool` behaviour
  ([j2-values.txt](evidence/j2-values.txt)).
- These forms all work: the bare `ImplicitInput` reference,
  `rulesets/explicitness.xml/ImplicitOutput`, `explicitness.xml`,
  `<exclude>`, `--only`, and `--disable`. `rust` alone gives no `Implicit*`
  finding ([j2-composition.txt](evidence/j2-composition.txt)).

### 3. Edge cases

Goal: check the syntax forms that users will write
([edges.rs](fixtures/edges.rs), [j3-edges.txt](evidence/j3-edges.txt)). These
forms were correct: a `tokio::` path, `stdin().read_line(&mut local)`,
`log::info!(target: ..)`, a function in a nested module, a method in a generic
impl, `File::open(..).read_to_string(..)`, `stdout().flush()`, and local-only
mutation. `DEPTH.with(|d| d.set(..))` was a third instance of C2.

## Confirmed bugs

Both bugs are in the unpushed branch code. Both are fixed in the same change.
Each failure repeated 2/2 from the same fixture
([c1-c2-replay.txt](evidence/c1-c2-replay.txt)).

### C1: a nested interior-mutable static is not a shared static

User impact: `static AUDIT: LazyLock<Mutex<Vec<String>>>` is a common global.
The tool gave no finding for reads or writes of it. `docs/explicitness.md`
says that "a `static` whose type contains ... `Mutex`" is shared.

Replay: `./run.sh fixtures/c1_lazylock_mutex.rs text explicitness`.

Expected: `ImplicitOutput ... writes the global 'AUDIT'` at line 5.
Actual: no finding for `audit`. The control `static PLAIN: Mutex<..>` in the
same file is reported at line 8.

Minimal repro ([phase2-minimise.txt](evidence/phase2-minimise.txt)):
`static A: Option<Mutex<u32>> = x(); fn f() { A.lock(); }`. `LazyLock` is not
necessary. `Mutex<u32>`, `std::sync::Mutex<u32>`, and `[Mutex<u32>; 2]` are
reported correctly.

Cause (hypothesis C1-H1, confirmed by a probe in
[phase4-probes.txt](evidence/phase4-probes.txt)): `SharedStaticCollector` used
`type_names_in`. Its `TypeNameCollector::visit_type_path` records the last
segment and does not visit generic arguments. `Option<Mutex<u32>>` gave
`["Option"]`. An array type is visited by default, so `[Mutex<u32>; 2]` gave
`["Mutex"]`.

Fix: `has_interior_mutable_type` in `src/analyze/model/build.rs` visits
generic arguments. The shared `TypeNameCollector` is not changed, because
other rules use it.

Regression test: `write_through_lazy_lock_mutex_static_is_an_implicit_output`
in `tests/explicitness.rs`.

### C2: a write through a thread-local `with` closure is not an output

User impact: `KEY.with(|v| v.borrow_mut().push(x))` and `KEY.with(|v| v.set(..))`
are the usual way to change a thread-local value. The tool reported only a
read. The rule description says it reports "writes to ... thread-local keys".

Replay: `./run.sh fixtures/c2_thread_local_with.rs text explicitness`.

Expected: `ImplicitOutput ... writes the global 'SCRATCH'` at line 6.
Actual: `ImplicitInput ... reads the global 'SCRATCH'` at line 6. The control
`SCRATCH.with_borrow_mut(..)` is reported as an output at line 9.

Minimal repro: `thread_local! { static K: Cell<u32> = x(); }
fn f() { K.with(|s| s.set(1)); }`. `K.set(1)` without `with` is reported
correctly. `K.with(|s| s.get())` is correctly a read.

Cause (hypothesis C2-H1, confirmed by a probe): `place_root` gave the closure
parameter `s` as the write root. `s` is not a shared static, so `note_write`
ignored the write.

Fix: in `KEY.with(|value| ..)`, where `KEY` is a shared static, `value` is an
alias of `KEY` while the closure is visited. A write through `value` is a
write to `KEY`. If there is no write, the call is a read of `KEY`.
`docs/explicitness.md` now says this.

Regression test: `write_through_thread_local_with_closure_is_an_implicit_output`.

### Hypotheses that were tested

| Hypothesis | Result |
| --- | --- |
| C1-H1: `type_names_in` does not visit generic arguments | confirmed |
| C1-H2: `is_interior_mutable` does not know `LazyLock` | falsified: `Option<Mutex<..>>` also failed |
| C1-H3: the static is found, but the use site does not find it | falsified: the static was not added |
| C2-H1: the closure parameter is the write root, not the key | confirmed |
| C2-H2: the closure body is not visited | falsified: `println!` in the closure is reported |
| C2-H3: `place_root` does not go through method chains | falsified: `s.set(1)` also failed |

### Verification after the fixes

- The regression tests failed before the fixes and pass after them.
- The original fixtures and all minimal fixtures give the expected findings
  ([phase5-after-fix.txt](evidence/phase5-after-fix.txt)). `service.rs` now
  gives 9 of 9 expected findings.
- `cargo test` passes, including `self_analysis` and `pack_install_smoke`.
- `cargo clippy --all-targets` gives no warning for the changed code.
- `cargo fmt --check` gives no diff for the new code, other than the repo's
  double blank lines.
- No `[DEBUG-` tag remains in `src` or `tests`.

## Candidate classification

Confirmed and fixed in the branch: C1, C2.

Rejected, documented behaviour: `// messrust-disable-next-line ImplicitOutput`
above a `fn` line does not suppress a finding in the body. `docs/usage.md` says
that this comment applies only to the next physical line
([j1-suppress.txt](evidence/j1-suppress.txt)).

Pre-existing and not in scope, filed as
[#175](https://github.com/quality-gates/messrust/issues/175): a later
reference without property values does not restore a default that an earlier
XML file set. With `max-a.xml,codesize`, `max-b.xml,codesize`, or
`max-b.xml,rust`, `ExcessiveParameterList` keeps `minimum=2`. `max20.xml,naming`
keeps `LongVariable` `maximum=20`. The branch and `origin/main` give the same
result, 2/2 for each combination
([j2-override-differential.txt](evidence/j2-override-differential.txt)).
`strict.xml,explicitness` shows the same behaviour. The #148 test passes only
because `rust.xml` sets `LongVariable` `maximum` explicitly.

Not a grounded bug: `&raw mut BUFFER` and `addr_of_mut!(BUFFER)` on a
`static mut` are reported as reads, not writes. The docs list `&mut X` as a
write, but they do not list raw borrows.

## Usability observations

- Observation: a user who wants to suppress all findings for one function must
  use a region, because `disable-next-line` above the `fn` line does not
  include the body.
- Observation: `include-self=on` silently leaves the mode off.
- Suggestion: count `&raw mut` and `addr_of_mut!` on a shared static as a
  write. The Rust 2024 edition makes these the usual form.

## Issue filing

C1 and C2 were not filed. They were in unpushed branch code and are fixed in
the same change. The pre-existing override gap is
[#175](https://github.com/quality-gates/messrust/issues/175), labelled `bug`
and `needs-triage`.

## Unexplored areas and limitations

- XML, SARIF, HTML, GitLab, Checkstyle, and GitHub output formats were not
  checked for the new rules. Only text and JSON were checked.
- Multi-file projects were not checked. The docs say that a static from a
  different file is not known.
- The known limits in `docs/explicitness.md` were not retested as bugs.
- The fixes changed the code under test. The findings before the fixes are in
  the `j1`, `j2`, `j3`, and `c1-c2` evidence files.

## Cleanup

The Docker volumes `messrust-et-target`, `messrust-et-cargo`, and
`messrust-et-main-target`, the temporary `origin/main` worktree, and the host
`target/` directory were removed. The report, [run.sh](run.sh), fixtures,
rulesets, and evidence are kept. `run.sh` needs a `cargo build` into the
`messrust-et-target` volume before a replay.
