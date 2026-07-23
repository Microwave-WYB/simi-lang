# Simi Language Support for VS Code

Visual Studio Code language support for Simi, including:

- `.simi` file association;
- `simi-lsp` diagnostics, symbols, navigation, references, rename, hover, and completion;
- TextMate-based syntax highlighting that remains available when the server is absent;
- `--` line comments;
- bracket matching, auto-closing, and surrounding pairs;
- indentation rules for standalone `do` blocks and one-final-`end` repeated `of`/`catch` branches;
- indentation-based folding plus `-- region` / `-- endregion` folding markers.

The extension is a workspace extension. It launches an external `simi lsp` server; no platform-specific server binary is bundled in the VSIX.

## Language server

The executable is resolved in this strict order:

1. the `simi.languageServer.path` VS Code setting;
2. the `SIMI_PATH` environment variable;
3. `simi` on the extension host's `PATH`.

For development from this repository, install the server into a directory already on `PATH`:

```sh
cargo install --path .
```

Building `target/debug/simi` alone does not place it on `PATH`. Use **Simi: Restart Language Server** after changing the configured executable.

## Local installation

Requirements: Node.js/npm, Visual Studio Code's `code` command, an installed `simi` executable, and optionally [`just`](https://just.systems/).

From this directory:

```sh
npm ci
npm test
npm run package
code --install-extension --force simi-language-0.1.0.vsix
```

Or use the component-owned recipes:

```sh
just test
just package
just install-local
```

Reload any open Simi editor after installing or updating the VSIX. To develop interactively, open `editors/vscode` in VS Code and use **Run Extension** (`F5`) to launch an Extension Development Host.

## Packaging and publication

`npm run package` (or `just package`) validates the grammar and creates `simi-language-<version>.vsix`. Generated VSIX files and `node_modules` are ignored.

Marketplace publication is intentionally explicit and is not a dependency of any other task. After configuring the `simi` publisher and a Marketplace token:

```sh
CONFIRM_PUBLISH=1 VSCE_PAT=... just publish
```

The guard prevents an accidental `just publish`; `npm run publish` is the underlying unguarded `vsce publish` command for release automation that deliberately invokes it.

## TextMate and Tree-sitter boundary

VS Code's stable declarative grammar contribution point consumes TextMate grammars, not Tree-sitter parsers. Consequently, this extension uses `syntaxes/simi.tmLanguage.json` for highlighting and does **not** load a Tree-sitter grammar through an unsupported VS Code API.

The shared `editors/tree-sitter` parser is the structural syntax source for Zed and other Tree-sitter consumers. Keep this TextMate grammar's token and keyword inventory aligned with that source, but expect contextual highlighting to remain an independently maintained TextMate approximation unless VS Code exposes a supported Tree-sitter contribution mechanism. Language configuration remains editor-specific in either case.

Canonical pattern dispatch is `case expression of pattern [when guard] do block ... of pattern do block end`, with no per-branch `end`. Try handlers repeat `catch pattern [when guard] do block` under the try's single final `end`. Standalone `do ... end`, postfix `?`, and nil-aware `?>` pipelines share the normal block/operator highlighting. The removed `match`, `with`, per-arm `case`, catch-section headers, and `->` spellings are not highlighted as control syntax.

Runtime-category checks use the builtin call and ordinary comparison syntax, such as `type(value) == "integer"` and `type(callback) == "function"`. The shadowable builtin is highlighted as a builtin only when called, `==` uses the normal comparison scope, and `is` is an ordinary identifier.

## Validation

The tests load the grammar through the same `vscode-textmate` and Oniguruma libraries used by VS Code and assert scopes against a representative Simi fixture. They also validate package contributions, language configuration regexes, and the current lexer keyword inventory.
