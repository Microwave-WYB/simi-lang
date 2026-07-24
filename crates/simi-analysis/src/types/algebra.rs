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
        Type::FunctionArgs(items) => items
            .iter()
            .flat_map(|item| {
                [
                    max_generic(&item.ty),
                    item.post.as_ref().and_then(max_generic),
                ]
            })
            .flatten()
            .max(),
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
            .chain(callable.parameters.iter().flat_map(|parameter| {
                [
                    max_generic(&parameter.ty),
                    parameter.post.as_ref().and_then(max_generic),
                ]
                .into_iter()
                .flatten()
            }))
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
                if let Some(post) = &parameter.post {
                    collect_instantiable_generics(post, protected, false, targets);
                }
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
                if let Some(post) = &parameter.post {
                    collect_instantiable_generics(post, &nested_protected, false, targets);
                }
            }
            collect_instantiable_generics(&callable.result, &nested_protected, false, targets);
            collect_instantiable_generics(&callable.raised, &nested_protected, false, targets);
        }
        _ => {}
    }
}
pub(super) fn collect_generic_replacements(
    parameter: &Type,
    actual: &Type,
    replacements: &mut HashMap<u32, Type>,
) {
    match (parameter, actual) {
        (Type::Generic(id), actual) => {
            replacements
                .entry(*id)
                .and_modify(|existing| *existing = union(vec![existing.clone(), actual.clone()]))
                .or_insert_with(|| actual.clone());
        }
        (Type::ListRest(parameter), Type::ListExact(actuals)) => {
            for actual in actuals {
                collect_generic_replacements(parameter, actual, replacements);
            }
        }
        (Type::ListRest(_), Type::ListRest(actual)) if **actual == Type::Never => {}
        (Type::ListRest(parameter), Type::ListRest(actual)) => {
            collect_generic_replacements(parameter, actual, replacements);
        }
        (Type::ListExact(parameters), Type::ListExact(actuals)) => {
            for (parameter, actual) in parameters.iter().zip(actuals) {
                collect_generic_replacements(parameter, actual, replacements);
            }
        }
        (Type::Function(parameter), Type::Function(actual)) => {
            for (parameter, actual) in parameter.parameters.iter().zip(&actual.parameters) {
                collect_generic_replacements(&parameter.ty, &actual.ty, replacements);
                if let (Some(parameter), Some(actual)) = (&parameter.post, &actual.post) {
                    collect_generic_replacements(parameter, actual, replacements);
                }
            }
            collect_generic_replacements(&parameter.result, &actual.result, replacements);
            collect_generic_replacements(&parameter.raised, &actual.raised, replacements);
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
pub(super) fn substitute_post_generics(ty: Type, replacements: &HashMap<u32, Type>) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Generic(id) => replacements.get(&id).cloned().unwrap_or(Type::Never),
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
                parameter.post = parameter.post.take().map(|post| map_type(post, mapper));
            }
            callable.result = Box::new(map_type(*callable.result, mapper));
            callable.raised = Box::new(map_type(*callable.raised, mapper));
            Type::Function(callable)
        }
        Type::FunctionArgs(mut items) => {
            for item in &mut items {
                item.ty = map_type(item.ty.clone(), mapper);
                item.post = item.post.take().map(|post| map_type(post, mapper));
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
        Type::FunctionArgs(items) => items.iter().any(|item| {
            contains_specific_infer(&item.ty, target)
                || item
                    .post
                    .as_ref()
                    .is_some_and(|post| contains_specific_infer(post, target))
        }),
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
            }) || callable.parameters.iter().any(|parameter| {
                contains_specific_infer(&parameter.ty, target)
                    || parameter
                        .post
                        .as_ref()
                        .is_some_and(|post| contains_specific_infer(post, target))
            }) || contains_specific_infer(&callable.result, target)
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
        Type::FunctionArgs(items) => items
            .iter()
            .any(|item| contains_infer(&item.ty) || item.post.as_ref().is_some_and(contains_infer)),
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
            }) || callable.parameters.iter().any(|parameter| {
                contains_infer(&parameter.ty) || parameter.post.as_ref().is_some_and(contains_infer)
            }) || contains_infer(&callable.result)
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
                if let Some(post) = &parameter.post {
                    collect_constraint_bounds(post, bounds);
                }
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
                if let Some(post) = &parameter.post {
                    collect_constraint_bounds(post, bounds);
                }
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
pub(super) fn join_loop_state(current: Type, transition: Type) -> Type {
    if current == transition {
        return current;
    }
    match (current, transition) {
        (Type::Never, other) | (other, Type::Never) => other,
        (Type::ListExact(left), Type::ListExact(right)) if left.len() == right.len() => {
            Type::ListExact(
                left.into_iter()
                    .zip(right)
                    .map(|(left, right)| union(vec![left, right]))
                    .collect(),
            )
        }
        (Type::ListExact(left), Type::ListExact(right)) => {
            Type::ListRest(Box::new(union(left.into_iter().chain(right).collect())))
        }
        (Type::ListRest(left), Type::ListRest(right)) => {
            Type::ListRest(Box::new(union(vec![*left, *right])))
        }
        (Type::ListExact(items), Type::ListRest(item))
        | (Type::ListRest(item), Type::ListExact(items)) => Type::ListRest(Box::new(union(
            items.into_iter().chain(std::iter::once(*item)).collect(),
        ))),
        (
            Type::Map {
                fields: left_fields,
                index: left_index,
                open: left_open,
            },
            Type::Map {
                fields: right_fields,
                index: right_index,
                open: right_open,
            },
        ) => {
            let fields = left_fields
                .iter()
                .filter_map(|(name, left)| {
                    right_fields
                        .iter()
                        .find(|(field, _)| field == name)
                        .map(|(_, right)| {
                            (name.clone(), join_loop_state(left.clone(), right.clone()))
                        })
                })
                .collect();
            let same_fields = left_fields
                .iter()
                .all(|(name, _)| right_fields.iter().any(|(field, _)| field == name))
                && right_fields
                    .iter()
                    .all(|(name, _)| left_fields.iter().any(|(field, _)| field == name));
            let index = match (left_index, right_index) {
                (Some((left_key, left_value)), Some((right_key, right_value))) => Some((
                    Box::new(union(vec![*left_key, *right_key])),
                    Box::new(join_loop_state(*left_value, *right_value)),
                )),
                (Some(index), None) | (None, Some(index)) => Some(index),
                (None, None) => None,
            };
            Type::Map {
                fields,
                index,
                open: left_open || right_open || !same_fields,
            }
        }
        (left, right) => union(vec![left, right]),
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
                post: (left.post == right.post)
                    .then(|| left.post.clone())
                    .flatten(),
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
        Type::Float => 3,
        Type::String | Type::LiteralString(_) => 4,
        Type::ListExact(_) | Type::ListRest(_) => 5,
        Type::Map { .. } => 7,
        Type::Function(_) => 8,
        Type::Generic(_) | Type::Infer(_) => 9,
        Type::Nil => 10,
        Type::Unknown => 11,
        Type::Any => 12,
        Type::FunctionArgs(_) | Type::Union(_) => 13,
    }
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
        | Type::LiteralInt(_)
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
        Type::Int => vec![Type::Int],
        Type::Float => vec![Type::Float],
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
            });
            let index_matches = expected_index.as_ref().is_none_or(|(key, value)| {
                actual
                    .iter()
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
                    || (!*actual_open && actual.len() == expected.len()))
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
                    .all(|(actual, expected)| {
                        is_subtype(&expected.ty, &actual.ty)
                            && expected.post.as_ref().is_none_or(|expected_post| {
                                actual.post.as_ref().is_some_and(|actual_post| {
                                    is_subtype(actual_post, expected_post)
                                })
                            })
                    })
                && is_subtype(&actual.result, &expected.result)
                && is_subtype(&actual.raised, &expected.raised)
        }
        _ => false,
    }
}
pub(super) fn callable_post_scheme(ty: &Type) -> Option<(Vec<Type>, Vec<ParameterPostType>)> {
    let Type::Function(callable) = ty else {
        return None;
    };
    let posts = callable
        .parameters
        .iter()
        .enumerate()
        .filter_map(|(parameter_index, parameter)| {
            parameter.post.clone().map(|becomes| ParameterPostType {
                parameter_index,
                parameter_name: parameter
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("argument {}", parameter_index + 1)),
                becomes,
            })
        })
        .collect::<Vec<_>>();
    (!posts.is_empty()).then(|| {
        (
            callable
                .parameters
                .iter()
                .map(|parameter| parameter.ty.clone())
                .collect(),
            posts,
        )
    })
}
