use super::*;

impl Context<'_> {
    pub(super) fn bind_pattern(&mut self, pattern: syntax::Pattern, ty: Type) {
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
                    self.bind_pattern(child, item);
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
                let fields = match self.resolve_type(ty) {
                    Type::Map { fields, .. } => fields,
                    _ => Vec::new(),
                };
                for field in support::children::<syntax::MapPatternField>(node.syntax()) {
                    let field_name =
                        direct_token(field.syntax(), K::IDENT).map(|token| token.text().to_owned());
                    if let Some(child) = support::child::<syntax::Pattern>(field.syntax()) {
                        let ty = field_name
                            .and_then(|name| {
                                fields
                                    .iter()
                                    .find(|(field, _)| field == &name)
                                    .map(|(_, ty)| ty.clone())
                            })
                            .unwrap_or(Type::Unknown);
                        self.bind_pattern(child, ty);
                    }
                }
            }
            _ => {}
        }
    }
    pub(super) fn parse_function_type_posts(
        &mut self,
        node: &SyntaxNode,
        generics: &mut HashMap<String, u32>,
    ) -> Vec<ParameterPostType> {
        let Some(function) = support::child::<syntax::TypeFunction>(node) else {
            return Vec::new();
        };
        if direct_token(function.syntax(), K::ARROW).is_none() {
            let Some(name_node) = transparent_type_name(function.syntax()) else {
                return Vec::new();
            };
            let name = direct_token(name_node.syntax(), K::IDENT)
                .map(|token| token.text().to_owned())
                .unwrap_or_default();
            let arguments = support::child::<syntax::TypeArgumentList>(name_node.syntax())
                .map(|list| {
                    support::children::<syntax::TypeExpr>(list.syntax())
                        .map(|ty| self.parse_type(ty.syntax(), generics))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let Some(alias) = self.aliases.get(&name).cloned() else {
                return Vec::new();
            };
            if arguments.len() != alias.parameters.len() || !self.alias_stack.insert(name.clone()) {
                return Vec::new();
            }
            let mut alias_generics = alias
                .parameters
                .iter()
                .enumerate()
                .map(|(index, parameter)| (parameter.clone(), index as u32))
                .collect::<HashMap<_, _>>();
            let mut posts = self.parse_function_type_posts(&alias.body, &mut alias_generics);
            let replacements = arguments
                .into_iter()
                .enumerate()
                .map(|(index, ty)| (index as u32, ty))
                .collect::<HashMap<_, _>>();
            for post in &mut posts {
                post.becomes = substitute_generics(post.becomes.clone(), &replacements);
            }
            self.alias_stack.remove(&name);
            return posts;
        }
        let Some(arguments) = support::child::<syntax::TypeUnion>(function.syntax())
            .and_then(|union| support::child::<syntax::TypeParen>(union.syntax()))
        else {
            return Vec::new();
        };
        support::children::<syntax::TypeFunctionParam>(arguments.syntax())
            .enumerate()
            .filter_map(|(parameter_index, parameter)| {
                let post = support::child::<syntax::PostType>(parameter.syntax())?;
                let becomes = support::child::<syntax::TypeExpr>(post.syntax())
                    .map(|ty| self.parse_type(ty.syntax(), generics))?;
                Some(ParameterPostType {
                    parameter_index,
                    parameter_name: format!("argument {}", parameter_index + 1),
                    becomes,
                })
            })
            .collect()
    }
    pub(super) fn validate_annotated_posts(
        &mut self,
        function_ty: &Type,
        posts: Vec<ParameterPostType>,
        at: Span,
    ) -> Option<Vec<ParameterPostType>> {
        let Type::Function(callable) = function_ty else {
            self.diagnostic(
                AnalysisDiagnosticCode::InvalidType,
                "Post-state outside function type",
                "Post-state annotations require a function type.".to_owned(),
                at,
            );
            return None;
        };
        let mut valid = Vec::new();
        for post in posts {
            let Some(pre) = callable
                .parameters
                .get(post.parameter_index)
                .map(|parameter| &parameter.ty)
            else {
                continue;
            };
            if valid_post_transition(pre, &post.becomes) {
                valid.push(post);
            } else {
                self.diagnostic(
                    AnalysisDiagnosticCode::InvalidType,
                    "Invalid post-type",
                    format!(
                        "Post-type `{}` is not a valid transition from {} of type `{}`.",
                        post.becomes.display(),
                        post.parameter_name,
                        pre.display()
                    ),
                    at,
                );
            }
        }
        Some(valid)
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
        let keyword = direct_token(effect.syntax(), K::IDENT)
            .map(|token| token.text().to_owned())
            .unwrap_or_default();
        if keyword == "noraise" {
            return (Type::Never, RaisedAnnotation::NoRaise);
        }
        let raised = support::child::<syntax::TypeExpr>(effect.syntax())
            .map(|ty| self.parse_type(ty.syntax(), generics))
            .unwrap_or(Type::Unknown);
        (raised, RaisedAnnotation::Explicit)
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
                            post: None,
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
            K::TYPE_LITERAL => literal_type(node),
            K::TYPE_PAREN => {
                let items = support::children::<syntax::TypeFunctionParam>(node)
                    .filter_map(|parameter| {
                        let ty = support::child::<syntax::TypeExpr>(parameter.syntax())
                            .map(|ty| self.parse_type(ty.syntax(), generics))?;
                        let post = support::child::<syntax::PostType>(parameter.syntax())
                            .and_then(|post| support::child::<syntax::TypeExpr>(post.syntax()))
                            .map(|post| self.parse_type(post.syntax(), generics));
                        Some(CallableParameter {
                            name: direct_token(parameter.syntax(), K::IDENT)
                                .map(|token| token.text().to_owned()),
                            ty,
                            post,
                        })
                    })
                    .collect::<Vec<_>>();
                match items.as_slice() {
                    [one] if one.name.is_none() && one.post.is_none() => one.ty.clone(),
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
