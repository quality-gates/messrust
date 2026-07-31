# cleancode adaptations (Rust)

`messrust` ports the phpmd cleancode catalog onto Rust control-flow and
expression idioms. Rule class names stay phpmd-compatible.

## Shared conventions

Body walks cover free functions and inherent/trait methods. Nested `fn` items
and closures are skipped (same nested-function skip as messcript), so findings
inside them are not attributed.

## Live rules

### `BooleanArgumentFlag`

Flags function and method parameters whose type is bare `bool`.

Skips names that are `_` or start with `_`. Honours `exceptions` (comma-separated
enclosing type names for methods) and `ignorepattern` (phpmd-style regex on the
method/function name).

### `ElseExpression`

Flags a terminal `else` branch on `if`. Chains of `else if` alone do not fire;
only the final non-`if` else does (same as messcript).

### `IfStatementAssignment`

Flags `=` assignments that appear inside `if` / `while` conditions (including
condition blocks such as `if { x = 1; true }`).

`if let` / `while let` pattern bindings are not assignments and are not flagged.

### `DuplicatedArrayKey`

Flags duplicate field names in a struct literal (`ExprStruct`). Syn can parse
duplicates that rustc later rejects; the rule still reports them for catalog
parity. Map-macro literals are out of scope for syntax-only analysis.

### `StaticAccess`

Flags **calls** through a PascalCase type path other than the enclosing type,
e.g. `Helper::make()` inside `Worker`. Bare associated-item paths without a call
are not flagged. `Self::…` and snake_case module paths are not flagged. Honours
`exceptions` (type names) and `ignorepattern` (method name).

## Documented no-ops

These phpmd rules have no Rust analog and stay as XML comments only (same pattern
as `ConstructorWithNameAsEnclosingClass` in naming):

| Rule | Why omitted |
| --- | --- |
| `ErrorControlOperator` | Rust has no `@` error-suppression operator. |
| `MissingImport` | Unresolved paths are compile errors; `use` is not optional in the PHP sense. |
| `UndefinedVariable` | Unresolved binders are compile errors. |
