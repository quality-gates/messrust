//! messrust — PHPMD-style mess detector for Rust.
// messrust-disable UnusedLocalVariable,CamelCaseVariableName

mod analyze;
mod discover;
mod metrics;
mod report;
mod ruleset;
mod suppressions;

use std::io::Write;
use std::path::PathBuf;

use analyze::analyze_files;
use discover::{discover, DiscoverOptions};
use report::{exit_code_for, formats, is_known_format, render, WriteTarget};
use ruleset::{load_and_filter, LoadOptions};

/// Process exit codes (PHPMD family).
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_VIOLATION: i32 = 2;

// messrust-disable-next-line TooManyFields
struct Options {
    paths: Vec<String>,
    format: String,
    rulesets: Vec<String>,
    report_file: Option<PathBuf>,
    suffixes: Vec<String>,
    exclude: Vec<String>,
    only: Option<Vec<String>>,
    enable: Option<Vec<String>>,
    disable: Vec<String>,
    min_priority: u8,
    max_priority: u8,
    verbose: bool,
    ignore_tests: bool,
    ignore_errors: bool,
    ignore_violations: bool,
    color: bool,
    strict: bool,
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
         Formats: {}\n\n\
         Options:\n\
           --minimumpriority <n>            Only rules with priority <= n (1 highest)\n\
           --maximumpriority <n>            Only rules with priority >= n\n\
           --reportfile <path>              Write report to file (stdout empty)\n\
           --suffixes <ext[,ext...]>        Replace default source suffixes\n\
           --exclude <substr[,substr...]>   Skip paths containing a substring\n\
           --only, --enable <rules>         Keep only named loaded rules\n\
           --disable <rules>                Remove named loaded rules\n\
           --ignore-tests                   Skip conventional Rust test files\n\
           --strict                         Include source-suppressed findings\n\
           --color                          Colorize text output\n\
           --verbose, -v                    Ruleset/load diagnostics\n\
           --ignore-errors-on-exit          Exit 0/2 even when errors exist\n\
           --ignore-violations-on-exit      Exit 0/1 even when findings exist\n\
           --version                        Print version\n\
           --help, -h                       Print this help",
        formats().join(", ")
    );
}

// messrust-disable-next-line CyclomaticComplexity,ExcessiveMethodLength
fn parse_args(args: &[String]) -> Result<(Options, Vec<String>), String> {
    let mut opt = Options {
        paths: Vec::new(),
        format: String::new(),
        rulesets: Vec::new(),
        report_file: None,
        suffixes: Vec::new(),
        exclude: Vec::new(),
        only: None,
        enable: None,
        disable: Vec::new(),
        min_priority: 0,
        max_priority: 1,
        verbose: false,
        ignore_tests: false,
        ignore_errors: false,
        ignore_violations: false,
        color: false,
        strict: false,
    };
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(name) = a.strip_prefix("--") {
            match name {
                "reportfile" => {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| "missing value for --reportfile".to_string())?;
                    opt.report_file = Some(PathBuf::from(v));
                }
                "suffixes" => {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| "missing value for --suffixes".to_string())?;
                    opt.suffixes = suffix_list(v);
                }
                "exclude" => {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| "missing value for --exclude".to_string())?;
                    opt.exclude = split_list(v);
                }
                "only" => {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| "missing value for --only".to_string())?;
                    opt.only = Some(split_list(v));
                }
                "enable" => {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| "missing value for --enable".to_string())?;
                    opt.enable = Some(split_list(v));
                }
                "disable" => {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| "missing value for --disable".to_string())?;
                    opt.disable = split_list(v);
                }
                "minimumpriority" => {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| "missing value for --minimumpriority".to_string())?;
                    opt.min_priority = parse_priority("--minimumpriority", v)?;
                }
                "maximumpriority" => {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| "missing value for --maximumpriority".to_string())?;
                    opt.max_priority = parse_priority("--maximumpriority", v)?;
                }
                "verbose" => opt.verbose = true,
                "ignore-tests" => opt.ignore_tests = true,
                "strict" => opt.strict = true,
                "ignore-errors-on-exit" => opt.ignore_errors = true,
                "ignore-violations-on-exit" => opt.ignore_violations = true,
                "color" => opt.color = true,
                other => return Err(format!("unknown option: --{other}")),
            }
        } else if a == "-v" {
            opt.verbose = true;
        } else if a.starts_with('-') && a != "-" {
            return Err(format!("unknown option: {a}"));
        } else {
            positionals.push(a.clone());
        }
        i += 1;
    }
    Ok((opt, positionals))
}

fn parse_priority(flag: &str, value: &str) -> Result<u8, String> {
    let n: u8 = value
        .parse()
        .map_err(|_| format!("{flag} requires an integer"))?;
    if !(1..=5).contains(&n) {
        return Err(format!("{flag} must be between 1 and 5"));
    }
    Ok(n)
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

// messrust-disable-next-line CyclomaticComplexity
fn run_analysis(opt: Options, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if !is_known_format(&opt.format) {
        let _ = writeln!(
            stderr,
            "error: unknown report format {}. Available: {}",
            opt.format,
            formats().join(", ")
        );
        return EXIT_ERROR;
    }

    let only = opt.only.clone().or(opt.enable.clone()).unwrap_or_default();
    let load_opts = LoadOptions {
        min_priority: opt.min_priority,
        max_priority: opt.max_priority,
    };
    let verbose = opt.verbose;
    let mut warn = |msg: String| {
        if verbose {
            let _ = writeln!(stderr, "warning: {msg}");
        }
    };
    let rules = match load_and_filter(&opt.rulesets, &only, &opt.disable, &load_opts, &mut warn) {
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

    let report = analyze_files(&files, &rules, opt.strict, opt.ignore_tests);

    let target = match &opt.report_file {
        Some(path) => WriteTarget::File(path.clone()),
        None => WriteTarget::Stdout,
    };

    if let Err(e) = render(&opt.format, &report, opt.color, target, stdout) {
        let _ = writeln!(stderr, "error: {e}");
        return EXIT_ERROR;
    }

    exit_code_for(&report, opt.ignore_errors, opt.ignore_violations)
}
