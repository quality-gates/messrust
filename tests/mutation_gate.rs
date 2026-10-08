//! Policy seam for the hard mutation gate (#48 / #33).
//!
//! Reads `mutarust.yml` and production sources as text.
//! A commit that weakens the mutation policy must fail here in milliseconds.

use std::fs;
use std::path::{Path, PathBuf};

const POLICY: &str = "mutarust.yml";
const MIN_MSI: &str = "75";
const MIN_COVERED_MSI: &str = "80";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
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
