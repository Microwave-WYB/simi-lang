# Simi Agent Guide

This file is the shared guide for coding agents working on Simi. Keep changes consistent with the language described here. If implementation, tests, and this document disagree, investigate the conflict rather than silently choosing a new semantic rule.

## Project Purpose

Simi is a small, embeddable scripting language implemented in Rust. It combines a Lua-inspired dynamic runtime with expression-oriented control flow, pipelines, functional loops, structural pattern matching, and value-based errors.

The language is intended to be:

- small enough to understand and embed;
- dynamic and convenient for scripts;
- predictable about mutation, aliasing, and errors;
- expressive without requiring statements for every control-flow operation;
- close to Lua where Lua already provides a simple, proven runtime rule;
- willing to differ from Lua where Simi's expression-first and pattern-oriented design benefits.

Simi is not intended to reproduce Lua syntax or behavior exactly.

## Current Language Overview

### Values

Simi currently has these runtime value categories:

- integers;
- finite floating-point numbers;
- strings;
- booleans;
- `nil`;
- mutable lists;
- mutable tables;
- user functions;
- native functions.

The language is dynamically typed. Optional static typing may be explored later, but current runtime behavior must not depend on a future type system.

### Functions and expressions

Functions use expression-valued bodies and capture lexical environments. Recursion and closures are supported.

```simi
fn add(left, right) do
    left + right
end
```

Blocks and control-flow constructs evaluate to values. `if` supports `elseif` and an optional `else`; a missing branch evaluates to `nil`.

```simi
let label = if score >= 90 then
    "excellent"
elseif score >= 60 then
    "passing"
else
    "retry"
end
```

### Bindings and assignment

`let` introduces a binding. Assignment updates the nearest existing lexical binding and evaluates to its right-hand value.

```simi
let count = 1
count = count + 1
```

Assigning to an undefined name is a hard runtime error. Assignment is right-associative.

Field and index assignments mutate containers:

```simi
table.answer = 42
table[key] = value
values[index] = value
```

### Lists

Lists are mutable, ordered, and zero-based.

```simi
let values = [ 10, 20, 30 ]
values[0]
```

A nonnegative out-of-range read returns `nil`. Negative and non-integer indices are hard runtime errors. A write never grows a list; an out-of-range write raises a structural value:

```simi
{ error = "index_out_of_bounds", index = index, length = length }
```

Ordinary aliases observe the same mutations. List-rest pattern captures create independent copy-on-write views in O(1), while nested values retain shallow alias identity.

### Tables

Tables are mutable, insertion-ordered key/value containers. Supported keys currently include strings, integers, finite non-integral floats, and booleans. Integral floats and signed zero normalize to integer keys.

```simi
let settings = {
    name = "Simi",
    [10] = "ten",
    [true] = "enabled",
}
```

Missing reads return `nil`. Following Lua, tables cannot retain `nil` values:

```simi
settings.name = nil       # deletes the key
settings[dynamic] = nil   # deletes the key
```

Nil-valued table literal entries are omitted. Thus `table[key] != nil` is a valid key-existence check for script-created tables.

Lists may contain `nil`; nil-as-deletion applies only to tables.

### Numbers and operators

Simi supports finite decimal and exponent floats, integer/float arithmetic, exact boundary-aware mixed numeric comparisons, and these operators:

```simi
+  -  *  /  //  %
==  !=  <  <=  >  >=
and  or  not
```

`/` always produces a float. `//` and `%` follow Lua floor-division semantics. Division by zero raises:

```simi
{ error = "division_by_zero" }
```

Boolean operators are strict and short-circuiting. Simi does not use Lua-style truthiness.

### Pipelines

A pipeline stage must be a call. The incoming value is inserted as the first argument.

```simi
value |> transform(extra)
```

`tap` performs a call while preserving the piped value, which is useful for mutation-oriented operations.

### Functional loops

Loops are expression-valued and may thread state. The final expression of an ordinary iteration supplies the next state. `continue value` performs an early transition, bare `continue` supplies `nil`, and `break value` determines the loop result.

Maintain the existing loop syntax and control-flow contracts in the parser and integration tests. Do not introduce conventional imperative-loop assumptions without an explicit language-design decision.

### Pattern matching

Simi has structural, expression-valued matching:

```simi
match value with
    case pattern when guard ->
        body
    case _ ->
        fallback
end
```

Patterns support literals, bindings, wildcards, nested list/table patterns, and list/table rests. Guards must evaluate to booleans. Bindings are scoped to the selected case.

Table fields normally require key presence. The literal nil field pattern is the exception: `{ missing = nil }` matches an absent field, consistent with table lookup and deletion semantics.

### Errors

Simi distinguishes language raises from hard implementation/runtime diagnostics.

Any value may be raised and structurally caught:

```simi
raise { error = "invalid_input", value = input }

try operation() catch
    case { error = "invalid_input", value = value } ->
        recover(value)
end
```

Generated structural errors use an `error` discriminator and may gain additional fields over time. Preserve stable discriminator strings.

Hard errors—such as invalid operand types, undefined assignment targets, or invalid list index types—remain outside language catches unless a deliberate semantic decision promotes them to raised values.

The host boundary is:

```rust
pub type ScriptResult = Result<Value, Raised>;
pub fn eval(source: &str) -> Result<ScriptResult, SimiError>;
```

Do not collapse raised values and hard diagnostics into one result layer.

### Managed graphs and rendering

Runtime lists, tables, functions, bindings, and environments use tracing garbage collection. Strong cycles are legal and unreachable cycles must be collectible.

Every new managed edge that can contain or reach a Simi value must participate in tracing. Do not hide managed values in untraced Rust containers.

`Value::render()` is a human-readable inspector, not a serializer. It detects active-path container cycles and displays recursive edges as `<cycle>`. Repeated acyclic aliases must still render fully.

Future serializers must define cycle behavior explicitly; JSON-like serialization should reject cycles or encode references rather than inheriting inspector behavior.

## Source and Formatting Conventions

Canonical source examples use:

```simi
{ a = x, b = y }
[ a, b, c ]
```

Empty forms remain `{}` and `[]`. Trailing commas are accepted in comma-separated constructs.

Rust module layout must use a facade file plus a same-named directory:

```text
src/parser.rs
src/parser/expression.rs
```

Never introduce `mod.rs`.

Keep public APIs narrow. A method named `get` should not raise for ordinary absence. Use names such as `deref` for potentially failing access.

Prefer cohesive modules over extending already-large files. Place native library implementations under `src/native/` with focused tests.

## Repository and Worktree Discipline

- `main` is the integration branch.
- Use focused feature branches and separate worktrees for parallel work.
- Have only one writer modify a given worktree.
- Avoid assigning parallel branches changes to the same facade or registration file when possible.
- Merge one feature at a time and validate after each merge.
- Keep commits focused and describe semantic changes in commit messages.
- Do not commit generated build output.
- `demo/` is intentionally ignored and is not part of the current tracked baseline.
- `.pi-subagents/` and `target/` are ignored.

Do not perform unrelated cleanup in a feature branch. If additional work appears necessary, report it or request scope explicitly.

## Required Validation

Before considering a change complete, run:

```bash
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --bin simi
find src -type f -name mod.rs
git diff --check
```

The `find` command must produce no output.

Add tests at the lowest useful layer and at the public language boundary when semantics are user-visible. GC and aliasing changes require targeted identity, mutation, and collection tests.

## Near-Term Direction

The next intended milestone is a practical non-higher-order standard library:

1. string operations;
2. list mutation and slicing operations;
3. table inspection utilities;
4. general utilities such as `type` and `inspect`.

Likely later milestones include anonymous functions, higher-order collection operations, CLI arguments and basic I/O, modules, formatting, optional static typing, and editor tooling.

These are roadmap items, not implemented features. Do not add them opportunistically outside an approved task.

Broader I/O, modules, serialization, formatter/LSP work, tuples, and static typing remain out of scope until explicitly requested.
