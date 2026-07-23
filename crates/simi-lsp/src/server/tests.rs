use std::thread;

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types::notification::{Exit, Notification as _, PublishDiagnostics};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, PrepareRenameRequest,
    References, Rename, Request as _, Shutdown,
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
fn symbols_navigation_references_hover_and_completion_use_fresh_analysis() {
    let source = concat!(
        "fn add(left, right) do left + right end\n",
        "let [first, second] = [1, 2]\n",
        "let result = add(first, second)\n",
        "type(result)"
    );
    let mut backend = Backend::new();
    open(&mut backend, source);

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
    let names = symbols
        .into_iter()
        .map(|symbol| symbol.name)
        .collect::<Vec<_>>();
    assert_eq!(names, ["add", "first", "second", "result"]);

    let add_use = text_position(source, "add", 1);
    let definition: Option<GotoDefinitionResponse> = serde_json::from_value(
        request(
            &mut backend,
            GotoDefinition::METHOD,
            json!({ "textDocument": { "uri": uri() }, "position": add_use }),
        )
        .unwrap(),
    )
    .unwrap();
    let Some(GotoDefinitionResponse::Scalar(location)) = definition else {
        panic!("expected scalar definition")
    };
    assert_eq!(location.range.start, Position::new(0, 3));

    let references_without: Vec<Location> = serde_json::from_value(
        request(
            &mut backend,
            References::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": add_use,
                "context": { "includeDeclaration": false }
            }),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(references_without.len(), 1);
    let references_with: Vec<Location> = serde_json::from_value(
        request(
            &mut backend,
            References::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": add_use,
                "context": { "includeDeclaration": true }
            }),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(references_with.len(), 2);

    let hover: Option<Hover> = serde_json::from_value(
        request(
            &mut backend,
            HoverRequest::METHOD,
            json!({ "textDocument": { "uri": uri() }, "position": add_use }),
        )
        .unwrap(),
    )
    .unwrap();
    let HoverContents::Markup(markup) = hover.unwrap().contents else {
        panic!("expected markup")
    };
    assert!(markup.value.contains("function `add` (2 parameters)"));
    assert!(
        markup
            .value
            .contains("declared at file:///workspace/test.simi:1:4")
    );

    let completion: Option<CompletionResponse> = serde_json::from_value(
        request(
            &mut backend,
            Completion::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": position::position(source, source.len()).unwrap()
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let CompletionResponse::Array(items) = completion.unwrap() else {
        panic!("expected completion array")
    };
    let labels = items.into_iter().map(|item| item.label).collect::<Vec<_>>();
    assert!(labels.contains(&"add".to_owned()));
    assert!(labels.contains(&"result".to_owned()));
    assert!(labels.contains(&"require".to_owned()));
    assert!(labels.contains(&"type".to_owned()));
    assert!(labels.contains(&"inspect".to_owned()));
    assert_eq!(
        labels
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        labels.len()
    );
}

#[test]
fn duplicate_future_bindings_are_diagnosed_and_navigation_keeps_first_runtime_binding() {
    let source = "let closure = fn() do later end let later = 1 let later = 2";
    let mut backend = Backend::new();
    let diagnostics = diagnostics_from(open(&mut backend, source).remove(0));
    assert_eq!(diagnostics.diagnostics.len(), 1);
    assert!(
        diagnostics.diagnostics[0]
            .message
            .contains("already defined in this scope")
    );

    let definition: Option<GotoDefinitionResponse> = serde_json::from_value(
        request(
            &mut backend,
            GotoDefinition::METHOD,
            json!({
                "textDocument": { "uri": uri() },
                "position": text_position(source, "later", 0)
            }),
        )
        .unwrap(),
    )
    .unwrap();
    let Some(GotoDefinitionResponse::Scalar(location)) = definition else {
        panic!("expected recovery definition")
    };
    assert_eq!(location.range.start, text_position(source, "later", 1));
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
    assert_eq!(result.server_info.unwrap().name, "simi-lsp");
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
