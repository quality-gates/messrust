//! Policy seam for the hard mutation gate (#48 / #33).
//!
//! Reads the workflow file, `mutarust.yml`, and production sources as text.
//! A commit that weakens either gate mode must fail here in milliseconds.

use std::fs;
use std::path::{Path, PathBuf};

const WORKFLOW: &str = ".github/workflows/mutation.yml";
const POLICY: &str = "mutarust.yml";
const MIN_MSI: &str = "75";
const MIN_COVERED_MSI: &str = "80";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// Text of one named job body (from `job_name:` until the next top-level job key).
fn job_body(workflow: &str, job_name: &str) -> String {
    let marker = format!("  {job_name}:");
    let start = workflow
        .find(&marker)
        .unwrap_or_else(|| panic!("workflow must define job `{job_name}`"));
    let after = &workflow[start + marker.len()..];
    let end = after
        .lines()
        .skip(1)
        .position(|line| {
            line.starts_with("  ")
                && !line.starts_with("   ")
                && line.trim_end().ends_with(':')
                && !line.trim().starts_with('#')
        })
        .map(|i| {
            // skip(1) dropped the first line after the job key; rebuild offset
            let mut offset = 0;
            for (idx, line) in after.lines().enumerate() {
                if idx == 0 {
                    offset += line.len() + 1;
                    continue;
                }
                if idx - 1 == i {
                    return offset;
                }
                offset += line.len() + 1;
            }
            after.len()
        })
        .unwrap_or(after.len());
    after[..end].to_string()
}

fn assert_thresholds_in(text: &str, where_: &str) {
    assert!(
        text.contains(&format!("--min-msi {MIN_MSI}"))
            || text.contains(&format!("min_msi: {MIN_MSI}")),
        "{where_} must hold min MSI {MIN_MSI}"
    );
    assert!(
        text.contains(&format!("--min-covered-msi {MIN_COVERED_MSI}"))
            || text.contains(&format!("min_covered_msi: {MIN_COVERED_MSI}")),
        "{where_} must hold covered MSI {MIN_COVERED_MSI}"
    );
}

fn assert_cli_thresholds(text: &str, where_: &str) {
    assert!(
        text.contains(&format!("--min-msi {MIN_MSI}")),
        "{where_} must hold --min-msi {MIN_MSI}"
    );
    assert!(
        text.contains(&format!("--min-covered-msi {MIN_COVERED_MSI}")),
        "{where_} must hold --min-covered-msi {MIN_COVERED_MSI}"
    );
}

fn assert_no_forbidden_shared(text: &str, where_: &str) {
    let forbidden = [
        "continue-on-error",
        "|| true",
        "--baseline",
        "--fail-on-escaped",
        "mutarust-baseline.json",
        "--blacklist",
        "--match",
        "--run-mutant-id",
        "--dry-run",
        "--no-exec",
    ];
    for token in forbidden {
        assert!(
            !text.contains(token),
            "{where_} must not hold forbidden token `{token}`"
        );
    }
    // `--exec` is forbidden; `--exec-timeout` is allowed. Match a bare `--exec`
    // that is not a prefix of `--exec-timeout`.
    let mut search = text;
    while let Some(idx) = search.find("--exec") {
        let rest = &search[idx + "--exec".len()..];
        assert!(
            rest.starts_with("-timeout"),
            "{where_} must not hold bare `--exec` (found near `...{}`)",
            &search[idx..].chars().take(24).collect::<String>()
        );
        search = rest;
    }
}

fn assert_no_named_source_files(text: &str, where_: &str) {
    // Gate target must be the crate root (`.`), never a stale file list.
    for name in [
        "src/analyze.rs",
        "src/discover.rs",
        "src/lib.rs",
        "src/main.rs",
        "src/metrics.rs",
        "src/report.rs",
        "src/ruleset.rs",
        "src/suppressions.rs",
    ] {
        assert!(
            !text.contains(name),
            "{where_} must not name individual source file `{name}`"
        );
    }
}

fn walk_src_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            walk_src_rs(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn policy_file_holds_gate_thresholds() {
    let policy = read(POLICY);
    assert!(
        policy.contains(&format!("min_msi: {MIN_MSI}")),
        "mutarust.yml must hold min_msi: {MIN_MSI}"
    );
    assert!(
        policy.contains(&format!("min_covered_msi: {MIN_COVERED_MSI}")),
        "mutarust.yml must hold min_covered_msi: {MIN_COVERED_MSI}"
    );
    assert!(
        policy.contains("skip_without_test: false"),
        "mutarust.yml must keep skip_without_test false"
    );
    assert!(
        policy.contains("skip_with_cfg: false"),
        "mutarust.yml must keep skip_with_cfg false"
    );
    for key in [
        "exclude_dirs: []",
        "disable_mutators: []",
        "enable_mutators: []",
        "ignore_source_lines: []",
    ] {
        assert!(
            policy.contains(key),
            "mutarust.yml must keep empty list `{key}`"
        );
    }
}

#[test]
fn workflow_triggers_on_pull_request_and_push_to_main() {
    let workflow = read(WORKFLOW);
    assert!(
        workflow.contains("pull_request:"),
        "workflow must trigger on pull_request"
    );
    assert!(
        workflow.contains("push:"),
        "workflow must trigger on push"
    );
    assert!(
        workflow.contains("branches: [main]") || workflow.contains("- main"),
        "push trigger must target main"
    );
    assert!(
        !workflow.contains("paths:"),
        "workflow must not hold a paths filter"
    );
}

#[test]
fn pull_request_path_is_diff_aware_at_full_thresholds() {
    let workflow = read(WORKFLOW);
    let pr = job_body(&workflow, "mutation-pull-request");
    assert!(pr.contains("--coverage"), "PR path must use --coverage");
    assert!(
        pr.contains("--git-diff-lines"),
        "PR path must use --git-diff-lines"
    );
    assert!(
        pr.contains("--git-diff-base"),
        "PR path must use --git-diff-base"
    );
    assert!(
        pr.contains("--ignore-msi-with-no-mutations"),
        "PR path must use --ignore-msi-with-no-mutations"
    );
    assert!(
        pr.contains("--logger-github"),
        "PR path must use --logger-github"
    );
    assert_cli_thresholds(&pr, "PR path");
    assert!(
        pr.contains("pack_install_smoke"),
        "PR path must skip pack_install_smoke in the per-mutant suite"
    );
    assert_no_forbidden_shared(&pr, "PR path");
    assert_no_named_source_files(&pr, "PR path");
}

#[test]
fn push_path_is_full_scope_at_full_thresholds() {
    let workflow = read(WORKFLOW);
    let push = job_body(&workflow, "mutation-push");
    assert!(push.contains("--coverage"), "push path must use --coverage");
    assert!(
        push.contains("--logger-github"),
        "push path must use --logger-github"
    );
    assert_cli_thresholds(&push, "push path");
    assert!(
        !push.contains("--git-diff-lines"),
        "push path must not use --git-diff-lines"
    );
    assert!(
        !push.contains("--git-diff-base"),
        "push path must not use --git-diff-base"
    );
    assert!(
        !push.contains("--ignore-msi-with-no-mutations"),
        "push path must not use --ignore-msi-with-no-mutations"
    );
    assert!(
        push.contains("pack_install_smoke"),
        "push path must skip pack_install_smoke in the per-mutant suite"
    );
    assert_no_forbidden_shared(&push, "push path");
    assert_no_named_source_files(&push, "push path");
}

#[test]
fn workflow_and_policy_share_the_same_thresholds() {
    let workflow = read(WORKFLOW);
    let policy = read(POLICY);
    assert_thresholds_in(&policy, "mutarust.yml");
    assert_cli_thresholds(
        &job_body(&workflow, "mutation-pull-request"),
        "PR path",
    );
    assert_cli_thresholds(&job_body(&workflow, "mutation-push"), "push path");
}

#[test]
fn workflow_installs_tools_and_pins_actions() {
    let workflow = read(WORKFLOW);
    assert!(
        workflow.contains("cargo-llvm-cov"),
        "workflow must install cargo-llvm-cov"
    );
    assert!(
        workflow.contains("llvm-tools-preview"),
        "workflow must install llvm-tools-preview"
    );
    assert!(
        workflow.contains("mutarust") && workflow.contains("--version"),
        "workflow must install mutarust at a pinned version"
    );
    assert!(
        workflow.contains("fetch-depth: 0"),
        "checkout must use fetch-depth: 0 for the merge base"
    );
    assert!(
        workflow.contains("timeout-minutes:"),
        "each job must have a timeout"
    );
    // Full commit SHA: 40 hex characters after @
    let sha_pins = workflow
        .lines()
        .filter(|l| l.contains("uses:"))
        .all(|l| {
            l.split('@').nth(1).map(|s| {
                let sha = s.split_whitespace().next().unwrap_or("");
                sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit())
            }) == Some(true)
        });
    assert!(sha_pins, "every uses: action must be pinned by full commit SHA");
}

#[test]
fn workflow_documents_residual_risk_and_smoke_exclusion() {
    let workflow = read(WORKFLOW);
    let lower = workflow.to_lowercase();
    assert!(
        lower.contains("residual") || lower.contains("whole-crate"),
        "workflow must comment the accepted residual risk for the diff-aware PR gate"
    );
    assert!(
        lower.contains("pack_install_smoke")
            && (lower.contains("lowers") || lower.contains("lower")),
        "workflow must state that skipping pack_install_smoke lowers the score"
    );
}

#[test]
fn production_source_has_no_mutator_disable_comments() {
    let src = root().join("src");
    let mut files = Vec::new();
    walk_src_rs(&src, &mut files);
    assert!(!files.is_empty(), "expected production .rs files under src");
    for path in files {
        let text = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("read {}: {e}", path.display());
        });
        for token in [
            "mutator-disable-func",
            "mutator-disable-next-line",
            "mutator-disable-regexp",
            "mutator-disable",
        ] {
            assert!(
                !text.contains(token),
                "{} must not hold `{token}`",
                path.display()
            );
        }
    }
}
