# A tour of Simi

Simi is a small, embeddable scripting language with mutable containers, expression-valued control flow, pipelines, structural patterns, value-based errors, and optional erased types. This tour starts with every runtime value and literal form, then builds through Simi's expression forms and standard library.

The complete companion program is [`examples/language-tour.simi`](../examples/language-tour.simi).

## Table of contents

- [1. Runtime values and literals](#1-runtime-values-and-literals)
- [2. The expression model](#2-the-expression-model)
- [3. Bindings, declarations, and patterns](#3-bindings-declarations-and-patterns)
- [4. Mutation, aliases, and copies](#4-mutation-aliases-and-copies)
- [5. Iterators](#5-iterators)
- [6. Modules and the standard library](#6-modules-and-the-standard-library)
- [7. Explicit text IO](#7-explicit-text-io)
- [8. Optional erased types](#8-optional-erased-types)
- [9. Errors and embedding boundaries](#9-errors-and-embedding-boundaries)
- [10. Current alpha boundaries](#10-current-alpha-boundaries)

Run a file with:

```sh
simi run examples/language-tour.simi
```

Scripts control their output explicitly. To inspect a script's final value as well, use:

```sh
simi run --inspect examples/language-tour.simi
```

Simi comments begin with `--` and continue to the end of the line.

## 1. Runtime values and literals

Simi is dynamically typed. Every value belongs to one of eight runtime categories:

```text
nil
boolean
integer
float
string
list
map
function
```

The shadowable builtin `type(value)` returns those labels as ordinary strings.

### Nil and booleans

```simi
nil
true
false
```

`nil` represents absence. It is not false: Simi has no general truthiness, so conditions and `not`, `and`, and `or` require booleans.

### Integers and floats

```simi
0
42
-7
3.14
-0.5
2e3
1.5E-2
```

Integers and finite floating-point numbers are different runtime categories. A leading minus is a unary operator rather than part of the literal. `/` always returns a float; `//` and `%` use floor-division semantics.

```simi
5 / 2   -- 2.5
5 // 2  -- 2
-5 // 2 -- -3
```

### Strings

Strings are Unicode text enclosed in double quotes:

```simi
"Simi"
"line one\nline two"
"quote: \""
"backslash: \\"
```

Supported escapes are `\"`, `\\`, `\n`, `\r`, and `\t`. String concatenation is strict:

```simi
"Hello, " <> "Ada"
```

Both operands of `<>` must be strings. Convert other values explicitly with functions such as `number.to_string` or the builtin `inspect`.

### Lists

Lists are mutable, ordered, zero-based, and may contain any value—including `nil`:

```simi
[]
[1, 2, 3]
["name", true, nil]
[1, [2, 3], {answer = 42}]
```

Trailing commas are accepted:

```simi
[1, 2, 3,]
```

A non-negative out-of-range read returns `nil`. Negative and non-integer indices are hard diagnostics. Writes replace existing positions and never grow a list.

### Maps

Maps are mutable insertion-ordered key/value containers:

```simi
{}
{
    name = "Ada",
    visits = 1,
    [true] = "enabled",
    [10] = "ten",
}
```

String, integer, finite non-integral float, and boolean keys are supported. A computed key uses brackets. Missing reads return `nil`.

Maps cannot retain `nil` values. A nil-valued literal entry is omitted, and assigning `nil` deletes an existing key:

```simi
user.nickname = nil
user[dynamic_key] = nil
```

### Functions

Functions are values. Anonymous functions may appear anywhere an expression is accepted:

```simi
fn(value) do
    value * 2
end
```

They capture their lexical environment and may be stored, passed, and returned.

Native functions supplied by the host and Simi functions both report `"function"` through `type`.

## 2. The expression model

Simi programs are sequences of declarations and expressions. Most constructs that would be statements in other languages evaluate to values in Simi.

### Literal, name, and parenthesized expressions

A literal evaluates to its value. A name reads its current lexical binding. Parentheses group an expression:

```simi
42
name
(1 + 2) * 3
```

### Standalone blocks

`do ... end` creates a fresh child scope and evaluates to its final item, or `nil` when empty:

```simi
let answer = do
    let left = 20
    let right = 22
    left + right
end
```

Bindings created inside the block do not escape it.

### Unary expressions

```simi
-number
not condition
```

Unary `-` requires a number. `not` requires a boolean.

### Binary expressions

Numeric arithmetic:

```simi
left + right
left - right
left * right
left / right
left // right
left % right
```

Strict string concatenation:

```simi
left <> right
```

Equality and ordering:

```simi
left == right
left != right
left < right
left <= right
left > right
left >= right
```

Ordering operators require numbers. Numeric comparisons handle mixed integer and float values without silently losing integer-boundary precision.

Boolean composition is strict and short-circuiting:

```simi
ready and valid
missing or fallback
```

Use parentheses when grouping is important. Arithmetic binds more tightly than concatenation, concatenation more tightly than comparisons, and pipelines come later.

### Calls

A call evaluates its callee and arguments, then returns the function's result:

```simi
add(1, 2)
callback(value)
factory()(argument)
```

Arguments are evaluated once from left to right. Wrong arity and calling a non-function are hard diagnostics.

### Field and index access

```simi
user.name
user[dynamic_key]
values[0]
```

Field access is string-key map access. Postfix calls, fields, and indexing compose:

```simi
factory().users[0].name
```

### Assignment expressions

Assignment updates an existing binding or mutates a list/map location. It evaluates to the assigned value and associates to the right:

```simi
count = count + 1
user.name = "Grace"
values[0] = 10
left = right = 0
```

Assigning to an undefined name is a hard diagnostic. `let` introduces a binding; assignment never implicitly creates one.

### Conditional expressions

`if` chooses one branch and evaluates to that branch's final value:

```simi
let label = if score >= 90 then
    "excellent"
elseif score >= 60 then
    "passing"
else
    "retry"
end
```

Conditions must be booleans. A missing `else` produces `nil` when no condition matches. Each selected branch has a child scope.

### Anonymous function expressions

```simi
let multiplier = 3
let scale = fn(value) do
    value * multiplier
end
```

The body is expression-valued. A function boundary contains loop control and postfix nil propagation from its callers.

### Pipeline expressions

`|>` inserts its input as the first argument of a call stage:

```simi
value |> transform(extra)
```

This is equivalent in argument placement to:

```simi
transform(value, extra)
```

A pipeline stage must visibly be a call.

`?>` is a nil-aware stage. A nil input skips the stage's callee and every argument lazily; a non-nil input behaves like `|>`:

```simi
maybe_user
?> load_profile()
?> format_profile()
```

Nil-awareness is stage-local. A later ordinary `|>` stage still receives nil.

The compound operators `|> tap` and `?> tap` run a stage for its effects, discard the stage result, and preserve the incoming value with the same alias identity:

```simi
values
|> tap list.append(4)
|> tap list.reverse()
```

`tap` is part of the compound pipeline operator. It is not a function or an identifier that can be used elsewhere.

Binding a tap result creates another alias, not a copy:

```simi
let alias = values |> tap list.append(5)
```

Here `alias` and `values` denote the same mutated list.

### Direct and trailing callbacks

Callback APIs are ordinary functions. Introduce them using a direct argument first:

```simi
values
|> list.iter()
|> iter.map(fn(value) do
    value * 2
end)
|> iter.to_list()
```

The right-associative `<|` operator optionally appends one trailing argument to the call on its left. It is useful when a multiline callback should end cleanly with `end` rather than `end)`:

```simi
values
|> list.iter()
|> iter.map() <| fn(value) do
    value * 2
end
|> iter.to_list()
```

A left operand of `<|` must be a call. It appends exactly one argument; it is not general partial application.

### Postfix nil propagation

Postfix `?` passes a non-nil value through. A nil value aborts the nearest lexically enclosing standalone `do ... end` block and makes that block evaluate to nil:

```simi
fn greeting(maybe_name) do
    do
        let name = maybe_name?
        "Hello, " <> name
    end
end
```

Nested standalone blocks stop propagation at the nearest boundary. `?` cannot cross a named or anonymous function body, and it does not intercept raises or hard diagnostics.

### Case expressions

`case` evaluates a value and selects the first matching pattern whose optional guard is true:

```simi
let message = case result
of { kind = "ok", value = value } do
    "received " <> value
of { kind = "error", error = error } when error != nil do
    "failed: " <> error
of _ do
    "unknown"
end
```

Guards must be booleans. Clause bindings are visible only in that clause. An unmatched case is a hard diagnostic.

### Raise expressions and try expressions

Any value may be raised:

```simi
raise { error = "not_found", key = key }
```

`try` protects one or more items and structurally matches a raised value:

```simi
let recovered = try
    prepare()
    operation()
catch { error = "not_found", key = key } do
    "missing: " <> key
catch error do
    raise error
end
```

Only raises from the protected block are considered by its catches. Raises from catch guards or bodies escape. Hard diagnostics and postfix nil propagation are not catches.

### Loop, continue, and break expressions

Loops are expressions and may thread state:

```simi
let result = loop state = 0 do
    if state < 3 then
        continue state + 1
    else
        break state
    end
end
```

The initializer runs once. Each ordinary iteration result supplies the next state. `continue value` transitions early; bare `continue` supplies nil. `break value` determines the loop's result.

A stateless loop omits the initializer:

```simi
loop do
    if finished() then break result() end
    continue
end
```

Loop control targets the nearest lexical loop and cannot escape a function body.

## 3. Bindings, declarations, and patterns

### Let bindings and shadowing

`let` introduces a lexical binding:

```simi
let count = 1
```

A repeated `let` creates a new symbol, even in the same scope:

```simi
let value = "first"
let earlier = fn() do value end
let value = "second"
```

`earlier()` still reads the first binding. Later reads use the second.

### Destructuring let

The left side of `let` may be a structural pattern:

```simi
let [first, second, ..rest] = values
let { name = name, ..settings } = user
```

The right side is evaluated once. Matching is atomic: no bindings are installed unless the complete pattern succeeds. A mismatch is a hard diagnostic; use `case` when failure is expected.

### Patterns

Patterns include literals, bindings, wildcards, lists, and maps:

```simi
case input
of 42 do
    "literal"
of name do
    name
of _ do
    "wildcard"
of [first, ..rest] do
    first
of { kind = "ok", value = value } do
    value
of { name = name, ..other } do
    other
end
```

Patterns may nest. List-rest captures an independent O(1) copy-on-write view. Map-rest creates an independent shallow map. Nested values keep their alias identities.

Named map fields normally require presence. The literal nil pattern is the exception: `{ missing = nil }` also matches an absent field because map lookup uses nil for absence.

### Named functions

A named function is a declaration rather than a function expression:

```simi
fn factorial(n) do
    if n == 0 then 1 else n * factorial(n - 1) end
end
```

Named functions support recursion and capture surrounding lexical bindings.

## 4. Mutation, aliases, and copies

Lists and maps are reference-like mutable values. Ordinary assignment copies the alias:

```simi
let values = [1, 2]
let alias = values
values[0] = 10
-- alias is now [10, 2]
```

`std/list` owns list-specific reads, copies, iteration, and mutation:

```text
length  get  contains
copy    slice  iter
set     append  extend  insert  remove  pop  reverse
```

`list.reverse` mutates in place and returns nil. In a pipeline, tap makes that effect explicit:

```simi
values |> tap list.reverse()
```

`list.copy` and `list.slice` create independent outer copy-on-write views. Their nested values remain shallow aliases.

Maps use language field/index assignment for mutation and provide collection-specific helpers such as `length`, `has`, `copy`, `clear`, and `iter`. Assigning nil deletes a key.

## 5. Iterators

`std/iter` contains generic lazy traversal. List and map modules only create collection-specific iterators:

```simi
let list = require("std/list")
let map = require("std/map")
let iter = require("std/iter")
```

`list.iter(values)` traverses an O(1) copy-on-write snapshot. Structural mutation of the original list after iterator creation does not change that traversal.

`map.iter(entries)` snapshots insertion-ordered entries and yields maps shaped like:

```simi
{ key = key, value = value }
```

### Lazy adapters

`iter.map` and `iter.filter` return new iterators. They do not invoke callbacks until the result is consumed:

```simi
let transformed =
    values
    |> list.iter()
    |> iter.filter(fn(value) do value >= 0 end)
    |> iter.map(fn(value) do value * 2 end)
```

### Consumers

Consumers advance the remaining iterator:

```text
to_list
fold
find
find_index
contains
any
all
each
count
```

Iterators are single-pass. Searches and boolean queries short-circuit and leave later elements available. `each` returns nil. Predicate callbacks must return booleans. Callback raises propagate unchanged.

```simi
let total =
    values
    |> list.iter()
    |> iter.fold(0) <| fn(sum, value) do
        sum + value
    end
```

### Steps and custom iterators

`iter.next(iterator)` returns a tagged map, never an untagged nil sentinel:

```simi
{ done = false, value = item }
{ done = true }
```

A legitimate nil item is represented as `{ done = false }`, because maps omit nil-valued fields. The `done` field is therefore the authoritative completion signal.

A custom iterator is a zero-argument function returning those steps:

```simi
fn countdown(start) do
    let current = start

    iter.from(fn() do
        if current <= 0 then
            { done = true }
        else
            let value = current
            current = current - 1
            { done = false, value = value }
        end
    end)
end
```

`iter.from` makes exhaustion sticky: after the first done step, the wrapped producer is not called again. A non-map step, a missing `done`, or a non-boolean `done` is a hard contract diagnostic. Extra fields are ignored.

Use `iter.fold` and ordinary functions for custom collection logic. There is no generic `collect` operation; `iter.to_list` is the concrete list consumer.

## 6. Modules and the standard library

Modules are explicit host-registered capabilities:

```simi
let string = require("std/string")
```

Repeated `require` calls within one engine return the same mutable export map. Separate engines have separate module registries and caches.

Portable standard-library engines provide:

```text
std/list
std/map
std/iter
std/number
std/string
```

`string.to_number(text)` parses a complete signed decimal integer or decimal/exponent float and returns nil for malformed, overflowing, or non-finite input:

```simi
"42" |> string.to_number()   -- integer 42
"42.0" |> string.to_number() -- float 42.0
"nope" |> string.to_number() -- nil
```

`string.concat(left, right)` is the pipeline-friendly equivalent of strict `<>`:

```simi
name |> string.concat("!")
```

The globals `require`, `type`, and `inspect` are shadowable ordinary bindings. `inspect` is cycle-safe human-readable rendering, not serialization.

## 7. Explicit text IO

Standard IO is an opt-in host capability exposed by one module:

```simi
let io = require("std/io")
```

Its initial text API is:

```text
read_line() -> string | nil
print(string) -> nil
println(string) -> nil
eprint(string) -> nil
eprintln(string) -> nil
```

The print family accepts strings only and flushes automatically. Rendering another value is explicit:

```simi
value
|> inspect()
|> io.println()
```

`read_line` removes the line ending and returns nil at EOF. Operational failures raise maps with `error = "io_error"` and an operation name.

Raw bounded `read` and `write` are intentionally absent until Simi has a bytes type.

The CLI registers `std/io`; a plain portable embedding does not. `simi run` leaves output to the script. `simi run --inspect` additionally renders the final expression, including nil. A future REPL may inspect results by default.

## 8. Optional erased types

Annotations improve analysis and editor feedback but never alter runtime behavior:

```simi
let count: integer = 1

fn display(value: integer | float) -> string do
    require("std/number").to_string(value)
end
```

Primitive static types are:

```text
never
nil
boolean
integer
float
string
any
```

There is deliberately no static `number`; use `integer | float`. `never` is the bottom type, and `any` is the explicit dynamic escape hatch.

Transparent aliases may be generic:

```simi
alias maybe<'a> = 'a | nil
alias pair<'a, 'b> = ['a, 'b]
```

Functions use arrows in type expressions:

```simi
alias transform = integer -> integer
alias predicate = (integer, string) -> boolean
```

Lists may have exact positional shapes or homogeneous rest shapes:

```simi
alias row = [integer, string]
alias integers = [..integer]
```

Maps use structural fields, open rests, and index signatures:

```simi
alias person = { name: string, age: integer }
alias named = { name: string, .. }
alias counts = { [string]: integer }
```

Named functions may describe normal-return mutation effects:

```simi
fn append(xs: [..'a], value: 'b) -> nil
    after xs becomes [..'a | 'b]
do
    host.call("private/append", xs, value)
end
```

`after` and `becomes` are contextual in that declaration form. Static types are erased: annotations cannot turn a dynamic mismatch into a runtime check.

For the complete design, see [the erased type-system reference](type-system.md).

## 9. Errors and embedding boundaries

Simi distinguishes catchable raised values from hard diagnostics. The host API preserves that distinction:

```rust
pub type ScriptResult = Result<Value, Raised>;
pub fn eval(source: &str) -> Result<ScriptResult, SimiError>;
```

Use nil for expected absence, raised structured values for recoverable operational failures, and hard diagnostics for programmer contract violations.

## 10. Current alpha boundaries

The initial alpha intentionally does not include:

- filesystem or package module discovery;
- command-line argument values;
- a bytes type or raw stream IO;
- serialization;
- a formatter or REPL;
- runtime tuples;
- iterator collection protocols;
- sequence/shape variables;
- advanced traits, protocols, or type constraints.

These omissions keep the runtime and embedding contract understandable while the language's core API settles.
