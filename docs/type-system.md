# Future erased type system

> **Design only:** type annotations, aliases, and type grammar described here are
> not currently accepted by the Simi parser. Some examples also contain ordinary
> executable Simi expressions. The runtime remains dynamically typed, and
> analysis must never change program execution.

This document defines the initial target for optional static analysis. Its scope
is intentionally comparable to LuaLS: useful annotations, structural container
types, ordinary generics, unions, literals, and narrowing without dependent
shapes or compile-time execution.

## Core contract

Types are erased metadata in a namespace separate from runtime bindings. A type
alias and a runtime binding may therefore have the same lowercase name. Scripts
without annotations remain complete Simi programs, and annotations must not
change evaluation, mutation, errors, module behavior, or host result layering.

Annotations are inline and optional:

```simi
let count: int = 1

fn display(value: int) -> string do
    number.to_string(value)
end

let callback: (int, int) -> int = add
```

The initial primitive vocabulary includes:

```text
nil
boolean
int
float
number
string
any
```

`number` covers both `int` and `float`. `any` is the explicit dynamic escape
hatch: operations involving it remain valid but lose static precision.

The static integer spelling is `int`. Runtime reflection deliberately remains
unchanged for compatibility:

```simi
type(value) == "integer"
```

When `type` resolves to the builtin, the analyzer may narrow that comparison to
static `int`. Migrating the runtime label from `"integer"` to `"int"` is a
separate future compatibility decision.

## Functions and generics

Function types use arrows:

```simi
int -> int
(int, int) -> int
() -> int
int -> string -> boolean
```

Arrows associate to the right. Parentheses distinguish fixed parameter lists
from a single parameter.

Generic variables begin with an apostrophe. Alias parameters are explicit, and
type application uses parentheses rather than angle brackets:

```simi
alias option('a) = 'a | nil
alias pair('a, 'b) = ['a, 'b]

let name: option(string) = nil
```

Free generic variables in a function annotation are implicitly quantified:

```simi
fn identity(value: 'a) -> 'a do
    value
end

fn transform(value: 'a, callback: 'a -> 'b) -> 'b do
    callback(value)
end
```

Callers never supply explicit generic arguments. Forms such as
`identity(string)(value)` are not part of the initial design.

Aliases are transparent: expanding an alias does not create a new runtime or
nominal identity.

## Unions and literal types

`|` forms unions. String, integer, and Boolean literals may be types; `nil` is
also a type and ordinary union member:

```simi
alias mode = "read" | "write"
alias status_code = 200 | 404 | 500
alias switch = true | false
alias maybe_name = string | nil
```

Literal fields support discriminated structural records:

```simi
alias result('value, 'error) =
    { ok: true, value: 'value }
    | { ok: false, error: 'error }
```

Pattern matching and ordinary equality may narrow these unions. Exhaustiveness
analysis may warn about missing cases, but it does not change the current
runtime rule that an unmatched `case` is a hard error.

## Structural lists

All positional container types describe the existing mutable runtime `List`.
There is no runtime tuple category.

A bracketed comma list is an exact positional shape:

```simi
[int, string]
[boolean, int, string]
```

A rest element describes a homogeneous arbitrary-length list:

```simi
[..int]
[..string]
[..[..int]]
```

Nested lists are allowed and may be ragged. Exact tuples can describe fixed
positions, but the initial system does not track symbolic dimensions or prove
rectangular matrix shapes.

These structural forms are sufficient as the primitive surface. Libraries may
provide transparent aliases for common list shapes, but `list('a)` need not be
a primitive type constructor.

## Structural maps

All record and index-signature types describe the existing mutable runtime
`Map`. There is no user-defined runtime record or map category.

A record is closed by default:

```simi
{ name: string, age: int }
```

An open record permits additional unspecified fields:

```simi
{ name: string, .. }
```

An index signature describes dynamic entries:

```simi
{ [string]: int }
{ [int]: string }
{ [string | int]: boolean }
```

Known fields and an index signature may coexist when their value requirements
are compatible. Reads through a dynamic key include `nil` because a missing map
entry returns `nil`; known required record fields may be read at their declared
type while their presence remains proven.

As with lists, these are structural refinements of runtime `Map` values. A
primitive `map('key, 'value)` constructor is unnecessary, though a library may
define an equivalent transparent alias later.

## Mutation and analysis precision

Lists and maps remain mutable and aliased. The analyzer must update facts for
known mutations and conservatively widen them when mutation through an alias,
unknown host function, or unresolved call prevents a safe proof.

Examples of widening include losing an exact tuple shape after uncertain list
mutation or losing required-field presence after uncertain map mutation. A
wider type is preferable to assuming that an erased annotation restricts
runtime behavior.

## Narrowing

The initial analyzer may narrow through:

- resolved builtin comparisons such as `type(value) == "integer"`;
- literal equality and inequality where valid;
- discriminant fields such as `result.ok == true`;
- successful structural pattern clauses and strict Boolean guards;
- explicit nil comparisons.

Because `type` is shadowable, only calls resolved to the builtin receive special
narrowing behavior. There is no dedicated runtime-category operator.

## Explicit initial non-goals

The first implementation does not include:

- fixed repetition syntax such as `[T; N]`;
- symbolic dimensions, shape variables, or rectangularity proofs;
- type-level values, `const`, `static`, or `comptime` parameters;
- type-level arithmetic, refinement theorem solving, or analysis-time execution;
- traits, `where` constraints, operator overloading, or collection protocols;
- `TypeIs` or another user-defined narrowing predicate mechanism;
- explicit generic function application;
- annotations inside nested patterns;
- Lua-style multiple returns or a runtime tuple value;
- module type interfaces, type imports, or type exports;
- changing runtime reflection label `"integer"` to `"int"`.

The initial parser work should add type syntax only when parsing, resolution,
inference, diagnostics, erasure, and editor support can land together. Until
then, every annotation and alias example in this document must remain a syntax
error in executable Simi source.
