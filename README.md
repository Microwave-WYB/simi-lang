# Simi

Simi is a small, embeddable scripting language implemented in Rust. It combines a Lua-inspired dynamic runtime with expression-valued control flow, pipelines, closures, mutable lists and maps, structural pattern matching, value-based errors, lazy iterators, and optional erased type annotations.

> **Status:** early alpha. The core language and tooling are usable, but compatibility is not yet guaranteed.

## A small example

```simi
let list = require("std/list")
let iter = require("std/iter")
let io = require("std/io")

let doubled =
    [1, 2, 3]
    |> list.iter()
    |> iter.map() <| fn(value) do
        value * 2
    end
    |> iter.to_list()

io.println(inspect(doubled))
```

Simi is expression-oriented: blocks, conditionals, loops, cases, and error handlers all produce values. Lists and maps are mutable and preserve alias identity, while explicit copy operations provide shallow copy-on-write views where documented.

## Build and run

Simi currently requires Rust 1.85 or newer.

```sh
cargo build --bin simi
cargo run --bin simi -- run examples/language-tour.simi
```

Scripts control their own output. To also render the final value:

```sh
cargo run --bin simi -- run --inspect examples/language-tour.simi
```

Run the language server over standard input and output with:

```sh
cargo run --bin simi -- lsp
```

## Language highlights

- dynamic values with optional, runtime-erased type annotations;
- lexical closures, recursion, and same-scope shadowing;
- expression-valued `if`, `case`, `try`, standalone blocks, and functional loops;
- ordinary, nil-aware, tap, and trailing-callback pipeline operators;
- mutable zero-based lists and insertion-ordered maps;
- structural list/map patterns and catchable raised values;
- tracing garbage collection with cycle-safe inspection;
- explicit source-backed modules and host operations;
- lazy, single-pass iterators in `std/iter`;
- opt-in text IO through `std/io`;
- Rowan syntax, Salsa-backed analysis, LSP support, and VS Code, Zed, and Tree-sitter integrations.

Start with the [language tour](docs/language-tour.md) and its [runnable companion](examples/language-tour.simi). The erased type design is documented in [docs/type-system.md](docs/type-system.md).

## Embedding

The host API keeps hard diagnostics separate from values raised by a script:

```rust
pub type ScriptResult = Result<Value, Raised>;
pub fn eval(source: &str) -> Result<ScriptResult, SimiError>;
```

`eval` uses a fresh engine with the portable standard library. For persistent module state or custom capabilities, construct an `Engine`:

```rust
use simi::Engine;

let mut engine = Engine::with_stdlib();
let result = engine.eval("1 + 2")?;
```

Standard IO is deliberately opt-in:

```rust
let mut engine = Engine::builder().stdlib().stdio().build();
```

Hosts can register direct value modules or source-backed modules whose public facade is written in Simi.

## Standard modules

Portable engines provide:

- `std/list`
- `std/map`
- `std/iter`
- `std/number`
- `std/string`

The CLI additionally registers the opt-in `std/io` capability. Filesystem and package module discovery are not implemented yet; embedders register modules explicitly.

## Development

Run the Rust workspace checks with:

```sh
cargo fmt --check
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo run -p simi-xtask -- check
```

Editor integration checks are available through:

```sh
just editors test
```

## License

Simi is available under the [MIT License](LICENSE).
