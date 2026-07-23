# A tour of Simi

This tour introduces Simi through focused, independently runnable scripts. Start with Hello, world, then follow the current reading order below or jump directly to a topic.

> [!WARNING]
> GitHub does not currently recognize Simi code fences. The tour labels Simi examples as `elixir` to provide approximate syntax highlighting, so some tokens may be colored incorrectly. Every such example is Simi—not Elixir.

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](language-tour/hello-world.md)
- [Values](language-tour/values.md)
- [Optional types](language-tour/optional-types.md)
- [Expressions](language-tour/expressions.md)
- [Functions and bindings](language-tour/functions-and-bindings.md)
- [Control flow and patterns](language-tour/control-flow-and-patterns.md)
- [Mutation and copies](language-tour/mutation-and-copies.md)
- [Modules](language-tour/modules.md)
- [Text IO](language-tour/text-io.md)
- [Iterators](language-tour/iterators.md)
- [Errors and embedding](language-tour/errors-and-embedding.md)
<!-- tour:contents:end -->

Every highlighted Simi example is a complete script that can be considered independently. Run a saved example with:

```sh
simi run example.simi
```

Use `simi run --inspect example.simi` to render its final value as well as any output produced explicitly by the script.

For the authoritative erased-type design, see the [type-system reference](type-system.md).
