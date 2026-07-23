# Modules

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- [Optional types](optional-types.md)
- [Expressions](expressions.md)
- [Functions and bindings](functions-and-bindings.md)
- [Control flow and patterns](control-flow-and-patterns.md)
- [Mutation and copies](mutation-and-copies.md)
- Modules
  - [The portable standard library](#the-portable-standard-library)
  - [Module identity and state](#module-identity-and-state)
  - [Conversion and string helpers](#conversion-and-string-helpers)
  - [Prelude globals](#prelude-globals)
- [Text IO](text-io.md)
- [Iterators](iterators.md)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## The portable standard library

A standard-library engine registers these portable modules:

```text
std/list
std/map
std/iter
std/number
std/string
```

Each module groups operations for one kind of work. Import only the modules a script uses:

```elixir
let list = require("std/list")
let number = require("std/number")

let values = [10, 20, 30]
list.length(values)
|> number.to_string()
```

`std/io` is deliberately not in this portable set. It is a separate host capability covered on the next page.

## Module identity and state

A module's exports are a mutable map. Repeated `require` calls in one engine return the same map with the same alias identity, so mutations are visible through every reference:

```elixir
let first = require("std/string")
let second = require("std/string")

first.tour_marker = "shared"
second.tour_marker
```

The cache belongs to an `Engine`. Module state persists across evaluations made by that engine, while separate engines have separate registries and caches. The root `eval` convenience function uses a fresh standard-library engine for each call.

Source-backed modules are evaluated lazily on first use and then cached. A circular lazy load raises `{error = "circular_module_dependency", module = name}`.

## Conversion and string helpers

`string.to_number(text)` accepts a complete signed decimal integer or decimal/exponent float. Integer syntax produces an integer and float syntax produces a finite float. Malformed input, overflow, and non-finite results return `nil`.

```elixir
let string = require("std/string")

[
    string.to_number("42"),
    string.to_number("42.0"),
    string.to_number("6.02e23"),
    string.to_number("not a number"),
]
```

String concatenation with `<>` is strict: both operands must be strings. `string.concat(left, right)` provides the same operation in a pipeline-friendly call form.

```elixir
let string = require("std/string")
let name = "Ada"

name
|> string.concat("!")
|> string.upper()
```

## Prelude globals

Normal interpreters and all `Engine` evaluations provide `require`, `type`, and `inspect` as ordinary shadowable globals. `type` returns stable runtime category labels. `inspect` produces cycle-safe, human-readable text; it is not serialization.

```elixir
let list = require("std/list")
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
