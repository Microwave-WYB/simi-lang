# Simi Language Support for VS Code

Visual Studio Code language support for Simi, including:

- `.simi` file association;
- `simi-lsp` diagnostics, symbols, navigation, references, rename, hover, and completion;
- TextMate-based syntax highlighting that remains available when the server is absent;
- `--` line comments;
- bracket matching, single-character auto-closing and surrounding pairs;
- parser-backed structural indentation for sibling `case`/`catch` arms;
- explicit snippets with empty numeric tab stops for blocks, functions, and case expressions;
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

`npm run package` regenerates the Tree-sitter parser, compares its exact bytes with the committed WASM, runs the editor tests, and creates `simi-language-<version>.vsix`. The component `just package` recipe first removes any staged native binary so source-development packages use an external server. Generated VSIX files and `node_modules` are ignored.

The root `just release-vscode TARGET PLATFORM` recipe stages a platform-specific release VSIX by copying the already-built native server into `bin/` and invoking `package-bundled`. This release-only path validates that the committed WASM is tracked, unmodified, loadable, and able to parse Simi source, then packages it without regeneration. Normal development packages and CI continue to perform reproducible regeneration. The generated `bin/` directory is ignored and must never be committed.

Marketplace publication is intentionally explicit and is not a dependency of any other task. After configuring the `simi` publisher and a Marketplace token:

```sh
CONFIRM_PUBLISH=1 VSCE_PAT=... just publish
```

The guard prevents an accidental `just publish`; `npm run publish` is the underlying unguarded `vsce publish` command for release automation that deliberately invokes it.

## TextMate and Tree-sitter boundary

VS Code's stable declarative grammar contribution point consumes TextMate grammars, not Tree-sitter parsers. Consequently, this extension uses `syntaxes/simi.tmLanguage.json` for highlighting. VS Code also has no indentation-provider contribution point: line regexes alone cannot align sibling `=>` arms while preserving the enclosing final `end` level.

For exact structural Enter handling, the extension directly loads the bundled shared Simi Tree-sitter WASM parser through `web-tree-sitter`. This parser is controller implementation detail, not an unsupported grammar contribution: highlighting remains TextMate-based and the declarative indentation rules remain an incomplete-source fallback. The shared `editors/tree-sitter` grammar is authoritative for both the VS Code controller and Zed indentation.

Canonical pattern dispatch is `case expression of pattern [when guard] => expression ... end`. A zero- or multi-item arm result is an ordinary `do ... end` expression. Protected expressions use `do ... catch of`, followed by `pattern [when guard] => expression` arms and one final `end`. Standalone `do ... end`, postfix `?`, and nil-aware `?>` pipelines share the normal block/operator highlighting. Removed legacy spellings such as repeated `of` arms, `match ... with`, thin case arrows, `try`, and `catch pattern do` are not highlighted as control syntax.

Runtime-category checks use the builtin call and ordinary comparison syntax, such as `type(value) == "integer"` and `type(callback) == "function"`. The shadowable builtin is highlighted as a builtin only when called, `==` uses the normal comparison scope, and `is` is an ordinary identifier.

## Snippets

The extension provides three explicit snippets with empty numeric tab stops. Snippets are available as completion candidates on explicit acceptance only (such as Tab or Ctrl+Space), and standalone keywords plus Enter never auto-expand or intercept input:

| prefix | body |
| ------ | ---- |
| `case` | `case ${1} of` / `    ${2}` / `end` |
| `fn`   | `fn ${1}(${2}) ${3}` |
| `do`   | `do` / `    ${1}` / `end` |

Typing `fn`, `do`, or `case` followed by Enter never inserts closing `end` or other shell text automatically. The indentation controller only corrects whitespace for parsed case/catch arms and their enclosing final `end`; it does not expand standalone keywords, create blocks, add closers, or replace text. It observes VS Code's document-change-before-selection event sequence and never registers or overrides the global `type` command, preserving compatibility with VSCodeVim and other keyboard-driven editors.

## Structural indentation

With a single cursor, Enter after a line-ending `=>` arm header uses its parsed `case_clause` or `catch_arm` owner to place the arm at one indent and its direct body at two indents. Completing the enclosing final `end` restores the owner's indentation. Guarded parser completion handles an arm awaiting its body or enclosing ends; malformed syntax, comments, strings, selections, and multi-cursor entry remain untouched. Tabs and the configured indentation unit are preserved.

## Validation

The tests load the highlighting grammar through the same `vscode-textmate` and Oniguruma libraries used by VS Code and assert scopes against a representative Simi fixture. They also load the packaged Tree-sitter WASM parser and exercise exact case/catch levels, nested ownership, comments, strings, tabs, multi-cursor input, no automatic shell insertion, and VSCodeVim-compatible event handling. These focused Node tests are not an Extension Development Host test.
