# Types and analysis

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- Types and analysis
  - [The erasure contract](#the-erasure-contract)
  - [Primitive types and unions](#primitive-types-and-unions)
  - [Function types and generics](#function-types-and-generics)
  - [Callable bounds and raised effects](#callable-bounds-and-raised-effects)
  - [Structural list types](#structural-list-types)
  - [Structural map types](#structural-map-types)
  - [Flow analysis and narrowing](#flow-analysis-and-narrowing)
  - [Mutation, aliases, and analysis](#mutation-aliases-and-analysis)
- [Expressions](expressions.md)
- [Functions and bindings](functions-and-bindings.md)
- [Control flow and patterns](control-flow-and-patterns.md)
- [Mutation and copies](mutation-and-copies.md)
- [Modules](modules.md)
- [Text IO](text-io.md)
- [Iterators](iterators.md)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## The erasure contract

An annotated script is still an ordinary Simi script. Removing its type syntax must not change its values, mutation, errors, module behavior, or host result.

```simi
let count: integer = 1

fn display(value: integer | float) -> string do
    require("std/number").to_string(value)
end

display(count)
```

Annotations do not insert runtime checks or conversions. This script may receive a static diagnostic, but it still evaluates to the string at runtime:

```simi
-- Expected type diagnostic: the annotation says integer.
let deliberately_wrong: integer = "still dynamic"
deliberately_wrong
```

Types also occupy a separate namespace from runtime bindings, so an alias and a value may have the same lowercase name:

```simi
alias option<'a> = 'a | nil
let option = 42
option
```

## Primitive types and unions

The initial primitive static vocabulary is:

```text
never
nil
boolean
integer
float
string
any
```

There is deliberately no static `number` type. Use the union `integer | float` for values accepted from either numeric runtime category.

`never` is the bottom type: it describes an expression with no normal return path and disappears when unions are normalized. `any` is the explicit dynamic escape hatch; operations involving it remain valid but lose static precision. When analysis has insufficient evidence, editor presentation also falls back to `any`.

The `|` operator forms unions. String literals may be singleton types, which makes them useful as discriminants:

```simi
alias mode = "read" | "write"
alias maybe_name = string | nil

let selected: mode = "read"
let name: maybe_name = nil
[selected, name]
```

Every primitive literal can be written explicitly as a singleton annotation type: `nil`, `true`, `false`, strings, integers, and finite floats. Numeric literals and ordinary Boolean expression values still infer as `integer`, `float`, and `boolean`; an annotation such as `let port: 8080 = 8080` opts into the singleton contract. Each singleton is a subtype of its primitive category, but a broad primitive is not a subtype of one singleton. Integral float spellings remain float singletons, while `-0.0` normalizes to the same singleton as `0.0`.

Direct named map fields are the narrow Boolean inference exception: `{done = false}` preserves `false` as a record-discriminant fact, while arbitrary Boolean variables, calls, and operators remain `boolean`. The union `true | false` normalizes to `boolean`. `nil` remains an ordinary union member.

## Function types and generics

Function types use arrows:

```simi
alias transform = integer -> integer
alias predicate = (integer, string) -> boolean
alias supplier = () -> integer

let double: transform = fn(value: integer) -> integer do
    value * 2
end

double(21)
```

Arrows associate to the right, so `integer -> string -> boolean` means a function taking an integer and returning a function from string to Boolean. Parentheses distinguish a fixed parameter list from one parameter.

Generic variables begin with an apostrophe. Alias parameters are declared explicitly and applied with angle brackets:

```simi
alias option<'a> = 'a | nil
alias pair<'a, 'b> = ['a, 'b]

let name: option<string> = nil
let entry: pair<integer, string> = [1, "one"]
[name, entry]
```

Free generic variables in function annotations are implicitly quantified:

```simi
fn identity(value: 'a) -> 'a do
    value
end

fn transform(value: 'a, callback: 'a -> 'b) -> 'b do
    callback(value)
end

transform(identity(20), fn(value: integer) -> integer do
    value + 22
end)
```

Callers do not supply explicit generic arguments. Syntax such as `identity<string>(value)` is outside the initial design. Aliases are transparent: expanding one creates neither a nominal type nor a new runtime value category.

## Callable bounds and raised effects

An explicit callable header may bound generic variables with ordinary Simi types:

```simi
fn negate<'a: integer | float>(value: 'a) -> 'a noraise do
    -value
end

negate(42)
```

The leading `|` is optional for every union and is convenient when variants are written on separate lines. Bounds are erased and do not introduce traits, protocols, runtime checks, or explicit generic call arguments.

Callable parameter labels improve signatures while calls remain positional:

```simi
let compare: (left: integer, right: integer) -> boolean noraise =
    fn(left: integer, right: integer) -> boolean noraise do
        left < right
    end

compare(1, 2)
```

A function's raised type is separate from its normal result:

```simi
fn fail(value: 'e) -> never raises 'e do
    raise value
end

fn recover() -> integer noraise do
    do
        fail("missing")
    catch of "missing"
        0
    end
end

recover()
```

Omitting the clause infers the effect. `raises E` declares an upper bound, and `noraise` means `raises never`. Effect variables propagate through callback signatures, while hard diagnostics and postfix `?` remain outside the raised channel. An effect after a chained arrow belongs to the nearest right-hand callable; parentheses select an outer callable explicitly.

## Structural list types

All list types describe the existing mutable runtime list. Simi does not have a separate runtime tuple category.

A bracketed comma list is an exact positional shape, while a rest element describes a homogeneous list of arbitrary length:

```simi
alias row = [integer, string]
alias integers = [..integer]
alias matrix = [..[..integer]]

let row_value: row = [1, "Ada"]
let values: integers = [1, 2, 3]
let rows: matrix = [[1, 2], [3]]
[row_value, values, rows]
```

The empty list literal begins with exact shape `[]`. Analysis retains exact shapes through known mutations when possible, then widens when repeated growth or uncertain alias mutation prevents a safe exact description. Nested lists may be ragged; symbolic dimensions and rectangularity proofs are not part of the initial system.

## Structural map types

Structural records and index signatures describe the existing mutable runtime map. Records are closed by default; `..` permits additional fields:

```simi
alias person = {name: string, age: integer}
alias named = {name: string, ..}
alias counts = {[string]: integer}

let ada: person = {name = "Ada", age = 36}
let enabled: named = {name = "feature", active = true}
let totals: counts = {apples = 2, pears = 3}
[ada, enabled, totals]
```

An index signature may use a key union, and compatible known fields may coexist with it:

```simi
alias flags = {[string | integer]: boolean}
let values: flags = {ready = true, [1] = false}
values
```

A dynamic map read includes `nil`, because the requested key may be absent. A required known field can retain its declared type while analysis still proves its presence.

Discriminated record unions combine literal fields with structural maps. Write single-line unions without a leading `|`; when a union spans multiple lines, begin every member line with `|`:

```simi
alias result<'value, 'error> =
    | {kind: "ok", value: 'value}
    | {kind: "error", error: 'error}

let outcome: result<integer, string> = {kind = "ok", value = 42}

case outcome
of {kind = "ok", value = value}
    value
of {kind = "error", error = error}
    error
end
```

Boolean fields can discriminate record unions too:

```simi
alias Step<'a> =
    | {done: true, ..}
    | {done: false, value: 'a, ..}

fn step_value(step: Step<integer>) -> integer | nil do
    if step.done then
        nil
    else
        step.value
    end
end

step_value({done = false, value = 42})
```

The `if step.done` branches select the corresponding record variants, so `step.value` has type `integer` in the `else` branch. Before that check, the done variant makes `value` not definitely present.

Pattern matching and equality can narrow these unions. Exhaustiveness analysis may warn about a missing case, but erasure preserves the runtime rule: an unmatched `case` is still a hard diagnostic.

## Flow analysis and narrowing

Analysis narrows branch-local types through:

- comparisons against the resolved builtin `type`;
- literal equality and inequality;
- discriminant fields;
- successful structural patterns and strict Boolean guards;
- explicit comparisons with `nil`.

```simi
fn describe(value: integer | string) -> string do
    if type(value) == "integer" then
        require("std/number").to_string(value)
    else
        value
    end
end

describe(42)
```

Because `type` is an ordinary shadowable binding, only a call resolved to the builtin receives this special narrowing. Simi has no dedicated runtime-category operator.

`not`, `and`, and `or` compose narrowing facts according to their strict Boolean, short-circuiting semantics. Assignment replaces a flow fact, and mutation invalidates facts that depend on changed container structure.

Postfix `?` removes `nil` on the surviving path inside its nearest lexical block. The block's normal and nil-abort exits join again outside that boundary:

```simi
fn greeting(name: string | nil) -> string | nil do
    let present = name?
    "Hello, " <> present
end

greeting(nil)
```

A nil-abort directly from a loop body ends that iteration and starts the next one; it does not determine the loop result, which only comes from `break value`. A `?>` stage similarly splits its nil-skipped and active paths, applies call effects only on the active path, and rejoins before the next pipeline stage. A following ordinary `|>` therefore sees the complete result union.

## Mutation, aliases, and analysis

Runtime lists and maps remain mutable and aliased. An erased annotation cannot freeze a container or restrict later values. Analysis updates facts after known mutation and widens them after mutation through an alias, unresolved call, or unknown native function when a stronger claim is unsafe.

Simi refines mutable list and map shapes only while a binding remains in its defining lexical scope. An explicit annotation or closure capture seals that binding's analyzer-visible contract. A closure may mutate a sealed capture only compatibly; it cannot implicitly widen a captured list element type or add a captured map field. Calls do not carry caller-visible mutation postconditions.

Because maps delete `nil` values, `{count: integer | nil}` means that `count` may be absent or may hold an integer.

<!-- tour:navigation:start -->
---

[Previous: Values](values.md)

[Next: Expressions](expressions.md)
<!-- tour:navigation:end -->
