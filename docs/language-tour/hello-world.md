# Hello, world!

<!-- tour:contents:start -->
## Tour contents

- Hello, world!
  - [Inspecting a result](#inspecting-a-result)
  - [Comments](#comments)
- [Values](values.md)
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

## Inspecting a result

Simi programs are expression-oriented, so a script has a final value. This complete script evaluates to a string but does not print it itself:

```elixir
"Hello from a final value"
```

Use `--inspect` when you want the CLI to render that final value:

```sh
simi run --inspect hello.simi
```

This is useful while exploring the language. Regular programs can continue to control their output through `std/io`.

## Comments

A comment begins with `--` and continues to the end of the line:

```elixir
let io = require("std/io")

-- This line is ignored by Simi.
io.println("Comments keep notes beside the code.") -- This is a comment too.
```

In the next page, you will meet the values that Simi programs can create and manipulate.

<!-- tour:navigation:start -->
---

[Next: Values](values.md)
<!-- tour:navigation:end -->
