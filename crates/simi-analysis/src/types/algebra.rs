use super::*;

pub(super) fn public_type(ty: Type) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Unknown | Type::Infer(_) => Type::Any,
        Type::FunctionArgs(_) => Type::Any,
        other => other,
    })
}
pub(super) fn generalize_type(ty: Type, variables: &mut HashMap<u32, u32>, next: &mut u32) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Infer(id) => {
            let generic = *variables.entry(id).or_insert_with(|| {
                let generic = *next;
                *next += 1;
                generic
            });
            Type::Generic(generic)
        }
        other => other,
    })
}
pub(super) fn max_generic(ty: &Type) -> Option<u32> {
    match ty {
        Type::Generic(id) => Some(*id),
        Type::ListExact(items) | Type::Union(items) => items.iter().filter_map(max_generic).max(),
        Type::FunctionArgs(items) => items.iter().filter_map(|item| max_generic(&item.ty)).max(),
        Type::ListRest(item) => max_generic(item),
        Type::Map { fields, index, .. } => fields
            .iter()
            .filter_map(|(_, ty)| max_generic(ty))
            .chain(
                index
                    .iter()
                    .flat_map(|(key, value)| [max_generic(key), max_generic(value)])
                    .flatten(),
            )
            .max(),
        Type::Function(callable) => callable
            .constraints
            .iter()
            .filter_map(|constraint| max_generic(&constraint.variable))
            .chain(
                callable
                    .constraints
                    .iter()
                    .filter_map(|constraint| constraint.bound.as_ref().and_then(max_generic)),
            )
            .chain(
                callable
                    .parameters
                    .iter()
                    .filter_map(|parameter| max_generic(&parameter.ty)),
            )
            .chain(max_generic(&callable.result))
            .chain(max_generic(&callable.raised))
            .max(),
        _ => None,
    }
}
pub(super) fn instantiate_type(ty: Type, context: &mut Context<'_>) -> Type {
    let mut targets = HashSet::new();
    collect_instantiable_generics(&ty, &HashSet::new(), true, &mut targets);
    let mut variables = HashMap::new();
    for id in targets {
        variables.insert(id, context.fresh());
    }
    map_type(ty, &mut |ty| match ty {
        Type::Generic(id) => variables.get(&id).cloned().unwrap_or(Type::Generic(id)),
        other => other,
    })
}
pub(super) fn collect_instantiable_generics(
    ty: &Type,
    protected: &HashSet<u32>,
    root: bool,
    targets: &mut HashSet<u32>,
) {
    match ty {
        Type::Generic(id) => {
            if !protected.contains(id) {
                targets.insert(*id);
            }
        }
        Type::ListExact(items) | Type::Union(items) => {
            for item in items {
                collect_instantiable_generics(item, protected, false, targets);
            }
        }
        Type::FunctionArgs(parameters) => {
            for parameter in parameters {
                collect_instantiable_generics(&parameter.ty, protected, false, targets);
            }
        }
        Type::ListRest(item) => {
            collect_instantiable_generics(item, protected, false, targets);
        }
        Type::Map { fields, index, .. } => {
            for (_, value) in fields {
                collect_instantiable_generics(value, protected, false, targets);
            }
            if let Some((key, value)) = index {
                collect_instantiable_generics(key, protected, false, targets);
                collect_instantiable_generics(value, protected, false, targets);
            }
        }
        Type::Function(callable) => {
            let local = callable
                .constraints
                .iter()
                .filter_map(|constraint| match constraint.variable {
                    Type::Generic(id) => Some(id),
                    _ => None,
                })
                .collect::<HashSet<_>>();
            let mut nested_protected = protected.clone();
            if root {
                targets.extend(local.iter().copied());
            } else {
                nested_protected.extend(local);
            }
            for constraint in &callable.constraints {
                if let Some(bound) = &constraint.bound {
                    collect_instantiable_generics(bound, &nested_protected, false, targets);
                }
            }
            for parameter in &callable.parameters {
                collect_instantiable_generics(&parameter.ty, &nested_protected, false, targets);
            }
            collect_instantiable_generics(&callable.result, &nested_protected, false, targets);
            collect_instantiable_generics(&callable.raised, &nested_protected, false, targets);
        }
        _ => {}
    }
}
pub(super) fn substitute_generics(ty: Type, replacements: &HashMap<u32, Type>) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Generic(id) => replacements.get(&id).cloned().unwrap_or(Type::Generic(id)),
        other => other,
    })
}
pub(super) fn map_type(ty: Type, mapper: &mut impl FnMut(Type) -> Type) -> Type {
    let mapped = match ty {
        Type::ListExact(items) => Type::ListExact(
            items
                .into_iter()
                .map(|item| map_type(item, mapper))
                .collect(),
        ),
        Type::ListRest(item) => Type::ListRest(Box::new(map_type(*item, mapper))),
        Type::Map {
            fields,
            index,
            open,
        } => Type::Map {
            fields: fields
                .into_iter()
                .map(|(name, ty)| (name, map_type(ty, mapper)))
                .collect(),
            index: index.map(|(key, value)| {
                (
                    Box::new(map_type(*key, mapper)),
                    Box::new(map_type(*value, mapper)),
                )
            }),
            open,
        },
        Type::Function(mut callable) => {
            for constraint in &mut callable.constraints {
                constraint.variable = map_type(constraint.variable.clone(), mapper);
                constraint.bound = constraint.bound.take().map(|bound| map_type(bound, mapper));
            }
            for parameter in &mut callable.parameters {
                parameter.ty = map_type(parameter.ty.clone(), mapper);
            }
            callable.result = Box::new(map_type(*callable.result, mapper));
            callable.raised = Box::new(map_type(*callable.raised, mapper));
            Type::Function(callable)
        }
        Type::FunctionArgs(mut items) => {
            for item in &mut items {
                item.ty = map_type(item.ty.clone(), mapper);
            }
            Type::FunctionArgs(items)
        }
        Type::Union(items) => union(
            items
                .into_iter()
                .map(|item| map_type(item, mapper))
                .collect(),
        ),
        other => other,
    };
    mapper(mapped)
}
pub(super) fn contains_specific_infer(ty: &Type, target: u32) -> bool {
    match ty {
        Type::Infer(id) => *id == target,
        Type::ListExact(items) | Type::Union(items) => items
            .iter()
            .any(|item| contains_specific_infer(item, target)),
        Type::FunctionArgs(items) => items
            .iter()
            .any(|item| contains_specific_infer(&item.ty, target)),
        Type::ListRest(item) => contains_specific_infer(item, target),
        Type::Map { fields, index, .. } => {
            fields
                .iter()
                .any(|(_, ty)| contains_specific_infer(ty, target))
                || index.as_ref().is_some_and(|(key, value)| {
                    contains_specific_infer(key, target) || contains_specific_infer(value, target)
                })
        }
        Type::Function(callable) => {
            callable.constraints.iter().any(|constraint| {
                contains_specific_infer(&constraint.variable, target)
                    || constraint
                        .bound
                        .as_ref()
                        .is_some_and(|bound| contains_specific_infer(bound, target))
            }) || callable
                .parameters
                .iter()
                .any(|parameter| contains_specific_infer(&parameter.ty, target))
                || contains_specific_infer(&callable.result, target)
                || contains_specific_infer(&callable.raised, target)
        }
        _ => false,
    }
}
pub(super) fn remove_recursive_alternatives(ty: Type, target: u32) -> Type {
    match ty {
        Type::Union(items) => union(
            items
                .into_iter()
                .map(|item| {
                    if contains_specific_infer(&item, target) {
                        Type::Never
                    } else {
                        item
                    }
                })
                .collect(),
        ),
        other if contains_specific_infer(&other, target) => Type::Never,
        other => other,
    }
}
pub(super) fn contains_infer(ty: &Type) -> bool {
    match ty {
        Type::Infer(_) => true,
        Type::ListExact(items) | Type::Union(items) => items.iter().any(contains_infer),
        Type::FunctionArgs(items) => items.iter().any(|item| contains_infer(&item.ty)),
        Type::ListRest(item) => contains_infer(item),
        Type::Map { fields, index, .. } => {
            fields.iter().any(|(_, ty)| contains_infer(ty))
                || index
                    .as_ref()
                    .is_some_and(|(key, value)| contains_infer(key) || contains_infer(value))
        }
        Type::Function(callable) => {
            callable.constraints.iter().any(|constraint| {
                contains_infer(&constraint.variable)
                    || constraint.bound.as_ref().is_some_and(contains_infer)
            }) || callable
                .parameters
                .iter()
                .any(|parameter| contains_infer(&parameter.ty))
                || contains_infer(&callable.result)
                || contains_infer(&callable.raised)
        }
        _ => false,
    }
}
pub(super) fn collect_constraint_bounds(ty: &Type, bounds: &mut Vec<(u32, Type)>) {
    match ty {
        Type::Function(callable) => {
            for constraint in &callable.constraints {
                if let (Type::Infer(id), Some(bound)) = (&constraint.variable, &constraint.bound) {
                    bounds.push((*id, bound.clone()));
                }
            }
            for parameter in &callable.parameters {
                collect_constraint_bounds(&parameter.ty, bounds);
            }
            collect_constraint_bounds(&callable.result, bounds);
            collect_constraint_bounds(&callable.raised, bounds);
        }
        Type::Union(items) | Type::ListExact(items) => {
            for item in items {
                collect_constraint_bounds(item, bounds);
            }
        }
        Type::FunctionArgs(parameters) => {
            for parameter in parameters {
                collect_constraint_bounds(&parameter.ty, bounds);
            }
        }
        Type::ListRest(item) => collect_constraint_bounds(item, bounds),
        Type::Map { fields, index, .. } => {
            for (_, value) in fields {
                collect_constraint_bounds(value, bounds);
            }
            if let Some((key, value)) = index {
                collect_constraint_bounds(key, bounds);
                collect_constraint_bounds(value, bounds);
            }
        }
        _ => {}
    }
}
pub(super) fn collect_infers(ty: &Type, infers: &mut HashSet<u32>) {
    let _ = map_type(ty.clone(), &mut |candidate| {
        if let Type::Infer(id) = candidate {
            infers.insert(id);
            Type::Infer(id)
        } else {
            candidate
        }
    });
}
pub(super) fn list_append_result(list: Type, value: Type) -> Type {
    match list {
        Type::ListExact(mut items) => {
            items.push(value);
            Type::ListExact(items)
        }
        Type::ListRest(item) => Type::ListRest(Box::new(union(vec![*item, value]))),
        _ => Type::ListRest(Box::new(Type::Unknown)),
    }
}

pub(super) fn merge_callable(left: &CallableType, right: &CallableType) -> Option<CallableType> {
    if left.constraints != right.constraints
        || left.parameters.len() != right.parameters.len()
        || left
            .parameters
            .iter()
            .zip(&right.parameters)
            .any(|(left, right)| left.ty != right.ty)
        || left.result != right.result
    {
        return None;
    }
    let raised = union(vec![(*left.raised).clone(), (*right.raised).clone()]);
    let raised_annotation = if raised == Type::Never
        && left.raised_annotation == RaisedAnnotation::NoRaise
        && right.raised_annotation == RaisedAnnotation::NoRaise
    {
        RaisedAnnotation::NoRaise
    } else {
        RaisedAnnotation::Inferred
    };
    Some(CallableType {
        constraints: left.constraints.clone(),
        parameters: left
            .parameters
            .iter()
            .zip(&right.parameters)
            .map(|(left, right)| CallableParameter {
                name: (left.name == right.name)
                    .then(|| left.name.clone())
                    .flatten(),
                ty: left.ty.clone(),
            })
            .collect(),
        result: left.result.clone(),
        raised: Box::new(raised),
        raised_annotation,
    })
}
pub(super) fn union(items: Vec<Type>) -> Type {
    let mut flattened = Vec::new();
    let mut terminated = false;
    let mut pending = items.into_iter().rev().collect::<Vec<_>>();
    while let Some(item) = pending.pop() {
        match item {
            Type::Union(items) => pending.extend(items.into_iter().rev()),
            Type::Never => terminated = true,
            Type::Any => return Type::Any,
            item => flattened.push(item),
        }
    }
    let has_true = flattened.contains(&Type::LiteralBoolean(true));
    let has_false = flattened.contains(&Type::LiteralBoolean(false));
    if has_true && has_false {
        flattened.retain(|item| !matches!(item, Type::LiteralBoolean(_)));
        flattened.push(Type::Boolean);
    }
    let mut merged = Vec::new();
    for item in flattened {
        if let Type::Function(candidate) = &item
            && let Some((index, joined)) =
                merged.iter().enumerate().find_map(|(index, existing)| {
                    let Type::Function(existing) = existing else {
                        return None;
                    };
                    merge_callable(existing, candidate).map(|joined| (index, joined))
                })
        {
            merged[index] = Type::Function(Box::new(joined));
        } else {
            merged.push(item);
        }
    }
    let mut unique = Vec::new();
    for item in merged {
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    unique.sort_by_key(type_order);
    let snapshot = unique.clone();
    unique.retain(|item| {
        !snapshot
            .iter()
            .any(|other| item != other && is_subtype(item, other))
    });
    match unique.as_slice() {
        [] if terminated => Type::Never,
        [] => Type::Unknown,
        [one] => one.clone(),
        _ => Type::Union(unique),
    }
}
pub(super) fn type_order(ty: &Type) -> u8 {
    match ty {
        Type::Never => 0,
        Type::Boolean | Type::LiteralBoolean(_) => 1,
        Type::Int | Type::LiteralInt(_) => 2,
        Type::Float | Type::LiteralFloat(_) => 3,
        Type::String | Type::LiteralString(_) => 4,
        Type::Bytes => 5,
        Type::ListExact(_) | Type::ListRest(_) => 6,
        Type::Map { .. } => 7,
        Type::Named(_) => 8,
        Type::Function(_) => 9,
        Type::Generic(_) | Type::Infer(_) => 10,
        Type::Nil => 11,
        Type::Unknown => 12,
        Type::Any => 13,
        Type::FunctionArgs(_) | Type::Union(_) => 14,
    }
}
pub(super) fn type_contains_singleton(ty: &Type) -> bool {
    match ty {
        Type::Nil
        | Type::LiteralBoolean(_)
        | Type::LiteralInt(_)
        | Type::LiteralFloat(_)
        | Type::LiteralString(_) => true,
        Type::Union(items) => items.iter().any(type_contains_singleton),
        _ => false,
    }
}
pub(super) fn type_contains_exact(ty: &Type, expected: &Type) -> bool {
    ty == expected
        || matches!(ty, Type::Union(items) if items.iter().any(|item| type_contains_exact(item, expected)))
}
pub(super) fn equality_type(ty: &Type) -> bool {
    match ty {
        Type::Unknown
        | Type::Any
        | Type::Nil
        | Type::Boolean
        | Type::Int
        | Type::Float
        | Type::String
        | Type::Bytes
        | Type::LiteralInt(_)
        | Type::LiteralFloat(_)
        | Type::LiteralString(_)
        | Type::LiteralBoolean(_)
        | Type::Infer(_)
        | Type::Generic(_) => true,
        Type::Union(items) => items.iter().all(equality_type),
        _ => false,
    }
}
pub(super) fn numeric() -> Type {
    union(vec![Type::Int, Type::Float])
}
pub(super) fn numeric_atoms(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Int | Type::LiteralInt(_) => vec![Type::Int],
        Type::Float | Type::LiteralFloat(_) => vec![Type::Float],
        Type::Union(items) => items.iter().flat_map(numeric_atoms).collect(),
        _ => Vec::new(),
    }
}
pub(super) fn is_subtype(actual: &Type, expected: &Type) -> bool {
    if matches!(expected, Type::Any | Type::Unknown)
        || matches!(actual, Type::Never | Type::Unknown | Type::Any)
        || actual == expected
    {
        return true;
    }
    match (actual, expected) {
        (Type::LiteralInt(_), Type::Int) => true,
        (Type::LiteralFloat(_), Type::Float) => true,
        (Type::LiteralString(_), Type::String) => true,
        (Type::LiteralBoolean(_), Type::Boolean) => true,
        (Type::Union(items), expected) => items.iter().all(|item| is_subtype(item, expected)),
        (actual, Type::Union(items)) => items.iter().any(|item| is_subtype(actual, item)),
        (Type::ListExact(actual), Type::ListRest(expected)) => {
            actual.iter().all(|actual| is_subtype(actual, expected))
        }
        (Type::ListRest(actual), Type::ListRest(expected)) => is_subtype(actual, expected),
        (Type::ListExact(actual), Type::ListExact(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| is_subtype(actual, expected))
        }
        (
            Type::Map {
                fields: actual,
                index: actual_index,
                open: actual_open,
            },
            Type::Map {
                fields: expected,
                index: expected_index,
                open,
            },
        ) => {
            let fields_match = expected.iter().all(|(name, expected)| {
                actual
                    .iter()
                    .find(|(field, _)| field == name)
                    .is_some_and(|(_, actual)| is_subtype(actual, expected))
                    || type_may_be_nil(expected)
            });
            let index_matches = expected_index.as_ref().is_none_or(|(key, value)| {
                actual
                    .iter()
                    .filter(|(name, _)| !expected.iter().any(|(field, _)| field == name))
                    .all(|(_, actual)| is_subtype(&Type::String, key) && is_subtype(actual, value))
                    && actual_index
                        .as_ref()
                        .is_none_or(|(actual_key, actual_value)| {
                            is_subtype(actual_key, key) && is_subtype(actual_value, value)
                        })
            });
            fields_match
                && index_matches
                && (*open
                    || expected_index.is_some()
                    || (!*actual_open
                        && actual.len() <= expected.len()
                        && actual
                            .iter()
                            .all(|(name, _)| expected.iter().any(|(field, _)| field == name))))
        }
        (Type::Function(actual), Type::Function(expected)) => {
            actual.constraints.len() == expected.constraints.len()
                && actual
                    .constraints
                    .iter()
                    .zip(&expected.constraints)
                    .all(
                        |(actual, expected)| match (&actual.bound, &expected.bound) {
                            (None, _) => true,
                            (Some(actual), Some(expected)) => is_subtype(expected, actual),
                            (Some(actual), None) => *actual == Type::Any,
                        },
                    )
                && actual.parameters.len() == expected.parameters.len()
                && actual
                    .parameters
                    .iter()
                    .zip(&expected.parameters)
                    .all(|(actual, expected)| is_subtype(&expected.ty, &actual.ty))
                && is_subtype(&actual.result, &expected.result)
                && is_subtype(&actual.raised, &expected.raised)
        }
        _ => false,
    }
}
