#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_package(root: &Path, version: &str, arch: &str, binary: &Path, dist: &Path) -> Output {
    Command::new(root.join("scripts/package-release"))
        .args([version, arch])
        .arg(binary)
        .arg(dist)
        .arg("1700000000")
        .output()
        .expect("release package command must start")
}

#[test]
fn release_archive_has_the_public_homebrew_layout_and_exact_payload() {
    let root = repository_root();
    let work = TempDir::new().unwrap();
    let binary = work.path().join("fixture-messrust");
    let dist = work.path().join("dist");
    let extracted = work.path().join("extracted");
    let binary_bytes = b"#!/usr/bin/env sh\nprintf 'messrust 1.2.3\\n'\n";
    fs::write(&binary, binary_bytes).unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();

    let output = run_package(&root, "1.2.3", "arm64", &binary, &dist);
    assert!(
        output.status.success(),
        "package failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let archive = dist.join("messrust_1.2.3_darwin_arm64.tar.gz");
    assert!(archive.is_file(), "expected archive: {}", archive.display());
    let retry_dist = work.path().join("retry-dist");
    let retry = run_package(&root, "1.2.3", "arm64", &binary, &retry_dist);
    assert!(retry.status.success());
    assert_eq!(
        fs::read(&archive).unwrap(),
        fs::read(retry_dist.join("messrust_1.2.3_darwin_arm64.tar.gz")).unwrap(),
        "a retry must package the same source bytes identically"
    );
    let amd64_dist = work.path().join("amd64-dist");
    let amd64 = run_package(&root, "1.2.3", "amd64", &binary, &amd64_dist);
    assert!(amd64.status.success());
    assert!(
        amd64_dist
            .join("messrust_1.2.3_darwin_amd64.tar.gz")
            .is_file(),
        "the Intel archive name is part of the public release contract"
    );

    let listing = Command::new("tar")
        .args(["-tzf"])
        .arg(&archive)
        .output()
        .unwrap();
    assert!(listing.status.success());
    let mut entries: Vec<_> = String::from_utf8(listing.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    entries.sort();
    assert_eq!(entries, ["LICENSE", "messrust"]);

    fs::create_dir(&extracted).unwrap();
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(&archive)
        .args(["-C"])
        .arg(&extracted)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(fs::read(extracted.join("messrust")).unwrap(), binary_bytes);
    assert_eq!(
        fs::read(extracted.join("LICENSE")).unwrap(),
        fs::read(root.join("LICENSE")).unwrap()
    );
    assert_ne!(
        fs::metadata(extracted.join("messrust"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0
    );
}

#[test]
fn release_archive_rejects_nonstable_versions_and_unknown_architectures() {
    let root = repository_root();
    let work = TempDir::new().unwrap();
    let binary = work.path().join("messrust");
    fs::write(&binary, b"fixture").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();

    for (version, arch) in [("v1.2.3", "arm64"), ("1.2", "arm64"), ("1.2.3", "x64")] {
        let output = run_package(&root, version, arch, &binary, &work.path().join("dist"));
        assert!(
            !output.status.success(),
            "package accepted version={version}, arch={arch}"
        );
    }
}

#[test]
fn release_workflow_keeps_the_protected_tap_dispatch_contract() {
    let workflow = include_str!("../.github/workflows/release.yml");
    let required_contract = [
        "environment: homebrew",
        "actions/create-github-app-token@",
        "repositories: homebrew-tap",
        "permission-actions: write",
        "actions/workflows/publish-formula.yml/dispatches",
        "-f ref=main",
        "inputs[tool]=messrust",
        "inputs[tag]",
        "inputs[version]",
        "inputs[release_id]",
        "inputs[source_sha]",
        "inputs[arm64_asset]",
        "inputs[amd64_asset]",
        "inputs[arm64_sha]",
        "inputs[amd64_sha]",
    ];

    for contract_item in required_contract {
        assert!(
            workflow.contains(contract_item),
            "release workflow lost public contract item: {contract_item}"
        );
    }
    assert!(!workflow.contains("HOMEBREW_TAP_TOKEN"));
}
