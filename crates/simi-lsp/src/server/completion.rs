use super::*;

impl Backend {
    pub(super) fn completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>, ProtocolError> {
        let uri = params.text_document_position.text_document.uri;
        let (document, text, resolution, offset) =
            self.analysis_at(&uri, params.text_document_position.position)?;
        let prefix = identifier_prefix(&text, offset);
        let prefix_start = offset.saturating_sub(prefix.len());
        if prefix_start > 0 && text.as_bytes().get(prefix_start - 1) == Some(&b'.') {
            let items =
                member_completions(&self.db, document.file, &self.module_shapes, &text, offset)
                    .into_iter()
                    .map(|field| {
                        let is_function = field.parameters.is_some()
                            || matches!(field.ty.as_ref(), Some(Type::Function(_)));
                        CompletionItem {
                            label: field.name.clone(),
                            kind: Some(if is_function {
                                CompletionItemKind::FUNCTION
                            } else {
                                CompletionItemKind::FIELD
                            }),
                            detail: Some(typed_detail(
                                &field.name,
                                field.ty.as_ref(),
                                &field.posts,
                            )),
                            documentation: field.documentation.map(Documentation::String),
                            ..CompletionItem::default()
                        }
                    })
                    .collect();
            return Ok(Some(CompletionResponse::Array(items)));
        }
        let visible = resolution.visible_symbols(offset);
        if !prefix.is_empty()
            && visible
                .iter()
                .any(|symbol| resolution.hir.symbols[*symbol].name == prefix)
        {
            return Ok(Some(CompletionResponse::Array(Vec::new())));
        }

        let imported = imported_members(&self.db, document.file, &self.module_shapes);
        let inference = infer_types(&self.db, document.file, &self.module_shapes);
        let mut names = BTreeSet::new();
        let mut items = Vec::new();
        for symbol in visible {
            let data = &resolution.hir.symbols[symbol];
            if names.insert(data.name.clone()) {
                let builtin_rank = u8::from(data.builtin);
                let prefix_rank = if prefix.is_empty() || data.name.starts_with(prefix) {
                    0
                } else if data.name.contains(prefix) {
                    1
                } else {
                    2
                };
                let imported = imported.get(&symbol);
                let imported_parameters =
                    imported.and_then(|member| member.field.parameters.as_deref());
                let effective_ty = inference
                    .symbol_types
                    .get(&symbol)
                    .cloned()
                    .or_else(|| imported.and_then(|member| member.field.ty.clone()));
                let kind = if imported_parameters.is_some()
                    || matches!(effective_ty.as_ref(), Some(Type::Function(_)))
                {
                    CompletionItemKind::FUNCTION
                } else {
                    completion_kind(data.kind)
                };
                let detail = effective_ty.map_or_else(
                    || completion_detail(data.kind, &data.name, data.parameters.as_deref()),
                    |ty| {
                        let posts = inference
                            .symbol_posts
                            .get(&symbol)
                            .map(Vec::as_slice)
                            .or_else(|| imported.map(|member| member.field.posts.as_slice()))
                            .unwrap_or(&[]);
                        typed_detail(&data.name, Some(&ty), posts)
                    },
                );
                items.push(CompletionItem {
                    label: data.name.clone(),
                    kind: Some(kind),
                    detail: Some(detail),
                    documentation: imported
                        .and_then(|member| member.field.documentation.clone())
                        .map(Documentation::String),
                    sort_text: Some(format!("{builtin_rank}{prefix_rank}:{}", data.name)),
                    ..CompletionItem::default()
                });
            }
        }
        items.sort_by(|left, right| left.sort_text.cmp(&right.sort_text));
        Ok(Some(CompletionResponse::Array(items)))
    }
}
