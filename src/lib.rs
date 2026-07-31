//! messrust — PHPMD-style mess detector for Rust.

mod analyze;
mod discover;
mod report;

use std::io::Write;
use std::path::PathBuf;

use analyze::{analyze_files, RuleId};
use discover::{discover, DiscoverOptions};
use report::{exit_code_for, render_text, WriteTarget};

/// Process exit codes (PHPMD family).
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_VIOLATION: i32 = 2;

struct Options {
    paths: Vec<String>,
    format: String,
    rulesets: Vec<String>,
    report_file: Option<PathBuf>,
    suffixes: Vec<String>,
    exclude: Vec<String>,
    ignore_tests: bool,
    ignore_errors: bool,
    ignore_violations: bool,
}

/// Injectable CLI entry. `args` are argv without the program name.
/// Returns a process exit code.
pub fn run(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if args.is_empty() {
        print_usage(stderr);
        return EXIT_ERROR;
    }

    if let Some(code) = handle_info_flags(&args[0], stdout) {
        return code;
    }

    let (mut opt, positionals) = match parse_args(args) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            return EXIT_ERROR;
        }
    };

    if positionals.len() < 3 {
        print_usage(stderr);
        return EXIT_ERROR;
    }

    opt.paths = split_list(&positionals[0]);
    opt.format = positionals[1].clone();
    opt.rulesets = split_list(&positionals[2]);

    run_analysis(opt, stdout, stderr)
}

fn handle_info_flags(first: &str, stdout: &mut dyn Write) -> Option<i32> {
    match first {
        "--version" => {
            let _ = writeln!(stdout, "messrust {}", env!("CARGO_PKG_VERSION"));
            Some(EXIT_SUCCESS)
        }
        "--help" | "-h" | "help" => {
            print_usage(stdout);
            Some(EXIT_SUCCESS)
        }
        _ => None,
    }
}

fn print_usage(w: &mut dyn Write) {
    let _ = writeln!(
        w,
        "Usage: messrust <paths> <format> <ruleset[,ruleset...]> [options]\n\n\
         Options:\n\
           --reportfile <path>              Write report to file (stdout empty)\n\
           --suffixes <ext[,ext...]>        Replace default source suffixes\n\
           --exclude <substr[,substr...]>   Skip paths containing a substring\n\
           --ignore-tests                   Skip conventional Rust test files\n\
           --ignore-errors-on-exit          Exit 0/2 even when errors exist\n\
           --ignore-violations-on-exit      Exit 0/1 even when findings exist\n\
           --version                        Print version\n\
           --help, -h                       Print this help"
    );
}

fn parse_args(args: &[String]) -> Result<(Options, Vec<String>), String> {
    let mut opt = Options {
        paths: Vec::new(),
        format: String::new(),
        rulesets: Vec::new(),
        report_file: None,
        suffixes: Vec::new(),
        exclude: Vec::new(),
        ignore_tests: false,
        ignore_errors: false,
        ignore_violations: false,
    };
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(name) = a.strip_prefix("--") {
            match name {
                "reportfile" => {
                    i += 1;
                    let v = args.get(i).ok_or_else(|| "missing value for --reportfile".to_string())?;
                    opt.report_file = Some(PathBuf::from(v));
                }
                "suffixes" => {
                    i += 1;
                    let v = args.get(i).ok_or_else(|| "missing value for --suffixes".to_string())?;
                    opt.suffixes = suffix_list(v);
                }
                "exclude" => {
                    i += 1;
                    let v = args.get(i).ok_or_else(|| "missing value for --exclude".to_string())?;
                    opt.exclude = split_list(v);
                }
                "ignore-tests" => opt.ignore_tests = true,
                "ignore-errors-on-exit" => opt.ignore_errors = true,
                "ignore-violations-on-exit" => opt.ignore_violations = true,
                other => return Err(format!("unknown option: --{other}")),
            }
        } else if a.starts_with('-') && a != "-" {
            return Err(format!("unknown option: {a}"));
        } else {
            positionals.push(a.clone());
        }
        i += 1;
    }
    Ok((opt, positionals))
}

fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

fn suffix_list(s: &str) -> Vec<String> {
    split_list(s)
        .into_iter()
        .map(|ext| {
            if ext.starts_with('.') {
                ext
            } else {
                format!(".{ext}")
            }
        })
        .collect()
}

fn run_analysis(opt: Options, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if opt.format != "text" {
        let _ = writeln!(stderr, "error: unknown report format {}", opt.format);
        return EXIT_ERROR;
    }

    let rules = match load_rules(&opt.rulesets) {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            return EXIT_ERROR;
        }
    };

    let discover_opts = DiscoverOptions {
        suffixes: if opt.suffixes.is_empty() {
            vec![".rs".to_string()]
        } else {
            opt.suffixes.clone()
        },
        exclude: opt.exclude.clone(),
        ignore_tests: opt.ignore_tests,
    };

    let files = match discover(&opt.paths, &discover_opts) {
        Ok(f) => f,
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            return EXIT_ERROR;
        }
    };

    let report = analyze_files(&files, &rules);

    let target = match &opt.report_file {
        Some(path) => WriteTarget::File(path.clone()),
        None => WriteTarget::Stdout,
    };

    if let Err(e) = render_text(&report, target, stdout) {
        let _ = writeln!(stderr, "error: {e}");
        return EXIT_ERROR;
    }

    exit_code_for(&report, opt.ignore_errors, opt.ignore_violations)
}

fn load_rules(rulesets: &[String]) -> Result<Vec<RuleId>, String> {
    let mut rules = Vec::new();
    for name in rulesets {
        match name.as_str() {
            "codesize" => rules.push(RuleId::ExcessiveParameterList),
            other => return Err(format!("unknown ruleset: {other}")),
        }
    }
    if rules.is_empty() {
        return Err("no rulesets specified".to_string());
    }
    Ok(rules)
}
