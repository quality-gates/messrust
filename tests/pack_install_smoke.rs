//! Pack + isolated install smoke: spawn the real installed binary.
//!
//! Seam: process argv/stdout/stderr/exit of `messrust` after
//! `cargo package` and `cargo install --root <isolated>` — not the
//! injectable library entry used by `tests/cli.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use messrust::{EXIT_SUCCESS, EXIT_VIOLATION};
use tempfile::TempDir;

fn cargo_bin() -> PathBuf {
    PathBuf::from(option_env!("CARGO").unwrap_or("cargo"))
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_cargo(args: &[&str], cwd: &Path) -> (i32, String, String) {
    let output = Command::new(cargo_bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn cargo {}: {e}", args.join(" ")));
    (
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn spawn_messrust(bin: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(bin)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", bin.display()));
    (
        output.status.code().unwrap_or(1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn fixture_with_params(n: usize) -> String {
    let params: Vec<String> = (0..n).map(|i| format!("p{i}: i32")).collect();
    format!("fn f({}) {{}}\n", params.join(", "))
}

#[test]
fn pack_install_smoke_spawns_real_binary() {
    let work = TempDir::new().unwrap();
    let target_dir = work.path().join("target");
    let install_root = work.path().join("install");
    let fixture_dir = work.path().join("fixture");
    fs::create_dir_all(&fixture_dir).unwrap();
    fs::create_dir_all(&target_dir).unwrap();

    let target_dir_arg = target_dir.to_str().unwrap();
    let install_root_arg = install_root.to_str().unwrap();

    // Prove packaging works; failure must fail this test.
    // Use a dedicated --target-dir so nested cargo does not lock the
    // parent test build's target directory.
    let (code, out, err) = run_cargo(
        &[
            "package",
            "--allow-dirty",
            "--target-dir",
            target_dir_arg,
        ],
        &crate_root(),
    );
    assert_eq!(
        code, 0,
        "cargo package failed\nstdout={out}\nstderr={err}"
    );

    let packaged = target_dir.join(format!(
        "package/messrust-{}",
        env!("CARGO_PKG_VERSION")
    ));
    assert!(
        packaged.is_dir(),
        "expected packaged crate at {}",
        packaged.display()
    );

    // Install into an isolated root — not the developer global Cargo bin.
    let (code, out, err) = run_cargo(
        &[
            "install",
            "--path",
            packaged.to_str().unwrap(),
            "--root",
            install_root_arg,
            "--target-dir",
            target_dir_arg,
            "--force",
        ],
        &crate_root(),
    );
    assert_eq!(
        code, 0,
        "cargo install failed\nstdout={out}\nstderr={err}"
    );

    let bin_name = if cfg!(windows) {
        "messrust.exe"
    } else {
        "messrust"
    };
    let bin = install_root.join("bin").join(bin_name);
    assert!(
        bin.is_file(),
        "expected installed binary at {}",
        bin.display()
    );
    assert!(
        bin.starts_with(&install_root),
        "binary must live under isolated install root {}",
        install_root.display()
    );
    assert!(
        install_root.starts_with(work.path()),
        "install root must stay inside the smoke temp dir"
    );

    // --help
    let (code, out, err) = spawn_messrust(&bin, &["--help"]);
    assert_eq!(code, EXIT_SUCCESS, "messrust --help exit\nstderr={err}");
    assert!(out.contains("Usage:"), "help stdout={out:?}");
    assert!(out.contains("messrust"), "help stdout={out:?}");
    assert!(err.is_empty(), "help stderr={err:?}");

    // --version
    let (code, out, err) = spawn_messrust(&bin, &["--version"]);
    assert_eq!(code, EXIT_SUCCESS, "messrust --version exit\nstderr={err}");
    assert!(
        out.contains(env!("CARGO_PKG_VERSION")),
        "version stdout={out:?}"
    );
    assert!(out.starts_with("messrust "), "version stdout={out:?}");
    assert!(err.is_empty(), "version stderr={err:?}");

    // One fixture analysis: excessive parameter list → exit 2 + text shape.
    let fixture = fixture_dir.join("fixture.rs");
    fs::write(&fixture, fixture_with_params(11)).unwrap();
    let (code, out, err) = spawn_messrust(
        &bin,
        &[fixture.to_str().unwrap(), "text", "codesize"],
    );
    assert_eq!(
        code, EXIT_VIOLATION,
        "analysis exit\nstdout={out}\nstderr={err}"
    );
    assert!(
        out.contains("ExcessiveParameterList"),
        "analysis stdout={out:?}"
    );
    assert!(out.contains("11 parameters"), "analysis stdout={out:?}");
    assert!(
        out.contains("fixture.rs:"),
        "expected file:line location in text report: {out:?}"
    );
    assert!(err.is_empty(), "analysis stderr={err:?}");
}
