//! Source file discovery.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

pub struct DiscoverOptions {
    pub suffixes: Vec<String>,
    pub exclude: Vec<String>,
    pub ignore_tests: bool,
}

pub fn discover(paths: &[String], opts: &DiscoverOptions) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for raw in paths {
        let path = PathBuf::from(raw);
        let meta = std::fs::metadata(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;

        if meta.is_file() {
            // Explicit files are added as-is (messgo behaviour).
            push_unique(&mut out, &mut seen, path)?;
            continue;
        }

        if !meta.is_dir() {
            return Err(format!("{}: not a file or directory", path.display()));
        }

        for entry in WalkDir::new(&path).into_iter().filter_entry(|e| {
            if e.file_type().is_dir() {
                !should_skip_dir(e.file_name().to_string_lossy().as_ref())
            } else {
                true
            }
        }) {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path();
            if !matches_suffix(p, &opts.suffixes) {
                continue;
            }
            if opts.ignore_tests && is_test_file(p) {
                continue;
            }
            if is_excluded(p, &opts.exclude) {
                continue;
            }
            push_unique(&mut out, &mut seen, p.to_path_buf())?;
        }
    }

    out.sort();
    Ok(out)
}

fn push_unique(
    out: &mut Vec<PathBuf>,
    seen: &mut std::collections::HashSet<PathBuf>,
    path: PathBuf,
) -> Result<(), String> {
    let abs = std::fs::canonicalize(&path).unwrap_or(path);
    if seen.insert(abs.clone()) {
        out.push(abs);
    }
    Ok(())
}

fn should_skip_dir(name: &str) -> bool {
    matches!(name, "target" | ".git" | "node_modules")
}

fn matches_suffix(path: &Path, suffixes: &[String]) -> bool {
    let s = path.to_string_lossy();
    suffixes.iter().any(|suf| s.ends_with(suf.as_str()))
}

fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with("_test.rs"))
}

fn is_excluded(path: &Path, exclude: &[String]) -> bool {
    let s = path.to_string_lossy();
    exclude.iter().any(|sub| s.contains(sub.as_str()))
}
