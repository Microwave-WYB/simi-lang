use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender, TrySendError};

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
    AnalysisDatabase, AnalysisDiagnostic, AnalysisDiagnosticSeverity, FileId, ModuleShape,
    RenameError, Resolution, Span, SymbolKind, Type, TypeInference, diagnostics, document_symbols,
    expression_type_at, field_type_at, imported_members, member_at, member_completions, module_at,
    module_shape, parse, references, resolve, source_text, symbol_type_at, wildcard_type_at,
};

use crate::position;

mod diagnostic;

mod navigation;

mod hover;

mod completion;

mod keywords;

#[derive(Clone)]
struct Document {
    file: FileId,
    version: i32,
    generation: u64,
}

struct AnalysisJob {
    uri: Url,
    generation: u64,
    version: i32,
    source: String,
    module_shapes: HashMap<String, ModuleShape>,
}

struct AnalysisResult {
    uri: Url,
    generation: u64,
    version: i32,
    source: String,
    diagnostics: Vec<AnalysisDiagnostic>,
}

type AnalysisAt<'a> = (
    &'a Document,
    std::sync::Arc<String>,
    std::sync::Arc<Resolution>,
    usize,
);

pub struct Backend {
    db: AnalysisDatabase,
    documents: HashMap<Url, Document>,
    module_shapes: HashMap<String, ModuleShape>,
    inferences: RefCell<HashMap<FileId, Arc<TypeInference>>>,
    analysis_wakeups: Sender<()>,
    pending_analyses: Arc<Mutex<HashMap<Url, AnalysisJob>>>,
    completed_wakeups: Receiver<()>,
    completed_analyses: Arc<Mutex<HashMap<Url, AnalysisResult>>>,
    next_generation: u64,
    #[cfg(test)]
    inference_computations: std::cell::Cell<usize>,
    shutdown_requested: bool,
}

impl Default for Backend {
    fn default() -> Self {
        let (analysis_wakeups, wakeup_receiver) = crossbeam_channel::bounded(1);
        let pending_analyses = Arc::new(Mutex::new(HashMap::new()));
        let (completed_wakeups, completed_receiver) = crossbeam_channel::bounded(1);
        let completed_analyses = Arc::new(Mutex::new(HashMap::new()));
        let worker_pending = pending_analyses.clone();
        let worker_completed = completed_analyses.clone();
        std::thread::spawn(move || {
            diagnostic::background_analysis_worker(
                wakeup_receiver,
                worker_pending,
                completed_wakeups,
                worker_completed,
            )
        });
        Self {
            db: AnalysisDatabase::default(),
            documents: HashMap::new(),
            module_shapes: HashMap::new(),
            inferences: RefCell::new(HashMap::new()),
            analysis_wakeups,
            pending_analyses,
            completed_wakeups: completed_receiver,
            completed_analyses,
            next_generation: 0,
            #[cfg(test)]
            inference_computations: std::cell::Cell::new(0),
            shutdown_requested: false,
        }
    }
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
        self.next_generation += 1;
        let document = Document {
            file,
            version: item.version,
            generation: self.next_generation,
        };
        self.documents.insert(uri.clone(), document.clone());
        self.enqueue_analysis(uri, document);
        Vec::new()
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
        self.inferences.borrow_mut().remove(&document.file);
        let document = Document {
            file: document.file,
            version: params.text_document.version,
            generation: document.generation,
        };
        self.documents.insert(uri.clone(), document.clone());
        self.enqueue_analysis(uri, document);
        Ok(Vec::new())
    }

    pub fn close(&mut self, params: DidCloseTextDocumentParams) -> Vec<Notification> {
        let uri = params.text_document.uri;
        if let Some(document) = self.documents.remove(&uri) {
            self.inferences.borrow_mut().remove(&document.file);
        }
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

    fn enqueue_analysis(&self, uri: Url, document: Document) {
        let source = source_text(&self.db, document.file).as_ref().clone();
        self.pending_analyses
            .lock()
            .expect("analysis scheduler lock should not be poisoned")
            .insert(
                uri.clone(),
                AnalysisJob {
                    uri,
                    generation: document.generation,
                    version: document.version,
                    source,
                    module_shapes: self.module_shapes.clone(),
                },
            );
        match self.analysis_wakeups.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => {}
            Err(TrySendError::Disconnected(())) => {}
        }
    }

    fn completed_wakeups(&self) -> &Receiver<()> {
        &self.completed_wakeups
    }

    fn take_completed_analyses(&self) -> HashMap<Url, AnalysisResult> {
        std::mem::take(
            &mut *self
                .completed_analyses
                .lock()
                .expect("completed analysis lock should not be poisoned"),
        )
    }

    fn complete_analysis(&self, result: AnalysisResult) -> Option<Notification> {
        let document = self.documents.get(&result.uri)?;
        (document.generation == result.generation && document.version == result.version)
            .then(|| self.diagnostics_notification(result))
    }

    #[cfg(test)]
    fn wait_for_diagnostics(&self) -> Notification {
        loop {
            self.completed_wakeups
                .recv_timeout(std::time::Duration::from_secs(15))
                .expect("background diagnostics should complete");
            for result in self.take_completed_analyses().into_values() {
                if let Some(notification) = self.complete_analysis(result) {
                    return notification;
                }
            }
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

    fn inference(&self, file: FileId) -> Arc<TypeInference> {
        if let Some(inference) = self.inferences.borrow().get(&file) {
            return inference.clone();
        }
        #[cfg(test)]
        self.inference_computations
            .set(self.inference_computations.get() + 1);
        let inference = Arc::new(simi_analysis::infer_types(
            &self.db,
            file,
            &self.module_shapes,
        ));
        self.inferences.borrow_mut().insert(file, inference.clone());
        inference
    }

    #[cfg(test)]
    fn inference_computations(&self) -> usize {
        self.inference_computations.get()
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

    let completed_wakeups = backend.completed_wakeups().clone();
    loop {
        crossbeam_channel::select! {
            recv(connection.receiver) -> message => {
                let Ok(message) = message else {
                    return Ok(());
                };
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
            recv(completed_wakeups) -> wakeup => {
                let Ok(()) = wakeup else {
                    return Ok(());
                };
                for result in backend.take_completed_analyses().into_values() {
                    if let Some(notification) = backend.complete_analysis(result) {
                        connection.sender.send(Message::Notification(notification))?;
                    }
                }
            }
        }
    }
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
        SymbolKind::Parameter | SymbolKind::Let | SymbolKind::Pattern => LspSymbolKind::VARIABLE,
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
        SymbolKind::Let | SymbolKind::Pattern => CompletionItemKind::VARIABLE,
    }
}

fn typed_detail(name: &str, ty: Option<&simi_analysis::Type>) -> String {
    format!(
        "{name} : {}",
        ty.map_or_else(|| "any".to_owned(), Type::display)
    )
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
