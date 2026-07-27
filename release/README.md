# Simi build

This archive is produced from one immutable Git commit. Versioned prerelease
assets are published under tags such as `v0.1.0-alpha.1`; the moving `latest`
release remains a separate development convenience.

This archive contains one Simi command-line build:

- `simi` or `simi.exe`: the interpreter, command-line runner, and language server;
- `LICENSE`: the MIT license.

The full Git commit hash is part of the archive filename. Builds from different commits do not promise compatibility.

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

Download the platform-specific `simi-vscode-<full-sha>-<target>.vsix` asset from the same release. Its hash and target must match this archive. With VS Code's `code` command available, run:

```sh
code --install-extension /path/to/simi-vscode-<full-sha>-<target>.vsix
```

Reload VS Code after installation. No separate `simi` installation is needed for editor features: the VSIX contains its matching executable. Explicit overrides are resolved in this order:

1. the `simi.languageServer.path` VS Code setting;
2. the `SIMI_PATH` environment variable;
3. the executable bundled in the extension;
4. `simi` on `PATH` for source-development VSIX builds that contain no bundled executable.

After changing an override, run **Simi: Restart Language Server** from the command palette.

Run scripts with:

```sh
simi run path/to/script.simi
```

## Versioned prereleases

Versioned prereleases use immutable tags matching:

```text
vMAJOR.MINOR.PATCH-alpha.N
vMAJOR.MINOR.PATCH-beta.N
vMAJOR.MINOR.PATCH-rc.N
```

The versioned-release workflow validates that an existing tag still points to
the commit that was built, refuses to replace an existing release, and supports
a manual dry run. It never creates, retargets, or deletes a versioned tag; it
creates the versioned release only on first publication and refuses to replace
an existing one. The moving `latest` workflow is intentionally separate and may
be replaced.

### Tag creation and publication

The named release maintainer is the sole tag-ruleset bypass principal. After
validating the intended commit, that maintainer creates and pushes the
annotated prerelease tag:

```sh
git tag -a v0.1.0-alpha.1 <commit-sha> -m "Simi v0.1.0-alpha.1"
git push origin v0.1.0-alpha.1
```

That existing tag triggers the versioned-release workflow. For a manual run,
provide the same existing tag; Actions verifies both its local and remote
commit before publishing the release with `--verify-tag`. GitHub Actions is not
a tag-ruleset bypass actor and must not receive permission to create, update,
or delete versioned tags.

Repository administrators must configure the `v[0-9]*` tag ruleset to restrict
creations, updates, deletions, and force pushes. Its bypass list must contain
only the named release maintainer with an always-allow bypass; do not grant
broad repository roles, administrators, or GitHub Actions a bypass. GitHub
repository settings are not enforceable from this repository's workflow files;
without that ruleset, a user with sufficient repository permissions could move
a tag after publication.
