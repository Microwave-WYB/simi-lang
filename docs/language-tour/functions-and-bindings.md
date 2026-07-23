# Functions and bindings

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- [Optional types](optional-types.md)
- [Expressions](expressions.md)
- Functions and bindings
  - [Let bindings](#let-bindings)
  - [Lexical scope](#lexical-scope)
  - [Shadowing](#shadowing)
  - [Named functions](#named-functions)
  - [Anonymous functions](#anonymous-functions)
  - [Closures](#closures)
  - [Functions as arguments and results](#functions-as-arguments-and-results)
  - [Function call contracts](#function-call-contracts)
- [Control flow and patterns](control-flow-and-patterns.md)
- [Mutation and copies](mutation-and-copies.md)
- [Modules](modules.md)
- [Text IO](text-io.md)
- [Iterators](iterators.md)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## Let bindings

`let` introduces a new binding in the current lexical scope:

```elixir
let language = "Simi"
let release = "alpha"
language <> " " <> release
```

The right-hand expression is evaluated before the new binding is installed. Assignment is different: it updates the nearest existing binding and never creates one implicitly.

```elixir
let visits = 1
visits = visits + 1
visits
```

Bindings may hold any value, including mutable containers and functions. Structural binding patterns are introduced in [Control flow and patterns](control-flow-and-patterns.md).

## Lexical scope

A function's parameters and body-local bindings belong to that function call. They do not escape into the surrounding scope:

```elixir
let message = "outside"

fn decorate(message) do
    let punctuation = "!"
    message <> punctuation
end

[message, decorate("inside")]
```

Function bodies may contain multiple items. The final expression is the function's return value; there is no required `return` statement:

```elixir
fn full_name(first, last) do
    let separator = " "
    first <> separator <> last
end

full_name("Ada", "Lovelace")
```

## Shadowing

A later `let` may reuse a name. This creates a new binding; it does not overwrite the old one:

```elixir
let value = "first"
let value = "second"
value
```

Closures make the distinction visible. A function created before the second `let` keeps the earlier binding, while later code sees the new one:

```elixir
let value = "first"
let read_first = fn() do
    value
end

let value = "second"
let read_second = fn() do
    value
end

[read_first(), read_second(), value]
```

Assignment follows each closure's lexical view. Here `set_first` updates the first binding, while the top-level assignment updates the later binding:

```elixir
let value = 1
let read_first = fn() do value end
let set_first = fn(next) do value = next end

let value = 2
value = 3
set_first(4)

[read_first(), value]
```

This precise shadowing rule makes captured state predictable even when a scope reuses a convenient name.

## Named functions

A named function uses a declaration:

```elixir
fn area(width, height) do
    width * height
end

area(6, 7)
```

Parameters are fresh bindings for each call. A function captures bindings from its surrounding lexical environment:

```elixir
let tax_rate = 0.2

fn with_tax(price) do
    price + price * tax_rate
end

with_tax(50)
```

Named function declarations support recursion. [Control flow and patterns](control-flow-and-patterns.md) combines recursion with conditional expressions, where a terminating branch can be shown without introducing control flow early.

## Anonymous functions

An anonymous function is an expression and may appear anywhere an expression is accepted:

```elixir
let double = fn(value) do
    value * 2
end

double(21)
```

It can be stored in a container or called immediately:

```elixir
let operations = [
    fn(value) do value + 1 end,
    fn(value) do value * 3 end,
]

[operations[0](4), fn(value) do value * value end(5)]
```

Anonymous functions are ordinary function values, just like named functions and host-provided native functions. The builtin `type` reports `"function"` for all of them:

```elixir
fn named(value) do value end
let anonymous = fn(value) do value end
[type(named), type(anonymous), type(inspect)]
```

## Closures

A function may outlive the call that created it while retaining access to that call's lexical bindings. Such a function is a closure:

```elixir
fn make_adder(base) do
    fn(value) do
        base + value
    end
end

let add_two = make_adder(2)
let add_ten = make_adder(10)
[add_two(5), add_ten(5)]
```

Captured bindings remain assignable. Separate calls create separate captured state:

```elixir
fn make_counter(start) do
    let count = start
    fn() do
        count = count + 1
    end
end

let first = make_counter(0)
let second = make_counter(10)
[first(), first(), second(), first()]
```

The assignment is also the body’s final expression, so each call returns the new count.

## Functions as arguments and results

Higher-order functions accept or return other functions. Callbacks are passed directly like any other value:

```elixir
fn apply_twice(callback, value) do
    callback(callback(value))
end

fn increment(value) do
    value + 1
end

apply_twice(increment, 40)
```

A function factory returns a closure selected by its captured arguments:

```elixir
fn make_multiplier(factor) do
    fn(value) do
        value * factor
    end
end

let triple = make_multiplier(3)
triple(14)
```

Pipelines and the optional trailing-callback operator `<|` were introduced in [Expressions](expressions.md). Iterator-specific callback contracts and lazy execution are covered in [Iterators](iterators.md).

## Function call contracts

Calls use a fixed number of positional arguments. Supplying too few or too many arguments is a hard diagnostic, as is calling a value that is not a function. Argument expressions are evaluated once from left to right.

Named and anonymous functions can both close over mutable lists and maps. That captures the same container alias rather than making a copy; [Mutation and copies](mutation-and-copies.md) explains aliasing, mutation, and explicit copy operations.

<!-- tour:navigation:start -->
---

[Previous: Expressions](expressions.md)

[Next: Control flow and patterns](control-flow-and-patterns.md)
<!-- tour:navigation:end -->
