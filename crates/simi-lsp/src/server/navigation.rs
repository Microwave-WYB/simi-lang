use super::*;

impl Backend {
    #[allow(deprecated)]
    pub(super) fn document_symbols(
        &self,
        params: DocumentSymbolParams,
    ) -> Option<DocumentSymbolResponse> {
        let (document, text) = self.document(&params.text_document.uri).ok()?;
        let symbols = document_symbols(&self.db, document.file)
            .iter()
            .filter_map(|symbol| {
                let range = position::range(&text, symbol.span).ok()?;
                Some(lsp_types::DocumentSymbol {
                    name: symbol.name.clone(),
                    detail: None,
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
    pub(super) fn definition(
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
    pub(super) fn find_references(
        &self,
        params: ReferenceParams,
    ) -> Result<Vec<Location>, ProtocolError> {
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
    pub(super) fn prepare_rename(
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
    pub(super) fn rename(
        &self,
        params: RenameParams,
    ) -> Result<Option<WorkspaceEdit>, ProtocolError> {
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
}
