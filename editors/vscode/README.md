# Simi Language Support for VS Code

Visual Studio Code language support for Simi, including:

- `.simi` file association;
- `simi-lsp` diagnostics, symbols, navigation, references, rename, hover, and completion;
- TextMate-based syntax highlighting that remains available when the server is absent;
- `--` line comments;
- bracket matching, single-character auto-closing and surrounding pairs, plus extension-managed `do … end` block shells;
- indentation rules for standalone `do` blocks and one-final-`end` repeated `of`/`catch` branches;
- construct-specific snippets for blocks, functions, conditionals, loops, cases, and try/catch expressions;
- indentation-based folding plus `-- region` / `-- endregion` folding markers.

The extension is a workspace extension. Platform-specific VSIX release assets bundle the matching `simi` language server. Ordinary source-development packages created with `just package` contain no native binary and fall back to an externally installed server.

## Language server

The executable is resolved in this strict order:

1. the `simi.languageServer.path` VS Code setting;
2. the `SIMI_PATH` environment variable;
3. the platform-specific server bundled in the extension, when present;
4. `simi` on the extension host's `PATH`.

For development from this repository, install the server into a directory already on `PATH`:

```sh
cargo install --path .
```

Building `target/debug/simi` alone does not place it on `PATH`. Use **Simi: Restart Language Server** after changing the configured executable.

## Release installation

For the newest development build, open the latest successful [`main` CI run](https://github.com/Microwave-WYB/simi-lang/actions/workflows/ci.yml) and download the artifact for your platform. For a manually selected milestone, download `simi-vscode-<full-sha>-<target>.vsix` and its checksum from the [latest release](https://github.com/Microwave-WYB/simi-lang/releases/tag/latest). Install the matching self-contained VSIX and reload VS Code:

```sh
code --install-extension ./simi-vscode-<full-sha>-<target>.vsix
```

No separate CLI installation is required unless you choose to override the bundled language server.

## Local installation

Requirements: Node.js/npm, Rust 1.88 or newer, Visual Studio Code's `code` command, and optionally [`just`](https://just.systems/). The `install-local` recipe builds and bundles the language server from the current checkout, so no prior `simi` installation is required.

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

`npm run package` validates the grammar and creates `simi-language-<version>.vsix`. The component `just package` recipe first removes any staged native binary so source-development packages use an external server. Generated VSIX files and `node_modules` are ignored.

The root `just release-vscode TARGET PLATFORM` recipe stages a platform-specific release VSIX by copying the already-built native server into `bin/` and invoking `package-bundled`. The generated `bin/` directory is ignored and must never be committed.

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

## Block auto-pairing

With a single cursor, press Enter after a line-ending `do` keyword in Simi code to create an indented block shell, with the cursor on its empty body line. Waiting for Enter confirms the token boundary, so typing an identifier such as `document` never inserts a shell; comments and strings are excluded by the extension's line lexer. The `type` command delegates every keystroke to VS Code's `default:type` before applying this focused behavior. Multi-cursor entry retains normal VS Code typing without shell insertion.

The extension tracks each `end` that it generates. If the cursor is moved to the start of that tracked closer, typing `end` replaces its matching characters without producing a duplicate. This is extension-managed replacement, not VS Code multi-character close overtyping. Existing, untracked `end` text keeps normal typing behavior. VS Code 1.85 does not expose native multi-character paired deletion or overtyping, so the extension does not claim or emulate paired deletion.

## Validation

The tests load the grammar through the same `vscode-textmate` and Oniguruma libraries used by VS Code and assert scopes against a representative Simi fixture. They also validate package contributions, language configuration regexes, and the current lexer keyword inventory. Focused Node tests exercise the extension's `type` control path with faithful document, selection, edit, and change-event mocks; they are not an Extension Development Host test.
