# Editor tooling orchestration

The root `justfile` exposes editor tooling under the `editors` module. Component
commands are hierarchical:

```sh
just editors test
just editors zed publish
```

## Prerequisites and component contract

Optional Just modules require Just 1.52 or newer. A component branch must add
its module at `editors/<component>/justfile`, where `<component>` is one of
`tree-sitter`, `vscode`, or `zed`. Each component justfile must provide
`install`, `generate`, `test`, and `package` recipes for the aggregate commands.
Until a component justfile is present, aggregate commands report that component
as skipped.

Component justfiles own their tools, working-directory assumptions, and all
credentials. The aggregate module does not install global tools or inspect
publishing credentials.

## Publishing policy

Publishing is always an explicit component command, for example
`just editors zed publish`. There is deliberately no aggregate publish recipe.

Component implementations must follow these rules:

- `test` and aggregate recipes must never invoke publishing.
- Credential checks belong only in the component's publishing recipe.
- `package` must create local artifacts without publishing them.
- Provide a preparation or dry-run recipe when the ecosystem supports one,
  and run it before the explicit publishing step.
