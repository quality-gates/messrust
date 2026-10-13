# Implicit inputs and outputs

A function has explicit inputs (its parameters) and one explicit output (its
return value). All other data that goes into or out of the function is
implicit. These rules use terms from Eric Normand's *Grokking Simplicity*.
They classify actions, calculations in a documented subset, and unknown
effects. A clean report is a calculation verdict only inside that subset.

The `explicitness` ruleset is opt-in. It is not part of `rust` or
`opinionated`, because `&mut` parameters are ordinary Rust.

## Start here

```console
messrust src text explicitness --ignore-tests
```

One rule:

```console
messrust src text explicitness --ignore-tests --only ImplicitOutput
```

## What each rule catches

| Rule | Reports |
| --- | --- |
| `ImplicitInput` | Reads of shared statics or mutable captured storage. Calls that read the environment, the clock, the file system, standard input, or a random source. |
| `ImplicitOutput` | Writes to shared statics, mutable captured storage, and borrowed parameters, including borrowed `self`. Print and log macros. Calls that change the file system, the environment, or the process. |
| `UnknownEffect` | Operations whose effects this file analysis cannot establish, including unresolved calls, methods, macros, callbacks, and custom implicit dispatch. |

A **shared static** is one of these:

- a `static mut`
- a `static` whose type contains `Atomic*`, `Cell`, `RefCell`, `UnsafeCell`,
  `Mutex`, `RwLock`, `OnceCell`, or `OnceLock`
- a `thread_local!` key

A plain immutable `static` and a `const` are constants. They are not reported.

A write to a shared static is an assignment, a compound assignment,
or a call to a method that changes the value, such as `store`, `fetch_add`,
`set`, `replace`, or `take`. Read-modify-write operations report both an input
and an output. In
`KEY.with(|value| ..)`, a write through `value` is a write to `KEY`.

Each distinct finding is reported one time for each function, at its first
line.

```rust
static HITS: AtomicUsize = AtomicUsize::new(0);

fn record(log: &mut Vec<String>, name: &str) {
    HITS.fetch_add(1, Ordering::Relaxed);       // ImplicitOutput: writes the global 'HITS'
    println!("{name}");                         // ImplicitOutput: uses 'println!'
    log.push(name.to_string());
}

fn record_explicit(log: &[String], name: &str) -> Vec<String> {
    let mut next = log.to_vec();
    next.push(name.to_string());
    next
}
```

## Stricter mode for methods

Writes through borrowed `self` are implicit outputs in the default mode.
Set `include-self` to `true` to also report each read of `self` as an implicit
input.

```xml
<?xml version="1.0" encoding="UTF-8" ?>
<ruleset name="strict-explicitness">
  <rule ref="explicitness/ImplicitInput">
    <properties><property name="include-self" value="true"/></properties>
  </rule>
  <rule ref="explicitness/ImplicitOutput">
    <properties><property name="include-self" value="true"/></properties>
  </rule>
</ruleset>
```

## Trait implementations

A trait method can have effects. Writes through its borrowed parameters are
reported. The `include-self` option also applies to trait methods.
A mutable reference alone does not prove that the method has an effect.

## Supported calculation subset

A function with no findings is treated as a calculation only when all these
conditions hold:

- The full callable body is in the analyzed file.
- Calls resolve to a same-file function, a same-file method with an established
  receiver type, a closure created at the call site, or a recognized standard
  operation.
- Standard library bindings are not shadowed or replaced.
- Values are ordinary built-in scalars, plain standard containers, records, or
  references that match their visible Rust type.
- Arithmetic, comparisons, construction, cloning, shallow copies, indexing,
  borrowing, and iteration use their normal standard-library meaning.
- Returned aliases and fresh allocations stay inside the file-visible flow.

The solver records a callable summary with parameter-relative reads, writes,
returned storage, ambient effects, and unknown operations. Calls substitute
actual argument storage into that summary and propagate the facts to a fixed
point. A helper that mutates a caller argument is an action for that caller.
The same helper can stay private when the caller passes a fresh local
allocation. A shallow copy has fresh outer storage, but shared reachable
elements can still be caller-owned.

Closures have their own execution owner. Creating or returning a closure does
not add its body effects to the factory. Calling the closure does add those
effects to the caller when the callable is resolved. An unresolved callback
call is an `UnknownEffect`. A known action and an unknown effect can be
reported for the same function.

Async functions and async blocks have their own execution owner. Creating or
dropping an unexecuted future does not run its body. Awaiting a known future
adds its effects at the await line. Standard iterator adapters keep their
callbacks deferred. A consumer or a `for` loop adds the callback effects at
the execution line.

Reads through explicit `Cell`, `RefCell`, and atomic parameters are explicit
inputs in this subset. A write through those parameters is an implicit output.
The subset assumes no concurrent writer.

## Limits

The analysis uses the parsed Rust file and its same-file model. It does not
build or type-check the crate.

- A static that is declared in a different file is not known.
- A call outside the analyzed same-file graph is unknown unless it is a
  recognized standard operation.
- Type annotations do not prove the runtime behavior of dynamic user objects or
  replacement bindings.
- Dynamic dispatch, trait-object calls, function pointers that cannot be traced,
  async scheduling, generators, spawned work, and macro arguments that cannot
  be parsed keep uncertainty.
- A method name alone does not prove purity for a custom type.
- Custom destruction, dereferencing, operators, indexing, and formatting keep
  uncertainty when their dispatch cannot be established.
- Value and effect propagation has finite limits. Flow that does not converge
  reports `UnknownEffect`.
- A nested `fn` item and a closure are separate execution owners. If the outer
  function calls them, their effects propagate to that caller.
- `lock`, `write`, and `borrow_mut` are not reported as writes by themselves.
  A later observed mutation through the returned guard or reference is reported.

## Exploratory testing

- [2026-09-23 report](exploratory-testing/2026-09-23-explicitness/REPORT.md)
