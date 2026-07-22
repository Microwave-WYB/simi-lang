# Simi Language Support for VS Code

Local Visual Studio Code language support for Simi, including:

- `.simi` file association;
- TextMate-based syntax highlighting for the current language syntax;
- `--` line comments;
- bracket matching, auto-closing, and surrounding pairs;
- indentation rules for `do`/`then`/`of`/`catch`, explicit pattern-clause blocks, and their closing branches;
- indentation-based folding plus `-- region` / `-- endregion` folding markers.

This extension is declarative and has no activation-time JavaScript.

## Local installation

Requirements: Node.js/npm, Visual Studio Code's `code` command, and optionally [`just`](https://just.systems/).

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

Canonical pattern dispatch is `case expression of pattern [when guard] do block end ... end`. `try`/`catch` uses the same pattern-clause form. The removed `match`, `with`, per-arm `case`, and `->` spellings are not highlighted as control syntax.

The runtime-label operator is highlighted as a comparison operator in expressions such as `value is "integer"` and `callback is "function"`. TextMate highlights tokens only; it does not validate whether the right operand is a string literal or whether the literal is a supported runtime label.

## Validation

The tests load the grammar through the same `vscode-textmate` and Oniguruma libraries used by VS Code and assert scopes against a representative Simi fixture. They also validate package contributions, language configuration regexes, and the current lexer keyword inventory.
