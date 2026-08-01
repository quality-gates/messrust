//! Source file discovery.

use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

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
        discover_path(&path, opts, &mut out, &mut seen)?;
    }

    out.sort();
    Ok(out)
}

fn discover_path(
    path: &Path,
    opts: &DiscoverOptions,
    out: &mut Vec<PathBuf>,
    seen: &mut std::collections::HashSet<PathBuf>,
) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.is_file() {
        if !opts.ignore_tests || !is_test_path(path) {
            push_unique(out, seen, path.to_path_buf());
        }
        return Ok(());
    }
    if !meta.is_dir() {
        return Err(format!("{}: not a file or directory", path.display()));
    }
    discover_dir(path, opts, out, seen)
}

fn discover_dir(
    path: &Path,
    opts: &DiscoverOptions,
    out: &mut Vec<PathBuf>,
    seen: &mut std::collections::HashSet<PathBuf>,
) -> Result<(), String> {
    let entries = WalkDir::new(path)
        .into_iter()
        .filter_entry(|entry| walk_entry_allowed(entry, opts.ignore_tests));
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let candidate = entry.path();
        if entry.file_type().is_file()
            && matches_suffix(candidate, &opts.suffixes)
            && (!opts.ignore_tests || !is_test_path(candidate))
            && !is_excluded(candidate, &opts.exclude)
        {
            push_unique(out, seen, candidate.to_path_buf());
        }
    }
    Ok(())
}

fn walk_entry_allowed(entry: &DirEntry, ignore_tests: bool) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    !should_skip_dir(&name) && (!ignore_tests || !is_test_dir(&name))
}

fn push_unique(
    out: &mut Vec<PathBuf>,
    seen: &mut std::collections::HashSet<PathBuf>,
    path: PathBuf,
) {
    let abs = std::fs::canonicalize(&path).unwrap_or(path);
    if seen.insert(abs.clone()) {
        out.push(abs);
    }
}

fn should_skip_dir(name: &str) -> bool {
    matches!(name, "target" | ".git" | "node_modules")
}

fn matches_suffix(path: &Path, suffixes: &[String]) -> bool {
    let s = path.to_string_lossy();
    suffixes.iter().any(|suf| s.ends_with(suf.as_str()))
}

fn is_test_file(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
        n == "test.rs" || n == "tests.rs" || n.starts_with("test_") || n.ends_with("_test.rs")
    })
}

fn is_test_dir(name: &str) -> bool {
    matches!(name, "test" | "tests" | "__tests__")
}

fn is_test_path(path: &Path) -> bool {
    is_test_file(path)
        || path
            .components()
            .any(|component| component.as_os_str().to_str().is_some_and(is_test_dir))
}

fn is_excluded(path: &Path, exclude: &[String]) -> bool {
    let s = path.to_string_lossy();
    exclude.iter().any(|sub| s.contains(sub.as_str()))
}
