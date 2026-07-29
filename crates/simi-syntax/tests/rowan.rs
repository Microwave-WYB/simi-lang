use simi_syntax::generated::{AstNode, RequiresDecl, Root, Stmt};
use simi_syntax::{DiagnosticKind, SyntaxKind, parse_source};

#[test]
fn tree_is_lossless_and_preserves_trivia() {
    let source = "-- heading\nlet cafe = [ 1, 2 ] -- tail\n";
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);
    assert!(
        parse
            .syntax()
            .descendants_with_tokens()
            .any(|element| element.kind() == SyntaxKind::COMMENT)
    );
    assert!(
        parse
            .syntax()
            .descendants_with_tokens()
            .any(|element| element.kind() == SyntaxKind::WHITESPACE)
    );
}

#[test]
fn representative_tree_shape_is_stable() {
    let parse = parse_source("let x = 1 + 2 -- tail\n");
    assert!(parse.diagnostics().is_empty());
    let kinds = parse
        .syntax()
        .descendants()
        .map(|node| format!("{:?}", node.kind()))
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            "ROOT",
            "LET_STMT",
            "BINDING_PATTERN",
            "BINARY_EXPR",
            "LITERAL_EXPR",
            "LITERAL_EXPR"
        ]
    );
}

#[test]
fn bytes_literals_are_lossless_typed_expressions() {
    let source = r#"let data = #[0, "PNG", payload,]"#;
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);
    let bytes = parse
        .syntax()
        .descendants()
        .find(|node| node.kind() == SyntaxKind::BYTES_EXPR)
        .expect("bytes expression");
    assert!(
        bytes
            .children_with_tokens()
            .any(|element| element.kind() == SyntaxKind::HASH)
    );
}

#[test]
fn bytes_patterns_are_lossless_typed_patterns_and_recover() {
    let source =
        r#"case packet of #["猫", version, header:bytes(2), payload:bytes] => payload end"#;
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);
    assert_eq!(
        parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::BYTES_PATTERN_SEGMENT)
            .count(),
        4
    );

    let source = "case value of #[rest:bytes, later] => nil end fn later()
    nil";
    let parse = parse_source(source);
    assert!(
        parse
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message == "unsized bytes capture must be final"),
        "{:?}",
        parse.diagnostics()
    );
    assert!(
        Root::cast(parse.syntax().clone())
            .expect("root")
            .statements()
            .any(|statement| matches!(statement, Stmt::FunctionDecl(_)))
    );
}

#[test]
fn delimiters_belong_to_their_typed_nodes() {
    let source = concat!(
        "case [1] of [head, ..tail] => head end ",
        "do 1 catch _ => 2 end ",
        "if false then 0 else f(1) end",
    );
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    for (node_kind, token_kind) in [
        (SyntaxKind::CASE_EXPR, SyntaxKind::OF_KW),
        (SyntaxKind::CASE_CLAUSE, SyntaxKind::FAT_ARROW),
        (SyntaxKind::PROTECTED_EXPR, SyntaxKind::CATCH_KW),
        (SyntaxKind::CATCH_ARM, SyntaxKind::FAT_ARROW),
        (SyntaxKind::REST_PATTERN, SyntaxKind::DOT_DOT),
        (SyntaxKind::ELSE_BRANCH, SyntaxKind::ELSE_KW),
        (SyntaxKind::ARG_LIST, SyntaxKind::L_PAREN),
    ] {
        let node = parse
            .syntax()
            .descendants()
            .find(|node| node.kind() == node_kind)
            .expect("typed node");
        assert!(
            node.children_with_tokens()
                .any(|element| element.kind() == token_kind),
            "{token_kind:?} must be a direct child of {node_kind:?}"
        );
    }
}

#[test]
fn legacy_catch_of_reports_migration_diagnostic() {
    let source = "do 1 catch of _ => 2 end";
    let parse = parse_source(source);
    assert_eq!(
        parse
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        ["`catch of` was removed; write `catch pattern => \u{2026}` instead"],
        "{:?}",
        parse.diagnostics()
    );
    assert_eq!(parse.syntax().to_string(), source);
    assert!(
        parse
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::PROTECTED_EXPR)
    );
}

#[test]
fn bang_never_functions_accept_varied_direct_expression_bodies() {
    let source = concat!(
        "fn identity(value: integer) -> integer ! never value\n",
        "fn text() -> string ! never \"ok\"\n",
        "fn values() -> [integer] ! never [1]\n",
        "fn nothing() -> nil ! never nil\n",
        "fn grouped() -> integer ! never (1 + 2)\n",
        "fn append(xs: [..integer]) -> nil ! never host.append(xs)",
    );
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);

    let body_kinds = parse
        .syntax()
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::BODY)
        .map(|body| body.children().next().expect("direct body").kind())
        .collect::<Vec<_>>();
    assert_eq!(
        body_kinds,
        [
            SyntaxKind::NAME_EXPR,
            SyntaxKind::LITERAL_EXPR,
            SyntaxKind::LIST_EXPR,
            SyntaxKind::LITERAL_EXPR,
            SyntaxKind::PAREN_EXPR,
            SyntaxKind::CALL_EXPR,
        ]
    );
    assert_eq!(
        parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::EFFECT_ANNOTATION)
            .count(),
        6
    );
}

#[test]
fn recovery_keeps_later_declarations_typed() {
    let parse = parse_source("let broken = )\nfn later() do nil end");
    assert!(
        parse
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind == DiagnosticKind::Parse)
    );
    let root = Root::cast(parse.syntax().clone()).expect("root");
    assert!(
        root.statements()
            .any(|statement| matches!(statement, Stmt::FunctionDecl(_)))
    );
    assert_eq!(
        parse.syntax().to_string(),
        "let broken = )\nfn later() do nil end"
    );
}

#[test]
fn list_spread_elements_are_lossless_and_typed() {
    let source = "let values = [1, ..[2, 3], 4,]";
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);
    let elements = parse
        .syntax()
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::LIST_ELEMENT)
        .collect::<Vec<_>>();
    assert_eq!(elements.len(), 5);
    assert!(
        elements[1]
            .children_with_tokens()
            .any(|element| element.kind() == SyntaxKind::DOT_DOT)
    );
}

#[test]
fn malformed_list_spreads_recover_before_later_declarations() {
    let source = "let broken = [..,]\nfn later() do nil end";
    let parse = parse_source(source);
    assert!(
        parse
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind == DiagnosticKind::Parse),
        "{:?}",
        parse.diagnostics()
    );
    assert_eq!(parse.syntax().to_string(), source);
    let root = Root::cast(parse.syntax().clone()).expect("root");
    assert!(
        root.statements()
            .any(|statement| matches!(statement, Stmt::FunctionDecl(_)))
    );
}

#[test]
fn malformed_bytes_literals_recover_before_later_declarations() {
    for source in [
        "#\nfn later() do nil end",
        "#[1 fn later()
    nil",
    ] {
        let parse = parse_source(source);
        assert!(
            parse
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.kind == DiagnosticKind::Parse),
            "{source}: {:?}",
            parse.diagnostics()
        );
        assert_eq!(parse.syntax().to_string(), source);

        let root = Root::cast(parse.syntax().clone()).expect("root");
        assert!(
            root.statements()
                .any(|statement| matches!(statement, Stmt::FunctionDecl(_))),
            "{source}"
        );
    }
}

#[test]
fn postfix_nil_propagation_requires_an_enclosing_block() {
    let source = "nil?";
    let parse = parse_source(source);
    assert_eq!(parse.syntax().to_string(), source);
    assert_eq!(parse.diagnostics().len(), 1);
    let diagnostic = &parse.diagnostics()[0];
    assert_eq!(diagnostic.kind, DiagnosticKind::Parse);
    assert_eq!(diagnostic.span.start, 3);
    assert_eq!(diagnostic.span.end, 4);
    assert_eq!(diagnostic.message, "nil propagation `?` outside of a block");
    assert!(
        parse
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::NIL_PROPAGATE_EXPR)
    );
}

#[test]
fn utf8_tokens_keep_byte_ranges() {
    let source = "let cafe = \"猫\"";
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty());
    let identifier = parse
        .syntax()
        .descendants_with_tokens()
        .find_map(|element| {
            element
                .into_token()
                .filter(|token| token.kind() == SyntaxKind::STRING)
        })
        .expect("string");
    assert_eq!(u32::from(identifier.text_range().start()), 11);
    assert_eq!(u32::from(identifier.text_range().end()), 16);
}

#[test]
fn primitive_singleton_types_are_lossless() {
    let source = concat!(
        "alias Step<'a> =\n",
        "    | { done: true, .. }\n",
        "    | { done: false, value: 'a, .. }\n",
        "alias Literals = nil | \"ready\" | 42 | -7 | 0.5 | -0.0\n",
    );
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);

    let literals = parse
        .syntax()
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::TYPE_LITERAL)
        .collect::<Vec<_>>();
    assert_eq!(literals.len(), 8);
    assert_eq!(literals[0].to_string(), "true");
    assert_eq!(literals[1].to_string(), "false");
    assert_eq!(literals[2].to_string(), "nil");
    assert_eq!(literals[3].to_string(), "\"ready\"");
    assert_eq!(literals[4].to_string(), "42");
    assert_eq!(literals[5].to_string(), "-7");
    assert_eq!(literals[6].to_string(), "0.5");
    assert_eq!(literals[7].to_string(), "-0.0");
    assert!(
        literals[0]
            .children_with_tokens()
            .any(|element| element.kind() == SyntaxKind::TRUE_KW)
    );
    assert!(
        literals[1]
            .children_with_tokens()
            .any(|element| element.kind() == SyntaxKind::FALSE_KW)
    );
}

#[test]
fn erased_type_surface_is_lossless_and_alias_is_contextual() {
    let source = concat!(
        "let alias = 1\n",
        "alias option<'a> = 'a | nil\n",
        "let value: option<string> = nil\n",
        "fn apply(values: [integer, string], output: [..string]) -> nil
    nil\n",
        "let record: { name: string, [string | integer]: boolean, .. } = {}\n",
    );
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);
    let root = Root::cast(parse.syntax().clone()).unwrap();
    assert!(matches!(root.statements().nth(1), Some(Stmt::AliasDecl(_))));
    assert!(
        parse
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::TYPE_FUNCTION)
    );
    assert!(
        parse
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::TYPE_MAP)
    );
    assert!(
        parse
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::TYPE_LIST_REST)
    );
}

#[test]
fn callable_generics_labels_effects_and_leading_unions_are_lossless() {
    let source = concat!(
        "fn identity<'a: | integer | string>(value: 'a) -> 'a ! never
    value\n",
        "let mapper: fn<'a, 'error: { error: string, .. }>(value: 'a) -> 'a ! 'error = nil\n",
        "let callback: fn(input: | integer | string, state: [..integer]) -> nil = nil\n",
        "let anonymous = fn<'a: any>(value: 'a) -> 'a ! string
    value\n",
    );
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);
    assert_eq!(
        parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::EFFECT_ANNOTATION)
            .count(),
        3
    );
    assert_eq!(
        parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::TYPE_CONSTRAINT)
            .count(),
        3
    );
}

#[test]
fn callable_effect_tails_bind_to_the_nearest_arrow() {
    let sources_and_parents = [
        (
            "let value: fn(integer) -> fn(string) -> boolean ! string = nil",
            vec!["fn(string) -> boolean ! string"],
        ),
        (
            "let value: fn(integer) -> (fn(string) -> boolean) ! string = nil",
            vec!["fn(integer) -> (fn(string) -> boolean) ! string"],
        ),
        (
            "let value: fn(integer) -> (fn(string) -> boolean ! integer) ! string = nil",
            vec![
                "fn(string) -> boolean ! integer",
                "fn(integer) -> (fn(string) -> boolean ! integer) ! string",
            ],
        ),
    ];

    for (source, expected_parents) in sources_and_parents {
        let parse = parse_source(source);
        assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
        let actual = parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::EFFECT_ANNOTATION)
            .map(|effect| effect.parent().expect("effect parent").to_string())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected_parents, "{source}");
    }
}

#[test]
fn legacy_callable_types_are_rejected() {
    for source in [
        "let bad: integer -> string = nil",
        "let bad: <'a> 'a -> 'a = nil",
        "let bad: (value: integer) -> string = nil",
    ] {
        let parse = parse_source(source);
        assert!(!parse.diagnostics().is_empty(), "{source}");
    }

    for source in [
        "fn old() -> nil noraise
    nil",
        "fn old() -> nil raises string
    nil",
    ] {
        let parse = parse_source(source);
        assert!(!parse.diagnostics().is_empty(), "{source}");
        assert_eq!(
            parse
                .syntax()
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::EFFECT_ANNOTATION)
                .count(),
            0,
            "{source}"
        );
    }

    let effect = parse_source(
        "fn bad() ! string
    nil",
    );
    assert!(effect.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("raised-error contract requires `->` and a result type")
    }));
}

#[test]
fn malformed_callable_effects_recover_before_following_bodies_and_declarations() {
    let cases = [
        (
            "fn bad() -> nil ! do nil end\nlet after = 1",
            "expected a raised type after `!`",
        ),
        (
            "fn bad() -> nil ! [..]
    nil\nlet after = 1",
            "expected type, found `]`",
        ),
        (
            "let bad: (value: integer) = nil\nlet after = 1",
            "parameter labels are only valid in `fn(...)` callable types",
        ),
    ];
    for (source, expected) in cases {
        let parse = parse_source(source);
        assert!(
            parse
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "{source}: {:?}",
            parse.diagnostics()
        );
        let root = Root::cast(parse.syntax().clone()).expect("root");
        assert!(root.statements().any(|statement| {
            let Stmt::LetStmt(statement) = statement else {
                return false;
            };
            statement.syntax().to_string().contains("after")
        }));
    }
}

#[test]
fn leading_requires_declaration_is_typed_before_executable_statements() {
    let source = "requires {name = \"example\", version = 1}\nlet value = 40\nvalue + 2";
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);

    let root = Root::cast(parse.syntax().clone()).expect("root");
    let declaration = root
        .syntax()
        .children()
        .find_map(RequiresDecl::cast)
        .expect("typed requires declaration");
    assert!(
        declaration
            .syntax()
            .children()
            .any(|child| child.kind() == SyntaxKind::MAP_EXPR)
    );
    assert_eq!(root.statements().count(), 2);
}

#[test]
fn requires_declarations_diagnose_duplicate_and_later_placement() {
    let duplicate = parse_source("requires {} requires {}");
    assert_eq!(
        duplicate
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        ["a source unit may contain at most one `requires` declaration"]
    );
    assert_eq!(
        duplicate
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::REQUIRES_DECL)
            .count(),
        2
    );

    let later = parse_source("1 requires {}");
    assert_eq!(
        later
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        ["`requires` must appear before executable items"]
    );
    let root = Root::cast(later.syntax().clone()).expect("root");
    assert_eq!(root.statements().count(), 1);
}

#[test]
fn malformed_requires_declaration_recovers_to_following_declaration() {
    let source = "requires\nlet after = 2";
    let parse = parse_source(source);
    assert_eq!(parse.syntax().to_string(), source);
    assert_eq!(
        parse
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>(),
        ["`requires` must be followed by a map"]
    );

    let root = Root::cast(parse.syntax().clone()).expect("root");
    assert!(root.statements().any(|statement| {
        let Stmt::LetStmt(statement) = statement else {
            return false;
        };
        statement.syntax().to_string().contains("after")
    }));
}

#[test]
fn malformed_lexemes_are_preserved_as_error_tokens() {
    let source = "let x = $\nlet y = 2";
    let parse = parse_source(source);
    assert_eq!(parse.syntax().to_string(), source);
    assert!(
        parse
            .syntax()
            .descendants_with_tokens()
            .any(|element| element.kind() == SyntaxKind::ERROR_TOKEN)
    );
    let root = Root::cast(parse.syntax().clone()).expect("root");
    assert_eq!(
        root.statements()
            .filter(|statement| matches!(statement, Stmt::LetStmt(_)))
            .count(),
        2
    );
}

#[test]
fn map_local_binding_shorthand_is_a_map_entry_without_pattern_changes() {
    let source = "let first = 1 let map = {first, label = \"pair\", [true] = first}";
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);
    let entries = parse
        .syntax()
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::MAP_ENTRY)
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 3);
    assert!(
        entries[0]
            .children()
            .all(|child| child.kind() != SyntaxKind::NAME_EXPR)
    );
}

#[test]
fn map_pattern_binding_shorthand_is_accepted() {
    let source = "let {first, source = renamed, ..rest} = record";
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    assert_eq!(parse.syntax().to_string(), source);
    let fields = parse
        .syntax()
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::MAP_PATTERN_FIELD)
        .collect::<Vec<_>>();
    assert_eq!(fields.len(), 2);
    assert!(
        fields[0]
            .children()
            .all(|child| child.kind() != SyntaxKind::BINDING_PATTERN)
    );
    assert!(
        fields[1]
            .children()
            .any(|child| child.kind() == SyntaxKind::BINDING_PATTERN)
    );
}

#[test]
fn callable_post_state_syntax_is_rejected() {
    let parse = parse_source(
        "fn append(xs: [..integer] => [..integer]) -> nil
    nil\n",
    );
    assert!(parse.diagnostics().iter().any(|diagnostic| {
        diagnostic.message.contains("found `=>`") || diagnostic.message.contains("expected `)`")
    }));
}

#[test]
fn nested_named_function_declarations_are_rejected_at_the_fn_token() {
    let source = "fn outer() do\n    fn inner()
    nil\n    inner\nend\nfn later() do nil end\n";
    let parse = parse_source(source);
    assert_eq!(parse.syntax().to_string(), source);
    assert_eq!(parse.diagnostics().len(), 1, "{:?}", parse.diagnostics());
    let diagnostic = &parse.diagnostics()[0];
    assert_eq!(diagnostic.kind, DiagnosticKind::Parse);
    let start = source.find("fn inner").expect("nested declaration offset");
    assert_eq!(diagnostic.span.start, start);
    assert_eq!(diagnostic.span.end, start + "fn".len());
    assert_eq!(
        diagnostic.message,
        "named function declarations are only allowed at the top level; \
         use let name = fn(...) expression"
    );
    // Lossless recovery keeps the nested declaration node and still parses the
    // later top-level declaration.
    assert_eq!(
        parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::FUNCTION_DECL)
            .count(),
        3
    );
    let root = Root::cast(parse.syntax().clone()).expect("root");
    assert!(root.statements().any(|statement| {
        matches!(&statement, Stmt::FunctionDecl(declaration)
        if declaration.syntax().children_with_tokens().any(|element| {
            element.as_token().is_some_and(|token| {
                token.kind() == SyntaxKind::IDENT && token.text() == "later"
            })
        }))
    }));
}

#[test]
fn named_function_declarations_in_do_and_conditional_blocks_are_rejected() {
    for source in [
        "do\n    fn helper()
    nil\nend\n",
        "if ready then\n    fn helper()
    nil\nend\n",
        "if ready then\n    nil\nelse\n    fn helper()
    nil\nend\n",
    ] {
        let parse = parse_source(source);
        assert_eq!(parse.syntax().to_string(), source);
        let diagnostic = parse
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.message.contains("only allowed at the top level"))
            .unwrap_or_else(|| panic!("missing diagnostic for {source:?}"));
        let start = source.find("fn helper").expect("nested declaration offset");
        assert_eq!(diagnostic.span.start, start, "{source}");
        assert_eq!(diagnostic.span.end, start + "fn".len(), "{source}");
    }
}

#[test]
fn top_level_named_function_declarations_remain_accepted() {
    let parse = parse_source(
        "fn add(left, right)
    left + right\nadd(1, 2)\n",
    );
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
}

#[test]
fn binding_reassignment_is_rejected_at_the_target_with_lossless_recovery() {
    let source = "let value = 1\nvalue = 2\nvalue\n";
    let parse = parse_source(source);
    assert_eq!(parse.syntax().to_string(), source);
    assert_eq!(parse.diagnostics().len(), 1, "{:?}", parse.diagnostics());
    let diagnostic = &parse.diagnostics()[0];
    assert_eq!(diagnostic.kind, DiagnosticKind::Parse);
    let start = source.find("value = 2").expect("reassignment offset");
    assert_eq!(diagnostic.span.start, start);
    assert_eq!(diagnostic.span.end, start + "value".len());
    assert_eq!(
        diagnostic.message,
        "bindings are immutable and cannot be reassigned; \
         declare a new binding with let or mutate a list or map field"
    );
    // Lossless recovery keeps the assignment expression in the tree.
    assert_eq!(
        parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::ASSIGN_EXPR)
            .count(),
        1
    );
}

#[test]
fn field_and_index_assignment_targets_remain_accepted() {
    for source in [
        "settings.enabled = true\n",
        "values[0] = 3\n",
        "outer.inner[key] = outer.inner[key] + 1\n",
    ] {
        let parse = parse_source(source);
        assert!(
            parse.diagnostics().is_empty(),
            "{source}: {:?}",
            parse.diagnostics()
        );
    }
}
