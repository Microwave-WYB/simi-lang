use super::*;

pub(super) fn literal_type(node: &SyntaxNode) -> Type {
    if let Some(token) = direct_token(node, K::INT) {
        return token
            .text()
            .parse::<i64>()
            .map(|_| Type::Int)
            .unwrap_or(Type::Int);
    }
    if direct_token(node, K::FLOAT).is_some() {
        return Type::Float;
    }
    if let Some(token) = direct_token(node, K::STRING) {
        return Type::LiteralString(unquote(token.text()));
    }
    if direct_token(node, K::TRUE_KW).is_some() || direct_token(node, K::FALSE_KW).is_some() {
        return Type::Boolean;
    }
    Type::Nil
}
pub(super) fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(text)
        .to_owned()
}
pub(super) fn may_hold_mutable_value(ty: &Type) -> bool {
    matches!(ty, Type::Any | Type::Unknown | Type::Infer(_)) || has_mutable_category(ty)
}
pub(super) fn is_nested_read(expression: &syntax::Expr) -> bool {
    match expression {
        syntax::Expr::Field(_) | syntax::Expr::Index(_) => true,
        syntax::Expr::Paren(paren) => child_expr(paren.syntax(), 0)
            .as_ref()
            .is_some_and(is_nested_read),
        _ => false,
    }
}
pub(super) fn expression_symbol(
    expression: &syntax::Expr,
    resolution: &Resolution,
) -> Option<SymbolId> {
    match expression {
        syntax::Expr::Name(name) => {
            let token = direct_token(name.syntax(), K::IDENT)?;
            resolution.symbol_at(token_span(&token).start)
        }
        syntax::Expr::Paren(paren) => {
            let inner = child_expr(paren.syntax(), 0)?;
            expression_symbol(&inner, resolution)
        }
        _ => None,
    }
}
pub(super) fn mutation_owner_symbol(
    expression: &syntax::Expr,
    resolution: &Resolution,
) -> Option<SymbolId> {
    mutation_root_symbol(expression, resolution)
}
pub(super) fn mutation_root_symbol(
    expression: &syntax::Expr,
    resolution: &Resolution,
) -> Option<SymbolId> {
    match expression {
        syntax::Expr::Name(_) => expression_symbol(expression, resolution),
        syntax::Expr::Paren(paren) => {
            let inner = child_expr(paren.syntax(), 0)?;
            mutation_root_symbol(&inner, resolution)
        }
        syntax::Expr::Field(field) => {
            let owner = child_expr(field.syntax(), 0)?;
            mutation_root_symbol(&owner, resolution)
        }
        syntax::Expr::Index(index) => {
            let owner = child_expr(index.syntax(), 0)?;
            mutation_root_symbol(&owner, resolution)
        }
        _ => None,
    }
}
pub(super) fn pattern_symbol(
    pattern: &syntax::Pattern,
    resolution: &Resolution,
) -> Option<SymbolId> {
    let syntax::Pattern::Binding(binding) = pattern else {
        return None;
    };
    let token = direct_token(binding.syntax(), K::IDENT)?;
    resolution.symbol_at(token_span(&token).start)
}
pub(super) fn expr_children(node: &SyntaxNode) -> impl Iterator<Item = syntax::Expr> + '_ {
    node.children().filter_map(syntax::Expr::cast)
}
pub(super) fn child_expr(node: &SyntaxNode, index: usize) -> Option<syntax::Expr> {
    expr_children(node).nth(index)
}
pub(super) fn child_node(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children().next()
}
pub(super) fn transparent_type_name(node: &SyntaxNode) -> Option<syntax::TypeName> {
    if let Some(name) = syntax::TypeName::cast(node.clone()) {
        return Some(name);
    }
    if node.kind() == K::TYPE_PAREN {
        let mut parameters = support::children::<syntax::TypeFunctionParam>(node);
        let parameter = parameters.next()?;
        if parameters.next().is_some()
            || support::child::<syntax::PostType>(parameter.syntax()).is_some()
        {
            return None;
        }
        let ty = support::child::<syntax::TypeExpr>(parameter.syntax())?;
        return transparent_type_name(ty.syntax());
    }
    let mut children = node.children();
    let child = children.next()?;
    if children.next().is_some() {
        return None;
    }
    transparent_type_name(&child)
}
pub(super) fn direct_token(node: &SyntaxNode, kind: K) -> Option<SyntaxToken> {
    support::token(node, kind)
}
pub(super) fn span(node: &SyntaxNode) -> Span {
    let range = node.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}
pub(super) fn token_span(token: &SyntaxToken) -> Span {
    let range = token.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}
