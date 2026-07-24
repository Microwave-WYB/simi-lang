# Simi for Zed

A local [Zed language extension](https://zed.dev/docs/extensions/languages) for `.simi` files. It provides `simi-lsp` integration plus syntax highlighting, bracket matching, indentation, a function outline, and Vim function/comment text objects. Tree-sitter editing features remain available when the language server is absent.

## Language server

The Rust/WASM extension resolves `simi` with `worktree.which("simi")`, launches its `lsp` subcommand, and passes the worktree shell environment to the server. Install the binary into a directory visible on the worktree `PATH`, for example from the repository root:

```sh
cargo install --path .
```

A binary at `target/debug/simi` is not discovered automatically. If lookup fails, Zed reports that `simi` was not found on the worktree `PATH`; adjust the environment used to launch Zed or your worktree environment and reload the extension.

## Why the checked-in manifest has no grammar URL

Zed requires every grammar entry to name a Git repository and an exact revision. The shared Simi grammar lives at `../tree-sitter`, but an absolute `file://` URL is machine-specific and a relative one is not a portable Git URL. The checked-in `extension.toml` therefore contains only portable metadata. The setup recipe generates a complete dev extension without modifying tracked files:

1. copy `../tree-sitter` to `.local/tree-sitter-simi`;
2. initialize that copy as a grammar-root Git repository;
3. commit the snapshot with a tool-local, non-credential identity; and
4. copy the extension's pinned Cargo manifest and Rust source; and
5. generate `.local/extension/extension.toml` with the repository's absolute `file://` URL and exact `rev`.

No token, username, SSH URL, or developer Git identity is read or embedded.

## Local installation

Prerequisites:

- Zed;
- Rust installed through `rustup` (required by Zed for Rust/WASM dev extensions);
- Git;
- Python 3.11 or newer;
- [`just`](https://just.systems/);
- a generated shared grammar at `editors/tree-sitter` containing `grammar.js` and `src/parser.c`.

From this directory:

```sh
just setup-local
just test-local
```

To build the project-local Simi executable without installing it globally:

```sh
just build-simi
```

To build the server, prepare the dev extension, and launch Zed with the local server on `PATH`:

```sh
just dev
```

From the repository root, the same recipes are available as `just editors::zed::build-simi` and `just editors::zed::dev`.

If the shared grammar is elsewhere, pass it explicitly:

```sh
just setup-local /absolute/path/to/tree-sitter-simi
just test-local /absolute/path/to/tree-sitter-simi
```

In Zed, run **zed: install dev extension** (or click **Install Dev Extension** on the Extensions page) and select:

```text
editors/zed/.local/extension
```

After changing the shared grammar, rerun `just setup-local` and reinstall/reload the dev extension. Check `Zed.log` with **zed: open log** if grammar compilation fails.

`just test` always performs portable static checks. It additionally performs parser/query checks when the default sibling grammar is present; otherwise it prints an explicit skip. `just clean` removes all generated files.

## Shared grammar query contract

The queries expect the shared grammar to expose these named nodes/fields:

- lexical nodes: `comment`, `identifier`, `integer`, `float`, `string`, and `escape_sequence`;
- literals and patterns: `boolean`, `nil`, and `wildcard_pattern`;
- functions: `function_declaration` with `name` and `body` fields, `function_expression` with a `body`, `declared_parameter`, `declared_parameters`, `parameter`, `parameters`, and `block`;
- expressions: `call_expression` with a `function` field, `field_expression` with a `name` field, `pipeline_callee`, `block_expression`, and `nil_propagation_expression`;
- maps/patterns: `map_field` and `map_pattern_field`, each with a `name` field;
- control flow: `if_expression`, `loop_expression`, `case_expression`, `elseif_clause`, `else_clause`, and `try_expression`;
- pattern dispatch: `case_expression` contains repeated `case_clause` nodes and `try_expression` contains repeated `catch_clause` nodes; each clause has a `pattern` field and optional `guard` and `body` fields, while only the enclosing expression owns `end`.

Anonymous nodes must preserve the source spellings used in the query files, including `fn`/`do`/`end`, `?`/`?>`, delimiters, operators, and keywords. Canonical dispatch repeats `of pattern [when guard] do block` or `catch pattern [when guard] do block` beneath one final `end`. Runtime-category checks use ordinary `type(value) == "label"` syntax, while `is` is highlighted as an ordinary identifier. The removed `match`, `with`, per-arm `case`, and legacy case-dispatch arrow spellings are outside the query contract; `->` remains the function return-type operator. This is the integration boundary with `editors/tree-sitter`; run `just test-local` after either component changes. Closures are deliberately excluded from function text-object navigation, following Zed's current guidance. Simi has no class-like construct or language injection, so no class text objects or `injections.scm` are provided.

## Preparing a future publishable manifest

Once the grammar is available as a public **grammar-root** HTTPS Git repository, prepare a clean extension directory with full, immutable Git IDs:

```sh
just prepare-publish \
  https://github.com/OWNER/tree-sitter-simi \
  0123456789abcdef0123456789abcdef01234567 \
  https://github.com/OWNER/simi
```

The recipe rejects non-HTTPS URLs, embedded credentials, branch/tag names, and abbreviated revisions. Output is written to `.local/publish`; the source manifest remains unchanged. Before submitting to the Zed extension registry, choose and add an [accepted extension license](https://zed.dev/docs/extensions/developing-extensions#extension-license-requirements), set the real repositories, perform a manual Zed install, and follow Zed's current publishing review checklist.

The generated grammar entry uses canonical current manifest spelling:

```toml
[grammars.simi]
repository = "https://github.com/OWNER/tree-sitter-simi"
rev = "0123456789abcdef0123456789abcdef01234567"
```
