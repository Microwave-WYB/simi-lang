use super::*;

impl Backend {
    pub(super) fn hover(&self, params: HoverParams) -> Result<Option<Hover>, ProtocolError> {
        let uri = params.text_document_position_params.text_document.uri;
        let (document, text, resolution, offset) =
            self.analysis_at(&uri, params.text_document_position_params.position)?;
        if let Some(module) = module_at(&self.db, document.file, &self.module_shapes, offset) {
            let mut value = resolution.hover(offset).map_or_else(
                || module.module,
                |facts| {
                    let inference = infer_types(&self.db, document.file, &self.module_shapes);
                    typed_detail(
                        &facts.name,
                        inference.symbol_types.get(&facts.symbol),
                        inference
                            .symbol_posts
                            .get(&facts.symbol)
                            .map_or(&[], Vec::as_slice),
                    )
                },
            );
            if let Some(documentation) = module.documentation {
                value.push_str("\n\n");
                value.push_str(&documentation);
            }
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value,
                }),
                range: None,
            }));
        }
        if let Some(member) = member_at(&self.db, document.file, &self.module_shapes, &text, offset)
        {
            let mut value = typed_detail(
                &member.field.name,
                member.field.ty.as_ref(),
                &member.field.posts,
            );
            if let Some(documentation) = member.field.documentation {
                value.push_str("\n\n");
                value.push_str(&documentation);
            }
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value,
                }),
                range: None,
            }));
        }
        let inference = infer_types(&self.db, document.file, &self.module_shapes);
        if let Some((span, ty)) = wildcard_type_at(&self.db, document.file, &inference, offset) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value: format!("_ : {}", ty.display()),
                }),
                range: Some(self.range(&text, span)?),
            }));
        }
        if let Some((name, span, ty)) = field_type_at(&self.db, document.file, &inference, offset) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value: format!("{name} : {}", ty.display()),
                }),
                range: Some(self.range(&text, span)?),
            }));
        }
        if let Some(facts) = resolution.hover(offset) {
            let imported = imported_members(&self.db, document.file, &self.module_shapes);
            let ty = symbol_type_at(&inference, &resolution, offset).or_else(|| {
                imported
                    .get(&facts.symbol)
                    .and_then(|member| member.field.ty.clone())
            });
            let posts = inference
                .symbol_posts
                .get(&facts.symbol)
                .map(Vec::as_slice)
                .or_else(|| {
                    imported
                        .get(&facts.symbol)
                        .map(|member| member.field.posts.as_slice())
                })
                .unwrap_or(&[]);
            let mut detail = typed_detail(&facts.name, ty.as_ref(), posts);
            let documentation = facts.documentation.or_else(|| {
                imported
                    .get(&facts.symbol)
                    .and_then(|member| member.field.documentation.clone())
            });
            if let Some(documentation) = documentation {
                detail.push_str("\n\n");
                detail.push_str(&documentation);
            }
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value: detail,
                }),
                range: resolution
                    .symbol_span_at(offset)
                    .map(|(_, span)| self.range(&text, span))
                    .transpose()?,
            }));
        }
        let Some((span, ty)) = expression_type_at(&inference, offset) else {
            return Ok(None);
        };
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::PlainText,
                value: ty.display(),
            }),
            range: Some(self.range(&text, span)?),
        }))
    }
}
