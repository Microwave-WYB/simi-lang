# Simi

Simi is a small, embeddable scripting language implemented in Rust. It combines a Lua-inspired dynamic runtime with expression-valued control flow, pipelines, closures, mutable lists and maps, structural pattern matching, value-based errors, lazy iterators, and optional erased type annotations.

> **Status:** unreleased. Simi has not published a release yet, and compatibility is not guaranteed.

## A small example

```elixir
let list = require("std/list")
let io = require("std/io")

fn two_sum(numbers, target) do
    loop remaining = numbers do
        case remaining
        of [] do
            break nil
        of [first, ..rest] do
            let second = target - first
            if list.contains(rest, second) then
                break {first = first, second = second}
            else
                rest
            end
        end
    end
end

let pair = two_sum([2, 7, 11, 15], 9)
io.println(inspect(pair))
```

Simi is expression-oriented: blocks, conditionals, loops, cases, and error handlers all produce values. Lists and maps are mutable and preserve alias identity, while explicit copy operations provide shallow copy-on-write views where documented.

## Language tour

Start with [Hello, world!](docs/language-tour/hello-world.md), follow the complete [language tour](docs/language-tour.md), then run the [explicit-state Fibonacci example](examples/fibonacci.simi).

## Install with Cargo

Simi currently requires Rust 1.85 or newer. First [install the Rust toolchain with rustup](https://rustup.rs/), then install the `simi` executable directly from the public repository:

```sh
cargo install --git https://github.com/Microwave-WYB/simi-lang --bin simi
```

Run a script with:

```sh
simi run examples/fibonacci.simi
```

Scripts control their own output. To also render the final value, including `nil`:

```sh
simi run --inspect examples/fibonacci.simi
```

The language server runs over standard input and output:

```sh
simi lsp
```

## Editor plugins

Simi includes editor integrations that launch the installed `simi lsp` server:

- [Visual Studio Code](editors/vscode/README.md): TextMate highlighting, language configuration, and LSP features;
- [Zed](editors/zed/README.md): Tree-sitter editing support and LSP features;
- [Tree-sitter](editors/tree-sitter/README.md): the shared structural grammar for compatible editors.

The editor extensions are currently installed from this repository rather than an extension marketplace. Follow each linked guide for local setup.

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

The erased type design is documented in [docs/type-system.md](docs/type-system.md).

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
