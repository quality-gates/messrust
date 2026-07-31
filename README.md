# messrust

**messrust** is a [PHP Mess Detector](https://phpmd.org) (phpmd) port for Rust: it
is written in Rust *and* analyzes Rust source code, applying phpmd's rule catalog,
ruleset format, message templates, CLI surface, and report renderers — adapted
to Rust semantics.

This is a sibling of [messgo](https://github.com/quality-gates/messgo),
[messcript](https://github.com/quality-gates/messcript), and the other
quality-gates mess detectors.

## Status

CLI, report formats, ruleset loading, and the full `codesize`, `naming`,
`unusedcode`, `cleancode`, `design`, and `controversial` rule catalogs are in
place. Policy rulesets (`rust`, `opinionated`) still land in later tickets.

Complexity metrics follow the quality-gates phpmd 2.15.0 pins; see
[docs/codesize-metrics.md](docs/codesize-metrics.md). Naming adaptations
(including the `ConstructorWithNameAsEnclosingClass` no-op) are in
[docs/naming-adaptations.md](docs/naming-adaptations.md). Unused-code
adaptations (single-file syntax analysis) are in
[docs/unusedcode-adaptations.md](docs/unusedcode-adaptations.md). Cleancode
adaptations (and PHP-only no-ops) are in
[docs/cleancode-adaptations.md](docs/cleancode-adaptations.md). Design
adaptations (including the quiet `GotoStatement` identity rule) are in
[docs/design-adaptations.md](docs/design-adaptations.md). Controversial
adaptations (PascalCase types, snake_case elsewhere; `Superglobals` omitted)
are in [docs/controversial-adaptations.md](docs/controversial-adaptations.md).

