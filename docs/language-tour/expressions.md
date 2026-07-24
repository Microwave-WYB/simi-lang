# Expressions

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- [Types and analysis](types-and-analysis.md)
- Expressions
  - [Basic expressions](#basic-expressions)
  - [Operators](#operators)
  - [Calls](#calls)
  - [Field and index access](#field-and-index-access)
  - [Assignment expressions](#assignment-expressions)
  - [Pipelines](#pipelines)
    - [Tap stages](#tap-stages)
  - [Callbacks and trailing arguments](#callbacks-and-trailing-arguments)
- [Functions and bindings](functions-and-bindings.md)
- [Control flow and patterns](control-flow-and-patterns.md)
- [Mutation and copies](mutation-and-copies.md)
- [Modules](modules.md)
- [Text IO](text-io.md)
- [Iterators](iterators.md)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## Basic expressions

A literal evaluates to its value, a name reads its current lexical binding, and parentheses control grouping:

```simi
let width = 6
let height = 7
let result = (width * height) + 1
result
```

Lists and maps can contain expressions too:

```simi
let base = 20
[base + 1, base + 2, {answer = base * 2 + 2}]
```

## Operators

Unary `-` negates a number. Unary `not` requires a boolean:

```simi
let temperature = 5
let enabled = false
[-temperature, not enabled]
```

Simi provides the usual numeric arithmetic operators:

| Operator | Meaning |
| --- | --- |
| `+` | addition |
| `-` | subtraction |
| `*` | multiplication |
| `/` | division, always producing a float |
| `//` | floor division |
| `%` | remainder using floor-division semantics |

```simi
[5 + 2, 5 - 2, 5 * 2, 5 / 2, 5 // 2, 5 % 2, -5 // 2]
```

The strict string-concatenation operator is `<>`. Both operands must be strings:

```simi
let greeting = "Hello"
let name = "Ada"
greeting <> ", " <> name <> "!"
```

Equality uses `==` and `!=`. Ordering uses `<`, `<=`, `>`, and `>=` and requires numeric operands. Integers and floats compare numerically, including at integer boundaries where a lossy float conversion would give the wrong answer:

```simi
[
    1 == 1.0,
    1 != 2,
    3 < 4,
    4 <= 4.0,
    5 > 2,
    5 >= 5,
]
```

Boolean `and` and `or` require booleans and short-circuit. Simi has no general truthiness:

```simi
let ready = true
let valid = false
[ready and valid, ready or valid, not valid]
```

From tighter to looser grouping, the common operator levels are unary operators, multiplication/division, addition/subtraction, concatenation, comparisons, `and`, and `or`. Use parentheses whenever they make intent clearer:

```simi
let subtotal = 10
let surcharge = 2
let label = "total: "
[label <> inspect(subtotal + surcharge), (1 + 2) * 3 == 9]
```

Operator contract violations—such as multiplying a string, applying `not` to a number, or dividing by zero—do not quietly produce `nil`. Later pages distinguish hard diagnostics from raised values.

## Calls

A call evaluates its callee and arguments, then produces the function's result. Arguments are evaluated once, from left to right:

```simi
fn add(left, right) do
    left + right
end

let result = add(20, 22)
result
```

Calls compose with other postfix operations, so a returned function can be called immediately:

```simi
fn make_greeter(greeting) do
    fn(name) do
        greeting <> ", " <> name
    end
end

make_greeter("Hello")("Ada")
```

Calling a non-function or supplying the wrong number of arguments is a hard diagnostic.

## Field and index access

Dot access reads a string-keyed map field. Brackets read a computed map key or a zero-based list index:

```simi
let selected_key = "role"
let user = {name = "Ada", role = "engineer"}
let scores = [91, 95, 98]
[user.name, user[selected_key], scores[1]]
```

Missing map fields and nonnegative out-of-range list reads return `nil`. Postfix calls, fields, and indexing can be chained:

```simi
fn load_team() do
    {members = [{name = "Ada"}, {name = "Grace"}]}
end

load_team().members[1].name
```

## Assignment expressions

`let` introduces a binding; assignment updates an existing one. Assignment itself evaluates to the assigned value:

```simi
let count = 1
let new_count = count = count + 1
[count, new_count]
```

Assignment is right-associative:

```simi
let left = 1
let right = 2
left = right = 0
[left, right]
```

A field or index assignment mutates a container location and also evaluates to the assigned value:

```simi
let user = {name = "Ada"}
let values = [1, 2, 3]
let renamed = user.name = "Grace"
let replaced = values[0] = 10
[user, values, renamed, replaced]
```

Assigning to an undefined name is a hard diagnostic. List writes replace existing positions and never grow a list. Assigning `nil` to a map field or key deletes that entry; mutation and copy behavior are covered in [Mutation and copies](mutation-and-copies.md).

## Pipelines

`|>` passes its input as the first argument to a call stage:

```simi
fn add(value, extra) do
    value + extra
end

fn multiply(value, factor) do
    value * factor
end

10
|> add(5)
|> multiply(2)
```

The first stage above is equivalent to `add(10, 5)`. A pipeline stage must visibly be a call.

The nil-aware `?>` behaves like `|>` for a non-`nil` input. For a `nil` input, it skips that stage's callee and all its arguments:

```simi
fn increment(value) do
    value + 1
end

let maybe_number = nil
maybe_number
?> increment()
?> increment()
```

Nil-awareness is stage-local. An ordinary `|>` later in the same chain still receives the preceding `nil`. [Control flow and patterns](control-flow-and-patterns.md) covers broader nil-directed control flow.

### Tap stages

`|> tap` performs a stage call for its effects, discards the call's result, and preserves the original input with the same alias identity:

```simi
let list = require("std/list")
let values = [1, 2, 3]

values
|> tap list.append(4)
|> tap list.reverse()
```

`?> tap` adds the same nil-aware skipping rule. `tap` belongs to these compound pipeline operators; it is not a function or an identifier that can be used elsewhere.

Binding a tap pipeline result creates another alias rather than a copy:

```simi
let list = require("std/list")
let values = [1, 2, 3]
let alias = values |> tap list.append(4)
[values, alias]
```

## Callbacks and trailing arguments

Functions are values, so callback APIs first work through ordinary call arguments. This complete script creates a lazy mapped iterator and consumes it into a list:

```simi
let list = require("std/list")
let iter = require("std/iter")
let values = [1, 2, 3]

values
|> list.iter()
|> iter.map(fn(value) do
    value * 2
end)
|> iter.to_list()
```

The optional `<|` operator appends its right operand as exactly one final argument to the call on its left. It is useful when a multiline callback should end with `end` instead of `end)`:

```simi
let list = require("std/list")
let iter = require("std/iter")
let values = [1, 2, 3]

values
|> list.iter()
|> iter.map() <| fn(value) do
    value * 2
end
|> iter.to_list()
```

The left side of `<|` must be a call. The operator is right-associative and is not general partial application. [Functions and bindings](functions-and-bindings.md) develops named functions, anonymous functions, and closures; [Iterators](iterators.md) returns to iterator callbacks in depth.

<!-- tour:navigation:start -->
---

[Previous: Types and analysis](types-and-analysis.md)

[Next: Functions and bindings](functions-and-bindings.md)
<!-- tour:navigation:end -->
