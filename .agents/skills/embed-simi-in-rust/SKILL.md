---
name: embed-simi-in-rust
description: Embed Simi in Rust applications and expose host capabilities safely. Use when configuring Engine, evaluating scripts, registering modules, building source-backed facades, or handling Simi results and raised values.
license: MIT
compatibility: Requires Rust 1.85 or newer and the simi crate from this repository.
---

# Embed Simi in Rust

## Read the public contract first

Use the [embedding tour](../../../docs/language-tour/errors-and-embedding.md) as the authoritative guide and [public API compile test](../../../tests/public_api.rs) as a current inventory. Simi is unreleased, so pinning a Git revision is appropriate for external applications.

## Choose the evaluation surface

- Use `simi::eval(source)` for a fresh engine with the portable standard library.
- Use `Engine::new()` for no registered modules.
- Use `Engine::with_stdlib()` for portable `std/list`, `std/map`, `std/iter`, `std/number`, and `std/string` modules.
- Use `Engine::builder().stdlib().stdio().build()` only when text standard IO should be granted.
- Reuse an `Engine` when registered module instances and their cached mutable state should persist across evaluations.

Always preserve the two result layers:

```rust
pub type ScriptResult = Result<Value, Raised>;
pub fn eval(source: &str) -> Result<ScriptResult, SimiError>;
```

`SimiError` represents lexing, parsing, and hard runtime diagnostics. `Raised` contains an uncaught script-raised value, origin, frames, and optional cause. Normal completion includes `Value::Nil`.

## Register modules

Use `Module::builder(name)` for a direct Rust-built public value module. Use `Module::source(name, source)` when Simi source should define the public facade, documentation, erased types, closures, or additional behavior.

A source module receives one private traced `host: Value` before facade evaluation:

```rust
use simi::{Module, Value, host_value};

let host = host_value! {
    name: "acme/constants",
    values: {
        "answer" => Value::Int(42),
    },
};

let module = Module::source(
    "acme/constants",
    "let answer: integer = host.answer {answer = answer}",
)
.host(host)
.build();
```

For map-shaped hosts, construct the complete map with `host_value!` or `ModuleBuilder::build_value()` and then call `.host(value)`. `.host(Value)` is the sole source-host primitive; do not recreate callback-ID dispatch or piecemeal source-host registration.

## Native function rules

- Native functions use ordinary fixed arity.
- `host_value!` function entries specify that arity explicitly.
- Callbacks may capture Rust state but must be `Send + Sync + 'static`.
- Never hide managed Simi values in untraced Rust containers or callback captures. Put managed data/functions directly into the traced host value.
- The macro `name` prefixes native rendering and diagnostics; it is not a field in the generated map.
- Duplicate registrations are last-wins. Map entries with `nil` are absent.

Prefer a direct typed alias in the Simi facade when it adds only erased metadata:

```simi
let append: ([..'a] => [..('a | 'b)], 'b) -> nil = host.append
```

Use a Simi wrapper only when it deliberately adds Simi behavior. The final facade expression may be any public `Value`, not only a map.

## Lifecycle and diagnostics

Source modules evaluate lazily and cache their result once per engine, including `nil`. Failed loads may be retried; circular loads raise a structural `circular_module_dependency` value. Missing modules raise `module_not_found`.

Source spans do not carry source identities. When raises cross source-module domains, function names are retained but origins and frame spans are remapped to the caller's public boundary. Do not expose private facade spans as though they belonged to the caller's source.

## Verification

Add focused tests for result-layer handling, exact arity/type failures, arbitrary exports, caching, engine isolation or intentional sharing, retries, circular dependencies, raised-value boundaries, and GC release. Then run:

```sh
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --bin simi
find src -type f -name mod.rs
git diff --check
```

The `find` command must produce no output.
