# Simi Language Support for VS Code

Visual Studio Code language support for Simi, including:

- `.simi` file association;
- `simi-lsp` diagnostics, symbols, navigation, references, rename, hover, and completion;
- TextMate-based syntax highlighting that remains available when the server is absent;
- `--` line comments;
- bracket matching, single-character auto-closing and surrounding pairs, plus extension-managed `do … end` block shells;
- parser-backed structural indentation for sibling `case`/`catch` arms, with declarative rules limited to incomplete-source recovery;
- construct-specific snippets for blocks, functions, conditionals, loops, cases, and protected expressions;
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
code --install-extension --force simi-language-0.1.0-alpha.1.vsix
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

VS Code's stable declarative grammar contribution point consumes TextMate grammars, not Tree-sitter parsers. Consequently, this extension continues to use `syntaxes/simi.tmLanguage.json` for highlighting. VS Code also has no indentation-provider contribution point: line regexes alone cannot distinguish a first `of` arm from a later sibling while preserving the enclosing final `end` level.

For exact structural Enter handling, the extension directly loads the bundled shared Simi Tree-sitter WASM parser through `web-tree-sitter`. This parser is controller implementation detail, not an unsupported grammar contribution: highlighting remains TextMate-based and the declarative indentation rules remain recovery fallback for incomplete input. The shared `editors/tree-sitter` grammar is therefore authoritative for the controller's case/catch ownership as well as for Zed.

Canonical pattern dispatch is `case expression of pattern [when guard] expression ... end`. A zero- or multi-item arm body is an explicit `do ... end` block. Protected expressions use `do ... catch` followed by repeated `of` arms and one final `end`. Standalone `do ... end`, postfix `?`, and nil-aware `?>` pipelines share the normal block/operator highlighting. Removed legacy spellings such as `match ... with`, case arrows, `try`, and `catch pattern do` are not highlighted as control syntax.

Runtime-category checks use the builtin call and ordinary comparison syntax, such as `type(value) == "integer"` and `type(callback) == "function"`. The shadowable builtin is highlighted as a builtin only when called, `==` uses the normal comparison scope, and `is` is an ordinary identifier.

## Structural indentation and block auto-pairing

With a single cursor, Enter after an `of` header uses the parsed `case_clause` or `catch_arm` and its enclosing expression to place the arm at one indent and its direct body at two indents. Completing the enclosing final `end` immediately restores the owner's indentation. The controller uses guarded parser completion for a structurally valid arm that is still awaiting its body or enclosing ends; malformed syntax, comments, strings, selections, and multi-cursor entry remain untouched. Tabs and the editor's configured indentation unit are preserved.

Press Enter after a line-ending `do` keyword in Simi code to create an indented block shell, with the cursor on its empty body line. Waiting for Enter confirms the token boundary, so typing an identifier such as `document` never inserts a shell; comments and strings are excluded by the extension's line lexer. The structural controller composes with this behavior when `do ... end` is the direct expression of an arm.

The extension observes VS Code's document-change and editor-selection events for this behavior and does not register or override the global `type` command. Because VS Code reports the document change before moving the cursor after Enter, the extension records a pending shell from the change and inserts it only after the matching selection event. Typing therefore remains owned by VS Code and extensions such as VSCodeVim.

The extension tracks each `end` that it generates. If the cursor is moved to the start of that tracked closer, typing `end` replaces its matching characters without producing a duplicate. This is extension-managed replacement after observing a document change, not VS Code multi-character close overtyping. Existing, untracked `end` text keeps normal typing behavior. VS Code 1.85 does not expose native multi-character paired deletion or overtyping, so the extension does not claim or emulate paired deletion.

## Validation

The tests load the highlighting grammar through the same `vscode-textmate` and Oniguruma libraries used by VS Code and assert scopes against a representative Simi fixture. They also load the packaged Tree-sitter WASM parser used by the extension and exercise exact sibling-arm edits, nested cases, direct `do ... end` expressions, comments and strings, tabs, undo grouping, and VS Code's document-change-before-selection ordering with faithful document, selection, edit, and event mocks. These focused Node tests are not an Extension Development Host test.
