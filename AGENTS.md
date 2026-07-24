# Simi Agent Guide

This file is the shared guide for coding agents working on Simi. Keep changes consistent with the language described here. If implementation, tests, and this document disagree, investigate the conflict rather than silently choosing a new semantic rule.

## Repository Skills

Before specialized work, inspect [`.agents/skills/`](.agents/skills/) and load the matching skill.

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

A standalone `do ... end` is a primary block expression with zero or more items. It evaluates in a fresh child scope to its last item's value, or to `nil` when empty, and composes with postfix calls, field access, indexing, and `?`.

Postfix `?` passes a non-`nil` value through unchanged. A `nil` value stops the nearest lexically enclosing block and makes that block evaluate to `nil`. Every function body, standalone block, conditional branch, case or catch arm, try protected body, and loop body is such a boundary. Raises and hard diagnostics are unaffected, and nil propagation from a protected try body bypasses catches. In a functional loop, the body block's ordinary value supplies the next state, so propagation directly from that body is equivalent to `continue nil`, not `break nil`; only `break value` determines the loop expression's result. The canonical Rust parser rejects `?` at the operator only when there is no enclosing block, while the Tree-sitter editor grammar may parse that form permissively for editor recovery. The same recovery policy applies to incomplete parenthesized post-state types: the canonical parser and LSP require the parameter list to be followed by `->`, while Tree-sitter may retain a recoverable type node before the signature is finished.

### Bindings and assignment

`let` introduces bindings. Its left side may be any existing structural pattern; a refutable pattern is an assertion and a mismatch is a hard runtime error. The right side is evaluated once, matching is atomic, and bindings are installed only after the complete pattern succeeds.

```simi
let count = 1
let [first, second, ..rest] = values
let { name = name, ..settings } = user
count = count + 1
```

Use `case` when pattern failure is expected and requires recovery. Assignment updates the nearest existing lexical binding and evaluates to its right-hand value. Assigning to an undefined name is a hard runtime error. Assignment is right-associative.

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

Ordinary aliases observe the same mutations. List-rest pattern captures, `list.slice`, and `list.copy` create independent copy-on-write views in O(1), while nested values retain shallow alias identity. `list.copy` covers the source's full visible range; mutating either outer list detaches its backing as needed.

The standard `std/list` module provides list-specific mutation, shallow copying, slicing, inspection, and an O(1) snapshot iterator. Generic lazy traversal belongs to `std/iter`; its adapters include `map` and `filter`, while consumers include `to_list`, `fold`, `find`, `find_index`, `contains`, `any`, `all`, `each`, and predicate-based `count`. Iterators are single-pass and sticky after exhaustion. Predicates must return booleans, searches short-circuit and leave later elements unconsumed, callback raises propagate, and `each` returns `nil`. `map.iter` snapshots insertion-ordered `{ key = key, value = value }` entries.

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

Nil-valued map literal entries are omitted. Thus `map[key] != nil` is a valid key-existence check for script-created maps. `map.copy` creates an independent shallow copy in O(n), preserving normalized keys, insertion order, and aliases to nested values.

Lists may contain `nil`; nil-as-deletion applies only to maps.

### Numbers and operators

Simi supports finite decimal and exponent floats, integer/float arithmetic, exact boundary-aware mixed numeric comparisons, and these operators:

```simi
+  -  *  /  //  %
==  !=  <  <=  >  >=
and  or  not
<|  |>  ?>  ?
```

`/` always produces a float. `//` and `%` follow Lua floor-division semantics. Division by zero raises:

```simi
{ error = "division_by_zero" }
```

Boolean operators are strict and short-circuiting. Simi does not use Lua-style truthiness.

Portable conversion APIs follow source-type naming. `string.to_number(text)` accepts complete signed decimal integer and decimal/exponent float forms with no surrounding whitespace. Integer syntax returns an integer and never falls back to float on overflow; float syntax returns only finite floats. Overflow and malformed text return `nil`. `number.to_string(value)` accepts only integers and floats and uses canonical Simi numeric rendering, including a visible float marker for whole-valued floats. Strict string concatenation uses `<>`; `string.concat(left, right)` provides the pipeline-friendly call form.

Runtime categories are inspected with the shadowable builtin `type(value)` and ordinary equality:

```simi
type(value) == "integer"
type(callback) == "function"
```

Stable labels are `"nil"`, `"boolean"`, `"integer"`, `"float"`, `"string"`, `"list"`, `"map"`, and `"function"`. Both user and native functions produce `"function"`. Labels are ordinary string values, and these checks follow normal call, equality, precedence, and shadowing rules. There is no dedicated runtime-category operator or syntax.

### Pipelines

A `|>` or `?>` pipeline stage must be a call. The incoming value is inserted as the first argument.

The compound operators `|> tap` and `?> tap` perform their stage call for its effects, discard the call's result, and preserve the incoming value with the same alias identity. `tap` is part of these pipeline operators; it is not an identifier or callable function and cannot be used outside a pipeline stage.

```simi
value |> transform(extra)
value |> tap observe()
value ?> tap observe()
```

Binding the result of a tap pipeline does not create a copy: `let alias = values |> tap list.append(item)` makes `alias` and `values` denote the same mutated list.

`?>` follows the same stage-call and first-argument rules as `|>`, but a `nil` input skips that stage's callee and all arguments lazily. The compound `?> tap` operator preserves the skipped `nil`, while a non-`nil` input behaves like `|> tap`. The pipeline operators may mix, and nil-awareness is stage-local: `nil ?> skipped() |> classify()` still calls `classify(nil)`. Only ordinary `nil` triggers skipping; raises and hard diagnostics from the input or an active stage propagate normally.

Pass callbacks directly in the ordinary call form first:

```simi
values
|> list.iter()
|> iter.map(fn(value) do
    value * 2
end)
```

The right-associative trailing-argument operator `<|` is an optional alternative. It requires a call on its left, appends its right operand as exactly one final argument, and binds more tightly than pipelines. This lets a multiline callback end its own scope without a trailing closing parenthesis:

```simi
values
|> list.iter()
|> iter.map() <| fn(value) do
    value * 2
end
```

`operation(first) <| second <| third` is rejected because right associativity makes `second <| third` invalid.

### Modules and native extensions

Scripts acquire modules explicitly through the shadowable global `require` function:

```simi
let list = require("std/list")
list.length([ 1, 2, 3 ])
```

Normal/default interpreters and all `Engine` evaluations provide the shadowable globals `type(value)` and `inspect(value)` alongside `require`. The low-level `Interpreter::with_globals` constructor intentionally treats its environment as complete and does not add a prelude. `type` returns the stable reflective labels listed above, including `"function"` for both user and native functions. Detailed runtime diagnostics may still distinguish native functions. `inspect` is cycle-safe human-readable rendering, not serialization.

Modules are registered by the embedding host and cached per `Engine`. Repeated `require` calls return the same cached export value, and module state persists across evaluations performed by that engine. Separate engines have separate module registries. `Engine::new()` has no registered modules; `Engine::with_stdlib()` includes `std/list`, `std/map`, `std/iter`, `std/number`, and `std/string`. The root `eval` convenience function uses a fresh standard-library engine.

Text standard IO is one opt-in capability named `std/io`. The CLI registers it; `Engine::with_stdlib()` and root `eval` do not. Embedders can opt in with `Engine::builder().stdlib().stdio()`. It supplies `read_line`, `print`, `println`, `eprint`, and `eprintln`. Output functions accept strings only and flush automatically; other values require explicit `inspect`. EOF returns `nil`, and successful writes return `nil`. Failures from either the write or its automatic flush raise `{ error = "io_error", operation = operation, message = message }` using the originating operation name. Raw `read` and `write` remain deferred until Simi has bytes.

Rust extension crates can construct direct value modules with `Module::builder` or source-backed modules with `Module::source`; `host_value!` generates the common map-shaped private host value. Source modules are registered as source strings, evaluated lazily in a private environment, and cached per `Engine`; their final value is the module export. Before facade evaluation, `host` is bound to an arbitrary private Simi value supplied by Rust, conventionally a map of ordinary fixed-arity native functions and data. Facades may attach erased types to direct native aliases without call overhead, define additional Simi functions and state, and choose any final public value. Native callbacks may capture Rust state but must be `Send + Sync + 'static`; this prevents safe callbacks from capturing Simi's non-`Send` managed values as untraced edges. Do not weaken this boundary or implement `require` as a closure that captures managed module values.

Source-level documentation uses consecutive `---` comments for the following declaration and leading consecutive `----` comments for the module itself. Module documentation belongs at the start of the source (leading blank lines are allowed), remains distinct from the first declaration's documentation, and is surfaced when hovering a literal `require` target or a binding that still denotes the module value.

A missing module raises `{ error = "module_not_found", module = name }`. Circular lazy loading raises `{ error = "circular_module_dependency", module = name }`. A non-string module name, calling a missing or non-function host field, and invalid native function arguments are hard runtime errors. Filesystem and package discovery are not implemented; embedders register source strings explicitly.

### Functional loops

Loops are expression-valued and may thread state. The final expression of an ordinary iteration supplies the next state. `continue value` performs an early transition, bare `continue` supplies `nil`, and `break value` determines the loop result.

Maintain the existing loop syntax and control-flow contracts in the parser and integration tests. Do not introduce conventional imperative-loop assumptions without an explicit language-design decision.

### Pattern matching

Simi has structural, expression-valued matching:

```simi
case value
of pattern when guard do
    body
of _ do
    fallback
end
```

The canonical case grammar requires one or more `of` clauses, repeats `of` before each sibling clause, and uses one final `end` for the whole expression rather than a per-clause `end`. Patterns support literals, bindings, wildcards, nested list/map patterns, and list/map rests. Guards must evaluate to booleans. Bindings are scoped to the selected clause; its `do` body extends until the next `of` or the final `end`, so clauses remain whitespace-independent and may appear on one line.

Map patterns are closed by default: `{field = pattern}` rejects maps with any additional string or computed keys. Add `..` to allow additional keys or `..rest` to capture them. Named fields normally require key presence. The literal nil field pattern is the exception: `{missing = nil}` matches an absent field, consistent with map lookup and deletion semantics; without a rest marker, unrelated keys still make that closed pattern fail.

### Errors

Simi distinguishes language raises from hard implementation/runtime diagnostics.

Any value may be raised and structurally caught:

```simi
raise { error = "invalid_input", value = input }

try
    prepare()
    operation()
catch { error = "invalid_input", value = value } do
    recover(value)
catch error do
    raise error
end
```

The canonical try grammar requires one or more protected items followed by one or more `catch` clauses, repeats `catch` before each sibling clause, and uses one final `end` for the whole expression. The protected items evaluate as a block in a fresh child scope. Only a raise from that protected block is matched by the catches: nil propagation and hard diagnostics bypass them, while raises from catch guards or handler bodies escape rather than being considered by later catches.

Generated structural errors use an `error` discriminator and may gain additional fields over time. Preserve stable discriminator strings.

Hard errors—such as invalid operand types, undefined assignment targets, or invalid list index types—remain outside language catches unless a deliberate semantic decision promotes them to raised values.

For new APIs, classify failures consistently:

- expected data-dependent absence or failure, such as a missing search result, parse failure, or EOF, returns `nil`;
- programmer contract violations, such as wrong arity or argument types, are hard diagnostics;
- recoverable operational failures that need details, such as stream or module failures, raise structural values with stable `error` discriminators;
- application-defined failures use explicit `raise` values.

Do not use `nil` to hide contract violations or partially completed mutations. Apply this policy to new APIs without changing established behavior unless a separate compatibility decision approves the change.

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

Canonical source examples use compact delimiters with spaces after commas and around `=`:

```simi
{a = x, b = y}
[a, b, c]
```

Empty forms remain `{}` and `[]`. Trailing commas are accepted in comma-separated constructs.

When a multiline pipeline is the right-hand side of a binding, break after `=` and indent the continuation:

```simi
let doubled =
    values
    |> list.iter()
    |> iter.map(fn(value) do
        value * 2
    end)
    |> iter.to_list()
```

The language tour is split into stable, unnumbered topic files under `docs/language-tour/`. Its current order lives in `docs/language-tour/order.txt`; run `just docs tour` after changing pages, headings, or order to regenerate every page's shared contents and two-line Previous/Next navigation and to validate snippets and links. Each page lists its own title as plain text, links its own subsections, and links sibling topics. Tour examples use `simi` fences and must be independently complete; examples intended to demonstrate a static diagnostic begin with an `-- Expected type` comment.

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

The portable standard library currently includes `std/list`, `std/map`, `std/iter`, `std/number`, and `std/string`; `type` and `inspect` are globals. Anonymous functions, trailing callback application, and lazy single-pass iterators are implemented. The CLI additionally registers the opt-in `std/io` text module.

Rowan syntax, Salsa-backed lexical and type analysis, `simi-lsp`, and the VS Code/Zed adapters are implemented. The erased optional type system parses inline annotations and transparent aliases, infers stable body-based function and binding types, reports definite contradictions, and supplies typed hover/completion for source modules.

Likely later milestones include script-visible command-line arguments, filesystem/package module discovery, and formatting. These are roadmap items, not implemented features. Do not add them opportunistically outside an approved task.

The authoritative erased-type design is documented in [`docs/type-system.md`](docs/type-system.md). Its primitive static vocabulary is `never`, `nil`, `boolean`, `integer`, `float`, `string`, and `any`; numeric APIs use `integer | float`, never a special static `number` type. Annotations and aliases are erased and must not affect runtime semantics. Mutable parameter transitions use local `before => after` annotations, such as `fn append(xs: [..'a] => [..('a | 'b)], value: 'b) -> nil`; inline function types require an explicit parameter list, as in `([..'a] => [..('a | 'b)], 'b) -> nil`, and ambiguous unparenthesized forms are rejected.

Builtin `type(value) == "label"` comparisons remain the primitive runtime category check and may later be recognized by the analyzer for narrowing. Static `integer` will correspond to the existing runtime label `"integer"`; changing that label is a separate compatibility decision. `TypeIs` is not part of the initial type-system scope.

Filesystem/package module discovery, serialization, formatter work, runtime tuples, and advanced type features beyond the documented initial scope remain out of scope until explicitly requested.
