use super::*;

pub(super) fn widen_mutable_type(ty: Type) -> Type {
    match ty {
        Type::ListExact(_) | Type::ListRest(_) => Type::ListRest(Box::new(Type::Any)),
        Type::Map { .. } => Type::Map {
            fields: Vec::new(),
            index: None,
            open: true,
        },
        Type::Union(items) => union(items.into_iter().map(widen_mutable_type).collect()),
        other => other,
    }
}
pub(super) fn update_map_field(ty: Type, field: &str, value: Type) -> Type {
    match ty {
        Type::Union(items) => union(
            items
                .into_iter()
                .map(|item| update_map_field(item, field, value.clone()))
                .collect(),
        ),
        Type::Map {
            mut fields,
            index,
            open,
        } => {
            let definitely_nil = value == Type::Nil;
            let may_delete = type_may_be_nil(&value);
            if !definitely_nil && !may_delete {
                if let Some((_, existing)) = fields.iter_mut().find(|(name, _)| name == field) {
                    *existing = value;
                } else {
                    fields.push((field.to_owned(), value));
                }
            } else {
                fields.retain(|(name, _)| name != field);
            }
            Type::Map {
                fields,
                index,
                open: open || (may_delete && !definitely_nil),
            }
        }
        other => other,
    }
}
pub(super) fn has_mutable_category(ty: &Type) -> bool {
    match ty {
        Type::ListExact(_) | Type::ListRest(_) | Type::Map { .. } => true,
        Type::Union(items) => items.iter().any(has_mutable_category),
        _ => false,
    }
}
pub(super) fn valid_post_transition(pre: &Type, post: &Type) -> bool {
    match post {
        Type::Union(items) => items.iter().all(|item| valid_post_transition(pre, item)),
        _ => match pre {
            Type::Any | Type::Unknown | Type::Generic(_) | Type::Infer(_) => true,
            Type::Union(items) => items.iter().any(|item| valid_post_transition(item, post)),
            Type::ListExact(_) | Type::ListRest(_) => {
                matches!(post, Type::ListExact(_) | Type::ListRest(_))
            }
            Type::Map { .. } => matches!(post, Type::Map { .. }),
            _ => is_subtype(post, pre),
        },
    }
}
