# Code Size and Complexity Rules for Rust

Use these rules to find code that is difficult to understand, test, or change.
The rules measure functions and Rust types. They do not build or run the crate.

## Start with the recommended policy

```console
messrust src text rust --ignore-tests
```

The `rust` ruleset includes all code size rules. To see only the main function
complexity findings, use:

```console
messrust src text rust --ignore-tests \
  --only CyclomaticComplexity,NPathComplexity,ExcessiveMethodLength
```

Start with the largest values. Do not split code only to make a number smaller.
First identify the separate decisions or responsibilities. Then extract a
function or type that has a clear name and interface.

## Rule summary

| Rule | Default | What it shows |
| --- | ---: | --- |
| `CyclomaticComplexity` | Report at 10 | A function has many decision points. |
| `NPathComplexity` | Report at 200 | A function has many possible paths. |
| `ExcessiveMethodLength` | Report at 100 lines | A function or method is large. |
| `ExcessiveClassLength` | Report at 1,000 lines | A Rust type and its inherent methods are large. |
| `ExcessiveParameterList` | Report at 10 parameters | A function needs too much input. |
| `ExcessivePublicCount` | Report at 45 public items | A type has a large public interface. |
| `TooManyFields` | Report over 15 fields | A struct or union stores many separate values. |
| `TooManyMethods` | Report over 25 methods | A type has many non-accessor methods. |
| `TooManyPublicMethods` | Report over 10 public methods | A type exposes many operations. |
| `ExcessiveClassComplexity` | Report at 50 | The methods on one type have high total complexity. |

`ExcessiveMethodLength`, `ExcessiveClassLength`, `ExcessiveParameterList`,
`ExcessivePublicCount`, `CyclomaticComplexity`, `NPathComplexity`, and
`ExcessiveClassComplexity` report when the value is equal to the threshold.
The three `TooMany...` rules report only when the value is greater than the
configured maximum.

## How to use each result

### Function complexity

`CyclomaticComplexity` is useful when one function contains many decisions.
A high value often means that error handling, policy, and data conversion are
in one function.

`NPathComplexity` estimates the number of paths through a function. It grows
faster than cyclomatic complexity when independent branches are in sequence.
A function can have an acceptable cyclomatic value and still have a high NPath
value.

For both rules, inspect these changes first:

1. Replace repeated conditions with one named predicate.
2. Return early for errors and special cases.
3. Move one independent operation to a small function.
4. Replace related booleans with an enum when the states are exclusive.
5. Use a `match` when it makes the state cases explicit.

### Function and type size

`ExcessiveMethodLength` counts the full source span of a free function or
method. By default, blank and comment-only lines count. Set
`ignore-whitespace=true` in a custom ruleset to exclude them.

`ExcessiveClassLength` applies to `struct`, `union`, `enum`, and `trait`. It
adds the type source span and the spans of methods attached to that type. For
a trait, the trait source span already contains its method declarations, and
the metric adds those method spans again.

A long item is not always wrong. Generated code, protocol tables, and direct
translations can be clear even when they are large. Use a source suppression
or a custom threshold for these cases.

### Interface size

`ExcessiveParameterList` counts typed function and method parameters. It does
not count a `self` receiver or a typed wildcard such as `_: Context`. A finding
can show that related values need a small input struct. Do not create a
parameter object if the values do not belong together.

`ExcessivePublicCount` counts public fields and public methods on a type.
`TooManyPublicMethods` counts public methods only. A large public interface is
hard to change because more callers can depend on it.

Trait methods count as public only when the trait is public.

### Type size and responsibility

`TooManyFields` applies to struct and union fields. Enum variants do not count
as fields for this rule.

`TooManyMethods` counts inherent methods and methods declared on a trait. It
does not count methods in `impl Trait for Type` blocks. By default, names that
start with `set`, `get`, `is`, `has`, or `with` do not count.
`TooManyPublicMethods` uses the same name filter.

`ExcessiveClassComplexity` is the weighted method count. It adds the
cyclomatic complexity of all methods on a type. A finding can show that a type
owns too many decisions even when each method is not large by itself.

## Exact complexity model

Cyclomatic complexity starts at 1 for a function. It adds 1 for each:

- `if` or `if let`;
- `while` or `while let`;
- `for`;
- `loop`;
- non-wildcard `match` arm;
- match guard;
- `&&` or `||`.

A lone `_` match arm is the default arm and does not increase cyclomatic
complexity. Rust has no direct form of PHP `catch`, `??`, or `?:`, so these
items do not add a value.

NPath uses the Nejmeh and pdepend formulas:

- `for`: expression paths plus 1 plus body paths;
- `match`: scrutinee paths plus the sum of all arm body paths, including `_`;
- `loop`: 1 plus body paths.

The rules calculate one value for each free function and method in the file.
Cyclomatic complexity also visits nested function items and closures, so their
decision points add to the value of the enclosing function. NPath treats a
nested item statement as linear code.

## Set thresholds for your project

Use a custom XML ruleset when the defaults do not match the project. This
example creates a small code size policy:

```xml
<ruleset name="project code size">
  <rule ref="codesize/CyclomaticComplexity">
    <properties>
      <property name="reportLevel" value="12" />
    </properties>
  </rule>
  <rule ref="codesize/NPathComplexity">
    <properties>
      <property name="minimum" value="250" />
    </properties>
  </rule>
  <rule ref="codesize/ExcessiveMethodLength">
    <properties>
      <property name="minimum" value="120" />
      <property name="ignore-whitespace" value="true" />
    </properties>
  </rule>
</ruleset>
```

Run it with:

```console
messrust src text path/to/code-size.xml --ignore-tests
```

Commit the XML file so local checks and CI use the same limits. Change one
limit at a time. A baseline that has no unexplained findings is more useful
than a strict policy that the team always ignores.

## Suppress one intentional case

```rust
// messrust-disable-next-line ExcessiveMethodLength
fn generated_parser_table() {
    // Large generated body.
}
```

Use `--strict` to include suppressed findings in a review report.
