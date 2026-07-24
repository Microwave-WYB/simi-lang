# Simi

![Simi `two_sum` example](assets/two_sum.png)

Simi is a small scripting language designed to be easy to run and embed.

## Try the language

### Hello, name!

```simi
let io = require("std/io")

fn greet(name) do
    io.println("Hello, " <> name <> "!")
end

greet("Simi")
```

### Functional loop

```simi
let io = require("std/io")

--- Finds two numbers whose sum equals the target.
fn two_sum(numbers: [..integer], target: integer) -> [integer, integer] | nil noraise
do
    loop state = {seen = {}, numbers = numbers} do
        case state.numbers
        of [] do
            break nil
        of [number, ..rest] do
            let complement = target - number
            if state.seen[complement] != nil then
                break [complement, number]
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

Save this as `two_sum.simi`, install Simi using one of the options below, and run:

```sh
simi run two_sum.simi
```

## Language tour

Start with [Hello, world!](docs/language-tour/hello-world.md), continue through the complete [language tour](docs/language-tour.md), and explore the runnable programs in [`examples/`](examples/).

## Installation

> **Status:** Simi is under active development. It does not use version numbers yet; download names include the exact source commit instead.

### Latest release

The easiest way to try Simi is the [latest release](https://github.com/Microwave-WYB/simi-lang/releases/tag/latest). Choose the CLI download for your system:

| System | Download name ends with |
| --- | --- |
| Linux x86-64 | `x86_64-unknown-linux-gnu.tar.gz` |
| macOS Intel | `x86_64-apple-darwin.tar.gz` |
| Windows x86-64 | `x86_64-pc-windows-msvc.zip` |

Extract the download, then copy `simi` or `simi.exe` to a directory on your `PATH`. The included `README.md` has platform-specific steps. Each download also has a matching `.sha256` checksum file.

### Latest development build

To try changes newer than the published release, open the [CI workflow](https://github.com/Microwave-WYB/simi-lang/actions/workflows/ci.yml), select the newest successful `main` run, and download the artifact for your system. After unzipping the Actions artifact, you will find both the CLI download and the self-contained VS Code extension, with checksums for each.

Development artifacts are produced for Linux x86-64, Intel macOS, and Windows x86-64. They are temporary and expire according to GitHub's artifact-retention policy.

### Cargo

To build the newest source directly, first [install Rust with rustup](https://rustup.rs/). Simi currently requires Rust 1.88 or newer. Then run:

```sh
cargo install --git https://github.com/Microwave-WYB/simi-lang --bin simi
```

## Editor support

### Visual Studio Code

Download the `simi-vscode-...vsix` file for your system from either the [latest release](https://github.com/Microwave-WYB/simi-lang/releases/tag/latest) or a successful `main` CI artifact. Install it and reload VS Code:

```sh
code --install-extension ./simi-vscode-<full-sha>-<target>.vsix
```

The extension includes its matching language server, so it works without a separate CLI installation. See the [VS Code guide](editors/vscode/README.md) for configuration and source-development instructions.

### Other editors

- [Zed](editors/zed/README.md): Tree-sitter editing support and language-server features;
- [Tree-sitter](editors/tree-sitter/README.md): the shared structural grammar for compatible editors.

Zed and other editor integrations are currently installed from this repository.

## Use Simi

Run a script:

```sh
simi run two_sum.simi
```

Scripts control their own output. To also display the script's final value, including `nil`:

```sh
simi run --inspect two_sum.simi
```

Start the language server over standard input and output:

```sh
simi lsp
```

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
