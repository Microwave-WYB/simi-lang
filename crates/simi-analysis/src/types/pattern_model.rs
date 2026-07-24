use super::*;

pub(super) fn binary_operator(node: &SyntaxNode) -> Option<K> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .map(|token| token.kind())
        .find(|kind| {
            matches!(
                kind,
                K::PLUS
                    | K::MINUS
                    | K::STAR
                    | K::SLASH
                    | K::SLASH_SLASH
                    | K::PERCENT
                    | K::LESS_GREATER
                    | K::EQ_EQ
                    | K::BANG_EQ
                    | K::LESS
                    | K::LESS_EQ
                    | K::GREATER
                    | K::GREATER_EQ
                    | K::AND_KW
                    | K::OR_KW
            )
        })
}
pub(super) fn literal_string(expression: &syntax::Expr) -> Option<String> {
    let syntax::Expr::Literal(literal) = expression else {
        return None;
    };
    direct_token(literal.syntax(), K::STRING).map(|token| unquote(token.text()))
}
pub(super) fn comparison_matcher(expression: &syntax::Expr) -> Option<TypeMatcher> {
    let syntax::Expr::Literal(literal) = expression else {
        return None;
    };
    if direct_token(literal.syntax(), K::NIL_KW).is_some() {
        return Some(TypeMatcher::Exact(Type::Nil));
    }
    if let Some(token) = direct_token(literal.syntax(), K::STRING) {
        return Some(TypeMatcher::Exact(Type::LiteralString(unquote(
            token.text(),
        ))));
    }
    if let Some(token) = direct_token(literal.syntax(), K::INT)
        && let Ok(value) = token.text().parse()
    {
        return Some(TypeMatcher::Exact(Type::LiteralInt(value)));
    }
    if direct_token(literal.syntax(), K::TRUE_KW).is_some() {
        return Some(TypeMatcher::Exact(Type::LiteralBoolean(true)));
    }
    if direct_token(literal.syntax(), K::FALSE_KW).is_some() {
        return Some(TypeMatcher::Exact(Type::LiteralBoolean(false)));
    }
    None
}
pub(super) fn matcher_relation(ty: &Type, matcher: &TypeMatcher) -> Option<bool> {
    if matches!(
        ty,
        Type::Any | Type::Unknown | Type::Infer(_) | Type::Generic(_)
    ) {
        return None;
    }
    match matcher {
        TypeMatcher::Category(category) => Some(match *category {
            "nil" => matches!(ty, Type::Nil),
            "boolean" => matches!(ty, Type::Boolean | Type::LiteralBoolean(_)),
            "integer" => matches!(ty, Type::Int | Type::LiteralInt(_)),
            "float" => matches!(ty, Type::Float),
            "string" => matches!(ty, Type::String | Type::LiteralString(_)),
            "list" => matches!(ty, Type::ListExact(_) | Type::ListRest(_)),
            "map" => matches!(ty, Type::Map { .. }),
            "function" => matches!(ty, Type::Function(_)),
            _ => false,
        }),
        TypeMatcher::Exact(expected) => match (ty, expected) {
            (Type::String, Type::LiteralString(_))
            | (Type::Int, Type::LiteralInt(_))
            | (Type::Boolean, Type::LiteralBoolean(_)) => None,
            (Type::LiteralString(left), Type::LiteralString(right)) => Some(left == right),
            (Type::LiteralInt(left), Type::LiteralInt(right)) => Some(left == right),
            (Type::LiteralBoolean(left), Type::LiteralBoolean(right)) => Some(left == right),
            _ => Some(ty == expected),
        },
    }
}
pub(super) fn narrow_type(ty: Type, matcher: &TypeMatcher, keep: bool) -> Type {
    if let Type::Union(items) = ty {
        return union(
            items
                .into_iter()
                .map(|item| narrow_type(item, matcher, keep))
                .collect(),
        );
    }
    match matcher_relation(&ty, matcher) {
        Some(matches) if matches == keep => ty,
        Some(_) => Type::Never,
        None if keep => match (&ty, matcher) {
            (Type::String, TypeMatcher::Exact(Type::LiteralString(value))) => {
                Type::LiteralString(value.clone())
            }
            _ => ty,
        },
        None => ty,
    }
}
pub(super) fn narrow_map_field(ty: Type, field: &str, matcher: &TypeMatcher, keep: bool) -> Type {
    if let Type::Union(items) = ty {
        return union(
            items
                .into_iter()
                .map(|item| narrow_map_field(item, field, matcher, keep))
                .collect(),
        );
    }
    let Type::Map {
        mut fields,
        index,
        open,
    } = ty
    else {
        return ty;
    };
    if let Some((_, field_ty)) = fields.iter_mut().find(|(name, _)| name == field) {
        let narrowed = narrow_type(field_ty.clone(), matcher, keep);
        if narrowed == Type::Never {
            return Type::Never;
        }
        *field_ty = narrowed;
        return Type::Map {
            fields,
            index,
            open,
        };
    }
    if open || index.is_some() {
        return Type::Map {
            fields,
            index,
            open,
        };
    }
    let nil_matches = matcher_relation(&Type::Nil, matcher).unwrap_or(false);
    if nil_matches == keep {
        Type::Map {
            fields,
            index,
            open,
        }
    } else {
        Type::Never
    }
}
pub(super) fn field_lookup_type(ty: Type, field: &str) -> Type {
    match ty {
        Type::Union(items) => union(
            items
                .into_iter()
                .map(|item| field_lookup_type(item, field))
                .collect(),
        ),
        Type::Map {
            fields,
            index,
            open,
        } => fields
            .into_iter()
            .find(|(name, _)| name == field)
            .map(|(_, ty)| ty)
            .or_else(|| index.map(|(_, value)| union(vec![*value, Type::Nil])))
            .unwrap_or(if open { Type::Any } else { Type::Nil }),
        Type::Any => Type::Any,
        _ => Type::Unknown,
    }
}
pub(super) fn type_may_be_nil(ty: &Type) -> bool {
    match ty {
        Type::Nil | Type::Any | Type::Unknown | Type::Infer(_) | Type::Generic(_) => true,
        Type::Union(items) => items.iter().any(type_may_be_nil),
        _ => false,
    }
}
pub(super) fn type_may_be_callable(ty: &Type) -> bool {
    match ty {
        Type::Function(_) | Type::Any | Type::Unknown | Type::Infer(_) => true,
        Type::Union(items) => items.iter().any(type_may_be_callable),
        _ => false,
    }
}
pub(super) fn pattern_partition(source: Type, pattern: &syntax::Pattern) -> (Type, Type) {
    match pattern {
        syntax::Pattern::Binding(_) | syntax::Pattern::Wildcard(_) => (source, Type::Never),
        syntax::Pattern::Literal(node) => {
            let expression = if direct_token(node.syntax(), K::NIL_KW).is_some() {
                TypeMatcher::Exact(Type::Nil)
            } else if let Some(token) = direct_token(node.syntax(), K::STRING) {
                TypeMatcher::Exact(Type::LiteralString(unquote(token.text())))
            } else if let Some(token) = direct_token(node.syntax(), K::INT) {
                TypeMatcher::Exact(Type::LiteralInt(token.text().parse().unwrap_or_default()))
            } else if direct_token(node.syntax(), K::TRUE_KW).is_some() {
                TypeMatcher::Exact(Type::LiteralBoolean(true))
            } else if direct_token(node.syntax(), K::FALSE_KW).is_some() {
                TypeMatcher::Exact(Type::LiteralBoolean(false))
            } else {
                return (source.clone(), source);
            };
            (
                narrow_type(source.clone(), &expression, true),
                narrow_type(source, &expression, false),
            )
        }
        syntax::Pattern::List(node) => partition_list_pattern(source, node),
        syntax::Pattern::Map(node) => partition_map_pattern(source, node),
    }
}
pub(super) fn unresolved_pattern_shape(pattern: &syntax::Pattern) -> Type {
    match pattern {
        syntax::Pattern::Binding(_) | syntax::Pattern::Wildcard(_) => Type::Unknown,
        syntax::Pattern::Literal(node) => literal_type(node.syntax()),
        syntax::Pattern::List(list) => {
            let items = support::children::<syntax::Pattern>(list.syntax())
                .map(|child| unresolved_pattern_shape(&child))
                .collect::<Vec<_>>();
            if support::child::<syntax::RestPattern>(list.syntax()).is_some() {
                Type::ListRest(Box::new(union(items)))
            } else {
                Type::ListExact(items)
            }
        }
        syntax::Pattern::Map(map) => Type::Map {
            fields: support::children::<syntax::MapPatternField>(map.syntax())
                .filter_map(|field| {
                    let name = direct_token(field.syntax(), K::IDENT)?;
                    let child = support::child::<syntax::Pattern>(field.syntax())?;
                    Some((name.text().to_owned(), unresolved_pattern_shape(&child)))
                })
                .collect(),
            index: None,
            open: support::child::<syntax::RestPattern>(map.syntax()).is_some(),
        },
    }
}
pub(super) fn partition_list_pattern(source: Type, pattern: &syntax::ListPattern) -> (Type, Type) {
    if let Type::Union(items) = source {
        let (matched, unmatched): (Vec<_>, Vec<_>) = items
            .into_iter()
            .map(|item| partition_list_pattern(item, pattern))
            .unzip();
        return (union(matched), union(unmatched));
    }
    let children = support::children::<syntax::Pattern>(pattern.syntax()).collect::<Vec<_>>();
    let has_rest = support::child::<syntax::RestPattern>(pattern.syntax()).is_some();
    match source {
        Type::ListExact(mut items)
            if (has_rest && items.len() >= children.len())
                || (!has_rest && items.len() == children.len()) =>
        {
            let original = Type::ListExact(items.clone());
            for (index, child) in children.iter().enumerate() {
                let (matched, _) = pattern_partition(items[index].clone(), child);
                if matched == Type::Never {
                    return (Type::Never, original);
                }
                items[index] = matched;
            }
            let matched = Type::ListExact(items);
            if matched == original {
                (matched, Type::Never)
            } else {
                (matched, original)
            }
        }
        Type::ListExact(items) => (Type::Never, Type::ListExact(items)),
        Type::ListRest(item) => {
            let original = Type::ListRest(item.clone());
            if !has_rest {
                return (original.clone(), original);
            }
            for child in children {
                let (matched, _) = pattern_partition((*item).clone(), &child);
                if matched == Type::Never {
                    return (Type::Never, original);
                }
            }
            (original.clone(), original)
        }
        unresolved @ (Type::Infer(_) | Type::Unknown | Type::Any) => {
            let shape = unresolved_pattern_shape(&syntax::Pattern::List(pattern.clone()));
            (shape, unresolved)
        }
        other => (Type::Never, other),
    }
}
pub(super) fn partition_map_pattern(source: Type, pattern: &syntax::MapPattern) -> (Type, Type) {
    if let Type::Union(items) = source {
        let (matched, unmatched): (Vec<_>, Vec<_>) = items
            .into_iter()
            .map(|item| partition_map_pattern(item, pattern))
            .unzip();
        return (union(matched), union(unmatched));
    }
    if matches!(source, Type::Infer(_) | Type::Unknown | Type::Any) {
        let shape = unresolved_pattern_shape(&syntax::Pattern::Map(pattern.clone()));
        return (shape, source);
    }
    let has_rest = support::child::<syntax::RestPattern>(pattern.syntax()).is_some();
    if !has_rest
        && let Type::Map {
            fields,
            index: None,
            open: false,
        } = &source
    {
        let pattern_fields = support::children::<syntax::MapPatternField>(pattern.syntax())
            .filter_map(|field| direct_token(field.syntax(), K::IDENT))
            .map(|name| name.text().to_owned())
            .collect::<HashSet<_>>();
        if fields
            .iter()
            .any(|(name, _)| !pattern_fields.contains(name))
        {
            return (Type::Never, source);
        }
    }

    let mut matched = source;
    let mut failures = Vec::new();
    for field in support::children::<syntax::MapPatternField>(pattern.syntax()) {
        let Some(name) = direct_token(field.syntax(), K::IDENT) else {
            continue;
        };
        let Some(child) = support::child::<syntax::Pattern>(field.syntax()) else {
            continue;
        };
        let (field_match, field_failure) =
            partition_required_map_field(matched, name.text(), &child);
        failures.push(field_failure);
        matched = field_match;
        if matched == Type::Never {
            break;
        }
    }
    let unmatched = if failures.is_empty() {
        Type::Never
    } else {
        union(failures)
    };
    (matched, unmatched)
}
pub(super) fn partition_required_map_field(
    source: Type,
    field: &str,
    pattern: &syntax::Pattern,
) -> (Type, Type) {
    if let syntax::Pattern::Literal(literal) = pattern
        && direct_token(literal.syntax(), K::NIL_KW).is_some()
    {
        let matcher = TypeMatcher::Exact(Type::Nil);
        return (
            narrow_map_field(source.clone(), field, &matcher, true),
            narrow_map_field(source, field, &matcher, false),
        );
    }
    let Type::Map {
        fields,
        index,
        open,
    } = &source
    else {
        return (Type::Never, source);
    };
    if let Some((_, field_ty)) = fields.iter().find(|(name, _)| name == field) {
        let (matched_field, unmatched_field) = pattern_partition(field_ty.clone(), pattern);
        let matched = replace_required_map_field(source.clone(), field, matched_field);
        let unmatched = replace_required_map_field(source, field, unmatched_field);
        return (matched, unmatched);
    }
    if *open || index.is_some() {
        let possible = index
            .as_ref()
            .map(|(_, value)| (**value).clone())
            .unwrap_or(Type::Any);
        let (matched_field, _) = pattern_partition(possible, pattern);
        // Open/index maps do not establish presence. Even if a present value can
        // match, absence must remain in the failure partition.
        let matched = if matched_field == Type::Never {
            Type::Never
        } else {
            source.clone()
        };
        return (matched, source);
    }
    (Type::Never, source)
}
pub(super) fn replace_required_map_field(source: Type, field: &str, value: Type) -> Type {
    if value == Type::Never {
        return Type::Never;
    }
    let Type::Map {
        mut fields,
        index,
        open,
    } = source
    else {
        return Type::Never;
    };
    if let Some((_, current)) = fields.iter_mut().find(|(name, _)| name == field) {
        *current = value;
    }
    Type::Map {
        fields,
        index,
        open,
    }
}
pub(super) fn remove_nil(ty: Type) -> Type {
    match ty {
        Type::Nil => Type::Never,
        Type::Union(items) => union(
            items
                .into_iter()
                .map(|item| if item == Type::Nil { Type::Never } else { item })
                .collect(),
        ),
        other => other,
    }
}
