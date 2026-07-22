use simi_analysis::{
    AnalysisDatabase, OccurrenceKind, RenameError, SymbolId, SymbolKind, diagnostics,
    document_symbols, parse, references, resolve, source_text,
};

fn symbol_named(resolution: &simi_analysis::Resolution, name: &str, kind: SymbolKind) -> SymbolId {
    resolution
        .hir
        .symbols
        .iter()
        .find_map(|(id, symbol)| (symbol.name == name && symbol.kind == kind).then_some(id))
        .unwrap_or_else(|| panic!("missing {kind:?} symbol {name}"))
}

#[test]
fn resolves_shadowing_and_nearest_assignment() {
    let source = "let value = 0 do let value = 1 value = 2 end value = 3";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let values = resolution
        .hir
        .symbols
        .iter()
        .filter_map(|(id, symbol)| (symbol.name == "value").then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2);

    let assignments = resolution
        .hir
        .occurrences
        .iter()
        .zip(&resolution.occurrence_symbols)
        .filter(|(occurrence, _)| occurrence.kind == OccurrenceKind::Assignment)
        .collect::<Vec<_>>();
    assert_eq!(assignments.len(), 2);
    assert_ne!(assignments[0].1, assignments[1].1);
    assert_eq!(assignments[0].1, &Some(values[1]));
    assert_eq!(assignments[1].1, &Some(values[0]));
}

#[test]
fn assignment_before_a_later_inner_binding_uses_the_outer_binding() {
    let source = "let value = 0 do value = 1 let value = 2 end";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let outer = resolution
        .hir
        .symbols
        .iter()
        .find_map(|(id, symbol)| {
            (symbol.name == "value" && symbol.scope == resolution.hir.root_scope).then_some(id)
        })
        .expect("outer binding");
    let assignment = resolution
        .hir
        .occurrences
        .iter()
        .zip(&resolution.occurrence_symbols)
        .find(|(occurrence, _)| occurrence.kind == OccurrenceKind::Assignment)
        .expect("assignment");
    assert_eq!(*assignment.1, Some(outer));
}

#[test]
fn records_closure_captures_but_not_parameters() {
    let source = "let outer = 1 let closure = fn(parameter) do outer + parameter end";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let outer = symbol_named(&resolution, "outer", SymbolKind::Let);
    let parameter = symbol_named(&resolution, "parameter", SymbolKind::Parameter);
    assert!(
        resolution
            .captures
            .iter()
            .any(|capture| capture.symbol == outer)
    );
    assert!(
        !resolution
            .captures
            .iter()
            .any(|capture| capture.symbol == parameter)
    );
}

#[test]
fn closures_resolve_bindings_declared_later_in_captured_frames() {
    let source = "let closure = fn() do later end let later = 1";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let later = symbol_named(&resolution, "later", SymbolKind::Let);
    assert_eq!(resolution.references(later).len(), 1);
    assert!(
        resolution
            .captures
            .iter()
            .any(|capture| capture.symbol == later)
    );
}

#[test]
fn lowers_destructuring_case_catch_and_loop_bindings() {
    let source = r#"
let [first, { name = nested }, ..rest] = input
case input
of item when item != nil do item
end
try
    raise input
catch error when error != nil do
    error
end
loop state = 0 do
    break state
end
"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    assert!(diagnostics(&db, file).is_empty());
    let resolution = resolve(&db, file);
    for name in ["first", "nested", "rest", "item", "error"] {
        symbol_named(&resolution, name, SymbolKind::Pattern);
    }
    symbol_named(&resolution, "state", SymbolKind::LoopState);
    for name in ["item", "error", "state"] {
        let symbol = resolution
            .hir
            .symbols
            .iter()
            .find_map(|(id, data)| (data.name == name).then_some(id))
            .unwrap();
        assert!(
            !resolution.references(symbol).is_empty(),
            "missing reference to {name}"
        );
    }
}

#[test]
fn repeated_bindings_resolve_to_latest_preceding_declaration() {
    let source = "let value = 1 let value = 2 value";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let symbols = resolution
        .hir
        .symbols
        .iter()
        .filter_map(|(id, symbol)| (symbol.name == "value").then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(symbols.len(), 2);
    assert!(resolution.references(symbols[0]).is_empty());
    assert_eq!(resolution.references(symbols[1]).len(), 1);
}

#[test]
fn unresolved_host_reads_and_assignments_are_not_diagnostics() {
    let source = "host_value host_value = other_host";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    assert!(diagnostics(&db, file).is_empty());
    let resolution = resolve(&db, file);
    assert_eq!(resolution.hir.occurrences.len(), 3);
    assert!(resolution.occurrence_symbols.iter().all(Option::is_none));
}

#[test]
fn builtins_resolve_and_can_be_shadowed() {
    let source = "type(value) do let type = fn(value) do value end type(value) end inspect(value)";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let builtin_type = symbol_named(&resolution, "type", SymbolKind::Builtin);
    let local_type = symbol_named(&resolution, "type", SymbolKind::Let);
    assert_eq!(resolution.references(builtin_type).len(), 1);
    assert_eq!(resolution.references(local_type).len(), 1);
    assert_eq!(
        resolution.check_rename(builtin_type, "kind"),
        Err(RenameError::Builtin)
    );
}

#[test]
fn supports_symbol_lookup_hover_references_and_visible_symbols() {
    let source = "let value = 1 fn use(parameter) do value + parameter end use(value)";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let use_offset = source.rfind("use").unwrap();
    let function = resolution
        .symbol_at(use_offset)
        .expect("function reference");
    let hover = resolution.hover(use_offset).expect("hover facts");
    assert_eq!(hover.name, "use");
    assert_eq!(hover.arity, Some(1));
    assert_eq!(resolution.definition_span(function), hover.declaration);
    assert_eq!(references(&db, file, function).len(), 1);

    let parameter_offset = source.find("parameter end").unwrap();
    let visible = resolution.visible_symbols(parameter_offset);
    let names = visible
        .iter()
        .map(|id| resolution.hir.symbols[*id].name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"parameter"));
    assert!(names.contains(&"value"));
    assert!(names.contains(&"require"));
}

#[test]
fn rename_checks_same_scope_and_reference_rebinding_collisions() {
    let source = "let first = 1 let second = 2 do let hidden = 3 first + hidden end";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let first = symbol_named(&resolution, "first", SymbolKind::Let);
    assert!(matches!(
        resolution.check_rename(first, "second"),
        Err(RenameError::Collision { .. })
    ));
    assert!(matches!(
        resolution.check_rename(first, "hidden"),
        Err(RenameError::Collision { .. })
    ));
    assert_eq!(resolution.check_rename(first, "renamed"), Ok(()));
    assert_eq!(
        resolution.check_rename(first, "not-valid"),
        Err(RenameError::InvalidName)
    );
}

#[test]
fn parser_diagnostics_and_later_symbols_survive_recovery() {
    let source = "let broken = ) fn later() do nil end";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    assert!(!diagnostics(&db, file).is_empty());
    assert!(
        document_symbols(&db, file)
            .iter()
            .any(|symbol| symbol.name == "later")
    );
}

#[test]
fn source_updates_invalidate_dependent_queries() {
    let mut db = AnalysisDatabase::default();
    let file = db.add_file("let before = 1 before");
    let before_parse = parse(&db, file);
    assert!(
        document_symbols(&db, file)
            .iter()
            .any(|symbol| symbol.name == "before")
    );

    db.set_source(file, "let after = 2 after");
    let after_parse = parse(&db, file);
    assert_ne!(
        before_parse.syntax().text().to_string(),
        after_parse.syntax().text().to_string()
    );
    assert_eq!(source_text(&db, file).as_str(), "let after = 2 after");
    let symbols = document_symbols(&db, file);
    assert!(symbols.iter().any(|symbol| symbol.name == "after"));
    assert!(!symbols.iter().any(|symbol| symbol.name == "before"));
}
