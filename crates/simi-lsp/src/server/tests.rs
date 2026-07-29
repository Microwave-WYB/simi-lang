use std::thread;

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types::notification::{Exit, Notification as _, PublishDiagnostics};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, PrepareRenameRequest, Rename,
    Request as _, Shutdown,
};
use lsp_types::{
    CompletionResponse, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DocumentSymbolResponse, GotoDefinitionResponse, HoverContents, Position,
    PublishDiagnosticsParams, TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier,
};
use serde_json::{Value, json};

use super::*;

fn uri() -> Url {
    Url::parse("file:///workspace/test.simi").unwrap()
}

fn open(backend: &mut Backend, source: &str) -> Vec<Notification> {
    backend.open(
        serde_json::from_value(json!({
            "textDocument": {
                "uri": uri(),
                "languageId": "simi",
                "version": 1,
                "text": source
            }
        }))
        .unwrap(),
    )
}

fn diagnostics_from(notification: Notification) -> PublishDiagnosticsParams {
    assert_eq!(notification.method, PublishDiagnostics::METHOD);
    serde_json::from_value(notification.params).unwrap()
}

fn request(backend: &mut Backend, method: &str, params: Value) -> Result<Value, ProtocolError> {
    backend.request(method, params)
}

fn assert_simi_hover(markup: &MarkupContent, expected: &str) {
    let (detail, documentation) = expected
        .split_once("\n\n")
        .map_or((expected, None), |(detail, documentation)| {
            (detail, Some(documentation))
        });
    let mut formatted = format!("```simi\n{detail}\n```");
    if let Some(documentation) = documentation {
        formatted.push_str("\n\n");
        formatted.push_str(documentation);
    }
    assert_eq!(markup.kind, MarkupKind::Markdown);
    assert_eq!(markup.value, formatted);
}

fn assert_simi_hover_raw(markup: &MarkupContent) -> String {
    assert_eq!(markup.kind, MarkupKind::Markdown);
    let value = &markup.value;
    let start = value.find("```simi\n").unwrap();
    let body_start = start + "```simi\n".len();
    let end = value[body_start..].find("\n```").unwrap();
    value[body_start..body_start + end].to_owned()
}

fn text_position(source: &str, needle: &str, occurrence: usize) -> Position {
    let offset = source
        .match_indices(needle)
        .nth(occurrence)
        .unwrap_or_else(|| panic!("missing occurrence {occurrence} of {needle}"))
        .0;
    position::position(source, offset).unwrap()
}

#[test]
fn advertises_incremental_utf16_and_all_supported_features() {
    let capabilities = Backend::capabilities();
    let Some(TextDocumentSyncCapability::Options(sync)) = capabilities.text_document_sync else {
        panic!("expected sync options")
    };
    assert_eq!(sync.open_close, Some(true));
    assert_eq!(sync.change, Some(TextDocumentSyncKind::INCREMENTAL));
    assert_eq!(
        capabilities.position_encoding,
        Some(lsp_types::PositionEncodingKind::UTF16)
    );
    assert!(capabilities.document_symbol_provider.is_some());
    assert!(capabilities.definition_provider.is_some());
    assert!(capabilities.references_provider.is_some());
    assert!(capabilities.rename_provider.is_some());
    assert!(capabilities.hover_provider.is_some());
    assert!(capabilities.completion_provider.is_some());
}

#[test]
fn ordered_incremental_unicode_changes_replace_and_clear_diagnostics() {
    let mut backend = Backend::new();
    let source = "let value = \"😀\"\nlet = 1";
    let opened = open(&mut backend, source);
    let diagnostics = diagnostics_from(opened.into_iter().next().unwrap());
    assert_eq!(diagnostics.version, Some(1));
    assert!(!diagnostics.diagnostics.is_empty());

    let params = DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: uri(),
            version: 2,
        },
        content_changes: vec![
            TextDocumentContentChangeEvent {
                range: Some(lsp_types::Range::new(
                    Position::new(0, 13),
                    Position::new(0, 15),
                )),
                range_length: Some(2),
                text: "猫".to_owned(),
            },
            TextDocumentContentChangeEvent {
                range: Some(lsp_types::Range::new(
                    Position::new(1, 4),
                    Position::new(1, 4),
                )),
                range_length: Some(0),
                text: "x".to_owned(),
            },
        ],
    };
    let changed = backend.change(params).unwrap();
    let diagnostics = diagnostics_from(changed.into_iter().next().unwrap());
    assert_eq!(diagnostics.version, Some(2));
    assert!(diagnostics.diagnostics.is_empty());
    let document = backend.documents.get(&uri()).unwrap();
    assert_eq!(
        source_text(&backend.db, document.file).as_str(),
        "let value = \"猫\"\nlet x= 1"
    );

    let stale = backend
        .change(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "let stale = true".to_owned(),
            }],
        })
        .unwrap();
    assert!(stale.is_empty());

    let closed = backend.close(DidCloseTextDocumentParams {
        text_document: lsp_types::TextDocumentIdentifier { uri: uri() },
    });
    let diagnostics = diagnostics_from(closed.into_iter().next().unwrap());
    assert_eq!(diagnostics.version, None);
    assert!(diagnostics.diagnostics.is_empty());
}

#[test]
fn invalid_incremental_position_is_rejected_without_mutating_source() {
    let mut backend = Backend::new();
    open(&mut backend, "let value = \"😀\"");
    let result = backend.change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: uri(),
            version: 2,
        },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range::new(
                Position::new(0, 14),
                Position::new(0, 15),
            )),
            range_length: None,
            text: "x".to_owned(),
        }],
    });
    assert!(result.is_err());
    let document = backend.documents.get(&uri()).unwrap();
    assert_eq!(document.version, 1);
    assert_eq!(
        source_text(&backend.db, document.file).as_str(),
        "let value = \"😀\""
    );
}

#[test]
fn crlf_terminator_positions_are_rejected_without_mutating_source() {
    let mut backend = Backend::new();
    open(&mut backend, "a\r\nb");
    let result = backend.change(DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier {
            uri: uri(),
            version: 2,
        },
        content_changes: vec![TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range::new(
                Position::new(0, 1),
                Position::new(0, 2),
            )),
            range_length: None,
            text: String::new(),
        }],
    });
    assert!(result.is_err());
    let document = backend.documents.get(&uri()).unwrap();
    assert_eq!(document.version, 1);
    assert_eq!(source_text(&backend.db, document.file).as_str(), "a\r\nb");
}

#[test]
fn contextual_keyword_hover_does_not_capture_ordinary_identifiers() {
    let source = "let string = 1\nstring";
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(diagnostics.diagnostics.is_empty());

    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "string", 1),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("identifier hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "integer");
}

#[test]
fn syntax_diagnostics_use_structured_gleam_style_presentation() {
    let source = "let broken = )";
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert_eq!(diagnostics.diagnostics.len(), 1);
    let diagnostic = &diagnostics.diagnostics[0];
    assert_eq!(diagnostic.source.as_deref(), Some("simi"));
    assert_eq!(
        diagnostic.code,
        Some(lsp_types::NumberOrString::String("syntax_error".to_owned()))
    );
    assert_eq!(
        diagnostic.severity,
        Some(lsp_types::DiagnosticSeverity::ERROR)
    );
    assert_eq!(
        diagnostic.message,
        "Syntax error\n\nExpected expression, found `)`."
    );
    assert!(diagnostic.related_information.is_none());
    assert_eq!(diagnostic.range.start, text_position(source, ")", 0));
}

#[test]
fn completion_suppresses_exact_visible_identifiers_during_recovery() {
    let source = "fn fib(n) do\n    case n\n    of\nend";
    let mut backend = Backend::new();
    open(&mut backend, source);
    let cursor = source.find("case n").unwrap() + "case n".len();

    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(source, cursor).unwrap()
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    assert!(
        items.is_empty(),
        "exact parameter `n` should suppress completion"
    );
}

#[test]
fn completion_prioritizes_partial_lexical_matches_before_builtins() {
    let source = "fn find(needle) do\n    case ne\n    of\nend";
    let mut backend = Backend::new();
    open(&mut backend, source);
    let cursor = source.find("case ne").unwrap() + "case ne".len();

    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(source, cursor).unwrap()
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    let labels = items
        .iter()
        .map(|item| item.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels.first(), Some(&"needle"));
    assert!(labels.contains(&"inspect"));
    assert!(labels.contains(&"require"));
    assert!(
        items[0].sort_text.as_deref().unwrap()
            < items[labels.iter().position(|label| *label == "inspect").unwrap()]
                .sort_text
                .as_deref()
                .unwrap()
    );
}

#[test]
fn requires_keyword_has_completion_and_hover_help() {
    let completion_source = "requ";
    let mut backend = Backend::new();
    open(&mut backend, completion_source);
    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(completion_source, completion_source.len()).unwrap(),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    let item = items
        .iter()
        .find(|item| item.label == "requires")
        .expect("requires keyword completion");
    assert_eq!(item.kind, Some(CompletionItemKind::KEYWORD));
    assert_eq!(
        item.detail.as_deref(),
        Some("requires {alias = {git = url, rev = revision}}")
    );

    let hover_source = "requires {}";
    let mut backend = Backend::new();
    open(&mut backend, hover_source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(hover_source, "requires", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("requires keyword hover").contents else {
        panic!("expected markup")
    };
    assert_eq!(
        markup.value,
        "keyword `requires`\n\nDeclares static package requirements before executable source items.\n\nSyntax: requires {alias = {git = url, rev = revision}}"
    );
}

#[test]
fn same_scope_shadows_are_diagnostic_free_and_navigate_by_binding_version() {
    let source = "let closure = fn() do later end let later = 1 let later = 2 later";
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(diagnostics.diagnostics.is_empty());

    for (reference, declaration) in [(0, 1), (3, 2)] {
        let definition: Option<GotoDefinitionResponse> = serde_json::from_value(
            request(
                &mut backend,
                GotoDefinition::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, "later", reference)
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let Some(GotoDefinitionResponse::Scalar(location)) = definition else {
            panic!("expected shadow-aware definition")
        };
        assert_eq!(
            location.range.start,
            text_position(source, "later", declaration)
        );
    }
}

#[test]
fn navigation_reacquires_symbols_after_each_source_revision() {
    let mut backend = Backend::new();
    open(&mut backend, "let old = 1 old");
    backend
        .change(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "let fresh = 1 fresh".to_owned(),
            }],
        })
        .unwrap();
    let definition: Option<GotoDefinitionResponse> = serde_json::from_value(
        request(
            &mut backend,
            GotoDefinition::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": Position::new(0, 14)
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let Some(GotoDefinitionResponse::Scalar(location)) = definition else {
        panic!("expected fresh definition")
    };
    assert_eq!(location.range.start, Position::new(0, 4));
    assert_eq!(location.range.end, Position::new(0, 9));
}

#[test]
fn rename_preparation_edits_and_rejections_follow_analysis_rules() {
    let source = "let first = 1 let second = first type(first) host_value";
    let mut backend = Backend::new();
    open(&mut backend, source);
    let first_use = text_position(source, "first", 1);

    let prepared: Option<PrepareRenameResponse> = serde_json::from_value(
        request(
            &mut backend,
            PrepareRenameRequest::METHOD,
            json!({ "textDocument": { "uri": uri() }, "position": first_use }),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        matches!(prepared, Some(PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. }) if placeholder == "first")
    );

    let edit: Option<WorkspaceEdit> = serde_json::from_value(
        request(
            &mut backend,
            Rename::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": first_use,
                "newName": "renamed"
            }),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(edit.unwrap().changes.unwrap()[&uri()].len(), 3);

    for invalid in ["let", "café"] {
        assert!(
            request(
                &mut backend,
                Rename::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": first_use,
                    "newName": invalid
                }),
            )
            .is_err()
        );
    }
    assert!(
        request(
            &mut backend,
            Rename::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": first_use,
                "newName": "second"
            }),
        )
        .is_err()
    );

    let builtin = text_position(source, "type", 0);
    let prepared: Option<PrepareRenameResponse> = serde_json::from_value(
        request(
            &mut backend,
            PrepareRenameRequest::METHOD,
            json!({ "textDocument": { "uri": uri() }, "position": builtin }),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(prepared.is_none());
    assert!(
        request(
            &mut backend,
            Rename::METHOD,
            json!({ "textDocument": { "uri": uri() }, "position": builtin, "newName": "kind" }),
        )
        .is_err()
    );

    let unresolved = text_position(source, "host_value", 0);
    assert!(
        request(
            &mut backend,
            Rename::METHOD,
            json!({ "textDocument": { "uri": uri() }, "position": unresolved, "newName": "host" }),
        )
        .is_err()
    );
}

#[test]
fn rename_rejects_capture_of_an_unresolved_host_name() {
    let source = "let target = 1 do missing target end";
    let mut backend = Backend::new();
    open(&mut backend, source);
    let target = text_position(source, "target", 0);
    assert!(
        request(
            &mut backend,
            Rename::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": target,
                "newName": "missing"
            }),
        )
        .is_err()
    );
}

#[test]
fn malformed_documents_keep_later_symbols_available() {
    let source = "let broken = )\nfn later() do nil end";
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(!diagnostics.diagnostics.is_empty());
    let symbols: Option<DocumentSymbolResponse> = serde_json::from_value(
        request(
            &mut backend,
            DocumentSymbolRequest::METHOD,
            json!({ "textDocument": { "uri": uri() } }),
        )
        .unwrap(),
    )
    .unwrap();
    let DocumentSymbolResponse::Nested(symbols) = symbols.unwrap() else {
        panic!("expected nested symbols")
    };
    assert!(symbols.iter().any(|symbol| symbol.name == "later"));
}

#[test]
fn memory_transport_performs_initialize_shutdown_and_exit_lifecycle() {
    let (server, client) = Connection::memory();
    let task = thread::spawn(move || run_connection(server));
    client
        .sender
        .send(Message::Request(Request::new(
            RequestId::from(1),
            "initialize".to_owned(),
            json!({ "capabilities": {} }),
        )))
        .unwrap();
    let Message::Response(response) = client.receiver.recv().unwrap() else {
        panic!("expected initialize response")
    };
    assert!(response.error.is_none());
    let result: InitializeResult = serde_json::from_value(response.result.unwrap()).unwrap();
    let server_info = result.server_info.unwrap();
    assert_eq!(server_info.name, "simi-lsp");
    assert_eq!(server_info.version.as_deref(), Some("0.1.0-alpha.1"));
    client
        .sender
        .send(Message::Notification(Notification::new(
            "initialized".to_owned(),
            json!({}),
        )))
        .unwrap();

    client
        .sender
        .send(Message::Request(Request::new(
            RequestId::from(2),
            Shutdown::METHOD.to_owned(),
            (),
        )))
        .unwrap();
    let Message::Response(response) = client.receiver.recv().unwrap() else {
        panic!("expected shutdown response")
    };
    assert!(response.error.is_none());
    client
        .sender
        .send(Message::Notification(Notification::new(
            Exit::METHOD.to_owned(),
            (),
        )))
        .unwrap();
    drop(client);
    task.join().unwrap().unwrap();
}

#[test]
fn list_spread_hover_reports_the_flattened_exact_shape() {
    let source = "let spread = [1, ..[2, 3], \"four\"]";
    let mut backend = Backend::new();
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "spread", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("spread hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "[integer, integer, integer, \"four\"]");
}

#[test]
fn module_members_show_source_signatures_and_plain_text_docs() {
    let module = r#"
--- Append one value.
fn append(xs, x) do nil end
{ append = append }
"#;
    let mut backend = Backend::with_module_sources([("std/list", module)]);
    let incomplete = "let emoji = \"😀\"\nlet list = require(\"std/list\") list.";
    open(&mut backend, incomplete);
    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(incomplete, incomplete.len()).unwrap(),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "append");
    assert_eq!(
        items[0].detail.as_deref(),
        Some("append : fn(xs: 'a, x: 'b) -> nil")
    );
    assert_eq!(
        items[0].documentation,
        Some(Documentation::String("Append one value.".to_owned()))
    );

    let complete = "let list = require(\"std/list\") list.append";
    let mut backend = Backend::with_module_sources([("std/list", module)]);
    open(&mut backend, complete);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(complete, "append", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.unwrap().contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "fn(xs: 'a, x: 'b) -> nil\n\nAppend one value.");
}

#[test]
fn module_documentation_appears_on_require_literals_and_module_bindings() {
    let module = r#"
---- Standard output operations.
---- Values are flushed automatically.

fn println(value) do nil end
{ println = println }
"#;
    let source = "let stdout = require(\"std/io\") stdout";
    for (needle, occurrence, expected) in [
        (
            "std/io",
            0,
            "{ println: fn(value: 'a) -> nil }\n\nStandard output operations.\nValues are flushed automatically.",
        ),
        (
            "stdout",
            1,
            "{ println: fn(value: 'a) -> nil }\n\nStandard output operations.\nValues are flushed automatically.",
        ),
    ] {
        let mut backend = Backend::with_module_sources([("std/io", module)]);
        open(&mut backend, source);
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, needle, occurrence),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.unwrap().contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn direct_module_fields_and_aliases_keep_signatures_and_docs() {
    let module = r#"
--- Print one value.
fn println(value) do nil end
--- Inspect text through a native alias.
let inspect: fn(string) -> string ! never = host.inspect
{ println = println, identity = fn(value) do value end, inspect = inspect }
"#;

    for (source, needle, occurrence, expected) in [
        (
            "require(\"std/io\").println",
            "println",
            0,
            "fn(value: 'a) -> nil\n\nPrint one value.",
        ),
        (
            "let print = require(\"std/io\").println print",
            "print",
            2,
            "fn(value: 'a) -> nil\n\nPrint one value.",
        ),
        (
            "require(\"std/io\").identity",
            "identity",
            0,
            "fn(value: 'a) -> 'a",
        ),
        (
            "require(\"std/io\").inspect",
            "inspect",
            0,
            "fn(string) -> string ! never\n\nInspect text through a native alias.",
        ),
    ] {
        let mut backend = Backend::with_module_sources([("std/io", module)]);
        open(&mut backend, source);
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, needle, occurrence),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.unwrap().contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }

    let completion_source = "let print = require(\"std/io\").println pri";
    let mut backend = Backend::with_module_sources([("std/io", module)]);
    open(&mut backend, completion_source);
    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(completion_source, completion_source.len()).unwrap(),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    let print = items
        .iter()
        .find(|item| item.label == "print")
        .expect("print completion");
    assert_eq!(print.kind, Some(CompletionItemKind::FUNCTION));
    assert_eq!(
        print.detail.as_deref(),
        Some("print : fn(value: 'a) -> nil")
    );
    assert_eq!(
        print.documentation,
        Some(Documentation::String("Print one value.".to_owned()))
    );

    let typed_source = concat!(
        "let inspect = require(\"std/io\").inspect\n",
        "let callback: fn(integer) -> integer = fn(value) do value end\n",
    );
    let mut backend = Backend::with_module_sources([("std/io", module)]);
    open(&mut backend, typed_source);
    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(typed_source, typed_source.len()).unwrap(),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    for name in ["inspect", "callback"] {
        let item = items
            .iter()
            .find(|item| item.label == name)
            .expect("typed function completion");
        assert_eq!(item.kind, Some(CompletionItemKind::FUNCTION));
    }

    let member_source = "let io = require(\"std/io\") io.";
    let mut backend = Backend::with_module_sources([("std/io", module)]);
    open(&mut backend, member_source);
    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(member_source, member_source.len()).unwrap(),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    let inspect = items
        .iter()
        .find(|item| item.label == "inspect")
        .expect("inspect completion");
    assert_eq!(inspect.kind, Some(CompletionItemKind::FUNCTION));
    assert_eq!(
        inspect.detail.as_deref(),
        Some("inspect : fn(string) -> string ! never")
    );
    assert_eq!(
        inspect.documentation,
        Some(Documentation::String(
            "Inspect text through a native alias.".to_owned()
        ))
    );
}

#[test]
fn nested_module_hover_and_utf16_member_completion_use_catalog_without_diagnostics() {
    let module = r#"
--- Run a nested operation.
fn run(value) do value end
{ nested = { run = run } }
"#;
    let complete = "let emoji = \"😀\"\nlet module = require(\"nested\")\nmodule.nested.run";
    let mut backend = Backend::with_module_sources([("nested", module)]);
    let published = open(&mut backend, complete);
    let diagnostics = diagnostics_from(published.into_iter().next().unwrap());
    assert!(diagnostics.diagnostics.is_empty());
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(complete, "run", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.unwrap().contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "fn(value: 'a) -> 'a\n\nRun a nested operation.");

    let incomplete = "let emoji = \"😀\"\nlet module = require(\"nested\")\nmodule.nested.";
    let mut backend = Backend::with_module_sources([("nested", module)]);
    open(&mut backend, incomplete);
    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(incomplete, incomplete.len()).unwrap(),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].detail.as_deref(),
        Some("run : fn(value: 'a) -> 'a")
    );
}

#[test]
fn real_annotated_stdlib_facade_supplies_generic_member_types() {
    let module = include_str!("../../../../stdlib/iter.simi");
    let source = "let iter = require(\"std/iter\") iter.map";
    let mut backend = Backend::with_module_sources([("std/iter", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "map", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("stdlib hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(
        &markup,
        "fn<'a, 'b, 'c, 'd>(\n    iterator: fn() -> { done: true, .. } | { done: false, value: 'a, .. } ! 'c,\n    transform: fn('a) -> 'b ! 'd,\n) -> fn() -> { done: true, .. } | { done: false, value: 'b, .. } ! 'c | 'd ! never",
    );
}

#[test]
fn iterator_loop_hover_exposes_control_contract_and_documentation() {
    let module = include_str!("../../../../stdlib/iter.simi");
    let source = "let iter = require(\"std/iter\") iter.loop";
    let mut backend = Backend::with_module_sources([("std/iter", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "loop", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("stdlib hover").contents else {
        panic!("expected markup")
    };
    assert!(markup.value.contains("body: fn() ->"), "{}", markup.value);
    assert!(
        markup.value.contains("control: \"continue\"")
            && markup.value.contains("control: \"break\""),
        "{}",
        markup.value
    );
    assert!(
        markup
            .value
            .contains("Repeatedly run body until it returns an explicit break control."),
        "{}",
        markup.value
    );
}

#[test]
fn iterator_pair_adapter_hover_preserves_item_and_source_effect_types() {
    let module = include_str!("../../../../stdlib/iter.simi");
    let source = "let iter = require(\"std/iter\") iter.enumerate";
    let mut backend = Backend::with_module_sources([("std/iter", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "enumerate", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("stdlib hover").contents else {
        panic!("expected markup")
    };
    assert!(
        markup.value.contains("value: [integer, 'a]"),
        "{}",
        markup.value
    );
    assert!(markup.value.contains("! 'b ! never"), "{}", markup.value);
    assert!(
        markup
            .value
            .contains("Pair every value with its zero-based integer index."),
        "{}",
        markup.value
    );
}

#[test]
fn portable_prelude_members_have_the_same_lsp_metadata_as_require() {
    let source = "number.to_string";
    let mut backend = Backend::with_module_sources([(
        "std/number",
        include_str!("../../../../stdlib/number.simi"),
    )]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "to_string", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("portable prelude hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(
        &markup,
        "fn(value: integer | float) -> string ! never\n\nRender a number using canonical Simi notation.",
    );
}

#[test]
fn real_iterator_facades_publish_no_diagnostics() {
    for source in [
        include_str!("../../../../stdlib/list.simi"),
        include_str!("../../../../stdlib/map.simi"),
        include_str!("../../../../stdlib/iter.simi"),
    ] {
        let mut backend = Backend::new();
        let published = diagnostics_from(open(&mut backend, source).remove(0));
        assert!(
            published.diagnostics.is_empty(),
            "{:?}",
            published.diagnostics
        );
    }
}

#[test]
fn bytes_annotations_and_type_narrowing_hover_as_the_primitive_type() {
    let source = r#"fn first(value: bytes) do
    value[0]
end
fn classify(value: bytes | string) do
    if type(value) == "bytes" then value[0] else value end
end
"#;
    let mut backend = Backend::new();
    let published = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        published.diagnostics.is_empty(),
        "{:?}",
        published.diagnostics
    );

    for occurrence in [0, 1, 4] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, "value", occurrence),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("bytes hover").contents else {
            panic!("expected markup");
        };
        assert_simi_hover(&markup, "bytes");
    }
}

#[test]
fn bytes_literals_hover_as_bytes_and_reject_dynamic_text_segments() {
    let source = "let data = #[0, \"PNG\", 255]";
    let mut backend = Backend::new();
    let published = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        published.diagnostics.is_empty(),
        "{:?}",
        published.diagnostics
    );

    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "data", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("bytes literal hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "bytes");

    let source = "let text = \"PNG\" let data = #[text]";
    let published = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        published
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Type mismatch")),
        "{:?}",
        published.diagnostics
    );
}

#[test]
fn bytes_pattern_captures_hover_as_integer_and_bytes() {
    let source = r#"let result = case #[1, 2, 3] of
    #[byte, fixed:bytes(1), rest:bytes] => [byte, fixed, rest]
end"#;
    let mut backend = Backend::new();
    let published = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        published.diagnostics.is_empty(),
        "{:?}",
        published.diagnostics
    );

    for (name, expected) in [("byte", "integer"), ("fixed", "bytes"), ("rest", "bytes")] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, 0),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("bytes pattern capture hover").contents
        else {
            panic!("expected markup");
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn primitive_singleton_annotations_hover_without_narrowing_expression_inference() {
    let source = r#"let count = 42
let exact_integer: 42 = 42
let exact_float: 1.0 = 1.0
let normalized_zero: 0.0 = -0.0
let exact_text: "ready" = "ready"
let exact_flag: false = false
let mismatch: 42 = 43
"#;
    let mut backend = Backend::new();
    let published = diagnostics_from(open(&mut backend, source).remove(0));
    assert_eq!(
        published.diagnostics.len(),
        1,
        "{:?}",
        published.diagnostics
    );
    assert!(published.diagnostics[0].message.contains("Type mismatch"));

    for (name, expected) in [
        ("count", "integer"),
        ("exact_integer", "42"),
        ("exact_float", "1.0"),
        ("normalized_zero", "0.0"),
        ("exact_text", "\"ready\""),
        ("exact_flag", "false"),
    ] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, 0),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("singleton hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn contextual_singleton_function_bodies_and_mutations_publish_exact_hovers() {
    let source = r#"fn direct_true() -> true true
fn direct_int() -> 42 42
let anon = fn() -> false false
let annotated: fn() -> true ! never = fn() true
let tagged: {done: true} = {done = true}
tagged.done = true
let indexed: {done: true} = {done = true}
indexed["done"] = true
let initial_code: 41 = 41
let field_union: {code: 41 | 42} = {code = initial_code}
field_union.code = 42
field_union.code = 41
let index_union: {code: 41 | 42} = {code = initial_code}
index_union["code"] = 42
index_union["code"] = 41
"#;
    let mut backend = Backend::new();
    let published = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        published.diagnostics.is_empty(),
        "{:?}",
        published.diagnostics
    );

    for (name, expected) in [
        ("direct_true", "fn() -> true"),
        ("direct_int", "fn() -> 42"),
        ("anon", "fn() -> false"),
        ("annotated", "fn() -> true ! never"),
        ("tagged", "{ done: true }"),
        ("indexed", "{ done: true }"),
        ("field_union", "{ code: 41 | 42 }"),
        ("index_union", "{ code: 41 | 42 }"),
    ] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, 0),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("contextual singleton hover").contents
        else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn broad_mutation_values_publish_singleton_mismatch_diagnostics() {
    let source = r#"let field_flag: {done: true} = {done = true}
let index_flag: {done: true} = {done = true}
let broad_flag = false and true
field_flag.done = broad_flag
index_flag["done"] = broad_flag
"#;
    let mut backend = Backend::new();
    let published = diagnostics_from(open(&mut backend, source).remove(0));
    assert_eq!(
        published.diagnostics.len(),
        2,
        "{:?}",
        published.diagnostics
    );
    assert!(published.diagnostics.iter().all(|diagnostic| {
        diagnostic.code == Some(NumberOrString::String("type_mismatch".to_owned()))
            && diagnostic.message.contains("Type mismatch")
    }));
}

#[test]
fn iterator_step_hover_and_boolean_narrowing_are_sound() {
    let source = r#"let iter = require("std/iter")
let step = iter.next(map.iter({}))
if step.done then
    let exhausted_entry = step.value
else
    let live_entry = step.value
end
"#;
    let mut backend = Backend::with_module_sources([
        ("std/iter", include_str!("../../../../stdlib/iter.simi")),
        ("std/map", include_str!("../../../../stdlib/map.simi")),
    ]);
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        diagnostics.diagnostics.is_empty(),
        "{:?}",
        diagnostics.diagnostics
    );

    for (name, expected) in [
        (
            "next",
            "fn<'a, 'b>(\n    iterator: fn() -> { done: true, .. } | { done: false, value: 'a, .. } ! 'b,\n) -> { done: true, .. } | { done: false, value: 'a, .. } ! 'b",
        ),
        ("exhausted_entry", "any"),
        (
            "live_entry",
            "{ key: boolean | integer | float | string, value: any, .. }",
        ),
    ] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, 0),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("iterator step hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn real_iterator_pipeline_contextualizes_unannotated_fold_callback() {
    let source = r#"let iter = require("std/iter")
let number = require("std/number")
let io = require("std/io")
let total =
    [1, 2, 3]
    |> list.iter()
    |> iter.fold(0, fn(acc, item) do acc + item end)
let rendered = total |> number.to_string()
let mapped =
    [1, 2, 3]
    |> list.iter()
    |> iter.map(fn(mapped_item) do mapped_item + 1 end)
    |> iter.to_list()
let keys =
    map.iter({first = 1})
    |> iter.map(fn(entry) do entry.key end)
    |> iter.to_list()
[1, 2, 3, 4, 5]
|> list.iter()
|> iter.fold(0, fn(acc, n) do acc + n end)
|> number.to_string()
|> io.println()
"#;
    let mut backend = Backend::with_module_sources([
        ("std/list", include_str!("../../../../stdlib/list.simi")),
        ("std/iter", include_str!("../../../../stdlib/iter.simi")),
        ("std/map", include_str!("../../../../stdlib/map.simi")),
        ("std/number", include_str!("../../../../stdlib/number.simi")),
        ("std/io", include_str!("../../../../stdlib/io.simi")),
    ]);
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        diagnostics.diagnostics.is_empty(),
        "{:?}",
        diagnostics.diagnostics
    );

    for (name, occurrence, expected) in [
        ("total", 0, "integer"),
        ("acc", 0, "integer"),
        ("item", 0, "integer"),
        ("rendered", 0, "string"),
        ("mapped", 0, "[..integer]"),
        ("mapped_item", 0, "integer"),
        (
            "entry",
            0,
            "{ key: boolean | integer | float | string, value: any, .. }",
        ),
        ("keys", 0, "[..(boolean | integer | float | string)]"),
    ] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, occurrence),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("iterator hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn generic_callback_without_element_evidence_has_exact_empty_list_hovers() {
    let source = r#"fn bridge<'state>(initial: 'state, callback: fn('state) -> 'state) -> 'state do
    callback(initial)
end
let inferred = bridge([], fn(xs) do xs end)
let unchanged: [] = bridge([], fn(other) do other end)
"#;
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        diagnostics.diagnostics.is_empty(),
        "{:?}",
        diagnostics.diagnostics
    );

    for name in ["inferred", "unchanged", "xs", "other"] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, 0),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("exact empty list hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, "[]");
    }
}

#[test]
fn contextual_empty_map_fold_while_has_precise_protocol_hovers() {
    let source = r#"let iter = require("std/iter")

fn two_sum(values: [..integer], target: integer)
    values
    |> list.iter()
    |> iter.enumerate()
    |> iter.fold_while({}) <| fn(seen, item) do
        let index = item[0]
        let value = item[1]
        let match_index = seen[target - value]
        if match_index == nil then
            seen[value] = index
            iter.continue(seen)
        else
            iter.break([match_index, index])
        end
    end
"#;
    let mut backend = Backend::with_module_sources([
        ("std/list", include_str!("../../../../stdlib/list.simi")),
        ("std/iter", include_str!("../../../../stdlib/iter.simi")),
    ]);
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        diagnostics.diagnostics.is_empty(),
        "{:?}",
        diagnostics.diagnostics
    );

    for (name, expected) in [
        ("seen", "{ [integer]: integer }"),
        (
            "two_sum",
            "fn(\n    values: [..integer],\n    target: integer,\n) -> [integer, integer] | { [integer]: integer }",
        ),
    ] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, 0),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("contextual empty map hover").contents
        else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn contextual_empty_map_capture_and_nil_delete_hovers_remain_exact() {
    let source = r#"fn bridge<'state>(initial: 'state, callback: fn('state) -> 'state) -> 'state do
    callback(initial)
end
let captured = bridge({}, fn(state) do
    let mutate = fn(key) do state[key] = 1 end
    state
end)
let deleted = bridge({}, fn(state) do
    let key = "missing"
    state[key] = nil
    state
end)
let unchanged = bridge({}, fn(state) do state end)
"#;
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert_eq!(diagnostics.diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics.diagnostics[0]
            .message
            .starts_with("Captured mutation exceeds declared type"),
        "{diagnostics:?}"
    );

    for name in ["captured", "deleted", "unchanged"] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, 0),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("exact empty map hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, "{}");
    }
}

#[test]
fn fold_accumulator_nested_empty_lists_have_precise_protocol_hovers() {
    let source = r#"let iter = require("std/iter")
alias number = integer | float

fn partition(ns: [..number], pivot: number)
    ns
    |> list.iter()
    |> iter.fold({lower=[], higher=[]}) <| fn(acc, n) do
        case acc of
            {lower, higher} when n < pivot =>
                {lower=lower |> tap list.append(n), higher}
            {lower, higher} =>
                {lower, higher=higher |> tap list.append(n)}
        end
    end
"#;
    let mut backend = Backend::with_module_sources([
        ("std/list", include_str!("../../../../stdlib/list.simi")),
        ("std/iter", include_str!("../../../../stdlib/iter.simi")),
    ]);
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        diagnostics.diagnostics.is_empty(),
        "{:?}",
        diagnostics.diagnostics
    );

    for (name, occurrence, expected) in [
        (
            "partition",
            0,
            "fn(\n    ns: [..(integer | float)],\n    pivot: integer | float,\n) -> { lower: [..(integer | float)], higher: [..(integer | float)] }",
        ),
        (
            "acc",
            0,
            "{ lower: [..(integer | float)], higher: [..(integer | float)] }",
        ),
        ("lower", 1, "[..(integer | float)]"),
        ("higher", 1, "[..(integer | float)]"),
    ] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, occurrence),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("fold accumulator hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn cycle_shadow_and_mutation_hovers_preserve_precise_types() {
    let module = include_str!("../../../../stdlib/list.simi");
    let source = r#"let list = require("std/list")
let nums = [1, 2, 3]
let nums = nums |> tap list.append(nums)
nums[3]"#;
    let mut backend = Backend::with_module_sources([("std/list", module)]);
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(diagnostics.diagnostics.is_empty());

    let expected_nums = [
        "[integer, integer, integer]",
        "[integer, integer, integer, [integer, integer, integer]]",
        "[integer, integer, integer]",
        "[integer, integer, integer]",
        "[integer, integer, integer, [integer, integer, integer]]",
    ];
    for (occurrence, expected) in expected_nums.into_iter().enumerate() {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, "nums", occurrence),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("nums hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }

    let append: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "append", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = append.expect("append hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(
        &markup,
        "fn(xs: [..'a], value: 'b) -> nil ! never\n\nAppend a value to a list.",
    );
}

#[test]
fn empty_record_hover_uses_compact_delimiters() {
    let source = "let data = {}\ndata";
    let mut backend = Backend::default();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(diagnostics.diagnostics.is_empty());

    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "data", 1),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("data hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "{}");
}

#[test]
fn literal_require_call_hover_uses_the_evaluated_module_type() {
    let module = "let exports = { answer = 42, empty = {} } exports";
    let source = "let data = require(\"known\")\ndata";
    let mut backend = Backend::with_module_sources([("known", module)]);
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(diagnostics.diagnostics.is_empty());

    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, ")", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("require call hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "{ answer: integer, empty: {} }");
}

#[test]
fn hover_reports_branch_narrowed_symbol_types() {
    let source = r#"fn classify(value: integer | string) do
    if type(value) == "integer" then value else value end
end"#;
    let mut backend = Backend::default();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(diagnostics.diagnostics.is_empty());

    let expected = ["integer | string", "integer | string", "integer", "string"];
    for (occurrence, expected) in expected.into_iter().enumerate() {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, "value", occurrence),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("value hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn mutable_list_hovers_are_flow_position_sensitive() {
    let module = include_str!("../../../../stdlib/list.simi");
    let source = r#"let list = require("std/list")
let ns = [1, 2]
ns
list.append(ns, 3)
ns"#;
    let mut backend = Backend::with_module_sources([("std/list", module)]);
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(diagnostics.diagnostics.is_empty());

    let expected = [
        "[integer, integer]",
        "[integer, integer]",
        "[integer, integer]",
        "[integer, integer, integer]",
    ];
    for (occurrence, expected) in expected.into_iter().enumerate() {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, "ns", occurrence),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("ns hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn explicit_any_hover_does_not_fall_back_to_a_later_assignment() {
    let source = r#"let value: any = 1
value
value = "later"
value"#;
    let mut backend = Backend::default();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(diagnostics.diagnostics.is_empty());

    for (occurrence, expected) in [(1, "any"), (2, "any"), (3, "\"later\"")] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, "value", occurrence),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("value hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn typed_hover_uses_type_only_simi_code_blocks() {
    let source = r#"
fn process(n) do n + 1 end
fn increment(n: integer) do n + 1 end
fn identity(value) do value end
let selected = identity("text")
let values = [1, "two"]
let indexed: { [string]: integer } = { answer = 42 }
let key = "answer"
let found = indexed[key]
"#;
    let mut backend = Backend::new();
    open(&mut backend, source);
    for (name, expected) in [
        ("process", "fn(n: integer | float) -> integer | float"),
        ("increment", "fn(n: integer) -> integer"),
        ("identity", "fn(value: 'a) -> 'a"),
        ("selected", "\"text\""),
        ("values", "[integer, \"two\"]"),
        ("found", "integer | nil"),
    ] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, 0),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("typed hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn raised_contract_diagnostics_and_hover_use_protocol_types() {
    let source = "let prefix = \"😀\"\nfn bad() -> integer ! never do raise \"boom\" end\n";
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    let contract = diagnostics
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code == Some(NumberOrString::String("type_mismatch".to_owned()))
        })
        .expect("! never contract diagnostic");
    assert_eq!(contract.range.start.line, 1);
    assert_eq!(contract.range.start.character, 0);
    assert!(contract.message.contains("never"));

    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "bad", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("bad hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "fn() -> integer ! never");
}

#[test]
fn callable_or_nil_hover_roundtrips_through_annotation() {
    let source = r#"
fn nullable(flag: boolean) do
    if flag then fn(value: integer) do value end end
end
"#;
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        diagnostics.diagnostics.is_empty(),
        "{:?}",
        diagnostics.diagnostics,
    );
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "nullable", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("hover").contents else {
        panic!("expected markup")
    };
    let displayed = assert_simi_hover_raw(&markup);
    assert_eq!(
        displayed,
        "fn(flag: boolean) -> (fn(value: integer) -> integer) | nil"
    );
    // Round-trip: reusing the hover text as an annotation must parse.
    let parse = simi_syntax::parse_source(&format!("let _: {displayed} = nil"));
    assert!(
        parse.diagnostics().is_empty(),
        "round-trip parse failed for '{displayed}': {:?}",
        parse.diagnostics()
    );
}

#[test]
fn varied_direct_bang_never_bodies_are_clean_and_have_exact_hover_types() {
    let source = concat!(
        "fn identity(value: integer) -> integer ! never value\n",
        "fn text() -> string ! never \"ok\"\n",
        "fn values() -> [..integer] ! never [1, 2]\n",
        "fn nothing() -> nil ! never nil\n",
        "fn grouped() -> integer ! never (1 + 2)\n",
        "fn append(xs: [..integer]) -> nil ! never host.append(xs)\n",
    );
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(
        diagnostics.diagnostics.is_empty(),
        "{:?}",
        diagnostics.diagnostics
    );

    for (name, expected) in [
        ("identity", "fn(value: integer) -> integer ! never"),
        ("text", "fn() -> string ! never"),
        ("values", "fn() -> [..integer] ! never"),
        ("nothing", "fn() -> nil ! never"),
        ("grouped", "fn() -> integer ! never"),
        ("append", "fn(xs: [..integer]) -> nil ! never"),
    ] {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, name, 0),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("function hover").contents else {
            panic!("expected markup")
        };
        assert_simi_hover(&markup, expected);
    }
}

#[test]
fn type_errors_are_published_and_clear_after_incremental_repair() {
    let source = concat!(
        "let declared: integer = \"wrong\"\n",
        "let bad_operator = \"x\" + 1\n",
        "let not_callable = 1(2)\n",
        "fn one(value: integer) -> integer do value end\n",
        "one()\n",
    );
    let mut backend = Backend::new();
    let notifications = open(&mut backend, source);
    let diagnostics = diagnostics_from(notifications.into_iter().next().unwrap());
    let codes = diagnostics
        .diagnostics
        .iter()
        .filter_map(|diagnostic| match diagnostic.code.as_ref()? {
            lsp_types::NumberOrString::String(code) => Some(code.as_str()),
            lsp_types::NumberOrString::Number(_) => None,
        })
        .collect::<Vec<_>>();
    assert!(codes.contains(&"type_mismatch"));
    assert!(codes.contains(&"invalid_operator"));
    assert!(codes.contains(&"not_callable"));
    assert!(codes.contains(&"wrong_arity"));
    assert!(
        diagnostics
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR))
    );

    let repaired =
        "let declared: integer = 1\nfn one(value: integer) -> integer do value end\none(1)\n";
    let notifications = backend
        .change(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: repaired.to_owned(),
            }],
        })
        .unwrap();
    assert!(
        diagnostics_from(notifications.into_iter().next().unwrap())
            .diagnostics
            .is_empty()
    );
}

#[test]
fn rename_expands_map_local_binding_shorthand_without_renaming_its_key() {
    let source = "let first = 1 let map = {first, label = first}";
    let mut backend = Backend::new();
    open(&mut backend, source);
    let edit: Option<WorkspaceEdit> = serde_json::from_value(
        request(
            &mut backend,
            Rename::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "first", 0),
                "newName": "renamed"
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let mut edits = edit.unwrap().changes.unwrap()[&uri()].clone();
    edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
    assert_eq!(edits.len(), 3);
    assert_eq!(edits[0].new_text, "renamed");
    assert_eq!(edits[1].new_text, "first = renamed");
    assert_eq!(edits[2].new_text, "renamed");
}

#[test]
fn map_destructuring_hover_reports_optional_binding_type() {
    let source = r#"fn extract(values: {[string]: integer}) do
    let {value, ..} = values
    value
end"#;
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(diagnostics.diagnostics.is_empty(), "{diagnostics:?}");
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "value", 1),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("value hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "integer | nil");
}

#[test]
fn structural_map_pattern_shorthand_reports_absence_and_present_binding_types() {
    let source = r#"let case_absent = case {} of
{case_missing} =>
    "wrong"
_ =>
    0
end
let case_present = case {case_value = 1} of
{case_value} =>
    case_value
_ =>
    "wrong"
end
let catch_absent = do
    raise {}
catch of
    {catch_missing} =>
        "wrong"
    _ =>
        0
end
let catch_present = do
    raise {catch_value = 2}
catch of
    {catch_value} =>
        catch_value
end"#;
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert!(diagnostics.diagnostics.is_empty(), "{diagnostics:?}");

    let mut hover_at = |needle: &str, occurrence: usize| {
        let hover: Option<Hover> = serde_json::from_value(
            request(
                &mut backend,
                HoverRequest::METHOD,
                json!({
                    "textDocument": { "uri": uri() },
                    "position": text_position(source, needle, occurrence),
                }),
            )
            .unwrap(),
        )
        .unwrap();
        let HoverContents::Markup(markup) = hover.expect("pattern hover").contents else {
            panic!("expected markup")
        };
        markup
    };
    for (needle, occurrence) in [
        ("case_absent", 0),
        ("case_value", 1),
        ("catch_absent", 0),
        ("catch_value", 1),
    ] {
        assert_simi_hover(&hover_at(needle, occurrence), "integer");
    }
}

#[test]
fn rename_expands_map_pattern_binding_shorthand_without_renaming_its_key() {
    let source = "let record = {} let {name} = record name";
    let mut backend = Backend::new();
    open(&mut backend, source);
    let edit: Option<WorkspaceEdit> = serde_json::from_value(
        request(
            &mut backend,
            Rename::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "name", 0),
                "newName": "renamed"
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let mut edits = edit.unwrap().changes.unwrap()[&uri()].clone();
    edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
    assert_eq!(edits.len(), 2);
    assert_eq!(edits[0].new_text, "name = renamed");
    assert_eq!(edits[1].new_text, "renamed");
}

#[test]
fn real_bytes_module_hover_uses_its_typed_facade_documentation() {
    let module = include_str!("../../../../stdlib/bytes.simi");
    let source = "let bytes = require(\"std/bytes\")\nbytes.get";
    let mut backend = Backend::with_module_sources([("std/bytes", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "get", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("bytes get hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(
        &markup,
        "fn(data: bytes, index: integer) -> integer | nil ! never\n\nReturn the octet at an index, or nil when it is out of range.",
    );
}

#[test]
fn real_string_module_hover_wraps_export_map_at_presentation_width() {
    let module = include_str!("../../../../stdlib/string.simi");
    let source = "let string = require(\"std/string\")\nstring";
    let mut backend = Backend::with_module_sources([("std/string", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "std/string", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("string module hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(
        &markup,
        "{\n    to_number: fn(text: string) -> integer | float | nil ! never,\n    concat: fn(left: string, right: string) -> string ! never,\n    length: fn(text: string) -> integer ! never,\n    slice: fn(text: string, start: integer, stop: integer) -> string ! never,\n    contains: fn(text: string, needle: string) -> boolean ! never,\n    starts_with: fn(text: string, prefix: string) -> boolean ! never,\n    ends_with: fn(text: string, suffix: string) -> boolean ! never,\n    split: fn(text: string, separator: string) -> [..string] ! never,\n    trim: fn(text: string) -> string ! never,\n    lower: fn(text: string) -> string ! never,\n    upper: fn(text: string) -> string ! never,\n}\n\nUnicode-aware string inspection, transformation, and conversion.",
    );
}

#[test]
fn closed_map_destructuring_over_unknown_keys_publishes_extra_key_warnings() {
    let source = r#"fn indexed_closed(values: {[string]: integer}) do
    let {value} = values
    value
end
fn indexed_rest(values: {[string]: integer}) do
    let {value, ..} = values
    value
end
fn open_closed(values: {..}) do
    let {value} = values
    value
end
fn open_rest(values: {..}) do
    let {value, ..rest} = values
    [value, rest]
end"#;
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert_eq!(diagnostics.diagnostics.len(), 2, "{diagnostics:?}");
    assert!(diagnostics.diagnostics.iter().all(|diagnostic| {
        diagnostic.code
            == Some(lsp_types::NumberOrString::String(
                "destructuring_let_may_fail".to_owned(),
            ))
            && diagnostic.severity == Some(lsp_types::DiagnosticSeverity::WARNING)
            && diagnostic.message.contains("Use `case`")
    }));
}

#[test]
fn destructuring_let_certainty_diagnostics_publish_warnings_and_errors() {
    let source = concat!(
        "fn first(values: any) do\n",
        "    let [first, ..rest] = values\n",
        "    first\n",
        "end\n",
        "let [impossible, ..rest] = 42\n",
    );
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert_eq!(diagnostics.diagnostics.len(), 2, "{diagnostics:?}");
    assert_eq!(
        diagnostics.diagnostics[0].code,
        Some(lsp_types::NumberOrString::String(
            "destructuring_let_may_fail".to_owned()
        ))
    );
    assert_eq!(
        diagnostics.diagnostics[0].severity,
        Some(lsp_types::DiagnosticSeverity::WARNING)
    );
    assert!(diagnostics.diagnostics[0].message.contains("Use `case`"));
    assert_eq!(
        diagnostics.diagnostics[1].code,
        Some(lsp_types::NumberOrString::String(
            "destructuring_let_never_matches".to_owned()
        ))
    );
    assert_eq!(
        diagnostics.diagnostics[1].severity,
        Some(lsp_types::DiagnosticSeverity::ERROR)
    );
    assert!(diagnostics.diagnostics[1].message.contains("incompatible"));

    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "first", 1),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("destructured binding hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "any");
}

#[test]
fn registered_portable_builtins_hover_with_module_shape_type() {
    let module = "fn append(xs, x) do nil end { append = append }";
    let source = "list";
    let mut backend = Backend::with_module_sources([("std/list", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "list", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("registered builtin hover").contents else {
        panic!("expected markup")
    };
    assert!(
        markup.value.contains("append"),
        "expected module shape type, got {}",
        markup.value
    );
}

#[test]
fn bare_portable_builtins_hover_as_any_when_not_registered() {
    let source = "list";
    let mut backend = Backend::default();
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "list", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("bare builtin hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "any");
}

#[test]
fn portable_builtin_member_completion_when_registered() {
    let module = "fn append(xs, x) do nil end { append = append }";
    let source = "list.";
    let mut backend = Backend::with_module_sources([("std/list", module)]);
    open(&mut backend, source);
    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(source, source.len()).unwrap(),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "append");
}

#[test]
fn shadowed_builtin_does_not_export_module_members() {
    let module = "fn append(xs, x) do nil end { append = append }";
    let source = "let list = 42\nlist.";
    let mut backend = Backend::with_module_sources([("std/list", module)]);
    open(&mut backend, source);
    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(source, source.len()).unwrap(),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    assert!(items.is_empty(), "shadowed builtin should have no members");
}

#[test]
fn require_alias_retains_precise_member_metadata_with_registered_shape() {
    let module = r#"
--- Append one value.
fn append(xs, x) do nil end
{ append = append }
"#;
    let source = "let list = require(\"std/list\") list.append";
    let mut backend = Backend::with_module_sources([("std/list", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "append", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("require alias hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(&markup, "fn(xs: 'a, x: 'b) -> nil\n\nAppend one value.");
}

#[test]
fn static_requirement_metadata_diagnostics_are_published_over_lsp() {
    for (source, code, detail, needle) in [
        (
            "requires {tools = {git = \"\", rev = \"v1\"}}",
            "invalid_package_requirements",
            "Requirement `tools` must declare either `git` and `rev`, or `path`.",
            "tools",
        ),
        (
            "requires {tools = {path = \"../tools\"}}",
            "invalid_package_requirements",
            "Development path must be a non-escaping, package-root-relative slash-separated path.",
            "path",
        ),
        (
            "let value = 1 requires {tools = {path = \"tools\"}}",
            "syntax_error",
            "`requires` must appear before executable items.",
            "requires",
        ),
    ] {
        let mut backend = Backend::new();
        let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
        assert_eq!(diagnostics.diagnostics.len(), 1, "{source}");
        let diagnostic = &diagnostics.diagnostics[0];
        assert_eq!(
            diagnostic.code,
            Some(NumberOrString::String(code.to_owned())),
            "{source}"
        );
        assert_eq!(diagnostic.source.as_deref(), Some("simi"), "{source}");
        assert!(diagnostic.message.ends_with(detail), "{diagnostic:?}");
        assert_eq!(diagnostic.range.start, text_position(source, needle, 0));
    }
}

// ---------------------------------------------------------------------------
// codec module hovers
// ---------------------------------------------------------------------------

#[test]
fn real_integer_module_hover_uses_typed_facade() {
    let module = include_str!("../../../../stdlib/integer.simi");
    let source = "let integer = require(\"std/integer\")\ninteger.encode";
    let mut backend = Backend::with_module_sources([("std/integer", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "encode", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("integer encode hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(
        &markup,
        "fn(value: integer, format: string) -> bytes ! never\n\nEncode an integer as bytes in an explicit endian format.\n\nAccepted formats: i8le, i8be, u8le, u8be, i16le, i16be, u16le, u16be,\ni32le, i32be, u32le, u32be, i64le, i64be, u64le, u64be.\n\nA value outside the selected range — including any negative value for an\nunsigned format — is a hard runtime diagnostic.",
    );
}

#[test]
fn real_float_module_hover_reports_union_wire_type() {
    let module = include_str!("../../../../stdlib/float.simi");
    let source = "let float = require(\"std/float\")\nfloat.decode";
    let mut backend = Backend::with_module_sources([("std/float", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "decode", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("float decode hover").contents else {
        panic!("expected markup")
    };
    let raw = assert_simi_hover_raw(&markup);
    assert!(
        raw.contains("float | \"inf\" | \"-inf\" | \"nan\""),
        "{raw}"
    );
    assert!(raw.contains("bytes, format: string"), "{raw}");
}

#[test]
fn real_utf8_module_hover_uses_typed_facade() {
    let module = include_str!("../../../../stdlib/utf8.simi");
    let source = "let utf8 = require(\"std/utf8\")\nutf8.encode";
    let mut backend = Backend::with_module_sources([("std/utf8", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "encode", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("utf8 encode hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(
        &markup,
        "fn(text: string) -> bytes ! never\n\nEncode a string into its UTF-8 byte representation.",
    );
}

#[test]
fn real_utf16_module_hover_uses_typed_facade() {
    let module = include_str!("../../../../stdlib/utf16.simi");
    let source = "let utf16 = require(\"std/utf16\")\nutf16.decode_le";
    let mut backend = Backend::with_module_sources([("std/utf16", module)]);
    open(&mut backend, source);
    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "decode_le", 0),
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.expect("utf16 decode_le hover").contents else {
        panic!("expected markup")
    };
    assert_simi_hover(
        &markup,
        "fn(data: bytes) -> string | nil ! never\n\nStrictly decode little-endian UTF-16 bytes into a string, or nil when malformed.",
    );
}
