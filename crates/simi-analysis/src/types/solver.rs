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
                for actual in actual {
                    self.constrain(expected, actual, at);
                }
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
                    if let Some(expected_post) = &expected.post {
                        if let Some(actual_post) = &actual.post {
                            self.constrain(expected_post, actual_post, at);
                        } else {
                            self.diagnostic(
                                AnalysisDiagnosticCode::TypeMismatch,
                                "Type mismatch",
                                format!(
                                    "Expected a callback post-state of `{}`, but the provided callback has no post-state guarantee.",
                                    expected_post.display()
                                ),
                                at,
                            );
                        }
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
                if let Some(variable) = expected.iter().find(|item| contains_infer(item)) {
                    self.constrain(variable, &actual, at);
                } else {
                    self.require_subtype(&actual, &Type::Union(expected.clone()), at);
                }
            }
            (_, Type::Union(actual)) => {
                for actual in actual {
                    self.constrain(&expected, actual, at);
                }
            }
            _ => self.require_subtype(&actual, &expected, at),
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
                Type::ListRest(Box::new(self.resolve_type_inner(*item, resolving)))
            }
            Type::Map {
                fields,
                index,
                open,
            } => Type::Map {
                fields: fields
                    .into_iter()
                    .map(|(name, ty)| (name, self.resolve_type_inner(ty, resolving)))
                    .collect(),
                index: index.map(|(key, value)| {
                    (
                        Box::new(self.resolve_type_inner(*key, resolving)),
                        Box::new(self.resolve_type_inner(*value, resolving)),
                    )
                }),
                open,
            },
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
                    parameter.post = parameter
                        .post
                        .take()
                        .map(|post| self.resolve_type_inner(post, resolving));
                }
                callable.result = Box::new(self.resolve_type_inner(*callable.result, resolving));
                callable.raised = Box::new(self.resolve_type_inner(*callable.raised, resolving));
                Type::Function(callable)
            }
            Type::FunctionArgs(mut items) => {
                for item in &mut items {
                    item.ty = self.resolve_type_inner(item.ty.clone(), resolving);
                    item.post = item
                        .post
                        .take()
                        .map(|post| self.resolve_type_inner(post, resolving));
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
        let resolved = self.resolve_type(ty);
        let mut next = max_generic(&resolved).map_or(0, |index| index + 1);
        let mut variables = HashMap::new();
        generalize_type(resolved, &mut variables, &mut next)
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
            self.diagnostic(
                AnalysisDiagnosticCode::TypeMismatch,
                "Type mismatch",
                format!(
                    "Expected `{}`, but found `{}`.",
                    expected.display(),
                    actual.display()
                ),
                at,
            );
        }
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
