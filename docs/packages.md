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
`PackageTree` loader rejects a symlink root and any symlink component used by the manifest, a
declared public module, or a reachable literal local source; it never discovers arbitrary private
sources.

## Restricted metadata

The top-level map permits only:

- `name`: non-empty lowercase package identity;
- `simi`: compatible Simi runtime revision string;
- `modules`: non-empty list of public module-name strings, including `name` itself;
- `native`: optional `{manifest = "relative/path/Cargo.toml"}` metadata for later native runners.

Functions, calls, bindings, variables, computed values, duplicate keys, and unrecognized fields
are invalid. `PackageTree` exposes deterministic digest inputs consisting of the manifest, declared
public modules, and every reachable package-local source sorted by canonical source path. Unlisted
private, generated, and native files remain excluded. Metadata itself never controls filesystem or
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

Each alias is a unique lowercase Simi identifier within its declaring source file. Aliases are
lexical metadata only, so independent packages may reuse an alias for different dependencies. Its
value is restricted to exactly one of these static maps:

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

A package source unit may import a private source with a literal `require("./...")` path relative
to the importing source file:

```simi
let schema = require("./src/schema.simi")
```

The resolver discovers these imports while it prepares the locked package graph, before any package
code evaluates. Paths must begin with `./`, be non-empty slash-separated paths, and contain no `.`,
`..`, empty, or backslash components. This keeps every loaded source below the package root; a
traversal attempt is rejected during resolution. A dynamic `require` is not a package-local import,
and bare engines continue to treat all `require` calls only as registered-module lookups.

Every reachable local source is read through the same non-symlink package-tree checks as public
modules, added to the package tree digest, and registered under a deterministic package-scoped
identity. If a local path names a declared public source, it uses that existing public module
identity instead. Repeated local imports therefore share normal source-module cache identity, and
cycles retain the normal `{ error = "circular_module_dependency", ... }` result. Plain catalog
imports such as `require("std/string")` are unchanged.

This is resolver work only: `Engine::eval` does not read package paths or gain filesystem, network,
or lockfile authority.

## Locks and source resolution

For `app.simi`, `simi lock app.simi` writes `app.lock.simi`. The lock is canonical restricted Simi
data, parsed but never evaluated:

```simi
{
    format = 2,
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

Requirement keys are resolved manifest package identities and are sorted, as are all complete
transitive entries. The resolver rejects a graph that resolves one package identity from
conflicting declared sources; aliases never identify lock entries or registered modules. Package
identities that are not Simi identifiers (for example, `tool-box`) use a quoted computed map key
in the canonical lockfile.
A Git revision is resolved to an exact commit. Tree digests use SHA-256 over sorted `PackageTree`
inputs, framing every path and content byte sequence with its UTF-8 byte length; filesystem iteration
order never affects a digest. Path requirements are relative to the declaring source file (or package
root for a package source), and their package tree is also locked by digest.

`run app.simi` resolves requirements and refreshes the lock before constructing an Engine. It registers
declared public package modules by their manifest names plus resolver-discovered package-local
sources by internal package-scoped identities, so `require("polars")` and `require("polars/csv")`
work without granting source code ambient filesystem access.
`run --locked app.simi` requires an existing canonical lock and validates source declarations,
commits, and tree digests without fetching or rewriting. `run --offline app.simi` additionally
requires the exact cached Git checkout and performs no network operation. `lock --offline app.simi`
may write only from Git objects already in the cache.

Git caches are bare repositories under `$XDG_CACHE_HOME/simi/git`, or `$HOME/.cache/simi/git` when
`XDG_CACHE_HOME` is unset. Git is invoked noninteractively with terminal prompts disabled; Simi does
not configure authentication, run hooks, invoke Cargo, or run package build scripts.
