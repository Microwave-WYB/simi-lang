# simi-lsp

`simi-lsp` is Simi's stdio Language Server Protocol adapter. It provides parser diagnostics, document symbols, lexical navigation and references, rename, hover, and completion over the Salsa-backed `simi-analysis` database.

Build and run it with:

```sh
cargo build --bin simi-lsp
cargo run --bin simi-lsp
```

Editors should start the binary with stdin/stdout connected to LSP transport. The server negotiates UTF-16 positions and incremental document synchronization. Each open document is analyzed independently; filesystem and script-module loading are not implemented.

The protocol layer owns document versions and UTF-16 conversion only. All syntax, HIR, resolution, and symbol decisions come from `simi-analysis`, and analysis IDs are reacquired after every source revision.
