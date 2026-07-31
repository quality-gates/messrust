# messrust

**messrust** is a [PHP Mess Detector](https://phpmd.org) (phpmd) port for Rust: it
is written in Rust *and* analyzes Rust source code, applying phpmd's rule catalog,
ruleset format, message templates, CLI surface, and report renderers — adapted
to Rust semantics.

This is a sibling of [messgo](https://github.com/quality-gates/messgo),
[messcript](https://github.com/quality-gates/messcript), and the other
quality-gates mess detectors.

## Status

CLI, report formats, ruleset loading, and the full `codesize` rule catalog are
in place. Other component rulesets still load as stubs until later tickets.

Complexity metrics follow the quality-gates phpmd 2.15.0 pins; see
[docs/codesize-metrics.md](docs/codesize-metrics.md) for Rust adaptations.

