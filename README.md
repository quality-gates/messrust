# messrust

`messrust` is a syntax-only static analyser for Rust. It uses a PHPMD-style
rule catalogue, ruleset XML, report formats, and exit codes. It does not build,
run, or execute the code that it checks.

## Install

Install the released command with Cargo:

```text
cargo install messrust
```

For a local checkout:

```text
cargo build --release
./target/release/messrust src text rust --ignore-tests
```

The package and binary are both named `messrust`. A release is ready for
crates.io after `cargo package --locked` succeeds. Publishing is a manual
release step:

```text
cargo publish
```

## Command

```text
messrust <paths> <format> <ruleset[,ruleset...]> [options]
```

Examples:

```text
messrust src text rust
messrust src json rust --ignore-tests --reportfile messrust.json
messrust src sarif rust --minimumpriority 2
messrust src github rust --ignore-violations-on-exit
messrust src text path/to/custom-rules.xml --only ExcessiveMethodLength
```

`<paths>` is a comma-separated list of files or directories. Directories are
searched recursively for `.rs` files. The analyser skips `.git`, `target`, and
`node_modules`. The default ruleset is not implicit: pass `rust`, a component
ruleset, or an XML file.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | No findings and no processing errors. |
| `1` | A file or option error occurred. Errors take precedence. |
| `2` | Findings exist and no processing error takes precedence. |

`--ignore-errors-on-exit` removes errors from exit-code selection.
`--ignore-violations-on-exit` removes findings from exit-code selection. These
options do not remove errors or findings from reports.

## Formats

The available formats are `text`, `ansi`, `xml`, `json`, `html`, `github`,
`gitlab`, `checkstyle`, and `sarif`. Structured formats include stable file,
line, rule, priority, description, and suppression fields. Findings are sorted
by file and source line. `--reportfile <path>` writes the report to a file and
leaves standard output empty. `--color` colorizes text output.

## Options

| Option | Action |
| --- | --- |
| `--minimumpriority <n>` | Keep priorities `<= n`; `1` is highest. |
| `--maximumpriority <n>` | Keep priorities `>= n`. |
| `--reportfile <path>` | Write the report to a file. |
| `--suffixes <ext[,ext...]>` | Replace the default `.rs` suffix list. |
| `--exclude <text[,text...]>` | Skip paths containing a value. |
| `--only <rules>` / `--enable <rules>` | Keep only named loaded rules. |
| `--disable <rules>` | Remove named loaded rules. |
| `--ignore-tests` | Skip test files, test directories, and `#[cfg(test)]` modules. |
| `--strict` | Include findings suppressed by source comments. |
| `--color` | Colorize text output. |
| `--verbose`, `-v` | Print ruleset load diagnostics. |
| `--ignore-errors-on-exit` | Ignore processing errors for exit-code selection. |
| `--ignore-violations-on-exit` | Ignore findings for exit-code selection. |
| `--help`, `-h` | Print command help. |
| `--version` | Print the package version. |

## Rulesets

`rust` is the recommended ruleset. It combines the component rules and omits
checks that commonly conflict with Rust idioms, such as short local names,
terminal `else` expressions, and associated-function access.

`opinionated` contains the stricter checks omitted from `rust`. Use
`rust,opinionated` to run the complete policy. Use component rulesets when you
need a smaller scope:

| Ruleset | Scope |
| --- | --- |
| `rust` | Recommended Rust policy. |
| `opinionated` | Opt-in checks omitted from the recommended policy. |
| `codesize` | Complexity and size. |
| `naming` | Identifier names. |
| `unusedcode` | Unused declarations and values. |
| `cleancode` | Clean-code checks. |
| `design` | Design and coupling checks. |
| `controversial` | PascalCase types and snake_case members. |

Rulesets can be loaded from compatible PHPMD-style XML files. XML `ref`,
`exclude`, rule properties, priorities, `--only`, `--enable`, and `--disable`
are supported. Analysis remains syntax-only.

## Source suppressions

Use `messrust-` comments with one or more rule names. Names are not
case-sensitive and can be separated by commas or spaces:

```rust
// messrust-disable-next-line LongVariable
let deliberately_long_variable_name_for_a_fixture = 1;

// messrust-disable ElseExpression, StaticAccess
// ... a region with these findings suppressed ...
// messrust-enable ElseExpression, StaticAccess
```

`disable-next-line` suppresses the next physical source line. `disable` starts
a region on the following line. `enable` ends the named suppressions. By
default, suppressed findings are omitted. `--strict` includes them and marks
them as `suppressed: true` in structured reports and `[suppressed]` in text.

## CI

The project CI runs tests, builds the release binary, and analyses the project
with the default policy:

```yaml
- run: cargo test --all-targets --locked
- run: cargo build --release --locked
- run: ./target/release/messrust src text rust --ignore-tests
```

The self-analysis step has no baseline and no violation-ignore option. A
finding fails CI with exit code `2`.

## Rule adaptations

The component catalog is adapted to Rust syntax and semantics:

- [Code size](docs/codesize-metrics.md)
- [Naming](docs/naming-adaptations.md)
- [Unused code](docs/unusedcode-adaptations.md)
- [Clean code](docs/cleancode-adaptations.md)
- [Design](docs/design-adaptations.md)
- [Controversial rules](docs/controversial-adaptations.md)

## Development

Run the complete verification suite with:

```text
cargo test --all-targets --locked
```
