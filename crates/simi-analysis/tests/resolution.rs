use simi_analysis::{
    AnalysisDatabase, AnalysisDiagnosticCode, OccurrenceKind, RenameError, SymbolId, SymbolKind,
    diagnostics, document_symbols, parse, references, resolve, source_text,
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
fn closures_resolve_and_expose_bindings_declared_later_in_captured_frames() {
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
    let visible = resolution.visible_symbols(source.find("later end").unwrap());
    assert!(visible.contains(&later));
}

#[test]
fn repeated_let_shadows_while_earlier_closures_keep_the_prior_symbol() {
    let source = "let closure = fn() do later end let later = 1 let later = 2 later";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let laters = resolution
        .hir
        .symbols
        .iter()
        .filter_map(|(id, symbol)| (symbol.name == "later").then_some(id))
        .collect::<Vec<_>>();

    assert_eq!(laters.len(), 2);
    assert_eq!(resolution.references(laters[0]).len(), 1);
    assert_eq!(resolution.references(laters[1]).len(), 1);
    assert_eq!(
        resolution.symbol_at(source.find("later end").unwrap()),
        Some(laters[0])
    );
    assert_eq!(
        resolution.symbol_at(source.rfind("later").unwrap()),
        Some(laters[1])
    );
    assert!(diagnostics(&db, file).is_empty());
}

#[test]
fn later_outer_bindings_hide_prelude_symbols_inside_closures() {
    let source = "let closure = fn() do type(nil) end let type = fn(value) do value end";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let builtin = symbol_named(&resolution, "type", SymbolKind::Builtin);
    let local = symbol_named(&resolution, "type", SymbolKind::Let);
    let offset = source.find("type(nil)").unwrap();

    assert_eq!(resolution.symbol_at(offset), Some(local));
    let visible = resolution.visible_symbols(offset);
    assert!(visible.contains(&local));
    assert!(!visible.contains(&builtin));
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
fn shadow_versions_partition_initializer_closure_references_and_renames() {
    let source = r#"let value = 1
let before = fn() do value end
let value = value + 1
let after_value = fn() do value end
value"#;
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let mut values = resolution
        .hir
        .symbols
        .iter()
        .filter(|(_, symbol)| symbol.name == "value")
        .collect::<Vec<_>>();
    values.sort_by_key(|(_, symbol)| symbol.declaration.unwrap().start);
    let first = values[0].0;
    let second = values[1].0;

    assert_eq!(resolution.references(first).len(), 2);
    assert_eq!(resolution.references(second).len(), 2);
    assert_eq!(
        resolution.symbol_at(source.find("value + 1").unwrap()),
        Some(first)
    );
    assert_eq!(
        resolution.symbol_at(source.rfind("value").unwrap()),
        Some(second)
    );
    let first_rename = resolution.rename_spans(first);
    let second_rename = resolution.rename_spans(second);
    assert_eq!(first_rename.len(), 3);
    assert_eq!(second_rename.len(), 3);
    assert!(
        first_rename
            .iter()
            .all(|span| !second_rename.contains(span))
    );
    assert!(resolution.check_rename(first, "prior_value").is_ok());
    assert!(resolution.check_rename(second, "current_value").is_ok());
    assert!(diagnostics(&db, file).is_empty());
}

#[test]
fn repeated_bindings_create_distinct_symbols_and_later_reads_use_the_latest() {
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
    assert!(diagnostics(&db, file).is_empty());
}

#[test]
fn unresolved_host_reads_and_assignments_are_not_diagnostics_or_rename_targets() {
    let source = "host_value host_value = other_host";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    assert!(diagnostics(&db, file).is_empty());
    let resolution = resolve(&db, file);
    assert_eq!(resolution.hir.occurrences.len(), 3);
    assert!(resolution.occurrence_symbols.iter().all(Option::is_none));
    for offset in [
        source.find("host_value").unwrap(),
        source.rfind("host_value").unwrap(),
        source.find("other_host").unwrap(),
    ] {
        assert_eq!(resolution.symbol_at(offset), None);
        assert_eq!(resolution.hover(offset), None);
    }
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
fn visible_symbols_respect_activation_and_do_not_hide_outer_symbols() {
    let source = "let value = 0 do value let value = 1 value end";
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
        .expect("outer value");
    let inner = resolution
        .hir
        .symbols
        .iter()
        .find_map(|(id, symbol)| {
            (symbol.name == "value" && symbol.scope != resolution.hir.root_scope).then_some(id)
        })
        .expect("inner value");
    let before = resolution.visible_symbols(source.find("value let").unwrap());
    assert!(before.contains(&outer));
    assert!(!before.contains(&inner));
    let after = resolution.visible_symbols(source.rfind("value end").unwrap());
    assert!(!after.contains(&outer));
    assert!(after.contains(&inner));
}

#[test]
fn future_symbols_do_not_appear_or_hide_prelude_symbols() {
    let source =
        "do type(nil) future let type = fn(value) do value end let future = 1 type(nil) future end";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let builtin = symbol_named(&resolution, "type", SymbolKind::Builtin);
    let local = symbol_named(&resolution, "type", SymbolKind::Let);
    let future = symbol_named(&resolution, "future", SymbolKind::Let);

    let before = resolution.visible_symbols(source.find("type(nil)").unwrap());
    assert!(before.contains(&builtin));
    assert!(!before.contains(&local));
    assert!(!before.contains(&future));

    let after = resolution.visible_symbols(source.rfind("type(nil)").unwrap());
    assert!(!after.contains(&builtin));
    assert!(after.contains(&local));
    assert!(after.contains(&future));
}

#[test]
fn rename_checks_collisions_and_exact_lexer_identifier_rules() {
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
    for valid in ["renamed", "_", "_value2", "ASCII_9", "is"] {
        assert_eq!(resolution.check_rename(first, valid), Ok(()), "{valid}");
    }
    for invalid in ["", "1value", "not-valid", "with space", "café", "é"] {
        assert_eq!(
            resolution.check_rename(first, invalid),
            Err(RenameError::InvalidName),
            "{invalid}"
        );
    }
    for keyword in [
        "fn", "do", "end", "if", "then", "elseif", "else", "let", "tap", "nil", "true", "false",
        "and", "or", "not", "loop", "break", "continue", "case", "of", "when", "raise", "try",
        "catch",
    ] {
        assert_eq!(
            resolution.check_rename(first, keyword),
            Err(RenameError::InvalidName),
            "keyword {keyword}"
        );
    }
}

#[test]
fn rename_rejects_capture_of_unresolved_and_shadowed_occurrences() {
    for source in [
        "let target = 1 do missing target end",
        "let target = 1 do let missing = 2 target end",
        "let closure = fn() do missing end let target = 1 closure()",
    ] {
        let db = AnalysisDatabase::default();
        let file = db.add_file(source);
        let resolution = resolve(&db, file);
        let target = symbol_named(&resolution, "target", SymbolKind::Let);
        assert!(
            matches!(
                resolution.check_rename(target, "missing"),
                Err(RenameError::Collision { .. })
            ),
            "source: {source}"
        );
    }
}

#[test]
fn rename_preserves_unresolved_host_names_that_stay_out_of_scope() {
    let source = "do missing end let target = 1 target";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let target = symbol_named(&resolution, "target", SymbolKind::Let);
    assert_eq!(resolution.check_rename(target, "missing"), Ok(()));
}

#[test]
fn analysis_owns_symbol_and_rename_spans() {
    let source = "let target = 1 target";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let target = symbol_named(&resolution, "target", SymbolKind::Let);
    let declaration = source.find("target").expect("declaration");
    let reference = source.rfind("target").expect("reference");
    assert_eq!(
        resolution.symbol_span_at(reference),
        Some((target, simi_analysis::Span::new(reference, reference + 6)))
    );
    assert_eq!(
        resolution.rename_spans(target),
        vec![
            simi_analysis::Span::new(declaration, declaration + 6),
            simi_analysis::Span::new(reference, reference + 6),
        ]
    );
}

#[test]
fn parser_diagnostics_and_later_symbols_survive_recovery() {
    let source = "let broken = ) fn later() do nil end";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let diagnostics = diagnostics(&db, file);
    assert_eq!(diagnostics[0].code, AnalysisDiagnosticCode::SyntaxError);
    assert_eq!(diagnostics[0].title, "Syntax error");
    assert_eq!(diagnostics[0].detail, "Expected expression, found `)`.");
    assert_eq!(
        diagnostics[0].message(),
        "Syntax error\n\nExpected expression, found `)`."
    );
    assert!(diagnostics[0].related.is_empty());
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

    db.set_source(file, "let after_value = 2 after_value");
    let after_parse = parse(&db, file);
    assert_ne!(
        before_parse.syntax().text().to_string(),
        after_parse.syntax().text().to_string()
    );
    assert_eq!(
        source_text(&db, file).as_str(),
        "let after_value = 2 after_value"
    );
    let symbols = document_symbols(&db, file);
    assert!(symbols.iter().any(|symbol| symbol.name == "after_value"));
    assert!(!symbols.iter().any(|symbol| symbol.name == "before"));
}

#[test]
fn recovery_prefers_the_function_scope_when_it_ties_the_root_span() {
    let source = "fn fib(n) do\n    case n\n    of\nend";
    let db = AnalysisDatabase::default();
    let file = db.add_file(source);
    let resolution = resolve(&db, file);
    let offset = source.find("case n").unwrap() + "case n".len();
    let visible = resolution.visible_symbols(offset);
    let names = visible
        .iter()
        .map(|id| resolution.hir.symbols[*id].name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"n"), "visible names: {names:?}");
}
