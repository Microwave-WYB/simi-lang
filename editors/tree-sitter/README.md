# Tree-sitter Simi

This directory contains the local [Tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammar for the current Simi language. It is intentionally component-owned: installation, generation, tests, queries, and generated parser sources all live here.

## Requirements

- Node.js and npm
- a C compiler (for `tree-sitter test`)
- [`just`](https://just.systems/) (optional; the npm scripts can be used directly)

## Local workflow

Run commands from this directory:

```sh
just install   # install the pinned tree-sitter CLI
just generate  # regenerate src/parser.c and JSON artifacts
just test      # regenerate, compile, and run corpus tests
```

Equivalent npm commands are `npm ci`, `npm run generate`, and `npm test`.
Generated files under `src/` are committed so editors do not need Node.js to consume the parser. Do not hand-edit them; rerun `just generate` after changing `grammar.js`.

To inspect an individual file:

```sh
npm exec -- tree-sitter parse path/to/file.simi
```

## Coverage and editor queries

The grammar follows the Rust lexer and parser in `../../src/`, including ASCII identifiers, `--` line comments, string escapes, decimal/exponent numbers, expression-valued blocks, destructuring patterns, match/catch cases, functional loops, postfix calls/fields/adjacent indexes, assignment, pipelines, and right-associative trailing arguments.

Queries are provided for syntax highlighting (`queries/highlights.scm`), Neovim-style indentation captures (`queries/indents.scm`), and folding (`queries/folds.scm`). Consumers may need to map capture names to their editor's conventions.

Tree-sitter recognizes syntax rather than runtime validity. Contextual checks performed by Simi after parsing—such as duplicate names, loop control outside a loop, assignability beyond the syntactic target shape, and finite/i64 numeric bounds—remain the responsibility of the Simi implementation.
