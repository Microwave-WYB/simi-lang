# Simi package layout

> Package resolution is introduced incrementally. This document defines the static layout and
> metadata contract consumed by the resolver; it does not grant filesystem, network, Cargo, or
> native-code authority to `Engine::eval`.

A reusable source package has a `simi.package.simi` file at its Git-tree root. The file is
restricted Simi data: it is parsed but never evaluated. It contains exactly one map expression.

```text
simi-polars/
├── simi.package.simi
├── polars.simi
├── polars/
│   └── csv.simi
├── src/
│   └── schema.simi
└── native/
    ├── Cargo.toml
    └── src/lib.rs
```

```simi
---- Data-frame facades for Simi.
{
    name = "polars",
    simi = "0.1.0-alpha.1",
    modules = [
        "polars",
        "polars/csv",
    ],
    native = {
        manifest = "native/Cargo.toml",
    },
}
```

`native` is descriptive metadata only. Source-package resolution never invokes Cargo, and native
runner support is a later feature.

## Identity and public modules

`name` is a lowercase package identity and its root public module. A package named `polars` must
export `polars`; additional public modules must begin `polars/`. Public module names map exactly
to package-root-relative source paths:

```text
polars      -> polars.simi
polars/csv  -> polars/csv.simi
```

This intentionally has no `facade.simi` convention. A package may contain private implementation
sources anywhere below its root, such as `src/schema.simi`, but they are not public catalog modules.

The package root is the source root. Public names and metadata paths use slash-separated relative
paths. Absolute paths, backslashes, empty path segments, `.`, and `..` are rejected. The static
`PackageTree` loader rejects a symlink root and any symlink component used by the manifest or a
declared public module; it reads only those files, never discovers arbitrary private sources.

## Restricted metadata

The top-level map permits only:

- `name`: non-empty lowercase package identity;
- `simi`: compatible Simi runtime revision string;
- `modules`: non-empty list of public module-name strings, including `name` itself;
- `native`: optional `{manifest = "relative/path/Cargo.toml"}` metadata for later native runners.

Functions, calls, bindings, variables, computed values, duplicate keys, and unrecognized fields
are invalid. `PackageTree` exposes deterministic digest inputs consisting of the manifest followed
by declared public modules sorted by canonical source path; unlisted private, generated, and native
files are excluded at this static-layout stage. The later resolver extends those inputs with locked
requirements and reachable package-local sources, but metadata itself never controls filesystem or
network authority.

## Requirements and documentation

Requirements belong to a leading static `requires` declaration in source files, not to executable
package code. The shared `parse_requires` API parses and validates this header without evaluating
any Simi expression. CLI package resolution consumes that metadata transitively before evaluation;
`Engine::eval` itself never reads paths, accesses Git or the network, creates lockfiles, or receives
any package-resolution authority.

A module-level `----` documentation block comes first, followed by the `requires` declaration:

```simi
---- CSV tools for a Polars package.
requires {
    text = {
        git = "https://example.invalid/simi-text.git",
        rev = "v0.1.0",
    },
}

let schema = require("./src/schema.simi")
```

Leading blank lines are allowed. The module documentation comments must remain consecutive and
immediately precede `requires`; no declaration or expression may separate them. Without module
documentation, `requires` is the first non-comment source form. The parser preserves this ordering
for diagnostics, editor recovery, and module hover documentation.

Each alias is a unique lowercase Simi identifier. Its value is restricted to exactly one of these
static maps:

```simi
requires {
    remote = {git = "https://example.invalid/remote.git", rev = "v1.2.3"},
    development = {path = "dev/development"},
}
```

`git`, `rev`, and `path` values must be string literals. Unknown, duplicate, mixed, and missing
fields are invalid. Development paths are non-empty package-root-relative slash-separated paths:
absolute paths, backslashes, empty segments, `.`, and `..` are rejected. These checks are static;
they do not establish that a path or Git revision exists.

## Local source imports

Package-local imports will use literal `require("./...")` paths relative to the importing source
file and confined to the package root. That Git/path resolution and catalog preparation are
resolver work still deferred; `parse_requires` only validates static dependency metadata and a bare
`Engine` does not read source paths.

For example, a future resolved `polars.simi` package may use:

```simi
let schema = require("./src/schema.simi")
```

Local source imports remain deferred to issue #35. This ordinary `require` call therefore follows
the existing registered-module behavior and raises `module_not_found` unless an embedder explicitly
registers that exact name.

## Locks and source resolution

For `app.simi`, `simi lock app.simi` writes `app.lock.simi`. The lock is canonical restricted Simi
data, parsed but never evaluated:

```simi
{
    format = 1,
    source = {path = "app.simi", digest = "sha256:..."},
    requirements = {
        polars = {
            source = {git = "https://example.invalid/simi-polars.git", rev = "v0.1.0"},
            package = "polars",
            commit = "...",
            tree_digest = "sha256:...",
        },
    },
}
```

Requirement keys are package identities and are sorted, as are all complete transitive entries.
A Git revision is resolved to an exact commit. Tree digests use SHA-256 over sorted `PackageTree`
inputs, framing every path and content byte sequence with its UTF-8 byte length; filesystem iteration
order never affects a digest. Path requirements are relative to the declaring source file (or package
root for a package source), and their package tree is also locked by digest.

`run app.simi` resolves requirements and refreshes the lock before constructing an Engine. It registers
only declared public package modules by their manifest names, so `require("polars")` and
`require("polars/csv")` work without granting source code ambient filesystem access.
`run --locked app.simi` requires an existing canonical lock and validates source declarations,
commits, and tree digests without fetching or rewriting. `run --offline app.simi` additionally
requires the exact cached Git checkout and performs no network operation. `lock --offline app.simi`
may write only from Git objects already in the cache.

Git caches are bare repositories under `$XDG_CACHE_HOME/simi/git`, or `$HOME/.cache/simi/git` when
`XDG_CACHE_HOME` is unset. Git is invoked noninteractively with terminal prompts disabled; Simi does
not configure authentication, run hooks, invoke Cargo, or run package build scripts.
