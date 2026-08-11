# Design Rules for Rust

Use these rules to find code that is difficult to test, code that hides errors,
and types that contain too many unrelated responsibilities. The rules use Rust
syntax only. They do not build the crate or resolve types across files.

## Start with the recommended policy

```console
messrust src text rust --ignore-tests
```

| Rule | In `rust` | Main concern |
| --- | --- | --- |
| `DevelopmentCodeFragment` | Yes | Debug output remains in production code. |
| `EmptyCatchBlock` | Yes | An error result is silently ignored. |
| `CouplingBetweenObjects` | Yes | A type names many dependencies. |
| `GlobalVariable` | Yes | Mutable static state is written. |
| `LackOfCohesionOfMethods` | Yes | A type has separate method groups. |
| `CountInLoopExpression` | No; in `opinionated` | A loop recalculates a length or capacity. |
| `ExitExpression` | No; in `opinionated` | Library code stops the process. |
| `GotoStatement` | Quiet compatibility rule | Rust has no `goto`, so it never reports. |

Run the complete policy from all catalogs with:

```console
messrust src text rust,opinionated --ignore-tests
```

Run only the design component with:

```console
messrust src text design --ignore-tests
```

## Error and process control

### `ExitExpression`

This rule reports calls whose path ends in `exit` or `abort`, such as
`std::process::exit` and `process::abort`.

Process exit is normal in a small `main` function. It is difficult to test in
library and application logic because it prevents the caller from handling the
failure. Prefer to return `Result` or an exit status to `main`.

The rule is in `opinionated` because direct exit calls can be correct at a
program boundary.

### `EmptyCatchBlock`

Rust has no `try` and `catch` syntax. This rule reports the closest error
handling forms when their body is empty:

```rust
if let Err(_error) = save() {}

match save() {
    Ok(()) => continue_work(),
    Err(_error) => {}
}
```

Handle or return the error. You can also record it in the project log. If the
code must ignore it, add a narrow suppression and a comment with the reason:

```rust
// A missing optional cache file is safe during the first start.
// messrust-disable-next-line EmptyCatchBlock
if let Err(_error) = load_optional_cache() {}
```

Use `--strict` to include this suppressed finding in a report.

## Loop work

### `CountInLoopExpression`

This rule reports these calls when they occur in a `while` condition or a
`for` iterable expression:

- `.len()`;
- `.capacity()`;
- a function path whose final part is `len`, such as `len(...)` or
  `helper::len(...)`;
- a function path whose final part is `capacity`.

A loop that changes the collection can make its limit difficult to reason
about. A loop that does not change the collection can still repeat the call.
Bind a stable limit before the loop when that keeps the behavior correct:

```rust
let item_count = items.len();
let mut index = 0;
while index < item_count {
    process(&items[index]);
    index += 1;
}
```

Do not cache a value when the loop must observe a changing length. This rule is
in `opinionated` because many length calls are cheap and intentional.

## Development output

### `DevelopmentCodeFragment`

The default names are:

- `println` and `println!`;
- `print` and `print!`;
- `eprintln` and `eprintln!`;
- `dbg` and `dbg!`.

A finding can show debug output that a developer forgot to remove. It can also
show deliberate command-line output. For a CLI, put user output in `main` or a
dedicated output module. Suppress or configure the rule in that location.

Use `unwanted-functions` to add project-specific function or macro names.
Names are comma-separated.

## Type dependencies

### `CouplingBetweenObjects`

This rule counts distinct non-builtin type names used by a `struct`, `enum`, or
`union` in:

- fields;
- method parameters;
- method return types.

Primitive types, `str`, and `Self` do not count. The collector records the
last name of the outer type path. For example, `Vec<Service>` records `Vec`,
but it does not also record `Service`.

The default threshold is 13, and a value of 13 produces a finding. A finding
can mean that a type coordinates too many services or owns too many data
concepts.

Check whether the dependencies form separate groups. If they do, move one
group behind a smaller type or interface. Do not hide dependencies in a tuple
or type alias only to reduce the number.

Because the analysis is syntax-only, the value is a source-level signal. It is
not a complete crate dependency graph.

## Mutable static state

### `GlobalVariable`

This rule reports a `static mut` item when the same file assigns or
compound-assigns it:

```rust
static mut REQUEST_COUNT: usize = 0;

fn record_request() {
    unsafe {
        REQUEST_COUNT += 1;
    }
}
```

Immutable `static` and `const` items never produce this finding. By default, an
unmodified `static mut` also does not produce a finding. Set
`report-immutable=true` to report all `static mut` declarations.

Prefer owned state, synchronization types, or an explicit context passed to
the code. The correct replacement depends on the lifetime and concurrency
requirements.

## Type cohesion

### `LackOfCohesionOfMethods`

This rule calculates LCOM4 for each struct, enum, and union. It does not check
traits. Two methods are connected when they:

- use the same field; or
- call each other through `self`.

The metric is the number of disconnected method groups. The default maximum
is 1, so a value above 1 reports a finding.

Trivial getters and setters are not graph nodes. A call to a getter counts as
use of its field. A stateless method with no field use and no sibling call is
ignored.

A finding can show a type with separate responsibilities:

```rust
struct Server {
    connections: usize,
    samples: Vec<u64>,
}

impl Server {
    fn accept(&mut self) {
        self.connections += 1;
    }

    fn record_sample(&mut self, sample: u64) {
        self.samples.push(sample);
    }
}
```

Do not split the type automatically. First confirm that the method groups have
separate reasons to change. Then consider a smaller field type or a separate
service.

## Configure project limits

```xml
<ruleset name="project design">
  <rule ref="design/CouplingBetweenObjects">
    <properties>
      <property name="maximum" value="10" />
    </properties>
  </rule>
  <rule ref="design/LackOfCohesionOfMethods">
    <properties>
      <property name="maximum" value="1" />
    </properties>
  </rule>
  <rule ref="design/DevelopmentCodeFragment">
    <properties>
      <property name="unwanted-functions" value="trace_value,dump_state" />
    </properties>
  </rule>
</ruleset>
```

```console
messrust src text path/to/design.xml --ignore-tests
```

Set limits from reviewed project examples. A limit must identify code that a
developer can improve; it must not only reduce a score.

## Suppress one intentional case

```rust
// messrust-disable-next-line DevelopmentCodeFragment
println!("{report}");
```

Use a source suppression for one case. Use a custom ruleset when the same rule
needs a project-wide limit or function list. Use `--strict` to include
suppressed findings in a report.

## Walk boundaries

Body checks cover free functions, inherent methods, and trait methods.
`messrust` skips nested functions and closures. It does not assign their
contents to the enclosing function.
