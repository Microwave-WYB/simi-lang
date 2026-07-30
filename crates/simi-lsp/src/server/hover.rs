use simi_syntax::ast as support;
use simi_syntax::generated::{self as syntax, AstNode};

use super::*;

const HOVER_TYPE_WIDTH: usize = 80;

impl Backend {
    pub(super) fn hover(&self, params: HoverParams) -> Result<Option<Hover>, ProtocolError> {
        let uri = params.text_document_position_params.text_document.uri;
        let (document, text, resolution, offset) =
            self.analysis_at(&uri, params.text_document_position_params.position)?;
        if let Some((span, declaration, is_alias)) = named_type_at(&self.db, document.file, offset)
        {
            let description = if is_alias {
                "Transparent type alias. It is erased at runtime."
            } else {
                "Named recursive type. It is erased at runtime."
            };
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```simi\n{declaration}\n```\n\n{description}"),
                }),
                range: Some(self.range(&text, span)?),
            }));
        }
        if let Some((keyword, span)) = keywords::at(&self.db, document.file, offset) {
            let mut value = keywords::hover_text(keyword);
            let inference = self.inference(document.file);
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
            let inference = self.inference(document.file);
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
        let inference = self.inference(document.file);
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
) -> Option<(Span, String, bool)> {
    let parsed = parse(db, file);
    let root = syntax::Root::cast(parsed.syntax())?;
    let mut declarations = Vec::new();
    for statement in root.statements() {
        let (syntax, is_alias) = match statement {
            syntax::Stmt::AliasDecl(declaration) => (declaration.syntax().clone(), true),
            syntax::Stmt::TypeDecl(declaration) => (declaration.syntax().clone(), false),
            _ => continue,
        };
        // `type` and `alias` are contextual keywords, so their declaration
        // names are the second identifier token.
        let name = support::tokens(&syntax, simi_syntax::SyntaxKind::IDENT).nth(1)?;
        let range = name.text_range();
        let span = Span::new(range.start().into(), range.end().into());
        let declaration = syntax.to_string();
        if span.start <= offset && offset < span.end {
            return Some((span, declaration, is_alias));
        }
        declarations.push((name.text().to_owned(), declaration, is_alias));
    }
    for node in root
        .syntax()
        .descendants()
        .filter_map(syntax::TypeName::cast)
    {
        let name = support::token(node.syntax(), simi_syntax::SyntaxKind::IDENT)?;
        let range = name.text_range();
        let span = Span::new(range.start().into(), range.end().into());
        if span.start <= offset
            && offset < span.end
            && let Some((_, declaration, is_alias)) = declarations
                .iter()
                .find(|(declared_name, _, _)| declared_name == name.text())
        {
            return Some((span, declaration.clone(), *is_alias));
        }
    }
    None
}
