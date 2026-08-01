//! messrust — PHPMD-style mess detector for Rust.

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

struct Options {
    input: InputOptions,
    rules: RuleOptions,
    output: OutputOptions,
    behavior: BehaviorOptions,
}

#[derive(Default)]
struct InputOptions {
    paths: Vec<String>,
    suffixes: Vec<String>,
    exclude: Vec<String>,
    ignore_tests: bool,
}

struct RuleOptions {
    rulesets: Vec<String>,
    only: Option<Vec<String>>,
    enable: Option<Vec<String>>,
    disable: Vec<String>,
    min_priority: u8,
    max_priority: u8,
}

#[derive(Default)]
struct OutputOptions {
    format: String,
    report_file: Option<PathBuf>,
    color: bool,
}

#[derive(Default)]
struct BehaviorOptions {
    verbose: bool,
    ignore_errors: bool,
    ignore_violations: bool,
    strict: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            input: InputOptions::default(),
            rules: RuleOptions {
                rulesets: Vec::new(),
                only: None,
                enable: None,
                disable: Vec::new(),
                min_priority: 0,
                max_priority: 1,
            },
            output: OutputOptions::default(),
            behavior: BehaviorOptions::default(),
        }
    }
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

    opt.input.paths = split_list(&positionals[0]);
    opt.output.format = positionals[1].clone();
    opt.rules.rulesets = split_list(&positionals[2]);

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

fn parse_args(args: &[String]) -> Result<(Options, Vec<String>), String> {
    let mut opt = Options::default();
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(name) = a.strip_prefix("--") {
            let value = args.get(i + 1);
            if apply_value_option(name, value, &mut opt)? {
                i += 1;
            } else if !apply_flag_option(name, &mut opt) {
                return Err(format!("unknown option: --{name}"));
            }
        } else if a == "-v" {
            opt.behavior.verbose = true;
        } else if a.starts_with('-') && a != "-" {
            return Err(format!("unknown option: {a}"));
        } else {
            positionals.push(a.clone());
        }
        i += 1;
    }
    Ok((opt, positionals))
}

fn apply_value_option(
    name: &str,
    value: Option<&String>,
    opt: &mut Options,
) -> Result<bool, String> {
    match name {
        "reportfile" => opt.output.report_file = Some(PathBuf::from(required_value(name, value)?)),
        "suffixes" => opt.input.suffixes = suffix_list(required_value(name, value)?),
        "exclude" => opt.input.exclude = split_list(required_value(name, value)?),
        "only" => opt.rules.only = Some(split_list(required_value(name, value)?)),
        "enable" => opt.rules.enable = Some(split_list(required_value(name, value)?)),
        "disable" => opt.rules.disable = split_list(required_value(name, value)?),
        "minimumpriority" => {
            opt.rules.min_priority =
                parse_priority("--minimumpriority", required_value(name, value)?)?
        }
        "maximumpriority" => {
            opt.rules.max_priority =
                parse_priority("--maximumpriority", required_value(name, value)?)?
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn required_value<'a>(name: &str, value: Option<&'a String>) -> Result<&'a str, String> {
    value
        .map(String::as_str)
        .ok_or_else(|| format!("missing value for --{name}"))
}

fn apply_flag_option(name: &str, opt: &mut Options) -> bool {
    match name {
        "verbose" => opt.behavior.verbose = true,
        "ignore-tests" => opt.input.ignore_tests = true,
        "strict" => opt.behavior.strict = true,
        "ignore-errors-on-exit" => opt.behavior.ignore_errors = true,
        "ignore-violations-on-exit" => opt.behavior.ignore_violations = true,
        "color" => opt.output.color = true,
        _ => return false,
    }
    true
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

fn run_analysis(opt: Options, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if !is_known_format(&opt.output.format) {
        let _ = writeln!(
            stderr,
            "error: unknown report format {}. Available: {}",
            opt.output.format,
            formats().join(", ")
        );
        return EXIT_ERROR;
    }

    let (files, rules) = match prepare_analysis(&opt, stderr) {
        Ok(prepared) => prepared,
        Err(e) => {
            let _ = writeln!(stderr, "error: {e}");
            return EXIT_ERROR;
        }
    };

    let report = analyze_files(&files, &rules, opt.behavior.strict, opt.input.ignore_tests);

    let target = match &opt.output.report_file {
        Some(path) => WriteTarget::File(path.clone()),
        None => WriteTarget::Stdout,
    };

    if let Err(e) = render(
        &opt.output.format,
        &report,
        opt.output.color,
        target,
        stdout,
    ) {
        let _ = writeln!(stderr, "error: {e}");
        return EXIT_ERROR;
    }

    exit_code_for(
        &report,
        opt.behavior.ignore_errors,
        opt.behavior.ignore_violations,
    )
}

fn prepare_analysis(
    opt: &Options,
    stderr: &mut dyn Write,
) -> Result<(Vec<PathBuf>, Vec<ruleset::LoadedRule>), String> {
    let only = opt
        .rules
        .only
        .clone()
        .or(opt.rules.enable.clone())
        .unwrap_or_default();
    let load_opts = LoadOptions {
        min_priority: opt.rules.min_priority,
        max_priority: opt.rules.max_priority,
    };
    let verbose = opt.behavior.verbose;
    let mut warn = |message: String| {
        if verbose {
            let _ = writeln!(stderr, "warning: {message}");
        }
    };
    let rules = load_and_filter(
        &opt.rules.rulesets,
        &only,
        &opt.rules.disable,
        &load_opts,
        &mut warn,
    )?;
    let discover_opts = DiscoverOptions {
        suffixes: source_suffixes(&opt.input.suffixes),
        exclude: opt.input.exclude.clone(),
        ignore_tests: opt.input.ignore_tests,
    };
    let files = discover(&opt.input.paths, &discover_opts)?;
    Ok((files, rules))
}

fn source_suffixes(configured: &[String]) -> Vec<String> {
    if configured.is_empty() {
        vec![".rs".to_string()]
    } else {
        configured.to_vec()
    }
}
