---
name: develop-simi-language
description: Implement and validate changes to Simi syntax, runtime semantics, erased analysis, LSP behavior, standard-library facades, documentation, or editor grammars. Use for cross-layer language development in this repository.
license: MIT
compatibility: Requires the repository's Rust, Python, Node.js, Just, and Tree-sitter development tooling.
---

# Develop the Simi Language

## Establish scope and semantics

Read [AGENTS.md](../../../AGENTS.md) before editing. Treat it, the current implementation, [type-system design](../../../docs/type-system.md), and [language tour](../../../docs/language-tour.md) as the current contract. If they disagree, investigate rather than silently choosing a new rule. Do not implement roadmap features opportunistically.

Before changing behavior, write down:

1. the source syntax and runtime result;
2. whether failure is `nil`, a catchable raised value, or a hard diagnostic;
3. whether annotations are erased and how inference should present the result;
4. which syntax, runtime, analyzer, LSP, editor, documentation, and embedding layers are affected.

## Cross-layer procedure

1. Add the lowest-layer regression that demonstrates the intended semantic boundary.
2. Update canonical Rowan syntax and parser recovery under `crates/simi-syntax/`; regenerate typed syntax rather than hand-editing generated Rust.
3. Update erased runtime lowering and interpretation under `src/`. Preserve lexical block boundaries, alias identity, two-layer errors, and GC tracing for every new managed edge.
4. Update Salsa lowering/resolution/inference under `crates/simi-analysis/`. Keep public `any` distinct from internal uncertainty and preserve flow-sensitive mutation, callable post-states, capture effects, and shadowing.
5. Update `simi-lsp` diagnostics, hover, completion, navigation, and UTF-16 protocol tests when behavior is user-visible.
6. Update standard-library Simi facades and their native implementations together. Prefer typed direct native aliases; use wrappers only for intentional Simi behavior.
7. Update the shared Tree-sitter grammar, generated artifacts, highlight/indent queries, TextMate grammar, and Zed fixtures as applicable. Rowan remains authoritative; Tree-sitter may be permissive only for documented editor recovery.
8. Update authoritative docs and independently complete examples. Run `just docs tour` after changing tour pages, headings, order, snippets, or links.
9. Validate the entire milestone, inspect the final diff, and obtain an independent read-only review for broad or safety-sensitive changes.

## High-risk invariants

- Static annotations, aliases, generic bounds, callable labels, raised contracts, and post-states are erased and never alter runtime behavior.
- `=>` is parameter-local post-state metadata, not a general type operator. Ambiguous unparenthesized forms must receive the canonical targeted diagnostic.
- Callable labels are presentation-only and calls remain positional. Nested generic headers own distinct binders; bounds are ordinary Simi types, never traits or protocols.
- Omitted callable effects infer, `raises E` checks an upper bound, and `noraise` means `raises never`. Hard diagnostics and postfix `?` stay outside the raised channel; post-states apply only on normal completion.
- Same-scope repeated `let` creates a new binding version; earlier closures retain earlier versions.
- Map patterns are closed unless they contain `..` or `..rest`.
- Postfix `?` stops the nearest lexical block. In a loop body its `nil` value supplies the next state.
- Raised values and hard diagnostics remain separate result layers.
- Every new edge capable of reaching a managed Simi value participates in tracing.
- Native callbacks remain `Send + Sync + 'static`; do not capture managed values as untraced edges.
- Source-backed modules receive one arbitrary private traced host value and may return any public value.
- Rust modules use a facade file plus a same-named directory; never introduce `mod.rs`.
- Generated Tree-sitter C/JSON files are committed and changed only through regeneration.

## Required validation

Run the repository baseline before completion:

```sh
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --bin simi
find src crates -type f -name mod.rs
git diff --check
```

The `find` command must produce no output.

For generated syntax and cross-cutting language work, also run:

```sh
cargo run -p simi-xtask -- check
```

For tour changes:

```sh
just docs tour
```

For Tree-sitter, VS Code, or Zed changes:

```sh
just editors test
```

The absent `wasm32-wasip1` target may skip only Zed's WASI compilation; host and generated-extension checks must still pass.

Run every `demo/*.simi` through `target/debug/simi run` after broad language or stdlib changes, supplying deterministic stdin to interactive demos. Type-system work also requires focused runtime-erasure and protocol-level LSP tests asserting exact diagnostics, hover text, and completion metadata.

## Review checklist

Before committing, confirm generated artifacts are current, no stale terminology or old syntax remains, documentation examples are independently valid, `git diff --check` passes, and a fresh reviewer reports no concrete blocker.
