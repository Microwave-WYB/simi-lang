use super::*;

impl Context<'_> {
    pub(super) fn bind_pattern(&mut self, pattern: syntax::Pattern, ty: Type) {
        self.bind_pattern_with_mode(pattern, ty, false);
    }

    pub(super) fn bind_let_pattern(&mut self, pattern: syntax::Pattern, ty: Type) {
        self.bind_pattern_with_mode(pattern, ty, true);
    }

    fn bind_pattern_with_mode(
        &mut self,
        pattern: syntax::Pattern,
        ty: Type,
        let_destructure: bool,
    ) {
        self.pattern_types
            .push((span(pattern.syntax()), ty.clone()));
        match pattern {
            syntax::Pattern::Binding(node) => {
                if let Some(token) = direct_token(node.syntax(), K::IDENT)
                    && let Some(symbol) = self.resolution.symbol_at(token_span(&token).start)
                {
                    self.symbol_types.insert(symbol, ty.clone());
                    self.symbol_bounds.insert(symbol, ty);
                }
            }
            syntax::Pattern::List(node) => {
                let resolved = self.resolve_type(ty);
                let children =
                    support::children::<syntax::Pattern>(node.syntax()).collect::<Vec<_>>();
                for (index, child) in children.iter().cloned().enumerate() {
                    let item = match &resolved {
                        Type::ListExact(items) => {
                            items.get(index).cloned().unwrap_or(Type::Unknown)
                        }
                        Type::ListRest(item) => (**item).clone(),
                        _ => Type::Unknown,
                    };
                    self.bind_pattern_with_mode(child, item, let_destructure);
                }
                if let Some(rest) = support::child::<syntax::RestPattern>(node.syntax())
                    && let Some(token) = direct_token(rest.syntax(), K::IDENT)
                    && let Some(symbol) = self.resolution.symbol_at(token_span(&token).start)
                {
                    let rest_ty = match resolved {
                        Type::ListExact(items) => {
                            Type::ListExact(items.into_iter().skip(children.len()).collect())
                        }
                        Type::ListRest(item) => Type::ListRest(item),
                        _ => Type::Unknown,
                    };
                    self.symbol_types.insert(symbol, rest_ty.clone());
                    self.symbol_bounds.insert(symbol, rest_ty);
                }
            }
            syntax::Pattern::Map(node) => {
                let resolved = self.resolve_type(ty);
                let (fields, index, open, is_map) = match &resolved {
                    Type::Map {
                        fields,
                        index,
                        open,
                    } => (fields.clone(), index.clone(), *open, true),
                    _ => (Vec::new(), None, false, false),
                };
                for field in support::children::<syntax::MapPatternField>(node.syntax()) {
                    let Some(name) = direct_token(field.syntax(), K::IDENT) else {
                        continue;
                    };
                    let child = support::child::<syntax::Pattern>(field.syntax());
                    let direct_binding = child
                        .as_ref()
                        .is_none_or(|child| matches!(child, syntax::Pattern::Binding(_)));
                    let field_ty = fields
                        .iter()
                        .find(|(field, _)| field == name.text())
                        .map(|(_, ty)| ty.clone())
                        .unwrap_or_else(|| {
                            if direct_binding && is_map {
                                let possible = index
                                    .as_ref()
                                    .map(|(_, ty)| (**ty).clone())
                                    .unwrap_or(Type::Any);
                                if let_destructure {
                                    if open || index.is_some() {
                                        union(vec![possible, Type::Nil])
                                    } else {
                                        Type::Nil
                                    }
                                } else if open || index.is_some() {
                                    possible
                                } else {
                                    Type::Unknown
                                }
                            } else {
                                Type::Unknown
                            }
                        });
                    if let Some(child) = child {
                        self.bind_pattern_with_mode(child, field_ty, let_destructure);
                    } else if let Some(symbol) = self.resolution.symbol_at(token_span(&name).start)
                    {
                        self.symbol_types.insert(symbol, field_ty.clone());
                        self.symbol_bounds.insert(symbol, field_ty);
                    }
                }
            }
            _ => {}
        }
    }
    pub(super) fn parse_callable_constraints(
        &mut self,
        node: &SyntaxNode,
        generics: &mut HashMap<String, u32>,
    ) -> Vec<GenericConstraint> {
        let mut entries = Vec::<(String, Option<SyntaxNode>)>::new();
        for child in node.children() {
            match child.kind() {
                K::TYPE_VARIABLE => {
                    let name = direct_token(&child, K::IDENT)
                        .map(|token| token.text().to_owned())
                        .unwrap_or_default();
                    entries.push((name, None));
                }
                K::TYPE_CONSTRAINT => {
                    if let Some((_, constraint)) = entries.last_mut() {
                        *constraint = Some(child);
                    }
                }
                _ => {}
            }
        }
        for (name, _) in &entries {
            let next = generics.values().copied().max().map_or(0, |id| id + 1);
            generics.insert(name.clone(), next);
        }
        entries
            .into_iter()
            .filter_map(|(name, constraint)| {
                let variable = Type::Generic(*generics.get(&name)?);
                let bound = constraint
                    .and_then(|constraint| support::child::<syntax::TypeExpr>(&constraint))
                    .map(|bound| self.parse_type(bound.syntax(), generics));
                Some(GenericConstraint { variable, bound })
            })
            .collect()
    }
    pub(super) fn parse_effect_annotation(
        &mut self,
        parent: &SyntaxNode,
        generics: &mut HashMap<String, u32>,
    ) -> (Type, RaisedAnnotation) {
        let Some(effect) = support::child::<syntax::EffectAnnotation>(parent) else {
            return (self.fresh(), RaisedAnnotation::Inferred);
        };
        let raised = support::child::<syntax::TypeExpr>(effect.syntax())
            .map(|ty| self.parse_type(ty.syntax(), generics))
            .unwrap_or(Type::Unknown);
        let annotation = if raised == Type::Never {
            RaisedAnnotation::NoRaise
        } else {
            RaisedAnnotation::Explicit
        };
        (raised, annotation)
    }
    pub(super) fn parse_type(
        &mut self,
        node: &SyntaxNode,
        generics: &mut HashMap<String, u32>,
    ) -> Type {
        match node.kind() {
            K::TYPE_EXPR => child_node(node)
                .map(|child| self.parse_type(&child, generics))
                .unwrap_or(Type::Unknown),
            K::TYPE_UNION => union(
                node.children()
                    .map(|child| self.parse_type(&child, generics))
                    .collect(),
            ),
            K::TYPE_FUNCTION => {
                let mut scoped_generics = generics.clone();
                let header = support::child::<syntax::CallableTypeParamList>(node);
                let constraints = header
                    .as_ref()
                    .map(|header| {
                        self.parse_callable_constraints(header.syntax(), &mut scoped_generics)
                    })
                    .unwrap_or_default();
                let active_generics = if header.is_some() {
                    &mut scoped_generics
                } else {
                    generics
                };
                let left = support::child::<syntax::TypeUnion>(node)
                    .map(|child| self.parse_type(child.syntax(), active_generics))
                    .unwrap_or(Type::Unknown);
                if let Some(right) = support::child::<syntax::TypeFunction>(node) {
                    let parameters = match left {
                        Type::FunctionArgs(items) => items,
                        other => vec![CallableParameter {
                            name: None,
                            ty: other,
                        }],
                    };
                    let (raised, raised_annotation) =
                        self.parse_effect_annotation(node, active_generics);
                    Type::Function(Box::new(CallableType {
                        constraints,
                        parameters,
                        result: Box::new(self.parse_type(right.syntax(), active_generics)),
                        raised: Box::new(raised),
                        raised_annotation,
                    }))
                } else if matches!(left, Type::FunctionArgs(_)) {
                    self.diagnostic(
                        AnalysisDiagnosticCode::InvalidType,
                        "Invalid type",
                        "Parenthesized type lists are only valid as function parameters."
                            .to_owned(),
                        span(node),
                    );
                    Type::Unknown
                } else {
                    left
                }
            }
            K::TYPE_NAME => {
                let name = direct_token(node, K::IDENT)
                    .map(|token| token.text().to_owned())
                    .unwrap_or_default();
                let arguments = support::child::<syntax::TypeArgumentList>(node)
                    .map(|list| {
                        support::children::<syntax::TypeExpr>(list.syntax())
                            .map(|ty| self.parse_type(ty.syntax(), generics))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                match name.as_str() {
                    "never" => Type::Never,
                    "nil" => Type::Nil,
                    "boolean" => Type::Boolean,
                    "integer" => Type::Int,
                    "float" => Type::Float,
                    "string" => Type::String,
                    "any" => Type::Any,
                    _ => self.expand_alias(&name, arguments, generics, span(node)),
                }
            }
            K::TYPE_VARIABLE => {
                let name = direct_token(node, K::IDENT)
                    .map(|token| token.text().to_owned())
                    .unwrap_or_default();
                let next = generics.values().copied().max().map_or(0, |id| id + 1);
                Type::Generic(*generics.entry(name).or_insert(next))
            }
            K::TYPE_LITERAL => type_literal_type(node),
            K::TYPE_PAREN => {
                let items = support::children::<syntax::TypeFunctionParam>(node)
                    .filter_map(|parameter| {
                        let ty = support::child::<syntax::TypeExpr>(parameter.syntax())
                            .map(|ty| self.parse_type(ty.syntax(), generics))?;
                        Some(CallableParameter {
                            name: direct_token(parameter.syntax(), K::IDENT)
                                .map(|token| token.text().to_owned()),
                            ty,
                        })
                    })
                    .collect::<Vec<_>>();
                match items.as_slice() {
                    [one] if one.name.is_none() => one.ty.clone(),
                    _ => Type::FunctionArgs(items),
                }
            }
            K::TYPE_LIST => {
                if let Some(rest) = support::child::<syntax::TypeListRest>(node) {
                    let item = support::child::<syntax::TypeExpr>(rest.syntax())
                        .map(|ty| self.parse_type(ty.syntax(), generics))
                        .unwrap_or(Type::Unknown);
                    Type::ListRest(Box::new(item))
                } else {
                    Type::ListExact(
                        support::children::<syntax::TypeExpr>(node)
                            .map(|ty| self.parse_type(ty.syntax(), generics))
                            .collect(),
                    )
                }
            }
            K::TYPE_MAP => {
                let fields = support::children::<syntax::TypeMapEntry>(node)
                    .filter_map(|entry| {
                        let name = direct_token(entry.syntax(), K::IDENT)?.text().to_owned();
                        let ty = support::children::<syntax::TypeExpr>(entry.syntax()).last()?;
                        Some((name, self.parse_type(ty.syntax(), generics)))
                    })
                    .collect();
                let index = support::children::<syntax::TypeMapEntry>(node)
                    .find(|entry| direct_token(entry.syntax(), K::L_BRACKET).is_some())
                    .and_then(|entry| {
                        let mut types = support::children::<syntax::TypeExpr>(entry.syntax());
                        Some((
                            Box::new(self.parse_type(types.next()?.syntax(), generics)),
                            Box::new(self.parse_type(types.next()?.syntax(), generics)),
                        ))
                    });
                Type::Map {
                    fields,
                    index,
                    open: support::child::<syntax::TypeMapRest>(node).is_some(),
                }
            }
            _ => Type::Unknown,
        }
    }
    pub(super) fn expand_alias(
        &mut self,
        name: &str,
        arguments: Vec<Type>,
        outer: &mut HashMap<String, u32>,
        at: Span,
    ) -> Type {
        if name == "number" {
            if arguments.is_empty() {
                return union(vec![Type::Int, Type::Float]);
            }
            self.diagnostic(
                AnalysisDiagnosticCode::WrongTypeArity,
                "Wrong number of type arguments",
                format!(
                    "Type `{name}` expects 0 arguments, but received {}.",
                    arguments.len()
                ),
                at,
            );
            return Type::Unknown;
        }
        let Some(alias) = self.aliases.get(name).cloned() else {
            self.diagnostic(
                AnalysisDiagnosticCode::UnknownType,
                "Unknown type",
                format!("The type `{name}` is not defined."),
                at,
            );
            return Type::Unknown;
        };
        if arguments.len() != alias.parameters.len() {
            self.diagnostic(
                AnalysisDiagnosticCode::WrongTypeArity,
                "Wrong number of type arguments",
                format!(
                    "Type `{name}` expects {} arguments, but received {}.",
                    alias.parameters.len(),
                    arguments.len()
                ),
                at,
            );
            return Type::Unknown;
        }
        if !self.alias_stack.insert(name.to_owned()) {
            self.diagnostic(
                AnalysisDiagnosticCode::CyclicTypeAlias,
                "Cyclic type alias",
                format!("Type alias `{name}` expands recursively."),
                at,
            );
            return Type::Unknown;
        }
        let mut alias_generics = HashMap::new();
        let first_alias_local = outer.values().copied().max().map_or(0, |id| id + 1);
        let mut parameter_ids = Vec::new();
        for (offset, parameter) in alias.parameters.iter().enumerate() {
            let id = first_alias_local + offset as u32;
            alias_generics.insert(parameter.clone(), id);
            parameter_ids.push(id);
        }
        let mut expanded = self.parse_type(&alias.body, &mut alias_generics);
        let replacements = parameter_ids
            .into_iter()
            .zip(arguments)
            .collect::<HashMap<_, _>>();
        expanded = substitute_generics(expanded, &replacements);
        let first_nested = first_alias_local + alias.parameters.len() as u32;
        let mut next_nested = first_alias_local;
        let mut nested_renames = HashMap::new();
        expanded = map_type(expanded, &mut |candidate| match candidate {
            Type::Generic(id) if id >= first_nested => {
                let renamed = *nested_renames.entry(id).or_insert_with(|| {
                    let renamed = next_nested;
                    next_nested += 1;
                    renamed
                });
                Type::Generic(renamed)
            }
            other => other,
        });
        self.alias_stack.remove(name);
        expanded
    }
}
