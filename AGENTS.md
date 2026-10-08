Reply only in ASD-STE100 Simplified Technical English.

Do all development work in the capped development container. See "Development container".
Clean up after your work. See "Definition of Done".

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `quality-gates/messrust` (via `gh`). See `docs/agents/issue-tracker.md`.

### Triage labels

Default five-role vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `CONTEXT.md` + `docs/adr/`. See `docs/agents/domain.md`.

## Development container

`messrust` is one Rust binary crate (no other services). It analyzes Rust
source files and prints reports. See `README.md` for the CLI purpose and
`src/lib.rs` `print_usage` for the full flag list.

Build, test, lint, run, and debug only in the container that `dev.Dockerfile`
defines. Do not run `cargo`, `rustc`, or a host-built `messrust` binary on the
macOS host.

Build the image when `Cargo.lock` changes. Then start one container for the
checkout:

```bash
docker build -f dev.Dockerfile -t messrust-dev .
gitdir="$(git rev-parse --path-format=absolute --git-common-dir)"
name="messrust-dev-$(basename "$PWD")"
docker run -d --rm --init --name "$name" \
  --cpus=2 --memory=4g --memory-swap=4g --pids-limit=512 \
  --tmpfs /workspace/target:rw,exec,size=2g \
  --tmpfs /tmp:rw,exec,size=1g \
  -v "$PWD":/workspace -v "$gitdir":"$gitdir" \
  messrust-dev sleep 14400
```

Limits: 2 CPUs, 4 GiB memory with no swap, 512 processes, 2 GiB for `target/`,
and 1 GiB for `/tmp`. The two tmpfs mounts count toward the memory limit. The
test suite and a release build use about 2.7 GiB of memory and 1.5 GiB of
`target/`. The container stops after 4 hours. Build output stays in the
container and does not go to the host.

Run each command with `docker exec "$name"`:

* Test: `docker exec "$name" cargo test --all-targets --locked` (the
  `pack_install_smoke` test runs `cargo install`).
* Build and run: `docker exec "$name" cargo build --release --locked`, then
  `docker exec "$name" ./target/release/messrust src text rust --ignore-tests`.
  Exit code `2` means the tool found violations. That is normal, not a failure.
* Lint: `docker exec "$name" cargo clippy --all-targets` and
  `docker exec "$name" cargo fmt --check`.
* Stop: `docker rm -f "$name"`.

The image uses Rust `1.85.1`, the same version as the release workflow. CI
tests use stable Rust. `Cargo.lock` is in git; use `--locked`.

Caveat: CI does not run `cargo fmt --check` or `cargo clippy`. Both report
findings on existing code. Do not reformat existing files unless a task asks
for it.

Mutation-run disk: Mutarust uses one isolated target directory per worker and
removes those mutation areas by default. Keep that default:
`--do-not-remove-tmp-folder` can retain tens of GB. Use fewer `--workers` when
disk space is limited, then remove any retained areas that Mutarust reports.

## Definition of Done

* Respect the storage space of the host. Before you finish, remove the
  container (`docker rm -f "$name"`). Remove build output and other cruft from
  the checkout (`rm -rf target .mutants mutarust-agentic.json mutarust-summary.json mutarust-report.html`).
  Remove replaced development images
  (`docker image prune -f --filter label=dev-image=messrust`).
