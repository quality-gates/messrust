# Clean Code Rules for Rust

Use these rules to find control flow and dependencies that can make code hard
to test or change. The catalog keeps PHPMD rule names, but each live rule has a
Rust-specific meaning.

## Start with the recommended policy

```console
messrust src text rust --ignore-tests
```

The recommended policy includes only the checks that usually give a clear
signal in Rust:

| Rule | In `rust` | Main concern |
| --- | --- | --- |
| `IfStatementAssignment` | Yes | A condition changes state. |
| `DuplicatedArrayKey` | Yes | A struct literal repeats a field. |
| `BooleanArgumentFlag` | No; in `opinionated` | One function selects behavior with a `bool`. |
| `ElseExpression` | No; in `opinionated` | A terminal `else` can hide a simpler flow. |
| `StaticAccess` | No; in `opinionated` | A method calls an associated function on another type. |

Run the complete policy from all catalogs with:

```console
messrust src text rust,opinionated --ignore-tests
```

Run only this component with:

```console
messrust src text cleancode --ignore-tests
```

The component command includes the opinionated rules and can produce more
findings than `rust`.

## `BooleanArgumentFlag`

This rule reports a bare `bool` parameter on a free function or method:

```rust
fn write_report(report: &Report, compact: bool) {
    // Two behavior paths.
}
```

A boolean parameter is useful when it represents data. It is a concern when it
selects one of two operations. In that case, use one of these fixes:

```rust
enum ReportLayout {
    Compact,
    Detailed,
}

fn write_report(report: &Report, layout: ReportLayout) {
    // The call states its intent.
}
```

You can also create two named functions when the operations have separate
responsibilities.

The rule skips `_` and underscore-prefixed parameters. The `exceptions`
property accepts enclosing type names for methods. The `ignorepattern`
property accepts a PHPMD-style regular expression for function and method
names.

This rule is opinionated because boolean options are normal in small Rust
interfaces.

## `ElseExpression`

This rule reports the final non-`if` branch in an `if` expression. A chain that
contains only `else if` branches does not report a finding.

Before:

```rust
fn load(path: &Path) -> Result<Data, Error> {
    if path.exists() {
        read(path)
    } else {
        Err(Error::Missing)
    }
}
```

Possible early-return form:

```rust
fn load(path: &Path) -> Result<Data, Error> {
    if !path.exists() {
        return Err(Error::Missing);
    }

    read(path)
}
```

Do not remove an `else` when the expression form is clearer. Rust uses `if` as
an expression, so this rule is in `opinionated`.

## `IfStatementAssignment`

Rust does not permit a C-style assignment expression as a condition. It does
permit an assignment inside a condition block. This rule reports that form in
`if` and `while` conditions:

```rust
if {
    next = read_next();
    next.is_some()
} {
    process(next);
}
```

Move the state change before the condition, or use `let`, `if let`, or
`while let` so the binding is explicit. The rule checks `=` assignments. It
does not check compound assignments such as `+=`. Pattern bindings in `if let`
and `while let` are not assignments and do not produce a finding.

## `DuplicatedArrayKey`

For Rust, this PHPMD-compatible rule checks duplicate named fields in a struct
literal:

```rust
Point { x: 1, y: 2, x: 3 }
```

Rust rejects this code during compilation. `messrust` still reports it because
syntax-only checks can run before a build and because the catalog keeps PHPMD
rule identity. Map macros are not checked because `messrust` does not expand
macros.

## `StaticAccess`

This rule reports a call through a PascalCase type path when the called type is
not the enclosing type:

```rust
impl Worker {
    fn run(&self) -> Output {
        Helper::make()
    }
}
```

It does not report:

- `Self::make()`;
- a snake_case module path;
- an associated-item path that is not called;
- a type listed in `exceptions`;
- a call in a method that matches `ignorepattern`.

A finding shows a direct dependency on the named type. To replace that
dependency in a test, pass a trait implementation, a function, or an owned
service to the enclosing type or method. Do not add an interface for stable
value constructors and standard utility types. Calls to these types are
common in Rust, so the rule is in `opinionated`.

## Walk boundaries

Body checks cover free functions, inherent methods, and trait methods.
`messrust` skips nested functions and closures. It does not assign their
contents to the enclosing function.

## Configure an opinionated rule

This example enables `StaticAccess` and permits two stable utility types:

```xml
<ruleset name="project static access">
  <rule ref="cleancode/StaticAccess">
    <properties>
      <property name="exceptions" value="PathBuf,Duration" />
      <property name="ignorepattern" value="(^build_)" />
    </properties>
  </rule>
</ruleset>
```

```console
messrust src text path/to/clean-code.xml --ignore-tests
```

## Intentional exceptions

```rust
// messrust-disable-next-line StaticAccess
let path = PathBuf::new();
```

Prefer a configured exception when the same type is valid throughout the
project. Prefer a source suppression when only one call is intentional.

## Rules with no Rust equivalent

These PHPMD rules are not live:

| Rule | Reason |
| --- | --- |
| `ErrorControlOperator` | Rust has no PHP `@` error-suppression operator. |
| `MissingImport` | Unresolved Rust paths are compile errors. |
| `UndefinedVariable` | Unresolved Rust bindings are compile errors. |

`messrust` does not create substitute findings for language features that Rust
does not have.
