use simi_syntax::ast as support;
use simi_syntax::generated::{self as syntax, AstNode};

use super::*;

const HOVER_TYPE_WIDTH: usize = 80;

impl Backend {
    pub(super) fn hover(&self, params: HoverParams) -> Result<Option<Hover>, ProtocolError> {
        let uri = params.text_document_position_params.text_document.uri;
        let (document, text, resolution, offset) =
            self.analysis_at(&uri, params.text_document_position_params.position)?;
        if let Some((span, declaration)) = named_type_at(&self.db, document.file, offset) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "```simi\n{declaration}\n```\n\nNamed recursive type. It is erased at runtime."
                    ),
                }),
                range: Some(self.range(&text, span)?),
            }));
        }
        if let Some((keyword, span)) = keywords::at(&self.db, document.file, offset) {
            let mut value = keywords::hover_text(keyword);
            let inference = infer_types(&self.db, document.file, &self.module_shapes);
            if let Some((_, ty)) = expression_type_at(&inference, offset) {
                value.push_str("\n\nExpression type:\n\n```simi\n");
                value.push_str(&ty.pretty_display(HOVER_TYPE_WIDTH));
                value.push_str("\n```");
            }
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value,
                }),
                range: Some(self.range(&text, span)?),
            }));
        }
        if let Some(module) = module_at(&self.db, document.file, &self.module_shapes, offset) {
            let inference = infer_types(&self.db, document.file, &self.module_shapes);
            let ty = resolution.hover(offset).map_or_else(
                || {
                    self.module_shapes
                        .get(&module.module)
                        .and_then(|shape| shape.ty.clone())
                        .unwrap_or(Type::Any)
                },
                |facts| {
                    inference
                        .symbol_types
                        .get(&facts.symbol)
                        .cloned()
                        .unwrap_or(Type::Any)
                },
            );
            return Ok(Some(Hover {
                contents: type_hover(&ty, module.documentation.as_deref()),
                range: None,
            }));
        }
        if let Some(member) = member_at(&self.db, document.file, &self.module_shapes, &text, offset)
        {
            let ty = member.field.ty.clone().unwrap_or(Type::Any);
            return Ok(Some(Hover {
                contents: type_hover(&ty, member.field.documentation.as_deref()),
                range: None,
            }));
        }
        let inference = infer_types(&self.db, document.file, &self.module_shapes);
        if let Some((span, ty)) = wildcard_type_at(&self.db, document.file, &inference, offset) {
            return Ok(Some(Hover {
                contents: type_hover(&ty, None),
                range: Some(self.range(&text, span)?),
            }));
        }
        if let Some((_, span, ty)) = field_type_at(&self.db, document.file, &inference, offset) {
            return Ok(Some(Hover {
                contents: type_hover(&ty, None),
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
            let ty = ty.unwrap_or(Type::Any);
            let documentation = facts.documentation.or_else(|| {
                imported
                    .get(&facts.symbol)
                    .and_then(|member| member.field.documentation.clone())
            });
            return Ok(Some(Hover {
                contents: type_hover(&ty, documentation.as_deref()),
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
            contents: type_hover(&ty, None),
            range: Some(self.range(&text, span)?),
        }))
    }
}

fn type_hover(ty: &Type, documentation: Option<&str>) -> HoverContents {
    let mut value = format!("```simi\n{}\n```", ty.pretty_display(HOVER_TYPE_WIDTH));
    if let Some(documentation) = documentation {
        value.push_str("\n\n");
        value.push_str(documentation);
    }
    HoverContents::Markup(MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    })
}

fn named_type_at(
    db: &AnalysisDatabase,
    file: simi_analysis::FileId,
    offset: usize,
) -> Option<(Span, String)> {
    let parsed = parse(db, file);
    let root = syntax::Root::cast(parsed.syntax())?;
    let declarations = root
        .statements()
        .filter_map(|statement| match statement {
            syntax::Stmt::TypeDecl(declaration) => Some(declaration),
            _ => None,
        })
        .collect::<Vec<_>>();
    for declaration in &declarations {
        let contextual =
            support::token(declaration.syntax(), simi_syntax::SyntaxKind::TYPE_KW).is_none();
        let name = support::tokens(declaration.syntax(), simi_syntax::SyntaxKind::IDENT)
            .nth(usize::from(contextual))?;
        let range = name.text_range();
        let span = Span::new(range.start().into(), range.end().into());
        if span.start <= offset && offset < span.end {
            return Some((span, declaration.syntax().to_string()));
        }
    }
    for declaration in &declarations {
        let contextual =
            support::token(declaration.syntax(), simi_syntax::SyntaxKind::TYPE_KW).is_none();
        let declared_name = support::tokens(declaration.syntax(), simi_syntax::SyntaxKind::IDENT)
            .nth(usize::from(contextual))?;
        for node in declaration
            .syntax()
            .descendants()
            .filter_map(syntax::TypeName::cast)
        {
            let name = support::token(node.syntax(), simi_syntax::SyntaxKind::IDENT)?;
            let range = name.text_range();
            let span = Span::new(range.start().into(), range.end().into());
            if span.start <= offset && offset < span.end && name.text() == declared_name.text() {
                return Some((span, declaration.syntax().to_string()));
            }
        }
    }
    None
}
