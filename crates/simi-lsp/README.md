# Simi language-server library

This crate implements Simi's stdio Language Server Protocol adapter. It provides parser diagnostics, document symbols, lexical navigation and references, rename, hover, and completion over the Salsa-backed `simi-analysis` database.

The canonical executable entry point is the root CLI:

```sh
cargo run --bin simi -- lsp
```

After `cargo build --bin simi`, the equivalent project-local command is
`./target/debug/simi lsp`.

Editors should start `simi` with the `lsp` argument and connect stdin/stdout to LSP transport. The server negotiates UTF-16 positions and incremental document synchronization. Each open document is analyzed independently; filesystem and script-module loading are not implemented.

The protocol layer owns document versions and UTF-16 conversion only. All syntax, HIR, resolution, and symbol decisions come from `simi-analysis`, and analysis IDs are reacquired after every source revision.
