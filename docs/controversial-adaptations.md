# Rust Style Naming Rules

The PHPMD catalog calls this ruleset `controversial`. `messrust` keeps that
name for compatibility. Its live rules check normal Rust naming forms:
PascalCase for types
and snake_case for functions, methods, fields, parameters, and local bindings.

The recommended `rust` policy includes all of these rules. They are useful when
you want a fast source check without a build, or when one report format must
contain all quality findings.

## Run the checks

Use the recommended policy:

```console
messrust src text rust --ignore-tests
```

Use only this style component:

```console
messrust src text controversial --ignore-tests
```

Use one rule while you clean existing code:

```console
messrust src text controversial --ignore-tests --only CamelCaseMethodName
```

## Rule summary

| Rule | Checks | Required form |
| --- | --- | --- |
| `CamelCaseClassName` | Struct, enum, trait, and union names | `PascalCase` |
| `CamelCaseMethodName` | Free function and method names | `snake_case` |
| `CamelCasePropertyName` | Named fields | `snake_case` |
| `CamelCaseParameterName` | Function and method parameters | `snake_case` |
| `CamelCaseVariableName` | Local bindings | `snake_case` |

A bare `_` never produces a finding.

## Type names

`CamelCaseClassName` accepts PascalCase without underscores:

```rust
struct RequestQueue;
enum ConnectionState {
    Open,
    Closed,
}
trait MessageStore {}
```

With the default `camelcase-abbreviations=false`, the rule accepts a name such
as `HTTPClient`. Set the property to `true` when the project requires
Rust-style capitalization for abbreviations:

```rust
struct HttpClient;
struct HtmlParser;
```

This setting affects only consecutive uppercase letters. It does not define a
list of permitted abbreviations.

## Function and method names

`CamelCaseMethodName` checks free functions, inherent methods, and trait
methods:

```rust
fn read_config() {}

impl Worker {
    fn process_item(&self) {}
}
```

The PHPMD properties `allow-underscore` and `allow-underscore-test` remain in
the XML format for compatibility. They do not change findings because Rust
snake_case already permits leading and internal underscores.

## Field names

`CamelCasePropertyName` checks named fields. It skips tuple fields because they
do not have source names:

```rust
struct UserRecord {
    display_name: String,
}

struct UserId(u64);
```

## Parameter names

`CamelCaseParameterName` checks function and method parameters. It does not
check `self` receivers. It skips closure parameters.

```rust
fn send_message(message_id: u64) {}
```

An underscore-prefixed parameter can follow snake_case and can also state that
the value is intentionally unused:

```rust
fn callback(_request_id: u64) {}
```

## Local variable names

`CamelCaseVariableName` checks local bindings. It also checks names introduced
by `for` and `while let` patterns. The collector includes local bindings inside
a visited closure. It does not include closure parameter names.

```rust
let retry_count = 3;
for request_id in request_ids {
    send(request_id);
}
```

Fields and function parameters use their own rules, so one finding has one
clear source role.

## Configure abbreviation style

```xml
<ruleset name="project Rust style">
  <rule ref="controversial/CamelCaseClassName">
    <properties>
      <property name="camelcase-abbreviations" value="true" />
    </properties>
  </rule>
  <rule ref="controversial/CamelCaseMethodName" />
  <rule ref="controversial/CamelCasePropertyName" />
  <rule ref="controversial/CamelCaseParameterName" />
  <rule ref="controversial/CamelCaseVariableName" />
</ruleset>
```

```console
messrust src text path/to/rust-style.xml --ignore-tests
```

## Fix or suppress

Rename a private item. Then run the tests. For a public item, treat the rename
as an API change. Update downstream users as required.

Use a narrow suppression when an external protocol or generated interface
requires a non-Rust name:

```rust
// messrust-disable-next-line CamelCasePropertyName
externalFieldName: String,
```

Use `--strict` when you want the report to include suppressed findings.

## Rule with no Rust equivalent

`Superglobals` is not a live rule. Rust has no PHP `$_GET` or `$_POST`
equivalent, so `messrust` does not create a substitute finding.
