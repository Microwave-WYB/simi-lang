use super::*;

impl Context<'_> {
    pub(super) fn constrain(&mut self, expected: &Type, actual: &Type, at: Span) {
        let expected = self.resolve_type(expected.clone());
        let actual = self.resolve_type(actual.clone());
        match (&expected, &actual) {
            (Type::Function(_), Type::Any | Type::Unknown) => {
                let mut infers = HashSet::new();
                collect_infers(&expected, &mut infers);
                for id in infers {
                    self.bind_infer(Type::Infer(id), Type::Any);
                }
            }
            (Type::Any | Type::Unknown, _) | (_, Type::Any | Type::Unknown) => {}
            (Type::Infer(id), _) => {
                if let Some(bound) = self.vars[*id as usize].bound.clone() {
                    self.require_subtype(&actual, &bound, at);
                }
                self.bind_infer(expected, actual);
            }
            (_, Type::Infer(id)) => {
                if let Some(bound) = self.vars[*id as usize].bound.clone() {
                    self.require_subtype(&expected, &bound, at);
                }
                self.bind_infer(actual, expected);
            }
            (Type::ListRest(expected), Type::ListExact(actual)) => {
                self.constrain(expected, &union(actual.clone()), at);
            }
            (Type::ListRest(_), Type::ListRest(actual)) if **actual == Type::Never => {}
            (Type::ListRest(expected), Type::ListRest(actual)) => {
                self.constrain(expected, actual, at);
            }
            (Type::ListExact(expected), Type::ListExact(actual))
                if expected.len() == actual.len() =>
            {
                for (expected, actual) in expected.iter().zip(actual) {
                    self.constrain(expected, actual, at);
                }
            }
            (Type::Function(expected), Type::Function(actual))
                if expected.parameters.len() == actual.parameters.len() =>
            {
                for (expected, actual) in expected.parameters.iter().zip(&actual.parameters) {
                    if contains_infer(&expected.ty) {
                        self.constrain(&expected.ty, &actual.ty, at);
                    } else if !is_subtype(&expected.ty, &actual.ty) {
                        self.require_subtype(&expected.ty, &actual.ty, at);
                    }
                }
                self.constrain(&expected.result, &actual.result, at);
                self.constrain(&expected.raised, &actual.raised, at);
                if expected.constraints.len() != actual.constraints.len()
                    || !actual.constraints.iter().zip(&expected.constraints).all(
                        |(actual, expected)| match (&actual.bound, &expected.bound) {
                            (None, _) => true,
                            (Some(actual), Some(expected)) => is_subtype(expected, actual),
                            (Some(actual), None) => *actual == Type::Any,
                        },
                    )
                {
                    self.require_subtype(
                        &Type::Function(actual.clone()),
                        &Type::Function(expected.clone()),
                        at,
                    );
                }
            }
            (
                Type::Map {
                    fields: expected_fields,
                    index: expected_index,
                    ..
                },
                Type::Map {
                    fields: actual_fields,
                    index: actual_index,
                    ..
                },
            ) => {
                for (name, expected) in expected_fields {
                    if let Some((_, actual)) = actual_fields
                        .iter()
                        .find(|(actual_name, _)| actual_name == name)
                    {
                        self.constrain(expected, actual, at);
                    }
                }
                if let (Some((expected_key, expected_value)), Some((actual_key, actual_value))) =
                    (expected_index, actual_index)
                {
                    self.constrain(expected_key, actual_key, at);
                    self.constrain(expected_value, actual_value, at);
                }
                let expected = self.resolve_type(expected.clone());
                self.require_subtype(&actual, &expected, at);
            }
            (_, Type::Union(actual)) => {
                for actual in actual {
                    self.constrain(&expected, actual, at);
                }
            }
            (Type::Union(expected), _) => {
                let concrete = union(
                    expected
                        .iter()
                        .filter(|item| !contains_infer(item))
                        .cloned()
                        .collect(),
                );
                if !matches!(concrete, Type::Unknown) && is_subtype(&actual, &concrete) {
                    return;
                }
                if let Some(variable) = expected.iter().find(|item| {
                    contains_infer(item) && is_subtype(&actual, &public_type((*item).clone()))
                }) {
                    self.constrain(variable, &actual, at);
                } else if let Some(variable) = expected.iter().find(|item| contains_infer(item)) {
                    self.constrain(variable, &actual, at);
                } else {
                    self.require_subtype(&actual, &Type::Union(expected.clone()), at);
                }
            }
            _ => self.require_subtype(&actual, &expected, at),
        }
    }
    pub(super) fn finalize_deferred_empty_lists(
        &mut self,
        variables: impl IntoIterator<Item = u32>,
    ) {
        for variable in variables {
            if self.deferred_empty_list_infers.contains(&variable)
                && matches!(self.resolve_type(Type::Infer(variable)), Type::Infer(_))
            {
                self.exact_empty_list_infers.insert(variable);
            }
        }
    }
    pub(super) fn finalize_deferred_empty_maps(
        &mut self,
        variables: impl IntoIterator<Item = (u32, u32)>,
    ) {
        for (key, value) in variables {
            if matches!(self.resolve_type(Type::Infer(key)), Type::Infer(_))
                || matches!(self.resolve_type(Type::Infer(value)), Type::Infer(_))
            {
                self.exact_empty_map_infers.extend([key, value]);
            }
        }
    }
    pub(super) fn bind_infer(&mut self, variable: Type, ty: Type) {
        let Type::Infer(id) = variable else {
            return;
        };
        let resolved = self.resolve_type(ty);
        if resolved == Type::Infer(id) {
            return;
        }
        let resolved = remove_recursive_alternatives(resolved, id);
        if let Some(state) = self.vars.get_mut(id as usize) {
            state.binding = Some(match state.binding.take() {
                Some(existing) => union(vec![existing, resolved]),
                None => resolved,
            });
        }
    }
    pub(super) fn resolve_type(&self, ty: Type) -> Type {
        self.resolve_type_inner(ty, &mut HashSet::new())
    }
    pub(super) fn resolve_type_inner(&self, ty: Type, resolving: &mut HashSet<u32>) -> Type {
        match ty {
            Type::Infer(id) => {
                let Some(binding) = self
                    .vars
                    .get(id as usize)
                    .and_then(|state| state.binding.clone())
                else {
                    return Type::Infer(id);
                };
                if !resolving.insert(id) {
                    return Type::Never;
                }
                let resolved = self.resolve_type_inner(binding, resolving);
                resolving.remove(&id);
                resolved
            }
            Type::ListExact(items) => Type::ListExact(
                items
                    .into_iter()
                    .map(|item| self.resolve_type_inner(item, resolving))
                    .collect(),
            ),
            Type::ListRest(item) => {
                let deferred_empty = match item.as_ref() {
                    Type::Infer(id) => self.exact_empty_list_infers.contains(id),
                    _ => false,
                };
                let item = self.resolve_type_inner(*item, resolving);
                if deferred_empty && matches!(item, Type::Infer(_)) {
                    Type::ListExact(Vec::new())
                } else {
                    Type::ListRest(Box::new(item))
                }
            }
            Type::Map {
                fields,
                index,
                open,
            } => {
                let deferred_empty = index.as_ref().is_some_and(|(key, value)| {
                    matches!(key.as_ref(), Type::Infer(id) if self.exact_empty_map_infers.contains(id))
                        && matches!(value.as_ref(), Type::Infer(id) if self.exact_empty_map_infers.contains(id))
                });
                let fields = fields
                    .into_iter()
                    .map(|(name, ty)| (name, self.resolve_type_inner(ty, resolving)))
                    .collect();
                let index = (!deferred_empty)
                    .then(|| {
                        index.map(|(key, value)| {
                            (
                                Box::new(self.resolve_type_inner(*key, resolving)),
                                Box::new(self.resolve_type_inner(*value, resolving)),
                            )
                        })
                    })
                    .flatten();
                Type::Map {
                    fields,
                    index,
                    open,
                }
            }
            Type::Function(mut callable) => {
                for constraint in &mut callable.constraints {
                    constraint.variable =
                        self.resolve_type_inner(constraint.variable.clone(), resolving);
                    constraint.bound = constraint
                        .bound
                        .take()
                        .map(|bound| self.resolve_type_inner(bound, resolving));
                }
                for parameter in &mut callable.parameters {
                    parameter.ty = self.resolve_type_inner(parameter.ty.clone(), resolving);
                }
                callable.result = Box::new(self.resolve_type_inner(*callable.result, resolving));
                callable.raised = Box::new(self.resolve_type_inner(*callable.raised, resolving));
                Type::Function(callable)
            }
            Type::FunctionArgs(mut items) => {
                for item in &mut items {
                    item.ty = self.resolve_type_inner(item.ty.clone(), resolving);
                }
                Type::FunctionArgs(items)
            }
            Type::Union(items) => union(
                items
                    .into_iter()
                    .map(|item| self.resolve_type_inner(item, resolving))
                    .collect(),
            ),
            other => other,
        }
    }
    pub(super) fn generalize(&self, ty: Type) -> Type {
        self.generalize_excluding(ty, &HashSet::new())
    }
    pub(super) fn generalize_excluding(&self, ty: Type, excluded: &HashSet<u32>) -> Type {
        let resolved = self.resolve_type(ty);
        let mut next = max_generic(&resolved).map_or(0, |index| index + 1);
        let mut variables = HashMap::new();
        map_type(resolved, &mut |candidate| match candidate {
            Type::Infer(id) if !excluded.contains(&id) => {
                let generic = *variables.entry(id).or_insert_with(|| {
                    let generic = next;
                    next += 1;
                    generic
                });
                Type::Generic(generic)
            }
            other => other,
        })
    }
    pub(super) fn instantiate(&mut self, ty: Type) -> Type {
        let instantiated = instantiate_type(ty, self);
        self.install_constraint_bounds(&instantiated);
        instantiated
    }
    pub(super) fn require_subtype(&mut self, actual: &Type, expected: &Type, at: Span) {
        let actual = self.resolve_type(actual.clone());
        let expected = self.resolve_type(expected.clone());
        let checked_actual = self.upper_bound_view(actual.clone());
        if !is_subtype(&actual, &expected) && !is_subtype(&checked_actual, &expected) {
            let displayed_expected = self.display_deferred_empty_lists(expected.clone());
            let displayed_actual = self.display_deferred_empty_lists(actual.clone());
            self.diagnostic(
                AnalysisDiagnosticCode::TypeMismatch,
                "Type mismatch",
                format!(
                    "Expected `{}`, but found `{}`.",
                    displayed_expected.display(),
                    displayed_actual.display()
                ),
                at,
            );
        }
    }
    fn display_deferred_empty_lists(&self, ty: Type) -> Type {
        map_type(ty, &mut |candidate| match candidate {
            Type::ListRest(item) if matches!(item.as_ref(), Type::Infer(id) if self.deferred_empty_list_infers.contains(id)) => {
                Type::ListExact(Vec::new())
            }
            Type::Map {
                fields,
                index: Some((key, value)),
                open,
            } if matches!(key.as_ref(), Type::Infer(id) if self.deferred_empty_map_infers.contains(id))
                && matches!(value.as_ref(), Type::Infer(id) if self.deferred_empty_map_infers.contains(id)) =>
            {
                Type::Map {
                    fields,
                    index: None,
                    open,
                }
            }
            other => other,
        })
    }
    pub(super) fn invalid_operator(&mut self, at: Span, left: &Type, right: Option<&Type>) {
        let detail = right.map_or_else(
            || format!("The operator does not accept `{}`.", left.display()),
            |right| {
                format!(
                    "The operator does not accept `{}` and `{}`.",
                    left.display(),
                    right.display()
                )
            },
        );
        self.diagnostic(
            AnalysisDiagnosticCode::InvalidOperator,
            "Invalid operator operands",
            detail,
            at,
        );
    }
}
