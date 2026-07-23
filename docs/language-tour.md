# A short tour of Simi

This is a progressive, approximately 15-minute tour. It uses the post-migration API: iteration lives in `std/iter`, strings use `<>` and `std/string` helpers, and standard I/O is an explicit opt-in. The complete example is [`examples/language-tour.simi`](../examples/language-tour.simi).

## 1. Values and final expressions

Simi has integers, finite floats, strings, booleans, `nil`, lists, maps, and functions. There is no special truthiness: `and`, `or`, and `not` require booleans. Every block is an expression; its last item is its value.

```simi
let answer = do
    let base = 6
    base * 7
end
```

`answer` is `42`. An empty block is `nil`. A line break does not end a construct: closing `end` is conventionally unindented.

## 2. Bindings, shadowing, and functions

`let` binds a name. A later `let` in a child scope may shadow it; assignment (`name = value`) updates an existing binding rather than creating one.

```simi
let greeting = "hello"
let greet = fn(name) do
    greeting <> ", " <> name
end

greet("Ada")
```

Functions are values, and closures retain their lexical environment. Named functions are convenient for recursion:

```simi
fn factorial(n) do
    if n == 0 then 1 else n * factorial(n - 1) end
end
```

`if` is also an expression. A missing `else` produces `nil`.

## 3. Lists and maps

Lists are mutable and zero-based. Maps are mutable, insertion-ordered, and accept string, integer, finite non-integral float, and boolean keys. Reading a missing key (or an out-of-range non-negative list index) returns `nil`; invalid indices are errors.

```simi
let user = { name = "Ada", visits = 1 }
user.visits = user.visits + 1
let numbers = [1, 2, 3]
numbers[0] = 10
let snapshot = numbers |> list.copy()
```

`list.copy` and `map.copy` are shallow independent containers: nested values remain aliases. Assigning `nil` to a map field deletes that field. List writes never grow a list.

## 4. Calls, callbacks, and pipelines

Start with an ordinary direct callback call. This keeps the callable and its arguments visible:

```simi
let doubled = list.iter(numbers).map(fn(value) do value * 2 end).collect()
```

The trailing-argument operator `<|` is an optional multiline spelling. It appends exactly one final argument to the call on its left:

```simi
let doubled = numbers |> list.iter() |> iter.map() <| fn(value) do
    value * 2
end |> iter.collect()
```

`|>` inserts its input as the first argument of a stage call. `?>` does the same, but skips the callee and all arguments when its input is `nil`; later stages still receive that `nil`. A stage must be a call.

`|> tap call(...)` and `?> tap call(...)` are compound operators: they perform the call for its effects, discard its result, and preserve the input—including alias identity. `tap` is not a function and cannot be used outside these pipeline stages.

## 5. Iterators

The post-migration standard library is `std/iter`. `list.iter(xs)` and `map.iter(table)` produce iterators; combinators are lazy until a terminal operation such as `collect`, `fold`, or `each`.

```simi
let iter = require("std/iter")
let evens = numbers
    |> list.iter()
    |> iter.filter(fn(value) do value % 2 == 0 end)
    |> iter.map(fn(value) do value * value end)
    |> iter.collect()
```

An iterator is consumed as it advances. `iter.next(iterator)` returns the next item or `nil`. Custom iterators can be made with `iter.unfold(initial, step)`: `step` returns `nil` to finish, or `{ value = item, state = next_state }` to yield an item and continue. This makes generators ordinary closures rather than a second control-flow system.

## 6. Patterns and control flow

`case` selects the first matching structural pattern. Guards must be booleans; list and map rests capture the remainder.

```simi
case user
of { name = name, visits = visits } when visits > 1 do
    name <> " is returning"
of { name = name } do
    name <> " is new"
of _ do
    "anonymous"
end
```

Loops are expressions too. `continue value` supplies the next state and `break value` supplies the result:

```simi
let total = loop state = 0 do
    if state < 4 then continue state + 1 else break state end
end
```

## 7. Raises and recovery

`raise` can carry any value. `try` catches only raises from its protected block; `nil` propagation and hard runtime diagnostics are not silently converted into catches.

```simi
let result = try
    raise { error = "not_found", key = "answer" }
catch { error = "not_found", key = key } do
    "missing: " <> key
end
```

Postfix `?` passes non-`nil` through, while `nil` aborts the nearest standalone `do ... end` block. It does not cross a function boundary.

## 8. Modules and explicit I/O

Use `require` for registered modules. The portable set includes `std/list`, `std/map`, `std/iter`, `std/number`, and `std/string`. `string.to_number(text)` returns a number or `nil`; `string.concat(left, right)` joins strings. The strict `<>` operator is the concise string concatenation form and rejects non-strings.

Standard I/O is one opt-in `std/io` capability. Its string-only `print`, `println`, and `flush` operations are explicit; input is `read_line`. A default `simi run` has no implicit I/O: request it with the explicit-I/O option, and use `--inspect` when you want the final value rendered. Embedders opt in through the engine builder instead.

## 9. Optional erased types

Annotations and aliases document and analyze programs but are erased at runtime:

```simi
alias MaybeText = string | nil
fn label(value: MaybeText) -> string do
    if value == nil then "none" else value end
end
```

The alpha is intentionally small: there is no filesystem/package discovery, serializer, formatter, tuple value, or static runtime enforcement. APIs and diagnostics may still evolve while the language settles.
