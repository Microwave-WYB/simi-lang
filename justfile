# Documentation orchestration.
mod docs 'docs/justfile'

# Editor tooling orchestration.
mod editors 'editors/justfile'

# Run the complete repository validation used by GitHub CI.
ci:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo fmt --check
    cargo check --workspace --all-targets
    cargo test --workspace --all-targets
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo build --bin simi
    cargo run -p simi-xtask -- check
    cargo doc --workspace --no-deps
    just docs tour
    export PATH="$PWD/editors/tree-sitter/node_modules/.bin:$PATH"
    just editors install
    just editors test
    test -z "$(find src crates -type f -name mod.rs -print)"
    git diff --check

# Build one native binary for the manually dispatched hash release workflow.
release-build target:
    cargo build --locked --release --bin simi --target "{{ target }}"

# Package one native binary and its checksum under dist/.
release-package target platform sha:
    python3 scripts/package-release.py "{{ target }}" "{{ platform }}" "{{ sha }}"
