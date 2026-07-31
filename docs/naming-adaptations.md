# naming adaptations (Rust)

`messrust` ports the phpmd naming catalog onto Rust roles. Rule class names stay
phpmd-compatible; messages and defaults follow Rust idioms.

## Type names (`ShortClassName` / `LongClassName`)

Apply to `struct`, `enum`, `trait`, and `union` names — not PHP/Java “class”
metaphors invented for Rust.

## Variables (`ShortVariable` / `LongVariable`)

Cover named fields, function parameters, and local bindings.

**Loop binders skipped:** `for` pattern binders and `while let` pattern binders
(Rust analogue of phpmd’s for-initializer skip) for both ShortVariable and
LongVariable. `if let` and `match` bindings are still checked.

## Methods (`ShortMethodName` / `BooleanGetMethodName`)

Cover free functions and inherent/trait methods.

`BooleanGetMethodName` flags names that start with `get` / `Get` and return
`bool`. Prefer `is_...` / `has_...`. With the default
`checkParameterizedMethods=false`, methods that take non-`self` parameters are
exempt.

## Constants (`ConstantNamingConventions`)

Covers `const` and `static` items (including associated consts).

**Default convention is `upper`** (SCREAMING_SNAKE_CASE), matching Rust and
clippy’s `non_upper_case_globals`. Set `convention=pascal` for PascalCase
(no underscores).

## Documented no-op

`ConstructorWithNameAsEnclosingClass` is omitted from the live catalog (XML
comment only). Rust has no PHP-style named constructor; inventing findings
would be PHP-class noise. See the comment in `rulesets/naming.xml`.
