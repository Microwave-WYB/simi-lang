use super::*;

pub(super) fn literal_type(node: &SyntaxNode) -> Type {
    if direct_token(node, K::INT).is_some() {
        return Type::Int;
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
pub(super) fn type_literal_type(node: &SyntaxNode) -> Type {
    if direct_token(node, K::TRUE_KW).is_some() {
        return Type::LiteralBoolean(true);
    }
    if direct_token(node, K::FALSE_KW).is_some() {
        return Type::LiteralBoolean(false);
    }
    if let Some(token) = direct_token(node, K::STRING) {
        return Type::LiteralString(unquote(token.text()));
    }
    let negative = direct_token(node, K::MINUS).is_some();
    if let Some(token) = direct_token(node, K::INT)
        && let Some(value) = parse_integer_literal(token.text())
        && let Some(value) = if negative {
            value.checked_neg()
        } else {
            Some(value)
        }
    {
        return Type::LiteralInt(value);
    }
    if let Some(token) = direct_token(node, K::FLOAT)
        && let Some(mut value) = parse_float_literal(token.text())
    {
        if negative {
            value = -value;
        }
        return Type::LiteralFloat(
            LiteralFloat::new(value).expect("the syntax lexer accepts only finite floats"),
        );
    }
    Type::Nil
}
pub(super) fn direct_literal_type(expression: &syntax::Expr) -> Option<Type> {
    match expression {
        syntax::Expr::Literal(literal) => Some(type_literal_type(literal.syntax())),
        syntax::Expr::Paren(paren) => child_expr(paren.syntax(), 0)
            .as_ref()
            .and_then(direct_literal_type),
        syntax::Expr::Unary(unary) if direct_token(unary.syntax(), K::MINUS).is_some() => {
            let value = child_expr(unary.syntax(), 0)?;
            let syntax::Expr::Literal(literal) = value else {
                return None;
            };
            if let Some(token) = direct_token(literal.syntax(), K::INT) {
                return parse_integer_literal(token.text())
                    .and_then(i64::checked_neg)
                    .map(Type::LiteralInt);
            }
            let token = direct_token(literal.syntax(), K::FLOAT)?;
            let value = -parse_float_literal(token.text())?;
            Some(Type::LiteralFloat(
                LiteralFloat::new(value).expect("the syntax lexer accepts only finite floats"),
            ))
        }
        _ => None,
    }
}
pub(super) fn direct_boolean_literal_type(expression: &syntax::Expr) -> Option<Type> {
    let syntax::Expr::Literal(literal) = expression else {
        return None;
    };
    if direct_token(literal.syntax(), K::TRUE_KW).is_some() {
        return Some(Type::LiteralBoolean(true));
    }
    if direct_token(literal.syntax(), K::FALSE_KW).is_some() {
        return Some(Type::LiteralBoolean(false));
    }
    None
}
pub(super) fn parse_integer_literal(text: &str) -> Option<i64> {
    let text = text.replace('_', "");
    if let Some(digits) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        i64::from_str_radix(digits, 2).ok()
    } else if let Some(digits) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        i64::from_str_radix(digits, 16).ok()
    } else {
        text.parse().ok()
    }
}
pub(super) fn parse_float_literal(text: &str) -> Option<f64> {
    let value = text.replace('_', "").parse::<f64>().ok()?;
    value.is_finite().then_some(value)
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
