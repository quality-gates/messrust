# Exploratory testing report: messrust

Date: 2026-11-02. Base: `f230f1e` on `origin/main`. Binary: messrust
0.1.16, built with `cargo build --release --locked` in the development image.
The container had 2 CPUs, 4 GiB memory, and an isolated target tmpfs. The
[fixtures](fixtures) and [policy](policy.xml) were isolated from product code.
The [driver](run.sh) records commands, output, exit codes, and file contents in
[the evidence](evidence/journeys.txt). After a release build, run
`sh docs/exploratory-testing/2026-11-02-messrust/run.sh` in the development
container. The driver removes its own `scratch` directory on exit.

## Journeys

### Scan source and use a report in CI

Goal: find unused locals, exclude test code, audit suppressions, and write JSON
for a CI consumer. The default scan found the production local on line 2 and
the test local on line 10. It omitted the suppressed local and exited 2.
With `--ignore-tests --strict`, the report omitted the test local and included
the suppressed local on line 4 with a suppression marker. The JSON report file
contained the production finding. `--ignore-violations-on-exit` returned 0
without removing the finding. Stdout was empty when the report went to a file.

### Apply a team policy

Goal: set a parameter limit and compose it with a built-in policy. The custom
policy set the minimum to 4 and found the five-parameter function. The command
exited 2. When `codesize` came after the custom policy, its default won and the
command exited 0. Reversing the order restored the custom limit and exit 2.
This matches the documented later-reference rule.

### Diagnose a failed scan

Goal: scan a valid file and a malformed file, retain the valid findings, and
explain the processing error. Both JSON and SARIF returned exit 1 and retained
the valid file's findings. JSON named the malformed file and the parse error.
SARIF omitted the error from both the report and the command output. The driver
removed both report files before each of two replays. Both replays gave the same
result. This is the known [issue #100](https://github.com/quality-gates/messrust/issues/100),
not a new issue. A CI user can see a failed job but cannot identify the bad file
from the SARIF report or the command output.

To reproduce from the repository root in a development container with the
release binary, run:

```console
./target/release/messrust docs/exploratory-testing/2026-11-02-messrust/fixtures/broken.rs,docs/exploratory-testing/2026-11-02-messrust/fixtures/service.rs sarif rust --only UnusedLocalVariable --reportfile /tmp/errors.sarif
```

Expected: exit 1, valid findings, and a diagnostic that identifies the bad
file. Actual: exit 1 and valid findings, but no processing diagnostic in the
report, stdout, or stderr. Replace `sarif` with `json` to see the parse error.

## Findings and limits

Confirmed: issue #100. No new issue was filed because the existing issue covers
this behavior and its latest brief specifies the intended diagnostic. No other
candidate was confirmed or left unresolved in these journeys.

The pass did not test all rules, output formats, operating systems, or Cargo
installation. It did not run the full test suite. It did not change product
code. The SARIF output is usable for valid findings, but it gives no location
or message for a malformed source file. The exit code alone does not explain
which source file needs repair.
