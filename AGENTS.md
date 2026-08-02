Only report to me in ASD-STE100 Simplified Technical English. 

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `quality-gates/messrust` (via `gh`). See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `CONTEXT.md` + `docs/adr/`. See `docs/agents/domain.md`.

## Cursor Cloud specific instructions

`messrust` is one Rust binary crate (no other services). It analyzes Rust
source files and prints reports. See `README.md` for the CLI purpose and
`src/lib.rs` `print_usage` for the full flag list.

Toolchain: use Rust stable `1.85` or newer. A transitive dependency needs the
`edition2024` feature, so the older `1.83` toolchain fails at `cargo fetch`. The
VM snapshot has stable `1.97.1` as default; if `cargo` reports an `edition2024`
error, run `rustup update stable` and `rustup default stable`.

`Cargo.lock` is not in git (see `.gitignore`), so dependencies resolve to the
newest compatible versions each fresh build.

Standard commands (run from repo root):
* Build: `cargo build`
* Run: `cargo run -- <paths> <format> <ruleset[,ruleset...]>` (example:
  `cargo run -- src text codesize,naming,unusedcode`). Exit code `2` means the
  tool found violations; that is normal, not a failure.
* Test: `cargo test` (the `pack_install_smoke` test runs `cargo install` and
  takes ~20 s).
* Lint: `cargo clippy --all-targets` and `cargo fmt --check`.

Caveat: under stable `1.97`, `cargo fmt --check` reports diffs and `cargo
clippy` reports warnings on existing code. This is toolchain-version drift from
the older toolchain that formatted the code, not a code defect. Do not
auto-reformat existing files unless a task asks for it.
