# controversial adaptations (Rust)

`messrust` ports the phpmd controversial catalog onto Rust naming idioms.
Rule class names stay phpmd-compatible; messages and checks follow Rust style.

## Shared conventions

- **Types** (`struct`, `enum`, `trait`, `union`): PascalCase (no underscores).
- **Functions / methods / fields / parameters / locals**: snake_case
  (lowercase letters, digits, underscores; no uppercase).
- Bare `_` is never reported (Rust unused / wildcard binder).

## Live rules

### `CamelCaseClassName`

Flags type names that are not PascalCase. With
`camelcase-abbreviations=true`, consecutive uppercase letters (e.g. `HTTPClient`)
are also rejected; prefer `HttpClient`.

### `CamelCaseMethodName`

Flags free functions and inherent/trait methods that are not snake_case.
phpmd properties `allow-underscore` and `allow-underscore-test` remain in the
XML for catalog parity; snake_case already permits leading and internal
underscores, so those flags do not change findings.

### `CamelCasePropertyName`

Flags named fields that are not snake_case. Tuple fields (unnamed) are skipped.

### `CamelCaseParameterName`

Flags function/method parameters that are not snake_case. `self` / receivers
are not parameters for this check. Closure parameters are skipped (same nested
skip as other catalogs’ body walks).

### `CamelCaseVariableName`

Flags local bindings that are not snake_case. Fields and parameters are covered
by the property/parameter rules. Loop binders (`for` / `while let`) are still
checked when they introduce a local name. Locals inside closures are included
when the collector visits them; closure parameter names are not.

## Documented omission

`Superglobals` is omitted from the live catalog (XML comment only). Rust has no
PHP `$_GET` / `$_POST` analog; inventing findings would be PHP noise. See the
comment in `rulesets/controversial.xml`.
