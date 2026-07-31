# codesize metrics (Rust adaptations)

`messrust` pins cyclomatic complexity and NPath to the same phpmd 2.15.0
reference numbers as messgo / messharp (CCN=12, NPath=324) when the fixture is
expressed as equivalent Rust control flow.

## Decision points (CCN)

Base 1, plus one for each of:

- `if` / `if let`
- `while` / `while let`
- `for`
- `loop` (Rust-only; treated like a loop decision)
- non-wildcard `match` arms (maps to switch case labels)
- match guards
- `&&` and `||`

A lone `_` match arm is the default and does **not** increment CCN (same as Go
`default` / PHP default case).

## Not counted

phpmd also counts `catch`, `??`, and ternary `?:`. Rust has no direct forms of
those; they are omitted rather than inventing fake findings.

## NPath

Nejmeh / pdepend formulas, with:

- `for` as foreach (`E(iter) + 1 + NP(body)`)
- `match` as switch (`E(scrutinee)` + sum of arm bodies, including `_`)
- `loop` as `1 + NP(body)`

The reference fixture omits a `_` arm so it matches the phpmd switch that has
no default (keeping NPath at 324). Real code that includes `_` will count that
arm in NPath.

## Type-like artifacts

| phpmd “class” idea | Rust |
| --- | --- |
| Class / struct | `struct`, `union`, `enum`, `trait` |
| Fields (`TooManyFields`) | `struct` / `union` fields only (not enum variants) |
| Methods | inherent and trait `impl` methods; trait method items |
| Public | `pub` fields/methods; trait methods only if the trait is `pub` |

`ExcessiveClassLength` sums the type span plus inherent method spans (parallel
to messgo’s struct + methods LOC).
