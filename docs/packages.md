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
paths. Absolute paths, backslashes, empty path segments, `.`, and `..` are rejected. A resolver
must additionally reject package-root symlink escapes when it reads a checkout.

## Restricted metadata

The top-level map permits only:

- `name`: non-empty lowercase package identity;
- `simi`: compatible Simi runtime revision string;
- `modules`: non-empty list of public module-name strings, including `name` itself;
- `native`: optional `{manifest = "relative/path/Cargo.toml"}` metadata for later native runners.

Functions, calls, bindings, variables, computed values, duplicate keys, and unrecognized fields
are invalid. The resolver computes a source-tree digest from the declared package tree using this
canonical layout; generated files and symlink policy are resolver concerns, not executable
metadata.

## Requirements and documentation

Requirements belong to a leading static `requires` declaration in source files, not to executable
package code. The shared `parse_requires` API parses and validates this header without evaluating
any Simi expression. It returns only static metadata and source spans; it does not resolve aliases,
read paths, access Git or the network, create lockfiles, or grant `Engine::eval` any new authority.
A later package resolver may consume the validated metadata transitively before evaluation.

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

Package-local imports are literal `require("./...")` paths. They resolve relative to the importing
source file, are confined to the package root, and are prepared as catalog modules before
execution. They are not ambient filesystem access.

For example, `polars.simi` may use:

```simi
let schema = require("./src/schema.simi")
```

A bare engine without an explicitly resolved catalog rejects such imports before evaluation.
