# Simi

Simi is a small, Lua-like embeddable scripting language implemented in Rust. It is dynamically typed at runtime, with optional erased static type inference and checking for earlier feedback and editor tooling. Simi is expression-first, with value-producing control flow, pipelines, closures, mutable lists and maps, structural pattern matching, value-based errors, and lazy iterators.

> **Status:** development. Builds are identified only by their full Git commit hash; compatibility is not guaranteed.

## A small example

```simi
let io = require("std/io")

--- Finds two numbers whose sum equals the target.
--- Returns the matching values, or nil when no pair exists.
fn two_sum(numbers: [..integer], target: integer) -> {first: integer, second: integer} | nil do
    loop state = {seen = {}, numbers = numbers} do
        case state.numbers
        of [] do
            break nil
        of [number, ..rest] do
            let complement = target - number
            if state.seen[complement] != nil then
                break {first = complement, second = number}
            else
                state.seen[number] = true
                {seen = state.seen, numbers = rest}
            end
        end
    end
end

two_sum([2, 7, 11, 15], 9)
|> inspect()
|> io.println()
```

Simi is expression-oriented: blocks, conditionals, loops, cases, and error handlers all produce values. Lists and maps are mutable and preserve alias identity, while explicit copy operations provide shallow copy-on-write views where documented.

## Language tour

Start with [Hello, world!](docs/language-tour/hello-world.md), follow the complete [language tour](docs/language-tour.md), then run the [explicit-state Fibonacci example](examples/fibonacci.simi).

## Installation

### Cargo

Simi currently requires Rust 1.88 or newer. First [install the Rust toolchain with rustup](https://rustup.rs/), then install the `simi` executable directly from the public repository:

```sh
cargo install --git https://github.com/Microwave-WYB/simi-lang --bin simi
```

### Latest development build

Every validated `main` commit produces temporary downloads. Open the [CI workflow](https://github.com/Microwave-WYB/simi-lang/actions/workflows/ci.yml), select the newest successful `main` run, and download the artifact for your target:

| System | Target |
| --- | --- |
| Linux x86-64 | `x86_64-unknown-linux-gnu` |
| macOS Intel | `x86_64-apple-darwin` |
| Windows x86-64 | `x86_64-pc-windows-msvc` |

The downloaded Actions artifact contains a CLI archive, a self-contained platform VSIX, and a SHA-256 checksum for each. These development artifacts expire according to GitHub's artifact-retention policy.

### Published build

Selected milestones are promoted manually to the single moving [latest release](https://github.com/Microwave-WYB/simi-lang/releases/tag/latest). Download either the CLI archive for your target or its separate `simi-vscode-<full-sha>-<target>.vsix`, together with the corresponding `.sha256` file. Verify downloads on Linux with:

```sh
sha256sum -c <downloaded-file>.sha256
```

CLI archives contain `simi` or `simi.exe`, an installation README, and the MIT license. Asset names retain the complete 40-character source commit hash; Simi does not use versions yet.

### VS Code extension

The platform-specific VSIX includes its matching `simi` language server, so a separate CLI installation is not required for editor features. Install it and reload VS Code:

```sh
code --install-extension ./simi-vscode-<full-sha>-<target>.vsix
```

For rapid development, `simi.languageServer.path` or `SIMI_PATH` can override the bundled server with another build.

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

Simi includes editor integrations for its `simi lsp` server:

- [Visual Studio Code](editors/vscode/README.md): TextMate highlighting, language configuration, and LSP features;
- [Zed](editors/zed/README.md): Tree-sitter editing support and LSP features;
- [Tree-sitter](editors/tree-sitter/README.md): the shared structural grammar for compatible editors.

The VS Code extension is available as a self-contained platform VSIX in CI artifacts and the latest release. Other editor integrations are currently installed from this repository rather than an extension marketplace. Follow each linked guide for setup.

## Language highlights

- dynamic values with optional, runtime-erased annotations, bounded generics, callable labels, and raised-effect contracts;
- lexical closures, recursion, and same-scope shadowing;
- expression-valued `if`, `case`, `try`, standalone blocks, and functional loops;
- ordinary, nil-aware, tap, and trailing-callback pipeline operators;
- mutable zero-based lists and insertion-ordered maps;
- structural list/map patterns and catchable raised values;
- tracing garbage collection with cycle-safe inspection;
- explicit source-backed modules with private native host values;
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

Hosts can register direct value modules or use `host_value!` to generate a private host map for a source-backed Simi facade; `.host(value)` also accepts any other Simi value. The facade adds erased types and documentation, may define additional Simi behavior, and evaluates to the public module value.

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

Repository-aware coding agents can load the portable skills in [`.agents/skills/`](.agents/skills/):

- [`write-simi-scripts`](.agents/skills/write-simi-scripts/SKILL.md) for authoring and debugging Simi programs;
- [`embed-simi-in-rust`](.agents/skills/embed-simi-in-rust/SKILL.md) for Rust host integration;
- [`develop-simi-language`](.agents/skills/develop-simi-language/SKILL.md) for cross-layer implementation and validation.

## License

Simi is available under the [MIT License](LICENSE).
