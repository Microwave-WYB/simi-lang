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
        "fn apply(values: [integer, string], output: [..string]) -> nil ",
        "after values becomes [..integer | string] ",
        "after output becomes [..string] do nil end\n",
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
            .filter(|node| node.kind() == SyntaxKind::POST_CONDITION)
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
