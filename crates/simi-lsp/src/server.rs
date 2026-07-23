use std::collections::{BTreeSet, HashMap};
use std::error::Error;

use lsp_server::{Connection, ErrorCode, Message, Notification, Response, ResponseError};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Exit, Notification as _,
    PublishDiagnostics, ShowMessage,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, GotoDefinition, HoverRequest, PrepareRenameRequest,
    References, Rename, Request as _, Shutdown,
};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverContents, HoverParams, InitializeParams, InitializeResult,
    Location, MarkupContent, MarkupKind, MessageType, OneOf, Position, PrepareRenameResponse,
    PublishDiagnosticsParams, ReferenceParams, RenameOptions, RenameParams, ServerCapabilities,
    ServerInfo, ShowMessageParams, SymbolKind as LspSymbolKind, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextEdit, Url, WorkspaceEdit,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use simi_analysis::{
    AnalysisDatabase, FileId, RenameError, Resolution, Span, SymbolKind, diagnostics,
    document_symbols, references, resolve, source_text,
};

use crate::position;

#[derive(Clone)]
struct Document {
    file: FileId,
    version: i32,
}

type AnalysisAt<'a> = (
    &'a Document,
    std::sync::Arc<String>,
    std::sync::Arc<Resolution>,
    usize,
);

#[derive(Default)]
pub struct Backend {
    db: AnalysisDatabase,
    documents: HashMap<Url, Document>,
    shutdown_requested: bool,
}

#[derive(Debug)]
pub struct ProtocolError {
    pub code: i32,
    pub message: String,
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ProtocolError {}

impl ProtocolError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidParams as i32,
            message: message.into(),
        }
    }

    fn request_failed(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::RequestFailed as i32,
            message: message.into(),
        }
    }
}

impl Backend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn capabilities() -> ServerCapabilities {
        ServerCapabilities {
            position_encoding: Some(lsp_types::PositionEncodingKind::UTF16),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: None,
                    will_save_wait_until: None,
                    save: None,
                },
            )),
            document_symbol_provider: Some(OneOf::Left(true)),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: Default::default(),
            })),
            hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions::default()),
            ..ServerCapabilities::default()
        }
    }

    pub fn initialize_result() -> InitializeResult {
        InitializeResult {
            capabilities: Self::capabilities(),
            server_info: Some(ServerInfo {
                name: "simi-lsp".to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
        }
    }

    pub fn open(&mut self, params: DidOpenTextDocumentParams) -> Vec<Notification> {
        let item = params.text_document;
        let file = self.db.add_file(item.text);
        let uri = item.uri;
        self.documents.insert(
            uri.clone(),
            Document {
                file,
                version: item.version,
            },
        );
        vec![self.diagnostics_notification(&uri)]
    }

    pub fn change(
        &mut self,
        params: DidChangeTextDocumentParams,
    ) -> Result<Vec<Notification>, ProtocolError> {
        let uri = params.text_document.uri;
        let document = self
            .documents
            .get(&uri)
            .cloned()
            .ok_or_else(|| ProtocolError::invalid(format!("document is not open: {uri}")))?;
        if params.text_document.version <= document.version {
            return Ok(Vec::new());
        }
        let current = source_text(&self.db, document.file);
        let changed =
            position::apply_changes(&current, &params.content_changes).map_err(|error| {
                ProtocolError::invalid(format!("invalid document change: {error:?}"))
            })?;
        self.db.set_source(document.file, changed);
        self.documents.insert(
            uri.clone(),
            Document {
                file: document.file,
                version: params.text_document.version,
            },
        );
        Ok(vec![self.diagnostics_notification(&uri)])
    }

    pub fn close(&mut self, params: DidCloseTextDocumentParams) -> Vec<Notification> {
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        vec![Notification::new(
            PublishDiagnostics::METHOD.to_owned(),
            PublishDiagnosticsParams::new(uri, Vec::new(), None),
        )]
    }

    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, ProtocolError> {
        match method {
            DocumentSymbolRequest::METHOD => {
                let params: DocumentSymbolParams = decode(params)?;
                encode(self.document_symbols(params))
            }
            GotoDefinition::METHOD => {
                let params: GotoDefinitionParams = decode(params)?;
                encode(self.definition(params)?)
            }
            References::METHOD => {
                let params: ReferenceParams = decode(params)?;
                encode(self.find_references(params)?)
            }
            PrepareRenameRequest::METHOD => {
                let params: lsp_types::TextDocumentPositionParams = decode(params)?;
                encode(self.prepare_rename(params)?)
            }
            Rename::METHOD => {
                let params: RenameParams = decode(params)?;
                encode(self.rename(params)?)
            }
            HoverRequest::METHOD => {
                let params: HoverParams = decode(params)?;
                encode(self.hover(params)?)
            }
            Completion::METHOD => {
                let params: CompletionParams = decode(params)?;
                encode(self.completion(params)?)
            }
            Shutdown::METHOD => {
                self.shutdown_requested = true;
                Ok(Value::Null)
            }
            _ => Err(ProtocolError {
                code: ErrorCode::MethodNotFound as i32,
                message: format!("unsupported request: {method}"),
            }),
        }
    }

    pub fn notify(
        &mut self,
        method: &str,
        params: Value,
    ) -> Result<Vec<Notification>, ProtocolError> {
        match method {
            DidOpenTextDocument::METHOD => Ok(self.open(decode(params)?)),
            DidChangeTextDocument::METHOD => self.change(decode(params)?),
            DidCloseTextDocument::METHOD => Ok(self.close(decode(params)?)),
            Exit::METHOD => Ok(Vec::new()),
            _ => Ok(Vec::new()),
        }
    }

    fn diagnostics_notification(&self, uri: &Url) -> Notification {
        let Some(document) = self.documents.get(uri) else {
            return Notification::new(
                PublishDiagnostics::METHOD.to_owned(),
                PublishDiagnosticsParams::new(uri.clone(), Vec::new(), None),
            );
        };
        let text = source_text(&self.db, document.file);
        let items = diagnostics(&self.db, document.file)
            .iter()
            .filter_map(|diagnostic| {
                Some(Diagnostic {
                    range: position::range(&text, diagnostic.span).ok()?,
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("simi".to_owned()),
                    message: diagnostic.message.clone(),
                    ..Diagnostic::default()
                })
            })
            .collect();
        Notification::new(
            PublishDiagnostics::METHOD.to_owned(),
            PublishDiagnosticsParams::new(uri.clone(), items, Some(document.version)),
        )
    }

    #[allow(deprecated)]
    fn document_symbols(&self, params: DocumentSymbolParams) -> Option<DocumentSymbolResponse> {
        let (document, text) = self.document(&params.text_document.uri).ok()?;
        let symbols = document_symbols(&self.db, document.file)
            .iter()
            .filter_map(|symbol| {
                let range = position::range(&text, symbol.span).ok()?;
                Some(lsp_types::DocumentSymbol {
                    name: symbol.name.clone(),
                    detail: Some(symbol_kind_label(symbol.kind).to_owned()),
                    kind: lsp_symbol_kind(symbol.kind),
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                })
            })
            .collect();
        Some(DocumentSymbolResponse::Nested(symbols))
    }

    fn definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>, ProtocolError> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let (document, text, resolution, offset) = self.analysis_at(&uri, position)?;
        let Some(symbol) = resolution.symbol_at(offset) else {
            return Ok(None);
        };
        let Some(span) = resolution.definition_span(symbol) else {
            return Ok(None);
        };
        let location = self.location(uri, &text, span)?;
        let _ = document;
        Ok(Some(GotoDefinitionResponse::Scalar(location)))
    }

    fn find_references(&self, params: ReferenceParams) -> Result<Vec<Location>, ProtocolError> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let (document, text, resolution, offset) = self.analysis_at(&uri, position)?;
        let Some(symbol) = resolution.symbol_at(offset) else {
            return Ok(Vec::new());
        };
        let mut spans = references(&self.db, document.file, symbol).as_ref().clone();
        if params.context.include_declaration
            && let Some(declaration) = resolution.definition_span(symbol)
        {
            spans.push(declaration);
        }
        spans.sort_by_key(|span| (span.start, span.end));
        spans.dedup();
        spans
            .into_iter()
            .map(|span| self.location(uri.clone(), &text, span))
            .collect()
    }

    fn prepare_rename(
        &self,
        params: lsp_types::TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>, ProtocolError> {
        let uri = params.text_document.uri;
        let (_, text, resolution, offset) = self.analysis_at(&uri, params.position)?;
        let Some((symbol, span)) = resolution.symbol_span_at(offset) else {
            return Ok(None);
        };
        let data = &resolution.hir.symbols[symbol];
        if data.builtin {
            return Ok(None);
        }
        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: self.range(&text, span)?,
            placeholder: data.name.clone(),
        }))
    }

    fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>, ProtocolError> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let (_, text, resolution, offset) = self.analysis_at(&uri, position)?;
        let Some(symbol) = resolution.symbol_at(offset) else {
            return Err(ProtocolError::request_failed(
                "cannot rename an unresolved name",
            ));
        };
        resolution
            .check_rename(symbol, &params.new_name)
            .map_err(rename_error)?;
        let edits = resolution
            .rename_spans(symbol)
            .into_iter()
            .map(|span| {
                Ok(TextEdit {
                    range: self.range(&text, span)?,
                    new_text: params.new_name.clone(),
                })
            })
            .collect::<Result<Vec<_>, ProtocolError>>()?;
        let mut changes = HashMap::new();
        changes.insert(uri, edits);
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    fn hover(&self, params: HoverParams) -> Result<Option<Hover>, ProtocolError> {
        let uri = params.text_document_position_params.text_document.uri;
        let (_, text, resolution, offset) =
            self.analysis_at(&uri, params.text_document_position_params.position)?;
        let Some(facts) = resolution.hover(offset) else {
            return Ok(None);
        };
        let data = &resolution.hir.symbols[facts.symbol];
        let mut detail = format!("{} `{}`", symbol_kind_label(facts.kind), facts.name);
        if let Some(arity) = facts.arity {
            detail.push_str(&format!(" ({arity} parameters)"));
        }
        if let Some(declaration) = facts.declaration {
            let at = position::position(&text, declaration.start).map_err(|error| {
                ProtocolError::request_failed(format!("invalid symbol span: {error:?}"))
            })?;
            detail.push_str(&format!(
                "\ndeclared at {}:{}:{}",
                uri,
                at.line + 1,
                at.character + 1
            ));
        } else if data.builtin {
            detail.push_str("\nSimi prelude builtin");
        }
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::PlainText,
                value: detail,
            }),
            range: resolution
                .symbol_span_at(offset)
                .map(|(_, span)| self.range(&text, span))
                .transpose()?,
        }))
    }

    fn completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>, ProtocolError> {
        let uri = params.text_document_position.text_document.uri;
        let (_, _, resolution, offset) =
            self.analysis_at(&uri, params.text_document_position.position)?;
        let mut names = BTreeSet::new();
        let mut items = Vec::new();
        for symbol in resolution.visible_symbols(offset) {
            let data = &resolution.hir.symbols[symbol];
            if names.insert(data.name.clone()) {
                items.push(CompletionItem {
                    label: data.name.clone(),
                    kind: Some(completion_kind(data.kind)),
                    detail: Some(completion_detail(data.kind, data.arity)),
                    ..CompletionItem::default()
                });
            }
        }
        items.sort_by(|left, right| left.label.cmp(&right.label));
        Ok(Some(CompletionResponse::Array(items)))
    }

    fn document(&self, uri: &Url) -> Result<(&Document, std::sync::Arc<String>), ProtocolError> {
        let document = self
            .documents
            .get(uri)
            .ok_or_else(|| ProtocolError::invalid(format!("document is not open: {uri}")))?;
        Ok((document, source_text(&self.db, document.file)))
    }

    fn analysis_at(
        &self,
        uri: &Url,
        position_value: Position,
    ) -> Result<AnalysisAt<'_>, ProtocolError> {
        let (document, text) = self.document(uri)?;
        let offset = position::offset(&text, position_value)
            .map_err(|error| ProtocolError::invalid(format!("invalid position: {error:?}")))?;
        // Resolution is reacquired for every request so arena IDs never cross source revisions.
        let resolution = resolve(&self.db, document.file);
        Ok((document, text, resolution, offset))
    }

    fn location(&self, uri: Url, text: &str, span: Span) -> Result<Location, ProtocolError> {
        Ok(Location::new(uri, self.range(text, span)?))
    }

    fn range(&self, text: &str, span: Span) -> Result<lsp_types::Range, ProtocolError> {
        position::range(text, span).map_err(|error| {
            ProtocolError::request_failed(format!("invalid analysis span: {error:?}"))
        })
    }
}

pub fn run_connection(connection: Connection) -> Result<(), Box<dyn Error + Sync + Send>> {
    let (initialize_id, initialize_params) = connection.initialize_start()?;
    let _: InitializeParams = serde_json::from_value(initialize_params)?;
    connection.initialize_finish(
        initialize_id,
        serde_json::to_value(Backend::initialize_result())?,
    )?;

    let mut backend = Backend::new();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if backend.shutdown_requested {
                    connection.sender.send(Message::Response(Response::new_err(
                        request.id,
                        ErrorCode::InvalidRequest as i32,
                        "server has shut down".to_owned(),
                    )))?;
                    continue;
                }
                let response = match backend.request(&request.method, request.params) {
                    Ok(result) => Response::new_ok(request.id, result),
                    Err(error) => Response {
                        id: request.id,
                        result: None,
                        error: Some(ResponseError {
                            code: error.code,
                            message: error.message,
                            data: None,
                        }),
                    },
                };
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(notification) => {
                if notification.method == Exit::METHOD {
                    return if backend.shutdown_requested {
                        Ok(())
                    } else {
                        Err("received exit before shutdown".into())
                    };
                }
                match backend.notify(&notification.method, notification.params) {
                    Ok(notifications) => {
                        for outgoing in notifications {
                            connection.sender.send(Message::Notification(outgoing))?;
                        }
                    }
                    Err(error) => {
                        connection
                            .sender
                            .send(Message::Notification(Notification::new(
                                ShowMessage::METHOD.to_owned(),
                                ShowMessageParams {
                                    typ: MessageType::ERROR,
                                    message: error.message,
                                },
                            )))?;
                    }
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

fn rename_error(error: RenameError) -> ProtocolError {
    match error {
        RenameError::Builtin => ProtocolError::request_failed("cannot rename a prelude builtin"),
        RenameError::Unresolved => {
            ProtocolError::request_failed("cannot rename an unresolved name")
        }
        RenameError::InvalidName => {
            ProtocolError::invalid("new name is not a valid Simi identifier")
        }
        RenameError::Collision { name, at } => ProtocolError::request_failed(format!(
            "renaming would collide with `{name}` at byte {}",
            at.start
        )),
    }
}

fn symbol_kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Parameter => "parameter",
        SymbolKind::Let => "let binding",
        SymbolKind::Pattern => "pattern binding",
        SymbolKind::LoopState => "loop state",
        SymbolKind::Builtin => "builtin function",
    }
}

fn lsp_symbol_kind(kind: SymbolKind) -> LspSymbolKind {
    match kind {
        SymbolKind::Function | SymbolKind::Builtin => LspSymbolKind::FUNCTION,
        SymbolKind::Parameter | SymbolKind::Let | SymbolKind::Pattern | SymbolKind::LoopState => {
            LspSymbolKind::VARIABLE
        }
    }
}

fn completion_kind(kind: SymbolKind) -> CompletionItemKind {
    match kind {
        SymbolKind::Function | SymbolKind::Builtin => CompletionItemKind::FUNCTION,
        SymbolKind::Parameter => CompletionItemKind::VARIABLE,
        SymbolKind::Let | SymbolKind::Pattern | SymbolKind::LoopState => {
            CompletionItemKind::VARIABLE
        }
    }
}

fn completion_detail(kind: SymbolKind, arity: Option<usize>) -> String {
    match arity {
        Some(arity) => format!("{} ({arity} parameters)", symbol_kind_label(kind)),
        None => symbol_kind_label(kind).to_owned(),
    }
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, ProtocolError> {
    serde_json::from_value(value)
        .map_err(|error| ProtocolError::invalid(format!("invalid request parameters: {error}")))
}

fn encode<T: serde::Serialize>(value: T) -> Result<Value, ProtocolError> {
    serde_json::to_value(value)
        .map_err(|error| ProtocolError::request_failed(format!("cannot encode response: {error}")))
}

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;
