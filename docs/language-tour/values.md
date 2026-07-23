# Values

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- Values
  - [Nil and booleans](#nil-and-booleans)
  - [Integers and floats](#integers-and-floats)
  - [Strings](#strings)
  - [Lists](#lists)
  - [Maps](#maps)
  - [Functions are values](#functions-are-values)
- [Optional types](optional-types.md)
- [Expressions](expressions.md)
- [Functions and bindings](functions-and-bindings.md)
- [Control flow and patterns](control-flow-and-patterns.md)
- [Mutation and copies](mutation-and-copies.md)
- [Modules](modules.md)
- [Text IO](text-io.md)
- [Iterators](iterators.md)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## Nil and booleans

`nil` represents absence. Booleans are `true` and `false`:

```elixir
let missing = nil
let ready = true
let blocked = false

[missing, ready, blocked]
```

`nil` is not another spelling of `false`. Simi has no general truthiness: conditions and the operators `not`, `and`, and `or` require booleans.

```elixir
let has_name = true
let has_email = false

has_name and not has_email
```

Boolean operators short-circuit, so an unnecessary right-hand expression is not evaluated.

## Integers and floats

Simi distinguishes integers from finite floating-point numbers:

```elixir
[0, 42, -7, 3.14, -0.5, 2e3, 1.5E-2]
```

A leading `-` is a unary operator rather than part of the literal. The `/` operator always returns a float. `//` performs floor division, and `%` uses matching floor-division semantics:

```elixir
[
    5 / 2,    -- 2.5
    5 // 2,   -- 2
    -5 // 2,  -- -3
    -5 % 2,   -- 1
]
```

Arithmetic works across integer and float values. Mixed numeric comparisons preserve integer-boundary precision rather than silently converting every integer to a float.

## Strings

Strings are Unicode text enclosed in double quotes:

```elixir
[
    "Simi",
    "line one\nline two",
    "quote: \"",
    "backslash: \\",
]
```

Supported escapes are `\"`, `\\`, `\n`, `\r`, and `\t`.

The `<>` operator concatenates strings:

```elixir
let name = "Ada"
"Hello, " <> name <> "!"
```

Concatenation is strict: both operands must be strings. Convert numbers explicitly with `std/number`:

```elixir
let number = require("std/number")
"The answer is " <> number.to_string(42)
```

The builtin `inspect(value)` can produce a human-readable representation of any value. It is intended for display and debugging, not serialization.

## Lists

Lists are mutable, ordered, and zero-based. They may contain values of different categories, including `nil` and nested collections:

```elixir
let values = ["name", true, nil, [2, 3], {answer = 42}]
[values[0], values[3], values[4].answer]
```

An empty list is `[]`. Trailing commas are accepted:

```elixir
[1, 2, 3,]
```

Reading a nonnegative index beyond the end of a list returns `nil`:

```elixir
let values = [10, 20, 30]
values[10]
```

Negative or non-integer indices are hard runtime diagnostics. Writes replace existing positions and never grow a list; mutation and copying are covered later in the tour.

## Maps

Maps are mutable, insertion-ordered key/value containers. Identifier-like string keys use field syntax, while computed keys use brackets:

```elixir
let settings = {
    name = "Ada",
    visits = 1,
    [true] = "enabled",
    [10] = "ten",
}

[settings.name, settings["visits"], settings[true], settings[10]]
```

An empty map is `{}`. Map keys may be strings, integers, finite non-integral floats, or booleans. Missing reads return `nil`:

```elixir
let user = {name = "Ada"}
user.nickname
```

Maps cannot retain `nil` values. A nil-valued literal entry is omitted, and assigning `nil` deletes an existing key:

```elixir
let user = {name = "Ada", nickname = "ace", visits = 3}
let dynamic_key = "visits"

user.nickname = nil
user[dynamic_key] = nil
user
```

Lists are different: they may store `nil` as an element.

## Functions are values

Anonymous functions use `fn(parameters) do ... end`. Like other values, they can be stored in bindings, passed to other functions, and returned from functions:

```elixir
let multiplier = 2
let double = fn(value) do
    value * multiplier
end

double(21)
```

Functions capture bindings from their lexical environment; here, `double` captures `multiplier`. Functions written in Simi and native functions supplied by the host both report `"function"` through `type`.

The next page introduces optional erased type annotations for describing these runtime values without changing their behavior.

<!-- tour:navigation:start -->
---

[Previous: Hello, world!](hello-world.md)

[Next: Optional types](optional-types.md)
<!-- tour:navigation:end -->
