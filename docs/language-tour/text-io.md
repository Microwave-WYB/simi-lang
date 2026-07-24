# Text IO

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- [Types and analysis](types-and-analysis.md)
- [Expressions](expressions.md)
- [Functions and bindings](functions-and-bindings.md)
- [Control flow and patterns](control-flow-and-patterns.md)
- [Mutation and copies](mutation-and-copies.md)
- [Modules](modules.md)
- Text IO
  - [The text API](#the-text-api)
  - [Reading lines](#reading-lines)
  - [Rendering non-string values](#rendering-non-string-values)
  - [Failures and capability boundaries](#failures-and-capability-boundaries)
- [Iterators](iterators.md)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## The text API

The module currently exposes five operations:

```text
read_line() -> string | nil
print(string) -> nil
println(string) -> nil
eprint(string) -> nil
eprintln(string) -> nil
```

`print` and `println` write to standard output. `eprint` and `eprintln` write to standard error. The `ln` variants append a line ending. All four output operations flush automatically and return `nil` after a successful write.

```simi
let io = require("std/io")

io.print("loading... ")
io.println("done")
io.eprintln("This line goes to standard error")
```

Automatic flushing means a prompt written with `print` is visible before the following read waits for input.

## Reading lines

`read_line()` reads Unicode text, removes its line ending, and returns the remaining string. It returns `nil` at end of file, so handle that ordinary absence explicitly.

```simi
let io = require("std/io")

io.print("What is your name? ")
let name = io.read_line()

if name == nil then
    io.eprintln("No name was provided")
else
    io.println("Hello, " <> name <> "!")
end
```

A blank input line is an empty string, not `nil`. Only end of file produces `nil`.

## Rendering non-string values

The print family accepts strings only. Passing an integer, list, map, or other non-string value is a hard contract diagnostic. Convert numbers with `std/number`, or use the global `inspect` function when human-readable rendering is appropriate.

```simi
let io = require("std/io")
let value = {answer = 42}

value
|> inspect()
|> io.println()
```

`inspect` is intended for display and debugging, not as a serialization format.

## Failures and capability boundaries

Stream failures are recoverable language raises shaped like:

```text
{error = "io_error", operation = operation, message = message}
```

The `operation` field identifies the originating operation, including an automatic flush failure. Wrong arity and wrong argument types remain hard diagnostics rather than raised IO values.

Raw bounded `read` and `write` are intentionally absent until Simi has a bytes type.

When running a file, `simi run` leaves output entirely to the script. `simi run --inspect` also renders the script's final value, including `nil`; that extra rendering is CLI behavior, not an implicit language print.

<!-- tour:navigation:start -->
---

[Previous: Modules](modules.md)

[Next: Iterators](iterators.md)
<!-- tour:navigation:end -->
