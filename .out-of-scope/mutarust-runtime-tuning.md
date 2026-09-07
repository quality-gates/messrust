# Mutarust Runtime Tuning

This project does not maintain or optimize the execution engine of the `mutarust` mutation testing tool.

## Why this is out of scope

`messrust` is a consumer of `mutarust`, not its upstream codebase. Runtime performance, coverage-instrumentation profiling, process pooling, and build-artifact caching belong to the `quality-gates/mutarust` repository.

Within `messrust`, the long execution times on large files were caused by monolithic module architecture in `src/analyze.rs` (~1006 mutants). That issue was resolved internally by decomposing `src/analyze.rs` into smaller, focused modules under `src/analyze/` in issue #51 (pull request #52). Subsequent mutation engine optimizations were implemented upstream in `quality-gates/mutarust` across issues #117 through #122.

Modifications to upstream tooling should be proposed and implemented directly in `quality-gates/mutarust`.

## Prior requests

- #50: "Improve mutarust runtime so large-file mutation runs finish in CI time"
