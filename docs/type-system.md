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

fn display(value: number) -> string
    number.to_string(value)

let callback: fn(integer, integer) -> integer = add
```

The initial primitive vocabulary includes:

```text
never
nil
boolean
integer
float
string
bytes
any
```

`number` is a built-in erased alias for `integer | float`; it is not a distinct runtime category. Numeric APIs may use either spelling. `bytes` describes immutable packed byte values, including values constructed by `#[]`. `never` is the bottom type. An empty list literal has the exact shape `[]`; `never` still appears when an expression has no normal return path or as the bottom member removed while unions are normalized. `any` is the explicit dynamic escape hatch: operations involving it remain valid but lose static precision. Insufficient evidence is tracked as an internal unknown type and presented publicly as `any`.

Destructuring `let` patterns are runtime assertions. The analyzer reports a pattern it can prove impossible, but does not warn merely because a pattern may fail; use `case` when mismatch is expected and should be handled. Direct map bindings in `let` receive `nil` when a field is absent, so their inferred type is `T | nil` when map presence is not proven. In a `#[]` bytes pattern, a bare binding is `integer`, while `name:width` and a final `..name` capture are `bytes`. These classifications are erased and never weaken or replace the runtime's atomic hard diagnostic for a failed `let` match.

The static integer spelling is `integer`. Runtime reflection deliberately remains
unchanged for compatibility:

```simi
type(value) == "integer"
```

When `type` resolves to the builtin, the analyzer may narrow that comparison to
static `integer`. Static annotations and runtime reflection deliberately use the
same spelling.

## Functions and generics

Callable types use explicit `fn(parameters) -> result` syntax:

```simi
fn(integer) -> integer
fn(integer, integer) -> integer
fn() -> integer
fn(integer) -> fn(string) -> boolean
```

Callable parameter lists always use parentheses. Results may themselves be callable types, so nesting remains explicit and right-associative. Legacy bare arrow forms such as `integer -> integer` are rejected.

Generic variables begin with an apostrophe. Alias parameters are explicit, and
type application uses angle brackets:

```simi
alias option<'a> = 'a | nil
alias pair<'a, 'b> = ['a, 'b]

let name: option<string> = nil
```

Free generic variables in a function annotation are implicitly quantified. A callable may also declare an explicit generic header, with optional ordinary-type upper bounds:

```simi
fn identity(value: 'a) -> 'a ! never
    value

fn negate<'a: integer | float>(value: 'a) -> 'a ! never
    -value

fn transform<'e>(
    value: 'a,
    callback: fn(input: 'a) -> 'b ! 'e,
) -> 'b ! 'e
    callback(value)
```

Bounds are erased Simi types, not traits or protocols. Nested explicit headers introduce their own binders and may shadow an outer generic name. Unbounded entries are retained as callable metadata, though free variables remain implicitly quantified.

Callable parameter labels are erased presentation metadata. They improve hover and completion signatures but calls remain positional. Labels do not participate in callable equality or compatibility.

Callers never supply explicit generic arguments. Forms such as `identity<string>(value)` are not part of the initial design. Aliases are transparent: expanding an alias does not create a new runtime or nominal identity.

A callable has a normal result type and a separate raised type:

```simi
fn(string) -> integer
fn(string) -> integer ! {error: "invalid_input", ..}
fn(string) -> integer ! never
```

Omitting a raised-error contract asks the analyzer to infer it. `! E` declares and checks an upper bound; `! never` forbids language raises. Raised-error contracts may use the callable's generics and propagate through callbacks. Hard diagnostics and postfix `?` are outside this channel. In nested callable types, a raised-error contract belongs to the callable immediately before it.

## Unions and literal types

`|` forms unions. The syntax accepts an optional leading `|`. Canonical documentation omits it for single-line unions; multiline unions put every member, including the first, on a line beginning with `|`. Every primitive literal may be written as an explicit singleton type: `nil`, `true`, `false`, strings, integers, and finite floats. Each singleton is a subtype of its ordinary primitive category. Numeric literals and ordinary Boolean expression values still infer as `integer`, `float`, and `boolean`; singleton annotations do not introduce global literal inference. Direct named map fields preserve Boolean singleton facts as record discriminants. `nil` is also an ordinary union member:

```simi
alias mode = "read" | "write"
alias retry_count = 0 | 1 | 2
alias threshold = 0.5 | 1.0
alias maybe_name = string | nil
let port: 8080 = 8080
```

Literal fields support discriminated structural records:

```simi
alias result<'value, 'error> =
    | { kind: "ok", value: 'value }
    | { kind: "error", error: 'error }

alias step<'value> =
    | { done: true, .. }
    | { done: false, value: 'value, .. }
```

A direct field such as `{done = false}` proves the `false` variant. A field initialized from an arbitrary Boolean variable, call, or operator remains `boolean` and does not prove either singleton. `true | false` normalizes to `boolean`, and a broad primitive is never a subtype of one of its singletons.

Numeric singleton spelling preserves runtime categories: `1` is an integer singleton while `1.0` is a float singleton, including integral floats. Float zero is normalized by runtime numeric equality, so `-0.0` and `0.0` denote the same float singleton and display canonically as `0.0`. Finite-float lexical validation remains unchanged, and annotations remain erased.

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
unknown native function, or unresolved call prevents a safe proof.

Examples of widening include losing an exact list shape after uncertain list
mutation or losing required-field presence after uncertain map mutation. A
wider type is preferable to assuming that an erased annotation restricts
runtime behavior.

Known operations retain the strongest representable fact. Appending to an exact
list therefore extends its exact shape, while insertion at an unknown position
widens it to a homogeneous rest list.

Structural inference is local to a binding's defining lexical scope. A newly inferred list or map is unsealed there, so direct modeled mutations can refine its exact shape or element type. An explicit annotation or function/closure capture seals the analyzer-visible contract: subsequent captured mutation must stay compatible with that contract and cannot implicitly add fields or widen element/value unions. Function calls never publish caller-visible mutation transitions.

Maps never retain `nil`; therefore a field type containing `nil`, such as `{count: integer | nil}`, means that the field may be absent and, when present, contains an integer.

Raised exits snapshot flow independently from normal exits. Effects that may occur before a raise are visible in a matching catch, while no callable post-state is established at a call site.

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
lexical block. The block's nil-abort and normal exits join again outside that
boundary. Iterator controls are ordinary maps returned from callbacks and do not
introduce a lexical control-flow boundary. Each `?>` stage similarly splits
nil-skipped and active paths lazily,
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
