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
        Some("append : (xs: 'a, x: 'b) -> nil")
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
    assert_simi_hover(&markup, "(xs: 'a, x: 'b) -> nil\n\nAppend one value.");
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
            "{ println: (value: 'a) -> nil }\n\nStandard output operations.\nValues are flushed automatically.",
        ),
        (
            "stdout",
            1,
            "{ println: (value: 'a) -> nil }\n\nStandard output operations.\nValues are flushed automatically.",
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
let inspect: string -> string noraise = host.inspect
{ println = println, identity = fn(value) do value end, inspect = inspect }
"#;

    for (source, needle, occurrence, expected) in [
        (
            "require(\"std/io\").println",
            "println",
            0,
            "(value: 'a) -> nil\n\nPrint one value.",
        ),
        (
            "let print = require(\"std/io\").println print",
            "print",
            2,
            "(value: 'a) -> nil\n\nPrint one value.",
        ),
        (
            "require(\"std/io\").identity",
            "identity",
            0,
            "(value: 'a) -> 'a",
        ),
        (
            "require(\"std/io\").inspect",
            "inspect",
            0,
            "string -> string noraise\n\nInspect text through a native alias.",
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
    assert_eq!(print.detail.as_deref(), Some("print : (value: 'a) -> nil"));
    assert_eq!(
        print.documentation,
        Some(Documentation::String("Print one value.".to_owned()))
    );

    let typed_source = concat!(
        "let inspect = require(\"std/io\").inspect\n",
        "let callback: integer -> integer = fn(value) do value end\n",
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
        Some("inspect : string -> string noraise")
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
    assert_simi_hover(&markup, "(value: 'a) -> 'a\n\nRun a nested operation.");

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
    assert_eq!(items[0].detail.as_deref(), Some("run : (value: 'a) -> 'a"));
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
        "<'a, 'b, 'c, 'd> (\n    iterator: () -> { done: boolean, .. } raises 'c,\n    transform: 'a -> 'b raises 'd,\n) -> () -> { done: boolean, .. } raises 'c | 'd noraise",
    );
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
        "(xs: [..'a], value: 'b) -> nil noraise\n\nAppend a value to a list.",
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
        ("process", "(n: integer | float) -> integer | float"),
        ("increment", "(n: integer) -> integer"),
        ("identity", "(value: 'a) -> 'a"),
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
    let source = "let prefix = \"😀\"\nfn bad() -> integer noraise do raise \"boom\" end\n";
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    let contract = diagnostics
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code == Some(NumberOrString::String("type_mismatch".to_owned()))
        })
        .expect("noraise contract diagnostic");
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
    assert_simi_hover(&markup, "() -> integer noraise");
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
    let source = r#"let case_absent = case {}
of {case_missing} do "wrong"
of _ do 0
end
let case_present = case {case_value = 1}
of {case_value} do case_value
of _ do "wrong"
end
let catch_absent = try
    raise {}
catch {catch_missing} do
    "wrong"
catch _ do
    0
end
let catch_present = try
    raise {catch_value = 2}
catch {catch_value} do
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
        "{\n    to_number: (text: string) -> integer | float | nil noraise,\n    concat: (left: string, right: string) -> string noraise,\n    length: (text: string) -> integer noraise,\n    slice: (text: string, start: integer, stop: integer) -> string noraise,\n    contains: (text: string, needle: string) -> boolean noraise,\n    starts_with: (text: string, prefix: string) -> boolean noraise,\n    ends_with: (text: string, suffix: string) -> boolean noraise,\n    split: (text: string, separator: string) -> [..string] noraise,\n    trim: (text: string) -> string noraise,\n    lower: (text: string) -> string noraise,\n    upper: (text: string) -> string noraise,\n}\n\nUnicode-aware string inspection, transformation, and conversion.",
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
