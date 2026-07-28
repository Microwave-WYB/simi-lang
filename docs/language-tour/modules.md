# Modules

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- [Types and analysis](types-and-analysis.md)
- [Expressions](expressions.md)
- [Functions and bindings](functions-and-bindings.md)
- [Control flow and patterns](control-flow-and-patterns.md)
- [Mutation and copies](mutation-and-copies.md)
- Modules
  - [The portable standard library](#the-portable-standard-library)
  - [Module identity and state](#module-identity-and-state)
  - [Conversion and string helpers](#conversion-and-string-helpers)
  - [Byte helpers](#byte-helpers)
  - [Prelude globals](#prelude-globals)
- [Text IO](text-io.md)
- [Iterators](iterators.md)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## The portable standard library

`Engine::new()` and `Engine::with_stdlib()` provide the same portable, shadowable value prelude:

```text
list
map
iter
number
string
```

Each prelude value also has a canonical `std/*` module path for `require`. Direct use is the ordinary form:

```simi
let values = [10, 20, 30]
list.length(values)
|> number.to_string()
```

`std/io` is deliberately not in this portable set. It is a separate host capability covered on the next page.

## Module identity and state

A module's exports are a mutable map. Prelude values and repeated `require` calls in one engine share the same map with the same alias identity, so mutations are visible through every reference:

```simi
string.tour_marker = "shared"
let second = require("std/string")

second.tour_marker
```

The cache belongs to an `Engine`. Module state persists across evaluations made by that engine, while separate engines have separate registries and caches. The root `eval` convenience function uses a fresh standard-library engine for each call.

Source-backed modules are evaluated lazily on first use and then cached. A circular lazy load raises `{error = "circular_module_dependency", module = name}`.

Hosts may override a portable module's canonical path. During portable prelude installation, all five canonical module values are resolved before any of their global aliases are installed. Source-backed overrides therefore do not see a partially installed portable value prelude; they use explicit `require("std/...")` calls for module dependencies. If every load succeeds, the five globals are installed together and retain identity with their canonical cached values. A raised or circular override load remains a language raise from `Engine::eval`.

## Conversion and string helpers

`string.to_number(text)` accepts a complete signed decimal integer or decimal/exponent float. Integer syntax produces an integer and float syntax produces a finite float. Malformed input, overflow, and non-finite results return `nil`.

```simi
[
    string.to_number("42"),
    string.to_number("42.0"),
    string.to_number("6.02e23"),
    string.to_number("not a number"),
]
```

String concatenation with `<>` is strict: both operands must be strings. `string.concat(left, right)` provides the same operation in a pipeline-friendly call form.

```simi
let name = "Ada"

name
|> string.concat("!")
|> string.upper()
```

## Byte helpers

The portable `std/bytes` module is available explicitly through `require`; it intentionally does not add a `bytes` prelude global. It provides immutable octet inspection, O(1) range views, concatenation, and conversion to or from integer lists. List input must contain only integers from `0` through `255`.

```simi
let bytes = require("std/bytes")
let header = bytes.from_list([137, 80, 78, 71])
let body = bytes.slice(header, 1, 20)

[bytes.length(body), bytes.get(body, 2), bytes.to_list(body)]
```

`bytes.get` returns `nil` beyond the end. Negative and non-integer indices, invalid octets, and wrong argument categories are hard diagnostics.

## Prelude globals

Normal `Engine` evaluations provide `require`, `type`, `inspect`, `list`, `map`, `iter`, `number`, and `string` as ordinary shadowable globals. `require("std/list")` through `require("std/string")` return the same cached values as their prelude names, while `require("std/bytes")` provides the explicit bytes module. `type` returns stable runtime category labels. `inspect` produces cycle-safe, human-readable text; it is not serialization.

```simi
let values = []
list.append(values, values)

[type(values), inspect(values)]
```

The low-level `Interpreter::with_globals` Rust constructor is different: its supplied environment is complete, so it does not add this prelude automatically.

<!-- tour:navigation:start -->
---

[Previous: Mutation and copies](mutation-and-copies.md)

[Next: Text IO](text-io.md)
<!-- tour:navigation:end -->
