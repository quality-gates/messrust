# Coding standards

## Tests

- Strongly prefer integration tests and end-to-end tests over unit tests.
- Strongly prefer exercising real system behaviour over "the tests pass so it must work."
- Only mock third-party services we cannot control. Do not mock code we own.
- For this codebase, the default proof is: run the real CLI/analyzer on real (or fixture) source and assert findings, exit codes, and report output.

## Comments and docs

- Code comments use ASD-STE100 Simplified Technical English.
- Ground terms in `CONTEXT.md` domain language when that file exists. Do not invent synonyms for glossary terms.
- Do not write comments that only repeat what the code already makes clear.
- Do not put brittle references in README or comments (versions, line numbers, temporary paths, "as of today" claims) when those details are allowed to change.

## Common footguns

- Tautological tests (asserting the mock was called the way the test just configured it).
- Mocks of modules/services we own.
- "Green suite" treated as proof the product works for a user.
- Narrating comments and README drift magnets.
- Cheating complexity or quality gates with denser syntax, hidden branching, or indirection that does not reduce real complexity.

## Rust

- Stay on the package edition and MSRV posture in `Cargo.toml` / docs (stable with `edition2024` support for deps). Do not bump edition or add features casually.
- Prefer the existing error style: `Result<T, String>` (or `std::io::Result`) at API boundaries already using it. Do not introduce `anyhow`/`thiserror` unless the crate already takes that dependency on purpose.
- Library and analysis code must not `unwrap`/`expect` on input-dependent paths. Reserve panics for internal invariants that indicate a bug.
- Parse Rust with `syn` + existing visitors. Do not add a second syntax stack.
- Keep the crate a single binary package unless there is a strong split reason. New modules go under `src/` in the current layout (`analyze/`, `discover`, `ruleset`, `report`, …).
- New code must be `cargo fmt`-clean and `cargo clippy`-clean. Do not mass-reformat unrelated files to absorb toolchain drift.
- Prefer integration tests under `tests/` that shell the binary or call public library entrypoints over large `#[cfg(test)]` modules inside production files.
- Exit code `2` means violations found — tests and scripts must treat that as a successful detection outcome when that is the behaviour under test.
- Avoid `unsafe` unless there is no safe alternative; isolate it and document the invariant.
- Do not commit `Cargo.lock` policy changes lightly: this repo currently gitignores the lockfile — match that policy unless an explicit packaging decision changes it.
