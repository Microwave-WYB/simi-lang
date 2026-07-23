# Mutation and copies

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- [Optional types](optional-types.md)
- [Expressions](expressions.md)
- [Functions and bindings](functions-and-bindings.md)
- [Control flow and patterns](control-flow-and-patterns.md)
- Mutation and copies
  - [Assignment creates aliases](#assignment-creates-aliases)
  - [Mutating lists](#mutating-lists)
  - [Independent list views](#independent-list-views)
  - [Mutating maps](#mutating-maps)
  - [Copying maps](#copying-maps)
  - [Choosing aliases or copies](#choosing-aliases-or-copies)
- [Modules](modules.md)
- [Text IO](text-io.md)
- [Iterators](iterators.md)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## Assignment creates aliases

Binding an existing list or map to another name does not copy it. Both names refer to the same container, so mutation through either name is visible through the other:

```elixir
let values = [1, 2]
let alias = values

values[0] = 10

[values, alias]
```

The same rule applies to maps:

```elixir
let user = {name = "Ada", visits = 1}
let alias = user

alias.visits = 2

[user, alias]
```

This identity is preserved when containers are passed to functions or returned from a tap pipeline. Simi does not insert implicit defensive copies.

## Mutating lists

Lists are zero-based. Index assignment replaces an existing element and evaluates to the assigned value:

```elixir
let values = [10, 20, 30]
let assigned = values[1] = 99

[assigned, values]
```

A write never grows a list. A nonnegative out-of-range read returns `nil`, but an out-of-range write raises a structural `index_out_of_bounds` value. Negative and non-integer indices are hard diagnostics.

The `std/list` module supplies the rest of the list operations:

```text
length  get  contains
copy    slice  iter
set     append  extend  insert  remove  pop  reverse
```

Mutation functions operate on the supplied list. For example:

```elixir
let list = require("std/list")
let values = [2, 3]

list.insert(values, 0, 1)
list.append(values, 4)
let removed = list.remove(values, 1)
let popped = list.pop(values)

[removed, popped, values]
```

`list.set`, `append`, `extend`, `insert`, and `reverse` return `nil` after mutating. `remove` returns the removed value, while `pop` returns the final value or `nil` for an empty list.

When a mutating function returns `nil`, a tap pipeline can preserve the original list for the next stage:

```elixir
let list = require("std/list")
let values = [1, 2, 3]

let same_values =
    values
    |> tap list.append(4)
    |> tap list.reverse()

[values, same_values]
```

`same_values` is another alias to `values`, not a copy.

## Independent list views

Use `list.copy` when the outer list must be independently mutable. It creates an O(1) copy-on-write view covering the source's full visible range. Mutating either outer list detaches backing storage as needed:

```elixir
let list = require("std/list")
let source = [1, 2, 3]
let copied = list.copy(source)

source[0] = 10
list.append(copied, 4)

[source, copied]
```

`list.slice` creates the same kind of independent shallow view over a visible range:

```elixir
let list = require("std/list")
let source = [0, 1, 2, 3]
let middle = list.slice(source, 1, 3)

source[1] = 10
middle[1] = 20

[source, middle]
```

Copy-on-write is an implementation strategy, not delayed aliasing: from the language's perspective the outer containers are independent immediately.

These copies are shallow. Nested mutable values retain their identity:

```elixir
let list = require("std/list")
let nested = [1]
let source = [nested, [2]]
let copied = list.copy(source)

copied[0][0] = 9
copied[1] = [3]

[source, copied]
```

The outer replacement affects only `copied`, but the mutation inside the nested list is visible through both outer lists.

List-rest patterns use the same O(1), independent, shallow copy-on-write behavior:

```elixir
let list = require("std/list")
let source = [1, 2, 3]
let [first, ..rest] = source

rest[0] = 20
list.append(source, 4)

[first, source, rest]
```

## Mutating maps

Map field assignment is shorthand for assignment with a string key. Bracket syntax accepts a computed supported key:

```elixir
let settings = {theme = "light"}
let key = "language"

settings.theme = "dark"
settings[key] = "simi"

settings
```

Maps cannot retain `nil` values. Assigning `nil` deletes the key, and a missing read returns `nil`:

```elixir
let settings = {theme = "dark", temporary = true}

settings.temporary = nil

[settings, settings.temporary]
```

Because script-created maps cannot store a nil value, `map[key] != nil` is a valid existence check. The `std/map` module also provides `has` when the intent should be explicit.

```elixir
let map = require("std/map")
let settings = {theme = "dark"}

[map.has(settings, "theme"), map.has(settings, "missing")]
```

`map.clear` removes every entry in place and returns `nil`:

```elixir
let map = require("std/map")
let settings = {theme = "dark", language = "simi"}
let alias = settings

let result = map.clear(settings)

[result, settings, alias]
```

## Copying maps

`map.copy` creates an independent shallow map in O(n). It preserves normalized keys and insertion order:

```elixir
let map = require("std/map")
let source = {name = "Ada", visits = 1}
let copied = map.copy(source)

source.visits = 2
copied.name = "Grace"

[source, copied]
```

As with lists, shallow copying preserves aliases to nested values:

```elixir
let map = require("std/map")
let roles = ["admin"]
let source = {name = "Ada", roles = roles}
let copied = map.copy(source)

copied.name = "Grace"
copied.roles[0] = "editor"

[source, copied]
```

The top-level `name` fields are independent. Both maps still point to the same nested `roles` list.

A map-rest pattern also creates an independent shallow map:

```elixir
let source = {name = "Ada", role = "admin", active = true}
let {name = name, ..details} = source

details.role = "editor"
source.active = false

[name, source, details]
```

## Choosing aliases or copies

Use an alias when multiple parts of a program intentionally share one mutable container. Use `list.copy`, `list.slice`, a list-rest capture, `map.copy`, or a map-rest capture when the outer container must evolve independently.

In either case, remember that copying is shallow. Copy nested containers separately when their mutations must also be isolated.

<!-- tour:navigation:start -->
---

[Previous: Control flow and patterns](control-flow-and-patterns.md)

[Next: Modules](modules.md)
<!-- tour:navigation:end -->
