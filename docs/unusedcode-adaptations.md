# unusedcode adaptations (Rust)

`messrust` ports the phpmd unusedcode catalog with single-file, syntax-only
analysis (no crate graph / `rustc` resolve).

## Shared conventions

- Names that are `_` or start with `_` are skipped (Rust’s unused binder
  convention).
- Write-only sites do not count as a use. Assignment left-hand sides and
  compound-assign targets are ignored when collecting reads.

## `UnusedPrivateField`

Flags **private** (`Visibility::Inherited`) named fields that are never read in
this file via field access or field patterns.

`pub` and restricted visibility (`pub(crate)`, `pub(super)`, …) are not flagged.

## `UnusedLocalVariable`

Flags local binders from `let`, `if let`, `while let`, `for`, and `match`
arms that are never read as expressions in this file.

Honours the `exceptions` property (comma-separated names).

Function parameters are not covered here (see `UnusedFormalParameter`).

## `UnusedPrivateMethod`

Flags **private** inherent methods that are never called in this file via
method call (`self.m()`), path call (`Type::m()` / `Self::m()`), or bare
ident reference.

`pub` / restricted-visibility methods and trait-impl methods are not flagged.

## `UnusedFormalParameter`

Flags function and method parameters (excluding `self`) that are never read
in the body. Underscore names are skipped.
