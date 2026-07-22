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
- mutable maps;
- user functions;
- native functions.

The language is dynamically typed. Optional static typing may be explored later, but current runtime behavior must not depend on a future type system.

### Functions and expressions

Functions use expression-valued bodies and capture lexical environments. Recursion and closures are supported. Named functions are declarations, while anonymous functions use `fn(parameters) do body end` anywhere an expression is accepted.

```simi
fn add(left, right) do
    left + right
end

let add_one = fn(value) do
    value + 1
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

`let` introduces bindings. Its left side may be any existing structural pattern; a refutable pattern is an assertion and a mismatch is a hard runtime error. The right side is evaluated once, matching is atomic, and bindings are installed only after the complete pattern succeeds.

```simi
let count = 1
let [first, second, ..rest] = values
let { name = name, ..settings } = user
count = count + 1
```

Use `match` when pattern failure is expected and requires recovery. Assignment updates the nearest existing lexical binding and evaluates to its right-hand value. Assigning to an undefined name is a hard runtime error. Assignment is right-associative.

Field and index assignments mutate containers:

```simi
map.answer = 42
map[key] = value
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

Ordinary aliases observe the same mutations. List-rest pattern captures and `list.slice` create independent copy-on-write views in O(1), while nested values retain shallow alias identity.

The standard `list` module provides mutation, slicing, inspection, and higher-order operations. In addition to `map`, `filter`, and `fold`, its Gleam-inspired query surface includes `find`, `find_index`, `any`, `all`, `each`, and predicate-based `count`. Higher-order operations iterate over a snapshot, invoke Simi or native callables through the active interpreter, and propagate callback raises. Predicates must return booleans. Searches and boolean queries short-circuit; `all([])` is true and `any([])` is false. `each` returns the original list alias after visiting the snapshot from left to right.

### Maps

Maps are mutable, insertion-ordered key/value containers. Supported keys currently include strings, integers, finite non-integral floats, and booleans. Integral floats and signed zero normalize to integer keys.

```simi
let settings = {
    name = "Simi",
    [10] = "ten",
    [true] = "enabled",
}
```

Missing reads return `nil`. Following Lua, maps cannot retain `nil` values:

```simi
settings.name = nil       # deletes the key
settings[dynamic] = nil   # deletes the key
```

Nil-valued map literal entries are omitted. Thus `map[key] != nil` is a valid key-existence check for script-created maps.

Lists may contain `nil`; nil-as-deletion applies only to maps.

### Numbers and operators

Simi supports finite decimal and exponent floats, integer/float arithmetic, exact boundary-aware mixed numeric comparisons, and these operators:

```simi
+  -  *  /  //  %
==  !=  <  <=  >  >=
and  or  not
<|  |>
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

The right-associative trailing-argument operator `<|` requires a call on its left and appends its right operand as exactly one final argument. It binds more tightly than `|>`, allowing callback-heavy pipelines without nested closing parentheses:

```simi
values
|> list.map() <| fn(value) do
    value * 2
end
```

`operation(first) <| second <| third` is rejected because right associativity makes `second <| third` invalid. `tap` performs a call while preserving the piped value, which is useful for mutation-oriented operations.

### Modules and native extensions

Scripts acquire modules explicitly through the shadowable global `require` function:

```simi
let list = require("list")
list.length([ 1, 2, 3 ])
```

Normal/default interpreters and all `Engine` evaluations provide the shadowable globals `type(value)` and `inspect(value)` alongside `require`. The low-level `Interpreter::with_globals` constructor intentionally treats its environment as complete and does not add a prelude. `type` returns stable strings for Simi's runtime value categories. `inspect` is cycle-safe human-readable rendering, not serialization.

Modules are registered by the embedding host and cached per `Engine`. Repeated `require` calls return the same mutable export map, and module state persists across evaluations performed by that engine. Separate engines have separate module registries. `Engine::new()` has no registered modules; `Engine::with_stdlib()` includes `list`, `map`, and `string`. The root `eval` convenience function uses a fresh standard-library engine.

Standard streams are separate opt-in capabilities named `std/io/stdin`, `std/io/stdout`, and `std/io/stderr`. The CLI registers them; `Engine::with_stdlib()` and root `eval` do not. Embedders can opt in with `Engine::builder().stdlib().stdio()`. Input supplies `read_line`; output streams supply `print`, `println`, and `flush`. Strings print raw while other values use inspector rendering. EOF returns `nil`, successful writes return `nil`, and stream failures raise `{ error = "io_error", operation = operation, message = message }`.

Rust extension crates construct modules with `Module::builder`. Module and export registration is infallible and last-wins. Native callbacks may capture Rust state but must be `Send + Sync + 'static`; this prevents safe callbacks from capturing Simi's non-`Send` managed values as untraced edges. Do not weaken this boundary or implement `require` as a closure that captures managed module values. Interpreter-aware standard list operations use private, data-free intrinsic variants rather than exposing the interpreter to host callbacks.

A missing module raises `{ error = "module_not_found", module = name }`. A non-string module name is a hard runtime error. Filesystem and script-source module loading are not implemented.

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

Patterns support literals, bindings, wildcards, nested list/map patterns, and list/map rests. Guards must evaluate to booleans. Bindings are scoped to the selected case.

Map fields normally require key presence. The literal nil field pattern is the exception: `{ missing = nil }` matches an absent field, consistent with map lookup and deletion semantics.

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

Runtime lists, maps, functions, bindings, and environments use tracing garbage collection. Strong cycles are legal and unreachable cycles must be collectible.

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

The portable standard library currently includes `list`, `map`, and `string`; `type` and `inspect` are globals. Anonymous functions, trailing callback application, and Gleam-inspired higher-order list queries are implemented. The CLI additionally registers the opt-in `std/io/*` standard-stream modules.

Likely later milestones include CLI arguments, filesystem/script module loading, formatting, optional static typing, and editor tooling. These are roadmap items, not implemented features. Do not add them opportunistically outside an approved task.

Filesystem/script module loading, serialization, formatter/LSP work, tuples, and static typing remain out of scope until explicitly requested.
