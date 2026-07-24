use std::collections::HashMap;

use simi_syntax::ast as support;
use simi_syntax::generated::{self as syntax, AstNode};
use simi_syntax::{SyntaxKind as K, SyntaxNode, span::Span};

use crate::db::{FileId, parse, resolve, source_text};
use crate::model::{
    ExportField, ModuleMember, ModuleShape, ModuleValue, Resolution, SymbolId, SymbolKind,
};

#[derive(Clone, Debug)]
struct KnownValue {
    module: String,
    path: Vec<String>,
}

mod member;

pub use member::{imported_members, member_at, member_completions, module_at};

#[salsa::tracked(returns(clone))]
pub fn module_shape(db: &dyn salsa::Database, file: FileId) -> ModuleShape {
    let parsed = parse(db, file);
    let resolution = resolve(db, file);
    let root = syntax::Root::cast(parsed.syntax()).expect("parser produces a root");
    let mut maps = HashMap::<String, Vec<ExportField>>::new();
    let mut final_fields = Vec::new();

    for statement in root.statements() {
        // Type aliases are erased and therefore do not replace the final runtime value.
        if matches!(statement, syntax::Stmt::AliasDecl(_)) {
            continue;
        }
        // A module exports the value of its final runtime item. Clear the
        // previous candidate before every such item so an earlier map cannot leak
        // through a later declaration, assignment, or unsupported expression.
        final_fields.clear();
        match statement {
            syntax::Stmt::LetStmt(node) => {
                let Some(pattern) = support::child::<syntax::Pattern>(node.syntax()) else {
                    continue;
                };
                let syntax::Pattern::Binding(binding) = pattern else {
                    continue;
                };
                let Some(name) = support::token(binding.syntax(), K::IDENT) else {
                    continue;
                };
                if let Some(syntax::Expr::Map(map)) = support::child::<syntax::Expr>(node.syntax())
                {
                    maps.insert(
                        name.text().to_owned(),
                        fields_from_map(map.syntax(), &resolution, &maps),
                    );
                }
            }
            syntax::Stmt::ExprStmt(node) => {
                let Some(expression) = support::child::<syntax::Expr>(node.syntax()) else {
                    continue;
                };
                match expression {
                    syntax::Expr::Assign(assign) => {
                        let mut expressions =
                            assign.syntax().children().filter_map(syntax::Expr::cast);
                        let (Some(syntax::Expr::Field(target)), Some(value)) =
                            (expressions.next(), expressions.next())
                        else {
                            continue;
                        };
                        let Some(syntax::Expr::Name(receiver)) =
                            target.syntax().children().find_map(syntax::Expr::cast)
                        else {
                            continue;
                        };
                        let Some(receiver) = support::token(receiver.syntax(), K::IDENT) else {
                            continue;
                        };
                        let Some(field) = support::token(target.syntax(), K::IDENT) else {
                            continue;
                        };
                        let inferred = (!is_nil(&value)).then(|| {
                            field_from_value(
                                field.text().to_owned(),
                                token_span(&field),
                                value,
                                &resolution,
                                &maps,
                            )
                        });
                        let fields = maps.entry(receiver.text().to_owned()).or_default();
                        fields.retain(|existing| existing.name != field.text());
                        if let Some(inferred) = inferred {
                            fields.push(inferred);
                        }
                    }
                    syntax::Expr::Map(map) => {
                        final_fields = fields_from_map(map.syntax(), &resolution, &maps);
                    }
                    syntax::Expr::Name(name) => {
                        if let Some(name) = support::token(name.syntax(), K::IDENT)
                            && let Some(fields) = maps.get(name.text())
                        {
                            final_fields = fields.clone();
                        }
                    }
                    _ => {}
                }
            }
            syntax::Stmt::FunctionDecl(_) => {}
            syntax::Stmt::AliasDecl(_) => unreachable!("aliases continue above"),
        }
    }

    let inference = crate::types::infer_types(db, file, &HashMap::new());
    let expression_types = parsed
        .syntax()
        .descendants()
        .filter_map(syntax::MapEntry::cast)
        .filter_map(|entry| {
            let key = support::token(entry.syntax(), K::IDENT)?;
            let value = support::child::<syntax::Expr>(entry.syntax())?;
            let range = value.syntax().text_range();
            let value_span = Span::new(range.start().into(), range.end().into());
            let ty = inference
                .expression_types
                .iter()
                .find_map(|(at, ty)| (*at == value_span).then(|| ty.clone()))?;
            Some((token_span(&key), ty))
        })
        .collect::<Vec<_>>();
    attach_field_types(
        &mut final_fields,
        &inference,
        &resolution,
        &expression_types,
    );
    ModuleShape {
        documentation: module_documentation(&source_text(db, file)),
        fields: final_fields,
    }
}

fn attach_field_types(
    fields: &mut [ExportField],
    inference: &crate::model::TypeInference,
    resolution: &Resolution,
    expression_types: &[(Span, crate::model::Type)],
) {
    for field in fields {
        if let Some(symbol) = resolution.symbol_at(field.span.start) {
            field.ty = inference.symbol_types.get(&symbol).cloned();
            field.posts = inference
                .symbol_posts
                .get(&symbol)
                .cloned()
                .unwrap_or_default();
        } else if let Some((_, ty)) = expression_types.iter().find(|(at, _)| *at == field.span) {
            field.ty = Some(ty.clone());
        }
        attach_field_types(&mut field.fields, inference, resolution, expression_types);
    }
}

fn module_documentation(source: &str) -> Option<String> {
    let mut lines = source.lines().skip_while(|line| line.trim().is_empty());
    let mut documentation = Vec::new();
    for line in &mut lines {
        let line = line.trim_start();
        let Some(text) = line.strip_prefix("----") else {
            break;
        };
        documentation.push(text.strip_prefix(' ').unwrap_or(text).to_owned());
    }
    (!documentation.is_empty()).then(|| documentation.join("\n"))
}

fn fields_from_map(
    node: &SyntaxNode,
    resolution: &Resolution,
    maps: &HashMap<String, Vec<ExportField>>,
) -> Vec<ExportField> {
    support::children::<syntax::MapEntry>(node)
        .filter_map(|entry| {
            let key = support::token(entry.syntax(), K::IDENT)?;
            let value = support::child::<syntax::Expr>(entry.syntax())?;
            if is_nil(&value) {
                return None;
            }
            Some(field_from_value(
                key.text().to_owned(),
                token_span(&key),
                value,
                resolution,
                maps,
            ))
        })
        .collect()
}

fn field_from_value(
    name: String,
    span: Span,
    value: syntax::Expr,
    resolution: &Resolution,
    maps: &HashMap<String, Vec<ExportField>>,
) -> ExportField {
    let mut field = ExportField {
        name,
        span,
        parameters: None,
        documentation: None,
        ty: None,
        posts: Vec::new(),
        fields: Vec::new(),
    };
    match value {
        syntax::Expr::Name(node) => {
            if let Some(token) = support::token(node.syntax(), K::IDENT) {
                if let Some(symbol) = resolution.symbol_at(token_span(&token).start)
                    && let Some(data) = resolution.symbol_data(symbol)
                {
                    if matches!(data.kind, SymbolKind::Function | SymbolKind::Builtin) {
                        field.parameters = data.parameters.clone();
                    }
                    field.documentation = data.documentation.clone();
                    field.span = data.declaration.unwrap_or(field.span);
                } else if let Some(fields) = maps.get(token.text()) {
                    field.fields = fields.clone();
                }
            }
        }
        syntax::Expr::Function(node) => {
            let parameters = support::child::<syntax::ParamList>(node.syntax())
                .map(|params| {
                    support::children::<syntax::Param>(params.syntax())
                        .filter_map(|param| support::token(param.syntax(), K::IDENT))
                        .map(|token| token.text().to_owned())
                        .collect()
                })
                .unwrap_or_default();
            field.parameters = Some(parameters);
        }
        syntax::Expr::Map(map) => {
            field.fields = fields_from_map(map.syntax(), resolution, maps);
        }
        _ => {}
    }
    field
}

pub fn imported_modules(db: &dyn salsa::Database, file: FileId) -> HashMap<SymbolId, String> {
    known_bindings(db, file)
        .into_iter()
        .filter_map(|(symbol, value)| value.path.is_empty().then_some((symbol, value.module)))
        .collect()
}

fn known_bindings(db: &dyn salsa::Database, file: FileId) -> HashMap<SymbolId, KnownValue> {
    let parsed = parse(db, file);
    let resolution = resolve(db, file);
    let root = syntax::Root::cast(parsed.syntax()).expect("parser produces a root");
    let mut bindings = HashMap::new();
    for node in root
        .syntax()
        .descendants()
        .filter_map(syntax::LetStmt::cast)
    {
        let Some(syntax::Pattern::Binding(binding)) =
            support::child::<syntax::Pattern>(node.syntax())
        else {
            continue;
        };
        let Some(binding) = support::token(binding.syntax(), K::IDENT) else {
            continue;
        };
        let Some(value) = support::child::<syntax::Expr>(node.syntax()) else {
            continue;
        };
        let Some(value) = known_value(value, &resolution, &bindings) else {
            continue;
        };
        if let Some(symbol) = resolution.symbol_at(token_span(&binding).start) {
            bindings.insert(symbol, value);
        }
    }
    bindings
}

fn known_value(
    expression: syntax::Expr,
    resolution: &Resolution,
    bindings: &HashMap<SymbolId, KnownValue>,
) -> Option<KnownValue> {
    match expression {
        syntax::Expr::Call(call) => required_module(&call, resolution).map(|module| KnownValue {
            module,
            path: Vec::new(),
        }),
        syntax::Expr::Name(name) => {
            let token = support::token(name.syntax(), K::IDENT)?;
            bindings
                .get(&resolution.symbol_at(token_span(&token).start)?)
                .cloned()
        }
        syntax::Expr::Field(field) => {
            let base = field.syntax().children().find_map(syntax::Expr::cast)?;
            let mut value = known_value(base, resolution, bindings)?;
            let name = support::token(field.syntax(), K::IDENT)?;
            value.path.push(name.text().to_owned());
            Some(value)
        }
        syntax::Expr::Paren(paren) => {
            let inner = paren.syntax().children().find_map(syntax::Expr::cast)?;
            known_value(inner, resolution, bindings)
        }
        _ => None,
    }
}

fn required_module(call: &syntax::CallExpr, resolution: &Resolution) -> Option<String> {
    let syntax::Expr::Name(callee) = call.syntax().children().find_map(syntax::Expr::cast)? else {
        return None;
    };
    let callee = support::token(callee.syntax(), K::IDENT)?;
    let data = resolution.symbol_data(resolution.symbol_at(token_span(&callee).start)?)?;
    if !data.builtin || data.name != "require" {
        return None;
    }
    let arguments = support::child::<syntax::ArgList>(call.syntax())?;
    let mut expressions = support::children::<syntax::Expr>(arguments.syntax());
    let syntax::Expr::Literal(module) = expressions.next()? else {
        return None;
    };
    if expressions.next().is_some() {
        return None;
    }
    string_literal(support::token(module.syntax(), K::STRING)?.text())
}

fn is_nil(expression: &syntax::Expr) -> bool {
    matches!(expression, syntax::Expr::Literal(node) if support::token(node.syntax(), K::NIL_KW).is_some())
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset < span.end
}

fn string_literal(text: &str) -> Option<String> {
    text.strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .map(str::to_owned)
}

fn token_span(token: &simi_syntax::SyntaxToken) -> Span {
    let range = token.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}
