use std::path::PathBuf;

use messrust::{run, EXIT_SUCCESS};

#[test]
fn production_source_passes_strict_default_policy() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let args = vec![
        source.display().to_string(),
        "text".to_string(),
        "rust".to_string(),
        "--ignore-tests".to_string(),
        "--strict".to_string(),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = run(&args, &mut stdout, &mut stderr);

    assert_eq!(
        code,
        EXIT_SUCCESS,
        "strict self-analysis failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
}
