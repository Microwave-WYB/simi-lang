---
name: write-simi-scripts
description: Write, explain, migrate, and debug Simi scripts. Use when editing .simi files, choosing language constructs or standard modules, or diagnosing Simi parser, analyzer, or runtime behavior.
license: MIT
compatibility: Requires this Simi repository or an installed simi executable.
---

# Write Simi Scripts

## Start with the authoritative language guide

Read the relevant topic in the [language tour](../../../docs/language-tour.md) before choosing syntax. For static annotations and inference, also consult the [erased type-system design](../../../docs/type-system.md). Current examples must not assume roadmap features.

## Workflow

1. Identify whether the script needs only portable modules or the CLI-only `std/io` capability.
2. Write a complete `.simi` program using current syntax and compact delimiter formatting.
3. Prefer expression-valued control flow, structural patterns, and explicit-state `loop` when state evolves.
4. Run the script with `simi run FILE`; use `simi run --inspect FILE` when the final value should also be rendered.
5. If the script belongs to this repository, add a focused test or independently validated documentation example at the lowest useful layer.

## Core contracts

- Runtime typing is dynamic. Optional annotations and aliases are erased and must not change runtime behavior.
- `if`, `case`, `try`, `do`, function bodies, and loop bodies are value-producing lexical blocks.
- Postfix `?` passes non-`nil` values through; `nil` evaluates the nearest lexical block as `nil`. In a loop body that supplies the next state rather than breaking the loop.
- Booleans are strict; there is no truthiness. Use `type(value) == "integer"` for runtime category checks.
- Lists are zero-based and mutable. Maps are insertion-ordered, normalize numeric keys, and delete entries assigned `nil`.
- Map patterns are closed by default. Add `..` to permit extra fields or `..rest` to capture them.
- `|>` inserts the input as the first call argument. `?>` skips only that stage for `nil`. `|> tap` and `?> tap` preserve the incoming value identity.
- `<|` appends exactly one trailing argument to a call. `<>` is strict string concatenation.
- Return `nil` for expected absence, `raise` recoverable values, and leave programmer contract violations as hard diagnostics.

## Standard modules

- `std/list`: list primitives, mutation, copy, and slicing.
- `std/map`: map primitives and inspection.
- `std/iter`: lazy single-pass adapters and consumers.
- `std/number` and `std/string`: explicit conversions and scalar operations.
- `std/io`: opt-in text IO available from the CLI and engines configured with stdio.

Use `require("std/...")` explicitly. Filesystem/package discovery, serialization, bytes, a formatter, a REPL, runtime tuples, and script-visible command-line arguments are not implemented.

## Formatting and checks

Use compact forms such as `{a = x, b = y}` and `[a, b, c]`. For a multiline pipeline on a binding RHS, break after `=` and indent the continuation.

```sh
cargo build --bin simi
target/debug/simi run path/to/script.simi
target/debug/simi run --inspect path/to/script.simi
```

For documentation examples, use truthful `simi` fences and make each example independently complete. Examples intentionally producing a static diagnostic begin with an `-- Expected type` comment.
