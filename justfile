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

# Build and stage a platform-specific VS Code extension with its bundled server.
release-vscode target platform:
    #!/usr/bin/env bash
    set -euo pipefail
    executable=simi
    if [[ "{{ platform }}" == windows ]]; then executable=simi.exe; fi
    source="target/{{ target }}/release/$executable"
    test -f "$source"
    rm -rf editors/vscode/bin
    rm -f editors/vscode/*.vsix
    mkdir -p editors/vscode/bin release-input
    cp "$source" "editors/vscode/bin/$executable"
    chmod +x "editors/vscode/bin/$executable"
    just editors vscode package
    cp editors/vscode/simi-language-*.vsix release-input/simi-vscode.vsix

# Package one native binary, the VS Code extension, instructions, and a checksum.
release-package target platform sha vsix="release-input/simi-vscode.vsix":
    python3 scripts/package-release.py "{{ target }}" "{{ platform }}" "{{ sha }}" "{{ vsix }}"
