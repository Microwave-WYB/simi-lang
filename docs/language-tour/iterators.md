# Iterators

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- [Optional types](optional-types.md)
- [Expressions](expressions.md)
- [Functions and bindings](functions-and-bindings.md)
- [Control flow and patterns](control-flow-and-patterns.md)
- [Mutation and copies](mutation-and-copies.md)
- [Modules](modules.md)
- [Text IO](text-io.md)
- Iterators
  - [Collection snapshots](#collection-snapshots)
  - [Lazy adapters](#lazy-adapters)
  - [Consumers](#consumers)
  - [Single-pass traversal](#single-pass-traversal)
  - [Steps and custom iterators](#steps-and-custom-iterators)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## Collection snapshots

`list.iter(values)` takes an O(1) copy-on-write snapshot. Later structural mutations to the original outer list do not change the iterator's traversal.

```simi
let list = require("std/list")
let iter = require("std/iter")
let values = [1, 2]
let source = list.iter(values)

list.append(values, 3)

[iter.to_list(source), values]
```

`map.iter(value)` similarly snapshots the map's insertion-ordered entries. Snapshotting is shallow: nested mutable values retain their usual alias identity.

## Lazy adapters

`iter.map` and `iter.filter` return new iterators. They do not invoke their callbacks when the adapter is created; work begins only when a consumer requests values.

```simi
let list = require("std/list")
let iter = require("std/iter")
let calls = []

let transformed =
    [-2, 1, 3]
    |> list.iter()
    |> iter.filter(fn(value) do
        list.append(calls, value)
        value >= 0
    end)
    |> iter.map(fn(value) do
        value * 2
    end)

let calls_before_consuming = list.length(calls)
let result = iter.to_list(transformed)

[calls_before_consuming, result, calls]
```

Filter predicates must return booleans. A callback raise propagates unchanged through adapters and consumers.

## Consumers

The iterator consumers are:

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

Consumers advance the iterator they receive. `to_list` consumes all remaining values. `fold` threads an accumulator. `find` and `find_index` return `nil` when there is no match, and `each` always returns `nil` after successful traversal.

```simi
let list = require("std/list")
let iter = require("std/iter")
let values = [1, 2, 3, 4]

let total =
    values
    |> list.iter()
    |> iter.fold(0) <| fn(sum, value) do
        sum + value
    end

let even_count = iter.count(list.iter(values), fn(value) do
    value % 2 == 0
end)

[total, even_count]
```

Predicates passed to `find`, `find_index`, `any`, `all`, and predicate-based `count` must return booleans. Searches and boolean queries short-circuit, leaving later values unconsumed.

```simi
let list = require("std/list")
let iter = require("std/iter")
let source = list.iter([1, 2, 3, 4])

let found = iter.find(source, fn(value) do
    value == 2
end)

[found, iter.to_list(source)]
```

## Single-pass traversal

Iterators are single-pass: once a value has been consumed, it is not available again. They are also sticky after exhaustion—every later `iter.next` call remains done.

```simi
let list = require("std/list")
let iter = require("std/iter")
let values: [..string] = ["a", "b"]
let source = list.iter(values)

let first = iter.next(source)
let rest = iter.to_list(source)
let done = iter.next(source)
let still_done = iter.next(source)

[first, rest, done, still_done]
```

Do not reuse an iterator when two independent traversals are needed. Create two collection iterators instead.

## Steps and custom iterators

`iter.next(iterator)` returns a tagged map rather than using `nil` as an exhaustion sentinel:

```text
{done = false, value = item}
{done = true}
```

Lists may legitimately contain `nil`. Such an item produces `{done = false}` because maps omit nil-valued fields; the boolean `done` field is therefore the authoritative completion signal.

A custom producer is a zero-argument function returning these step maps. Wrap it with `iter.from` to obtain a public iterator:

```simi
let iter = require("std/iter")

fn countdown(start) do
    let current = start

    iter.from(fn() do
        if current <= 0 then
            {done = true}
        else
            let value = current
            current = current - 1
            {done = false, value = value}
        end
    end)
end

iter.to_list(countdown(3))
```

`iter.from` makes exhaustion sticky: after the producer first returns a done step, it is never called again. A non-map step, a missing `done` field, or a non-boolean `done` value is a hard contract diagnostic. Extra step fields are ignored.

Use `iter.fold` and ordinary functions for custom collection logic. There is no generic `collect` operation; `iter.to_list` is the concrete list consumer.

<!-- tour:navigation:start -->
---

[Previous: Text IO](text-io.md)

[Next: Errors and embedding](errors-and-embedding.md)
<!-- tour:navigation:end -->
