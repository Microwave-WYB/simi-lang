# Simi analysis

This crate provides file-local, incremental editor analysis without changing Simi runtime behavior.

- Salsa owns source inputs and caches parse, HIR, resolution, diagnostics, and document-symbol queries.
- Parse queries retain Rowan green trees because red `SyntaxNode` handles are thread-local; consumers create red roots on demand.
- HIR uses `la-arena` typed IDs. IDs are stable within one HIR snapshot and must be looked up again after a source revision.
- Resolution covers lexical bindings, shadowing, captures, assignments, and the `require`, `type`, and `inspect` prelude.
- Unknown reads and assignments are retained as unresolved occurrences and are not diagnostics: an embedding host may provide those globals.
- Each file is independent. Module interfaces, type syntax, and inference are intentionally outside this crate's current scope.
