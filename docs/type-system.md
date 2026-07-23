# Erased type system

Type annotations, aliases, inference, diagnostics, and editor presentation are implemented as erased analysis metadata. The runtime remains dynamically typed, and analysis never changes program execution.

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
let count: integer = 1

fn display(value: integer) -> string do
    require("std/number").to_string(value)
end

let callback: (integer, integer) -> integer = add
```

The initial primitive vocabulary includes:

```text
never
nil
boolean
integer
float
string
any
```

There is deliberately no static `number` type: numeric APIs use the explicit union `integer | float`. `never` is the bottom type. An empty list literal has the exact shape `[]`; `never` still appears when an expression has no normal return path or as the bottom member removed while unions are normalized. `any` is the explicit dynamic escape hatch: operations involving it remain valid but lose static precision. Insufficient evidence is tracked as an internal unknown type and presented publicly as `any`.

The static integer spelling is `integer`. Runtime reflection deliberately remains
unchanged for compatibility:

```simi
type(value) == "integer"
```

When `type` resolves to the builtin, the analyzer may narrow that comparison to
static `integer`. Static annotations and runtime reflection deliberately use the
same spelling.

## Functions and generics

Function types use arrows:

```simi
integer -> integer
(integer, integer) -> integer
() -> integer
integer -> string -> boolean
```

Arrows associate to the right. Parentheses distinguish fixed parameter lists
from a single parameter.

Generic variables begin with an apostrophe. Alias parameters are explicit, and
type application uses angle brackets:

```simi
alias option<'a> = 'a | nil
alias pair<'a, 'b> = ['a, 'b]

let name: option<string> = nil
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
`identity<string>(value)` are not part of the initial design.

Aliases are transparent: expanding an alias does not create a new runtime or
nominal identity.

## Unions and literal types

`|` forms unions. String literals may be singleton types; numeric and Boolean
literals widen to `integer`, `float`, and `boolean`. `nil` is also a type and an
ordinary union member:

```simi
alias mode = "read" | "write"
alias maybe_name = string | nil
```

Literal fields support discriminated structural records:

```simi
alias result<'value, 'error> =
    { kind: "ok", value: 'value }
    | { kind: "error", error: 'error }
```

Pattern matching and ordinary equality may narrow these unions. Exhaustiveness
analysis may warn about missing cases, but it does not change the current
runtime rule that an unmatched `case` is a hard error.

## Structural lists

All positional container types describe the existing mutable runtime `List`.
There is no runtime tuple category.

A nonempty bracketed comma list is an exact positional shape:

```simi
[integer, string]
[boolean, integer, string]
```

A rest element describes a homogeneous arbitrary-length list:

```simi
[..integer]
[..string]
[..[..integer]]
```

An empty list literal has the exact shape `[]`. Known mutations retain exact
shape, so appending an integer produces `[integer]`. Repeated control flow that
may grow through arbitrarily many exact shapes widens them to a homogeneous
rest list such as `[..integer]`.

Nested lists are allowed and may be ragged. Exact lists can describe fixed
positions, but the initial system does not track symbolic dimensions or prove
rectangular matrix shapes.

These structural forms are sufficient as the primitive surface. Libraries may
provide transparent aliases for common list shapes, but `list<'a>` need not be
a primitive type constructor.

## Structural maps

All record and index-signature types describe the existing mutable runtime
`Map`. There is no user-defined runtime record or map category.

A record is closed by default:

```simi
{ name: string, age: integer }
```

An open record permits additional unspecified fields:

```simi
{ name: string, .. }
```

An index signature describes dynamic entries:

```simi
{ [string]: integer }
{ [integer]: string }
{ [string | integer]: boolean }
```

Known fields and an index signature may coexist when their value requirements
are compatible. Reads through a dynamic key include `nil` because a missing map
entry returns `nil`; known required record fields may be read at their declared
type while their presence remains proven.

As with lists, these are structural refinements of runtime `Map` values. A
primitive `map<'key, 'value>` constructor is unnecessary, though a library may
define an equivalent transparent alias later.

## Mutation and analysis precision

Lists and maps remain mutable and aliased. The analyzer must update facts for
known mutations and conservatively widen them when mutation through an alias,
unknown host function, or unresolved call prevents a safe proof.

Examples of widening include losing an exact list shape after uncertain list
mutation or losing required-field presence after uncertain map mutation. A
wider type is preferable to assuming that an erased annotation restricts
runtime behavior.

Known operations retain the strongest representable fact. Appending to an exact
list therefore extends its exact shape, while insertion at an unknown position
widens it to a homogeneous rest list.

A named function may declare normal-return parameter post-types after its signature:

```simi
fn append(xs: [..'a], value: 'b) -> nil
    after xs becomes [..'a | 'b]
do
    host.call("org.simi-lang/std/list/append", xs, value)
end
```

Multiple parameters use repeated `after` clauses in source order. A post-type is
a guaranteed upper bound after normal return; it does not discard a stronger
fact inferred for a known operation. List and map post-types may transform their
internal structure while preserving runtime category and alias identity. Other
categories may only narrow. All aliases to the same mutable region receive the
post-state. Raised and nonreturning paths do not establish a caller-visible
post-state.

Named functions also infer missing mutable-parameter post-types from modeled
operations and already-known postconditions in their bodies. Normal-return paths
are joined conservatively, inferred posts share generic identities with the
function signature, and function aliases inherit them. An explicit `after`
clause takes precedence for its parameter and is checked against an ordinary
Simi body when the final state is provable. A direct `host.call` facade remains
a trusted native contract. Unknown calls may widen state but cannot establish a
stronger inferred guarantee.

## Narrowing

The analyzer narrows branch-local flow types through:

- resolved builtin comparisons such as `type(value) == "integer"`;
- literal equality and inequality where valid;
- discriminant fields such as `result.kind == "ok"`;
- successful structural pattern clauses and strict Boolean guards;
- explicit nil comparisons.

`not`, `and`, and `or` compose these facts with strict Boolean and short-circuit
semantics. Sibling branches receive the complement of earlier conditions, and
normal branch exits join their resulting states. Assignment replaces the current
flow fact; container mutation invalidates facts that may have depended on the
mutated structure.

Postfix `?` removes `nil` on the surviving continuation through the nearest
standalone block. The block's nil-abort and normal exits join again outside that
boundary. Each `?>` stage similarly splits nil-skipped and active paths lazily,
applies call effects only on the active path, and rejoins before the following
pipeline stage. An ordinary `|>` following it therefore receives the complete
result union.

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
- module type interfaces, type imports, or type exports.

Inference is local and body-based. Unannotated function parameters receive inference variables; operators, literals, calls, annotations, and return paths constrain them. Genuine unconstrained function relationships are generalized, and calls instantiate those generics without specializing stable non-generic signatures. Closed operator transfer relations mirror the finite runtime primitive cases and distribute over unions.
