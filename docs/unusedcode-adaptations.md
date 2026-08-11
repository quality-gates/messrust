# Unused Code Rules for Rust

Use these rules to find unused private fields, local bindings, private
methods, and parameters.

`messrust` uses single-file, syntax-only analysis. It does not use the crate
graph, name resolution, macro expansion, or type checking. This makes the
check fast and independent of the build, but it also limits what an unused
finding can prove.

## Start with the recommended policy

```console
messrust src text rust --ignore-tests
```

The recommended `rust` ruleset includes the first three rules below. It omits
`UnusedFormalParameter` because unused parameters are common in trait and
callback interfaces.

| Rule | In `rust` | What it reports |
| --- | --- | --- |
| `UnusedPrivateField` | Yes | A private named field has no read in the file. |
| `UnusedLocalVariable` | Yes | A local binder has no read in the file. |
| `UnusedPrivateMethod` | Yes | A private inherent method has no call in the file. |
| `UnusedFormalParameter` | No; in `opinionated` | A parameter name has no read in the file. |

Run the complete policy from all catalogs with:

```console
messrust src text rust,opinionated --ignore-tests
```

Run only this component with:

```console
messrust src text unusedcode --ignore-tests
```

## Read a finding correctly

A finding means that `messrust` did not find a direct field access, method
call, identifier read, field pattern, or recognized macro token in the source
file. Before you delete code, check these cases:

1. A descendant module in another file can use the private item.
2. A macro can generate or hide the use.
3. A framework can require a parameter name or method shape.
4. A test can use the item when you run with `--ignore-tests`.
5. A write can be deliberate even when there is no read.

If none of these cases applies, remove the unused item. Then run the crate
tests. The Rust compiler remains the authority for name resolution and type
checks.

## Shared Rust conventions

A name equal to `_`, or a name that starts with `_`, does not produce an
unused finding. Rename an intentionally unused binding instead of suppressing
the rule:

```rust
fn handle(_context: &Context, request: Request) {
    process(request);
}
```

The target of a simple `=` assignment is not a read. A compound assignment,
such as `retries += 1`, reads the old value, so it counts as a read.

The collector matches names across the complete file. It does not resolve each
name to one declaration. A read of the same local, parameter, field, or method
name in another scope can prevent a finding.

## `UnusedPrivateField`

This rule checks private named fields with inherited visibility. It does not
report these fields:

- `pub` fields;
- `pub(crate)`, `pub(super)`, and other restricted fields;
- tuple fields;
- fields on a type that derives `Serialize` or `Deserialize`;
- fields that a supported field access or field pattern reads in the file.

Example:

```rust
struct Worker {
    active_jobs: usize,
    old_label: String, // Finding when the file never reads this field.
}
```

A constructor write does not count as a read. If code only initializes a
field, confirm that the field is still required.

## `UnusedLocalVariable`

This rule checks binders introduced by:

- `let`;
- `if let`;
- `while let`;
- `for`;
- `match` arms.

Function and method parameters are not part of this rule. Use
`UnusedFormalParameter` for them.

The `exceptions` property accepts a comma-separated list of local names. Use
it for a stable project convention. Prefer an underscore prefix for a single
normal Rust binding.

## `UnusedPrivateMethod`

This rule checks private inherent methods. It does not report:

- public or restricted-visibility methods;
- trait implementation methods;
- methods called in the same file as `self.run()`;
- methods called in the same file as `Type::run()` or `Self::run()`;
- methods referenced by a supported bare identifier in the same file.

Because the analysis is per file, inspect calls from other modules before you
delete a method.

## `UnusedFormalParameter`

This rule checks function and method parameters, but not `self`. It reports a
parameter when the file-wide read set does not contain that parameter name. A
read of the same name in another function can prevent a finding.

The rule is in `opinionated`, not `rust`. Enable it when the project controls
the function signatures. Keep it disabled when an external interface requires
the parameter. If you enable the rule, prefix a required unused parameter with
an underscore.

## Configure an exception

This policy runs only the local-variable rule and permits one required name:

```xml
<ruleset name="project unused code">
  <rule ref="unusedcode/UnusedLocalVariable">
    <properties>
      <property name="exceptions" value="required_marker" />
    </properties>
  </rule>
</ruleset>
```

```console
messrust src text path/to/unused-code.xml --ignore-tests
```

## Suppress one intentional case

```rust
// messrust-disable-next-line UnusedPrivateField
compatibility_marker: u32,
```

Use the narrowest exception. A project-wide exception can hide a later defect
that uses the same name.
