use simi_syntax::generated::{AstNode, Root, Stmt};
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
fn delimiters_belong_to_their_typed_nodes() {
    let source = concat!(
        "case [1] of [head, ..tail] do head end ",
        "try 1 catch _ do 2 end ",
        "if false then 0 else f(1) end",
    );
    let parse = parse_source(source);
    assert!(parse.diagnostics().is_empty(), "{:?}", parse.diagnostics());
    for (node_kind, token_kind) in [
        (SyntaxKind::CASE_CLAUSE, SyntaxKind::OF_KW),
        (SyntaxKind::CATCH_CLAUSE, SyntaxKind::CATCH_KW),
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
fn erased_type_surface_is_lossless_and_alias_is_contextual() {
    let source = concat!(
        "let alias = 1\n",
        "alias option<'a> = 'a | nil\n",
        "let value: option<string> = nil\n",
        "fn apply(values: [integer, string] => [..(integer | string)], ",
        "output: [..string] => [..string]) -> nil do nil end\n",
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
    assert_eq!(
        parse
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::POST_TYPE)
            .count(),
        2
    );
    assert!(
        parse
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::TYPE_LIST_REST)
    );
}

#[test]
fn post_state_types_require_unambiguous_parameter_boundaries() {
    let valid = parse_source("let append: ([..'a] => [..('a | 'b)], 'b) -> nil = host.append\n");
    assert!(valid.diagnostics().is_empty(), "{:?}", valid.diagnostics());
    assert_eq!(
        valid
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::POST_TYPE)
            .count(),
        1
    );

    let ambiguous = parse_source("let bad: 'a | 'b => 'b -> 'b = nil\n");
    assert!(ambiguous.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("ambiguous post-state annotation")
    }));

    let missing_result = parse_source("let bad: ('a => 'b) = nil\n");
    assert!(missing_result.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("must be followed by `->` and a result type")
    }));
}

#[test]
fn callable_generics_labels_effects_and_leading_unions_are_lossless() {
    let source = concat!(
        "fn identity<'a: | integer | string>(value: 'a) -> 'a noraise do value end\n",
        "let mapper: <'a, 'error: { error: string, .. }> (value: 'a) -> 'a raises 'error = nil\n",
        "let callback: (input: | integer | string, state: [..integer] => [..integer]) -> nil = nil\n",
        "let anonymous = fn<'a: any>(value: 'a) -> 'a raises string do value end\n",
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
            "let value: integer -> string -> boolean raises string = nil",
            vec!["string -> boolean raises string"],
        ),
        (
            "let value: integer -> (string -> boolean) raises string = nil",
            vec!["integer -> (string -> boolean) raises string"],
        ),
        (
            "let value: integer -> (string -> boolean raises integer) raises string = nil",
            vec![
                "string -> boolean raises integer",
                "integer -> (string -> boolean raises integer) raises string",
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
fn callable_headers_and_effects_require_a_result_arrow() {
    let generic = parse_source("let bad: <'a> 'a = nil");
    assert!(generic.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("generic header must be followed by `->`")
    }));

    let effect = parse_source("fn bad() raises string do nil end");
    assert!(effect.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("effect requires `->` and a result type")
    }));
}

#[test]
fn malformed_callable_effects_recover_before_following_bodies_and_declarations() {
    let cases = [
        (
            "fn bad() -> nil raises do nil end\nlet after = 1",
            "expected a raised type after `raises`",
        ),
        (
            "fn bad() -> nil noraise string do nil end\nlet after = 1",
            "`noraise` does not accept a type",
        ),
        (
            "let bad: (value: integer) = nil\nlet after = 1",
            "labeled parameter list must be followed by `->`",
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
fn malformed_lexemes_are_preserved_as_error_tokens() {
    let source = "let x = @\nlet y = 2";
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
