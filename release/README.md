# Simi development build

This archive contains one immutable Simi development build:

- `simi` or `simi.exe`: the interpreter, command-line runner, and language server;
- `simi-vscode.vsix`: VS Code syntax highlighting and the matching bundled language server;
- `LICENSE`: the MIT license.

The full Git commit hash is part of the archive filename. Development builds do not promise compatibility with other commits.

## Install the executable

Install the standalone executable to run Simi from a terminal. The VS Code extension already contains its own matching language server and does not require this separate PATH installation.

On Linux or macOS, copy the executable to a directory on `PATH`:

```sh
install -m 755 simi "$HOME/.local/bin/simi"
simi --help
```

Ensure `$HOME/.local/bin` is on `PATH`. On macOS, the operating system may ask you to approve this unsigned development binary.

On Windows, copy `simi.exe` to a permanent directory, add that directory to the user `PATH`, open a new terminal, and run:

```powershell
simi.exe --help
```

## Install the VS Code extension

With VS Code's `code` command available, run this from the extracted directory:

```sh
code --install-extension ./simi-vscode.vsix
```

Reload VS Code after installation. No separate `simi` installation is needed for editor features: this platform-specific VSIX uses its bundled executable. Explicit overrides are resolved in this order:

1. the `simi.languageServer.path` VS Code setting;
2. the `SIMI_PATH` environment variable;
3. the executable bundled in the extension;
4. `simi` on `PATH` for source-development VSIX builds that contain no bundled executable.

After changing an override, run **Simi: Restart Language Server** from the command palette.

Run scripts with:

```sh
simi run path/to/script.simi
```
