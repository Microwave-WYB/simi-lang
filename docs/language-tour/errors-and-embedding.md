# Errors and embedding

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- [Optional types](optional-types.md)
- [Expressions](expressions.md)
- [Functions and bindings](functions-and-bindings.md)
- [Control flow and patterns](control-flow-and-patterns.md)
- [Mutation and copies](mutation-and-copies.md)
- [Modules](modules.md)
- [Text IO](text-io.md)
- [Iterators](iterators.md)
- Errors and embedding
  - [Choosing a failure channel](#choosing-a-failure-channel)
  - [Catching raised values](#catching-raised-values)
  - [Minimal Rust embedding](#minimal-rust-embedding)
  - [The host result layers](#the-host-result-layers)
  - [Engines and capabilities](#engines-and-capabilities)
  - [Runtime and embedding invariants](#runtime-and-embedding-invariants)
  - [Current scope boundaries](#current-scope-boundaries)
<!-- tour:contents:end -->

## Choosing a failure channel

Use these rules for new APIs and application code:

- return `nil` for expected data-dependent absence or failure, such as EOF, a missing search result, or unsuccessful text-to-number conversion;
- raise a value for recoverable failures that callers may handle structurally;
- use hard diagnostics for programmer contract violations, such as wrong arity, invalid operand types, or assigning to an undefined binding.

For example, an unsuccessful numeric conversion is ordinary absence:

```simi
let string = require("std/string")
string.to_number("not a number")
```

Any Simi value can be raised, though structured maps with a stable `error` discriminator are the usual shape for recoverable failures:

```simi
raise {error = "invalid_input", value = "not a number"}
```

Generated error maps may gain fields over time, so consumers should match the fields they need rather than depending on an exact rendered map.

A programmer mistake is instead a hard diagnostic. This complete script intentionally applies numeric addition to a string:

```simi
-- Expected type and runtime diagnostics: addition requires numbers.
1 + "two"
```

Hard diagnostics must not be converted to `nil` or a partially successful mutation.

## Catching raised values

A `try` expression evaluates one or more protected items. Its `catch` clauses use the same structural patterns and Boolean guards as `case`, in source order:

```simi
fn load(key) do
    raise {error = "not_found", key = key}
end

try
    load("profile")
catch {error = "not_found", key = key} do
    "missing: " <> key
catch error do
    raise error
end
```

Only a raise from the protected block is considered by those catches. If no clause matches, the original raise continues unchanged. Bindings created by a catch pattern belong only to that handler.

A raise from a catch guard or handler body escapes the current `try`; it is not offered to later sibling catches:

```simi
try
    try
        raise "original"
    catch error do
        raise {error = "replacement", cause = error}
    end
catch {error = "replacement", cause = cause} do
    cause
end
```

`try` catches neither postfix nil propagation nor hard diagnostics. This complete script intentionally produces a hard operand diagnostic; its handler is not entered:

```simi
-- Expected type and runtime diagnostics: catch handles raises, not hard diagnostics.
try
    1 + "two"
catch _ do
    "not reached"
end
```

Raises cross function boundaries and accumulate trace frames. Raising again in a handler creates a new raised event that retains the caught event as its cause; it is not a special syntax-level “rethrow.”

## Minimal Rust embedding

Create a new Rust binary with `cargo new`, then add Simi directly from its public Git repository:

```sh
cargo new simi-embed
cd simi-embed
```

Add the dependency to `Cargo.toml`:

```toml
[dependencies]
simi = { git = "https://github.com/Microwave-WYB/simi-lang" }
```

Replace `src/main.rs` with this complete program:

```rust
use simi::eval;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = r#"
        let answer = 40 + 2
        answer
    "#;

    match eval(source)? {
        Ok(value) => println!("{}", value.render()),
        Err(raised) => eprintln!("script raised: {}", raised.value.render()),
    }

    Ok(())
}
```

Run it with `cargo run`. It prints `42`.

The `?` after `eval(source)` handles lexing, parsing, and hard runtime diagnostics as ordinary Rust errors. The inner match distinguishes normal script completion from an uncaught value raised by the script.

## The host result layers

That distinction is represented directly in the public API:

```rust
pub type ScriptResult = Result<Value, Raised>;
pub fn eval(source: &str) -> Result<ScriptResult, SimiError>;
```

Read the result from the outside in:

- `Err(SimiError)` is a lexing, parsing, or hard runtime diagnostic;
- `Ok(Err(Raised))` is a value raised by the script and not caught there;
- `Ok(Ok(Value))` is normal completion, including a normal `Value::Nil` result.

`Raised` exposes the raised `value`, its source `origin`, function-call `frames`, and an optional prior `cause`. `SimiError` exposes the relevant source span through `span()`.

The root `simi::eval` convenience function creates a fresh engine with the portable standard library for each call. It does not register `std/io`.

## Engines and capabilities

Construct an `Engine` when evaluations should share registered modules and module state:

```rust
use simi::Engine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::with_stdlib();
    let first = engine.eval("1 + 2")?;
    let second = engine.eval("require(\"std/string\").to_number(\"42\")")?;

    match (first, second) {
        (Ok(left), Ok(right)) => {
            println!("{}, {}", left.render(), right.render());
            Ok(())
        }
        (Err(raised), _) | (_, Err(raised)) => {
            eprintln!("script raised: {}", raised.value.render());
            Ok(())
        }
    }
}
```

`Engine::new()` starts with no registered modules. `Engine::with_stdlib()` registers the portable modules `std/list`, `std/map`, `std/iter`, `std/number`, and `std/string`. Text IO is a separate capability:

```rust
use simi::Engine;

fn main() {
    let engine = Engine::builder()
        .stdlib()
        .stdio()
        .build();

    let _ = engine.eval("require(\"std/io\").println(\"Hello from Simi\")");
}
```

Modules are registered by the embedding host and cached per engine. Repeated `require` calls on one engine return the same mutable export map, and source-module state persists across that engine's evaluations. Separate engines have separate registries and caches.

Hosts may register direct value modules with `Module::builder` or source-backed modules with `Module::source`. Native callbacks may capture Rust state, but they must be `Send + Sync + 'static`. Managed Simi values must not be hidden in untraced Rust containers or captured as untraced edges.

The low-level `Interpreter::with_globals` constructor treats its supplied environment as complete. Unlike normal/default interpreters and `Engine` evaluation, it does not add the `require`, `type`, and `inspect` prelude.

## Runtime and embedding invariants

Keep these boundaries intact when extending Simi:

- a missing module raises `{error = "module_not_found", module = name}` rather than becoming a hard diagnostic;
- circular lazy loading raises `{error = "circular_module_dependency", module = name}`;
- recoverable IO failures raise `{error = "io_error", operation = operation, message = message}`;
- non-string module names, invalid native contracts, wrong arity, and invalid operand types are hard diagnostics;
- `inspect` is cycle-safe human-readable rendering, not serialization;
- the host result remains `Result<Result<Value, Raised>, SimiError>` in expanded form.

Do not weaken the two-layer result contract by turning raised values into `SimiError` or hard diagnostics into catchable script values without a deliberate language-design decision.

## Current scope boundaries

The current implementation intentionally does not include:

- filesystem or package module discovery;
- script-visible command-line arguments;
- a bytes type or raw stream IO;
- serialization;
- a formatter or REPL;
- runtime tuples or multiple returns;
- a generic iterator collection protocol;
- sequence or shape variables;
- advanced traits, protocols, or type constraints.

These are roadmap boundaries, not invitations to infer behavior from a future design. Keeping them explicit makes the current runtime, error model, and embedding surface understandable while the core API settles.

---

<!-- tour:navigation:start -->
---

[Previous: Iterators](iterators.md)
<!-- tour:navigation:end -->
