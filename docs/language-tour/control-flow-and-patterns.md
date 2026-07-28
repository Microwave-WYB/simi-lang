# Control flow and patterns

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- [Types and analysis](types-and-analysis.md)
- [Expressions](expressions.md)
- [Functions and bindings](functions-and-bindings.md)
- Control flow and patterns
  - [Conditionals are expressions](#conditionals-are-expressions)
  - [Structural pattern matching](#structural-pattern-matching)
    - [Destructuring with `let`](#destructuring-with-let)
  - [Postfix nil propagation](#postfix-nil-propagation)
    - [Protected catch boundaries](#protected-catch-boundaries)
- [Mutation and copies](mutation-and-copies.md)
- [Modules](modules.md)
- [Text IO](text-io.md)
- [Iterators](iterators.md)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## Conditionals are expressions

An `if` evaluates only its selected branch and returns that branch's final value:

```simi
let score = 82
let label = if score >= 90 then
    "excellent"
elseif score >= 60 then
    "passing"
else
    "retry"
end

[label, score]
```

Conditions must be booleans; Simi does not use general truthiness. If no condition matches and there is no `else`, the result is `nil`.

Each branch is a child block. A binding created in a branch is not visible after the `if`:

```simi
let result = if true then
    let message = "local to this branch"
    message
else
    "not selected"
end

result
```

## Structural pattern matching

A `case` evaluates its input once, then selects the first matching arm whose optional guard succeeds. The selected arm's result supplies the value of the whole expression:

```simi
let response = {kind = "ok", value = "profile"}

let message = case response of
    {kind = "ok", value = value} =>
        "received " <> value
    {kind = "error", error = error} when error != nil =>
        "failed: " <> error
    _ =>
        "unknown response"
end

message
```

Guards must evaluate to booleans. Pattern bindings belong only to their clause, and an unmatched `case` is a hard diagnostic. Put a wildcard fallback last when non-matching input is expected.

Patterns include:

- literals such as `42`, `"ok"`, `true`, and `nil`;
- binding names;
- the `_` wildcard;
- nested list and map patterns;
- list and map rest captures.

```simi
let input = {kind = "point", coordinates = [3, 4, 5], color = "blue"}

case input of
    {
        kind = "point",
        coordinates = [x, y, ..remaining],
        ..metadata
    } =>
        [x, y, remaining, metadata]
    _ =>
        nil
end
```

A binding name matches any value. `_` matches without creating a binding. Patterns may be nested to mirror the structure being inspected.

Map patterns are closed by default. `{name = name}` matches a map containing exactly that field. Add `..` to allow and discard additional keys, or `..rest` to capture them in a new shallow map. Both string and computed extra keys count when checking whether a pattern is closed.

Named map fields normally require the key to be present. The literal `nil` field pattern is the exception: it also matches an absent key, because a missing map lookup produces `nil`. A closed pattern still rejects unrelated keys, so use `..` when testing absence inside a larger map.

```simi
let settings = {theme = "dark"}

case settings of
    {nickname = nil, ..} =>
        "no nickname"
    _ =>
        "has a nickname"
end
```

### Destructuring with `let`

The left side of `let` may use structural patterns. Direct named map bindings default an absent key to `nil`; shorthand `{name}` binds the `name` key to the `name` local, while `{source = name}` keeps a distinct lookup key and local name:

```simi
let values = [10, 20, 30, 40]
let [first, second, ..rest] = values

let user = {name = "Ada", role = "admin", active = true}
let {name, nickname, role = account_role, ..details} = user

[first, second, rest, name, nickname, account_role, details]
```

This default applies only to direct binding fields. Other field patterns, and every map field in `case` and `catch`, continue to require presence (except for the existing literal `nil` rule above). Map patterns remain closed: without `..` or `..rest`, unrelated keys still make a `let` pattern fail.

The right side is evaluated once, and matching is atomic: no bindings are installed unless the entire pattern succeeds. A mismatch in `let` is a hard diagnostic. Use `case` instead when mismatch is an ordinary possibility.

When analysis has enough type information, it reports an error for a pattern that cannot match and a warning for one that may fail, recommending `case` for explicit recovery. These are erased diagnostics only: ignoring a warning or skipping analysis never changes the runtime's atomic hard-failure behavior.

Rest captures are independent shallow containers. A list rest is an O(1) copy-on-write view, while a map rest is a new shallow map. Nested values inside either capture retain their existing alias identities. The next page develops these copy rules.

## Postfix nil propagation

Postfix `?` is for leaving the current block early when a value is absent. A non-`nil` value passes through unchanged. A `nil` value stops the nearest lexically enclosing block and makes that block evaluate to `nil`.

```simi
fn greeting(maybe_name: string | nil) -> string | nil do
    let name = maybe_name?
    "Hello, " <> name
end

[greeting("Ada"), greeting(nil)]
```

Here the function body is the nearest block. `greeting("Ada")` returns the greeting, while `greeting(nil)` stops before concatenation and returns `nil`.

Every control-flow body is a nil-propagation boundary: each `if` branch, each `case` arm, the protected body and selected catch arm of a protected `do`, every named or anonymous function body, every standalone `do ... end`. Propagation stops at the nearest one of these lexical boundaries rather than searching only for a standalone block.

For example, propagation inside an `if` branch makes that branch `nil`; it does not stop the surrounding standalone block:

```simi
let result = do
    let selected = if true then
        nil?
        "unreachable"
    else
        1
    end

    [selected, "the outer block continues"]
end

result
```

Nested standalone blocks follow the same nearest-boundary rule:

```simi
let result = do
    let inner = do
        nil?
        "unreachable"
    end

    [inner, "the outer block continues"]
end

result
```

`?` propagates only ordinary `nil`. It does not intercept a raised value or a hard diagnostic, and it cannot be used when there is no enclosing block.

### Protected catch boundaries

A protected `do` has a protected block, followed by `catch of` and `pattern [when guard] => expression` arms. `catch` matches raised values; it does **not** catch postfix nil propagation. If `?` sees `nil` in the protected block, that block simply evaluates to `nil` and catch selection never begins:

```simi
let selected = do
    nil?
    "unreachable"
catch of
    _ =>
        "not caught"
end

[selected, "execution continues after the protected expression"]
```

Likewise, `?` inside a selected catch arm stops that arm as `nil`; later arms are not tried:

```simi
let selected = do
    raise "missing"
catch of
    "missing" => do
        nil?
        "unreachable"
    end
    _ =>
        "not selected"
end

[selected, "execution continues after the protected expression"]
```

Raised values and the full error model are covered in [Errors and embedding](errors-and-embedding.md).

<!-- tour:navigation:start -->
---

[Previous: Functions and bindings](functions-and-bindings.md)

[Next: Mutation and copies](mutation-and-copies.md)
<!-- tour:navigation:end -->
