# design adaptations (Rust)

`messrust` ports the phpmd design catalog onto Rust idioms. Rule class names
stay phpmd-compatible.

## Shared conventions

Body walks cover free functions and inherent/trait methods. Nested `fn` items
and closures are skipped (same nested-function skip as other catalogs), so
findings inside them are not attributed.

## Live rules

### `ExitExpression`

Flags calls whose path ends in `exit` or `abort` (for example
`std::process::exit`, `process::abort`).

### `CountInLoopExpression`

Flags `.len()` / `.capacity()` method calls (and bare `len` / `capacity`
function calls) inside `while` conditions and `for` iterable expressions.

### `DevelopmentCodeFragment`

Flags default debug macros/functions: `println`, `print`, `eprintln`, `dbg`
(macro forms with `!` included). The `unwanted-functions` property adds more
names (comma-separated).

### `EmptyCatchBlock`

Rust has no try/catch. Flags empty `if let Err(…) = … {}` then-branches and
empty `Err(…)` match arms.

### `CouplingBetweenObjects`

Counts distinct non-builtin type names from fields, method parameters, and
return types on `struct` / `enum` / `union`. Default `maximum` is 13.
Primitives and `str` / `Self` are excluded.

### `GlobalVariable`

Flags `static mut` items that are mutated in the file (assign or
compound-assign). Immutable `static` and `const` are never flagged. Set
`report-immutable=true` to also flag un-mutated `static mut`.

### `LackOfCohesionOfMethods`

Computes LCOM4 per type: methods link when they share a field or call each
other via `self`. Trivial getters/setters are excluded; a call to a getter
counts as use of its field. Default `maximum` is 1 (report when above).

## Quiet identity rule

| Rule | Why quiet |
| --- | --- |
| `GotoStatement` | Rust has no `goto`. The rule stays loadable for catalog parity and never fabricates findings. |
