# Naming Rules for Rust

Use these rules to find names that hide intent or do not follow the project
policy. The rules understand Rust roles such as types, bindings, functions,
constants, and statics.

Rust style checks for PascalCase and snake_case are in the
[`controversial` ruleset](controversial-adaptations.md). This document covers
name length, constant names, and boolean getter names.

## Start with the recommended policy

```console
messrust src text rust --ignore-tests
```

The recommended `rust` ruleset avoids short-name findings for normal Rust
bindings such as `i`, `x`, and `v`. It also raises the long-variable limit from
20 to 35 characters.

To run the naming component with its original defaults, use:

```console
messrust src text naming --ignore-tests
```

This command is stricter than `rust`. It includes `ShortVariable`, and it uses
a 20-character limit for `LongVariable`.

## Rule summary

| Rule | Component default | Recommended `rust` policy | Applies to |
| --- | ---: | --- | --- |
| `ShortClassName` | Fewer than 3 characters | Included | Structs, enums, traits, and unions. |
| `LongClassName` | More than 40 characters | Included | Structs, enums, traits, and unions. |
| `ShortVariable` | Fewer than 3 characters | In `opinionated` only | Fields, parameters, and local bindings. |
| `LongVariable` | More than 20 characters | Included with a limit of 35 | Fields, parameters, and local bindings. |
| `ShortMethodName` | Fewer than 3 characters | Included | Free functions and methods. |
| `ConstantNamingConventions` | Not SCREAMING_SNAKE_CASE | Included | `const`, `static`, and associated constants. |
| `BooleanGetMethodName` | `get...` returns `bool` | Included | Free functions and methods. |

Run the complete policy from all catalogs with:

```console
messrust src text rust,opinionated --ignore-tests
```

Use the `naming` component command when you want only the rules in this guide.

## Type names

`ShortClassName` and `LongClassName` apply to `struct`, `enum`, `trait`, and
`union` names.

A short type name is often correct when the domain already gives it a clear
meaning. Common examples are `Id`, `Io`, and a well-known protocol name. Add
these names to the `exceptions` property instead of making the name longer
without adding meaning.

A long type name can show that one type represents too many details. Before
you shorten it, check if the type must be split or moved to a module that gives
the name more context.

`LongClassName` can subtract one configured prefix and one configured suffix
from its length calculation. This is useful for required names such as code
generation prefixes.

## Variable names

`ShortVariable` and `LongVariable` cover:

- named fields;
- function and method parameters;
- local bindings.

Both rules skip binders introduced by `for` and `while let`. This permits
normal Rust forms such as `for i in items`. Binders from `if let` and `match`
still receive checks.

The recommended policy does not use `ShortVariable`. Short local names are
common and clear when their scope is small. Use the `opinionated` ruleset only
when your project needs a strict minimum.

Do not shorten a long name only to pass the rule. First remove words that the
module, type, or function already makes clear.

## Function and method names

`ShortMethodName` covers free functions, inherent methods, and trait methods.
Use its `exceptions` property for established Rust names such as `id` when the
short form is the domain term.

`BooleanGetMethodName` reports a function or method when both conditions are
true:

- its name starts with `get` or `Get`;
- its return type is `bool`.

Prefer a question name:

```rust
fn is_ready(&self) -> bool {
    self.ready
}

fn has_items(&self) -> bool {
    !self.items.is_empty()
}
```

By default, the rule skips a method that has parameters other than `self`. It
also skips a free function that has a parameter. Set
`checkParameterizedMethods=true` to check these functions and methods.

## Constants and statics

`ConstantNamingConventions` checks `const` and `static` items, including
associated constants. The default is SCREAMING_SNAKE_CASE:

```rust
const MAX_RETRIES: usize = 3;
static REQUEST_COUNT: AtomicUsize = AtomicUsize::new(0);
```

Set `convention=pascal` only when the project has a deliberate PascalCase
policy. The setting does not change Rust compiler or Clippy behavior.

## Configure a project policy

This example defines three naming rules:

```xml
<ruleset name="project naming">
  <rule ref="naming/ShortClassName">
    <properties>
      <property name="minimum" value="3" />
      <property name="exceptions" value="Id,Io,Db" />
    </properties>
  </rule>
  <rule ref="naming/LongVariable">
    <properties>
      <property name="maximum" value="35" />
      <property name="subtract-prefixes" value="generated_" />
      <property name="subtract-suffixes" value="_for_test" />
    </properties>
  </rule>
  <rule ref="naming/ShortMethodName">
    <properties>
      <property name="minimum" value="3" />
      <property name="exceptions" value="id,io" />
    </properties>
  </rule>
</ruleset>
```

Run it with:

```console
messrust src text path/to/naming.xml --ignore-tests
```

Commit the policy file. This makes the same exceptions and limits available to
each developer and to CI.

## Intentional exceptions

Use an exception property for a name that is valid across the project. Use a
source suppression for one local case:

```rust
// messrust-disable-next-line ShortClassName
struct Id(u64);
```

`ConstructorWithNameAsEnclosingClass` has no Rust equivalent. It is not a live
rule. Rust uses struct literals and associated functions such as `new`; it
does not use PHP-style named constructors.
