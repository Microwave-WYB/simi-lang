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
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbolParams,
    DocumentSymbolResponse, Documentation, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverContents, HoverParams, InitializeParams, InitializeResult, Location, MarkupContent,
    MarkupKind, MessageType, NumberOrString, OneOf, Position, PrepareRenameResponse,
    PublishDiagnosticsParams, ReferenceParams, RenameOptions, RenameParams, ServerCapabilities,
    ServerInfo, ShowMessageParams, SymbolKind as LspSymbolKind, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextEdit, Url, WorkspaceEdit,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use simi_analysis::{
    AnalysisDatabase, AnalysisDiagnosticSeverity, FileId, ModuleShape, ParameterPostType,
    RenameError, Resolution, Span, SymbolKind, Type, diagnostics, document_symbols,
    expression_type_at, field_type_at, imported_members, infer_types, member_at,
    member_completions, module_at, module_shape, references, resolve, source_text, symbol_type_at,
    wildcard_type_at,
};

use crate::position;

mod diagnostic;

mod navigation;

mod hover;

mod completion;

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
    module_shapes: HashMap<String, ModuleShape>,
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

    pub fn with_module_sources<I, N, S>(sources: I) -> Self
    where
        I: IntoIterator<Item = (N, S)>,
        N: Into<String>,
        S: Into<String>,
    {
        let mut backend = Self::default();
        for (name, source) in sources {
            let file = backend.db.add_file(source.into());
            backend
                .module_shapes
                .insert(name.into(), module_shape(&backend.db, file));
        }
        backend
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
    run_connection_with_backend(connection, Backend::new())
}

pub fn run_connection_with_backend(
    connection: Connection,
    mut backend: Backend,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let (initialize_id, initialize_params) = connection.initialize_start()?;
    let _: InitializeParams = serde_json::from_value(initialize_params)?;
    connection.initialize_finish(
        initialize_id,
        serde_json::to_value(Backend::initialize_result())?,
    )?;

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

fn lsp_symbol_kind(kind: SymbolKind) -> LspSymbolKind {
    match kind {
        SymbolKind::Function | SymbolKind::Builtin => LspSymbolKind::FUNCTION,
        SymbolKind::Parameter | SymbolKind::Let | SymbolKind::Pattern | SymbolKind::LoopState => {
            LspSymbolKind::VARIABLE
        }
    }
}

fn identifier_prefix(text: &str, offset: usize) -> &str {
    let mut start = offset;
    while start > 0
        && (text.as_bytes()[start - 1].is_ascii_alphanumeric()
            || text.as_bytes()[start - 1] == b'_')
    {
        start -= 1;
    }
    &text[start..offset]
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

fn typed_detail(
    name: &str,
    ty: Option<&simi_analysis::Type>,
    posts: &[ParameterPostType],
) -> String {
    let displayed = match ty {
        Some(Type::Function(callable)) if !posts.is_empty() => {
            let mut callable = callable.clone();
            for post in posts {
                if let Some(parameter) = callable.parameters.get_mut(post.parameter_index) {
                    parameter.post = Some(post.becomes.clone());
                }
            }
            Type::Function(callable).display()
        }
        Some(ty) => ty.display(),
        None => "any".to_owned(),
    };
    format!("{name} : {displayed}")
}

fn completion_detail(_kind: SymbolKind, name: &str, _parameters: Option<&[String]>) -> String {
    format!("{name} : any")
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
