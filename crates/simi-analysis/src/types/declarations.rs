use super::*;

impl Context<'_> {
    pub(super) fn predeclare(&mut self, statements: impl Iterator<Item = syntax::Stmt>) {
        for statement in statements {
            let syntax::Stmt::FunctionDecl(function) = statement else {
                continue;
            };
            let Some(name) = direct_token(function.syntax(), K::IDENT) else {
                continue;
            };
            let Some(symbol) = self.resolution.symbol_at(token_span(&name).start) else {
                continue;
            };
            let mut generics = HashMap::new();
            let constraints = support::child::<syntax::CallableTypeParamList>(function.syntax())
                .map(|header| self.parse_callable_constraints(header.syntax(), &mut generics))
                .unwrap_or_default();
            let parameter_nodes = support::child::<syntax::ParamList>(function.syntax())
                .map(|list| support::children::<syntax::Param>(list.syntax()).collect::<Vec<_>>())
                .unwrap_or_default();
            let parameter_names = parameter_nodes
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    direct_token(parameter.syntax(), K::IDENT).map_or_else(
                        || format!("parameter {}", index + 1),
                        |token| token.text().to_owned(),
                    )
                })
                .collect::<Vec<_>>();
            let parameters = parameter_nodes
                .iter()
                .map(|parameter| {
                    support::child::<syntax::TypeAnnotation>(parameter.syntax())
                        .and_then(|annotation| {
                            support::child::<syntax::TypeExpr>(annotation.syntax())
                        })
                        .map(|ty| self.parse_type(ty.syntax(), &mut generics))
                        .unwrap_or_else(|| self.fresh())
                })
                .collect::<Vec<_>>();
            let result = support::child::<syntax::ReturnAnnotation>(function.syntax())
                .and_then(|annotation| support::child::<syntax::TypeExpr>(annotation.syntax()))
                .map(|ty| self.parse_type(ty.syntax(), &mut generics))
                .unwrap_or_else(|| self.fresh());
            let posts = parameter_nodes
                .iter()
                .enumerate()
                .filter_map(|(parameter_index, parameter)| {
                    let post = support::child::<syntax::PostType>(parameter.syntax())?;
                    let becomes = support::child::<syntax::TypeExpr>(post.syntax())
                        .map(|ty| self.parse_type(ty.syntax(), &mut generics))?;
                    Some(ParameterPostType {
                        parameter_index,
                        parameter_name: parameter_names[parameter_index].clone(),
                        becomes,
                    })
                })
                .collect::<Vec<_>>();
            let (raised, raised_annotation) =
                self.parse_effect_annotation(function.syntax(), &mut generics);
            let callable = CallableType {
                constraints,
                parameters: parameters
                    .into_iter()
                    .enumerate()
                    .map(|(index, ty)| CallableParameter {
                        name: parameter_names.get(index).cloned(),
                        post: posts
                            .iter()
                            .find(|post| post.parameter_index == index)
                            .map(|post| post.becomes.clone()),
                        ty,
                    })
                    .collect(),
                result: Box::new(result),
                raised: Box::new(raised),
                raised_annotation,
            };
            let function_ty = Type::Function(Box::new(callable));
            self.symbol_types.insert(symbol, function_ty.clone());
            self.symbol_bounds.insert(symbol, function_ty);
            self.symbol_posts.insert(symbol, posts);
        }
    }
    pub(super) fn statements(&mut self, statements: impl Iterator<Item = syntax::Stmt>) -> Type {
        let mut result = Type::Nil;
        for statement in statements {
            if result == Type::Never {
                break;
            }
            result = self.statement(statement);
        }
        result
    }
    pub(super) fn statement(&mut self, statement: syntax::Stmt) -> Type {
        match statement {
            syntax::Stmt::AliasDecl(_) => Type::Nil,
            syntax::Stmt::FunctionDecl(function) => {
                self.infer_function_decl(function);
                Type::Nil
            }
            syntax::Stmt::LetStmt(statement) => {
                let value_expression = support::child::<syntax::Expr>(statement.syntax());
                let inherited_region = value_expression
                    .as_ref()
                    .and_then(|expression| self.expression_region(expression));
                let inherited_callable = value_expression.as_ref().and_then(|expression| {
                    let syntax::Expr::Name(name) = expression else {
                        return None;
                    };
                    let token = direct_token(name.syntax(), K::IDENT)?;
                    let symbol = self.resolution.symbol_at(token_span(&token).start)?;
                    let posts = self.symbol_posts.get(&symbol)?.clone();
                    let ty = self.symbol_types.get(&symbol)?.clone();
                    Some((ty, posts))
                });
                let inherited_posts = inherited_callable.as_ref().map(|(_, posts)| posts.clone());
                let value = if let Some((ty, _)) = &inherited_callable {
                    if let Some(expression) = &value_expression {
                        self.expression_types
                            .push((span(expression.syntax()), ty.clone()));
                    }
                    ty.clone()
                } else {
                    value_expression
                        .clone()
                        .map(|expression| self.expression(expression))
                        .unwrap_or(Type::Unknown)
                };
                let annotation_node = support::child::<syntax::TypeAnnotation>(statement.syntax())
                    .and_then(|annotation| support::child::<syntax::TypeExpr>(annotation.syntax()));
                let mut annotation_generics = HashMap::new();
                let annotation_posts = annotation_node
                    .as_ref()
                    .map(|ty| self.parse_function_type_posts(ty.syntax(), &mut annotation_generics))
                    .unwrap_or_default();
                let annotation = annotation_node
                    .map(|ty| self.parse_type(ty.syntax(), &mut annotation_generics));
                let explicitly_annotated = annotation.is_some();
                let final_ty = if let Some(expected) = annotation {
                    self.constrain(&expected, &value, span(statement.syntax()));
                    self.resolve_type(expected)
                } else {
                    value.clone()
                };
                let inherited_capture_effects = value_expression
                    .as_ref()
                    .and_then(|expression| self.callable_capture_effects(expression));
                let inherited_assignment_effects = value_expression
                    .as_ref()
                    .and_then(|expression| self.callable_assignment_effects(expression));
                if let Some(pattern) = support::child::<syntax::Pattern>(statement.syntax()) {
                    if let Some(symbol) = pattern_symbol(&pattern, self.resolution) {
                        if explicitly_annotated {
                            self.annotated_symbols.insert(symbol);
                        }
                        if let Some(region) = inherited_region
                            && may_hold_mutable_value(&self.resolve_type(final_ty.clone()))
                        {
                            self.symbol_regions.insert(symbol, region);
                            if value_expression.as_ref().is_some_and(is_nested_read) {
                                self.conservative_regions.insert(region);
                            }
                        }
                        let posts = if annotation_posts.is_empty() {
                            inherited_posts
                        } else {
                            self.validate_annotated_posts(
                                &final_ty,
                                annotation_posts,
                                span(statement.syntax()),
                            )
                        };
                        if let Some(posts) = posts {
                            self.symbol_posts.insert(symbol, posts);
                        }
                        if let Some(effects) = inherited_capture_effects {
                            self.callable_capture_effects.insert(symbol, effects);
                        }
                        if let Some(effects) = inherited_assignment_effects {
                            self.callable_assignment_effects.insert(symbol, effects);
                        }
                    }
                    self.bind_pattern(pattern, final_ty);
                }
                value
            }
            syntax::Stmt::ExprStmt(statement) => support::child::<syntax::Expr>(statement.syntax())
                .map(|expression| self.expression(expression))
                .unwrap_or(Type::Unknown),
        }
    }
    pub(super) fn expression_region(&mut self, expression: &syntax::Expr) -> Option<u32> {
        match expression {
            syntax::Expr::Name(name) => direct_token(name.syntax(), K::IDENT)
                .and_then(|token| self.resolution.symbol_at(token_span(&token).start))
                .and_then(|symbol| self.symbol_regions.get(&symbol).copied()),
            syntax::Expr::List(_) | syntax::Expr::Map(_) => {
                let region = self.next_region;
                self.next_region += 1;
                Some(region)
            }
            syntax::Expr::Pipeline(pipeline)
                if support::children::<syntax::PipelineStage>(pipeline.syntax())
                    .all(|stage| direct_token(stage.syntax(), K::TAP_KW).is_some()) =>
            {
                child_expr(pipeline.syntax(), 0).and_then(|input| self.expression_region(&input))
            }
            syntax::Expr::Field(_) | syntax::Expr::Index(_) => {
                mutation_root_symbol(expression, self.resolution)
                    .and_then(|symbol| self.symbol_regions.get(&symbol).copied())
            }
            syntax::Expr::Paren(paren) => {
                child_expr(paren.syntax(), 0).and_then(|inner| self.expression_region(&inner))
            }
            _ => None,
        }
    }
    pub(super) fn infer_function_decl(&mut self, function: syntax::FunctionDecl) {
        let Some(name) = direct_token(function.syntax(), K::IDENT) else {
            return;
        };
        let Some(symbol) = self.resolution.symbol_at(token_span(&name).start) else {
            return;
        };
        let Some(Type::Function(mut callable)) = self.symbol_types.get(&symbol).cloned() else {
            return;
        };
        let parameters = callable
            .parameters
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect::<Vec<_>>();
        let expected_result = callable.result.clone();
        let outer_flow = self.flow_state();
        let outer_nil_aborts = std::mem::take(&mut self.nil_abort_states);
        let function_span = span(function.syntax());
        let capture_effects = self.function_captures(function_span);
        self.assignment_effect_frames
            .push((capture_effects.clone(), HashSet::new()));
        self.mutation_effect_frames.push(HashSet::new());
        let mut parameter_symbols = Vec::new();
        let mut parameter_names = Vec::new();
        if let Some(list) = support::child::<syntax::ParamList>(function.syntax()) {
            for (parameter, ty) in
                support::children::<syntax::Param>(list.syntax()).zip(parameters.iter())
            {
                let name = direct_token(parameter.syntax(), K::IDENT);
                let symbol = name
                    .as_ref()
                    .and_then(|token| self.resolution.symbol_at(token_span(token).start));
                if let Some(id) = symbol {
                    if support::child::<syntax::TypeAnnotation>(parameter.syntax()).is_some() {
                        self.annotated_symbols.insert(id);
                    }
                    self.symbol_types.insert(id, ty.clone());
                    self.symbol_bounds.insert(id, ty.clone());
                    if max_generic(ty).is_some() {
                        self.monomorphic_symbols.insert(id);
                    }
                    if has_mutable_category(ty) {
                        let region = self.next_region;
                        self.next_region += 1;
                        self.symbol_regions.insert(id, region);
                    }
                }
                parameter_names.push(
                    name.map_or_else(|| "parameter".to_owned(), |token| token.text().to_owned()),
                );
                parameter_symbols.push(symbol);
            }
        }
        let body = support::child::<syntax::Block>(function.syntax());
        let trusted_host_wrapper = body
            .as_ref()
            .is_some_and(|body| is_host_wrapper(body, self.resolution));
        self.generic_bound_frames.push(
            callable
                .constraints
                .iter()
                .filter_map(|constraint| match constraint.variable {
                    Type::Generic(id) => constraint.bound.clone().map(|bound| (id, bound)),
                    _ => None,
                })
                .collect(),
        );
        self.raised_exit_frames.push(Vec::new());
        let actual = body
            .as_ref()
            .map(|body| self.infer_block(body))
            .unwrap_or(Type::Nil);
        let raised_exits = self.raised_exit_frames.pop().unwrap_or_default();
        self.generic_bound_frames.pop();
        let actual_raised = Self::raised_type(&raised_exits);
        let assignment_effects = self
            .assignment_effect_frames
            .pop()
            .map(|(_, assigned)| assigned)
            .unwrap_or_default();
        let mutation_effects = self.mutation_effect_frames.pop().unwrap_or_default();
        let resolved_result = self.resolve_type((*expected_result).clone());
        let resolved_actual = self.resolve_type(actual.clone());
        if matches!(resolved_result, Type::Infer(_)) && resolved_actual == Type::Any {
            self.bind_infer((*expected_result).clone(), Type::Any);
        } else if let Type::Infer(id) = resolved_result
            && resolved_actual == Type::Infer(id)
            && let Some(state) = self.vars.get_mut(id as usize)
        {
            state.binding = Some(Type::Never);
        } else {
            if matches!(resolved_result, Type::Infer(_))
                && matches!(resolved_actual, Type::Infer(_))
                && body.as_ref().is_some_and(block_ends_in_direct_call)
            {
                self.bind_infer(resolved_actual, Type::Never);
            }
            self.constrain(&expected_result, &actual, span(function.syntax()));
        }

        let mut posts = self.symbol_posts.get(&symbol).cloned().unwrap_or_default();
        let post_nodes = support::child::<syntax::ParamList>(function.syntax())
            .map(|list| {
                support::children::<syntax::Param>(list.syntax())
                    .filter_map(|parameter| support::child::<syntax::PostType>(parameter.syntax()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (index, post) in posts.iter().enumerate() {
            let Some(pre) = parameters.get(post.parameter_index) else {
                continue;
            };
            let at = post_nodes
                .get(index)
                .map_or_else(|| span(function.syntax()), |node| span(node.syntax()));
            if !valid_post_transition(pre, &post.becomes) {
                self.diagnostic(
                    AnalysisDiagnosticCode::InvalidType,
                    "Invalid post-type",
                    format!(
                        "Post-type `{}` is not a valid transition from parameter `{}` of type `{}`.",
                        post.becomes.display(),
                        post.parameter_name,
                        pre.display()
                    ),
                    at,
                );
                continue;
            }
            if actual != Type::Never
                && !trusted_host_wrapper
                && let Some(Some(parameter_symbol)) = parameter_symbols.get(post.parameter_index)
            {
                let inferred = self
                    .symbol_types
                    .get(parameter_symbol)
                    .cloned()
                    .unwrap_or(Type::Unknown);
                let inferred = self.resolve_type(inferred);
                let promised = self.resolve_type(post.becomes.clone());
                if !is_subtype(&inferred, &promised) {
                    self.diagnostic(
                        AnalysisDiagnosticCode::TypeMismatch,
                        "Post-type is not established",
                        format!(
                            "Parameter `{}` has type `{}` on normal return, which does not satisfy `{}`.",
                            post.parameter_name,
                            inferred.display(),
                            promised.display()
                        ),
                        at,
                    );
                }
            }
        }

        if actual != Type::Never && !trusted_host_wrapper {
            let declared = posts
                .iter()
                .map(|post| post.parameter_index)
                .collect::<HashSet<_>>();
            for (parameter_index, ((parameter, parameter_symbol), parameter_name)) in parameters
                .iter()
                .zip(&parameter_symbols)
                .zip(&parameter_names)
                .enumerate()
            {
                if declared.contains(&parameter_index)
                    || parameter_symbol.is_none_or(|symbol| !mutation_effects.contains(&symbol))
                {
                    continue;
                }
                let pre = self.resolve_type(parameter.clone());
                let Some(parameter_symbol) = parameter_symbol else {
                    continue;
                };
                let post = self
                    .symbol_types
                    .get(parameter_symbol)
                    .cloned()
                    .map(|ty| self.resolve_type(ty))
                    .unwrap_or_else(|| pre.clone());
                if pre != post
                    && has_mutable_category(&pre)
                    && has_mutable_category(&post)
                    && valid_post_transition(&pre, &post)
                {
                    posts.push(ParameterPostType {
                        parameter_index,
                        parameter_name: parameter_name.clone(),
                        becomes: post,
                    });
                }
            }
        }

        let declared_raised = (*callable.raised).clone();
        let final_raised = match callable.raised_annotation {
            RaisedAnnotation::Inferred => {
                let resolved_declared = self.resolve_type(declared_raised.clone());
                let resolved_actual = self.resolve_type(actual_raised.clone());
                if matches!(resolved_declared, Type::Infer(_)) && resolved_actual == Type::Any {
                    self.bind_infer(declared_raised.clone(), Type::Any);
                } else if let Type::Infer(id) = resolved_declared
                    && resolved_actual == Type::Infer(id)
                {
                    self.bind_infer(declared_raised.clone(), Type::Never);
                } else {
                    self.constrain(&declared_raised, &actual_raised, function_span);
                }
                self.resolve_type(declared_raised)
            }
            RaisedAnnotation::Explicit | RaisedAnnotation::NoRaise => {
                if !trusted_host_wrapper {
                    self.require_subtype(&actual_raised, &declared_raised, function_span);
                }
                self.resolve_type(declared_raised)
            }
        };
        callable.result = Box::new(self.resolve_type(*expected_result));
        callable.raised = Box::new(final_raised);
        for (index, parameter) in callable.parameters.iter_mut().enumerate() {
            parameter.post = posts
                .iter()
                .find(|post| post.parameter_index == index)
                .map(|post| post.becomes.clone());
        }
        let resolved_function = self.resolve_type(Type::Function(callable));
        let mut next = max_generic(&resolved_function).map_or(0, |index| index + 1);
        for post in &posts {
            if let Some(index) = max_generic(&post.becomes) {
                next = next.max(index + 1);
            }
        }
        let mut variables = HashMap::new();
        let function_ty = generalize_type(resolved_function, &mut variables, &mut next);
        for post in &mut posts {
            post.becomes = generalize_type(
                self.resolve_type(post.becomes.clone()),
                &mut variables,
                &mut next,
            );
        }
        self.restore_outer_flow(&outer_flow);
        self.nil_abort_states = outer_nil_aborts;
        self.symbol_types.insert(symbol, function_ty.clone());
        self.symbol_bounds.insert(symbol, function_ty);
        self.symbol_posts.insert(symbol, posts);
        self.callable_capture_effects
            .insert(symbol, capture_effects);
        self.callable_assignment_effects
            .insert(symbol, assignment_effects);
    }
    pub(super) fn infer_block(&mut self, block: &syntax::Block) -> Type {
        self.nil_abort_states.push(Vec::new());
        let result = self.statements(block.statements());
        let aborts = self.nil_abort_states.pop().unwrap_or_default();
        if aborts.is_empty() {
            return result;
        }

        let mut exits = aborts;
        if result != Type::Never {
            exits.push(self.flow_state());
        }
        self.join_and_restore(exits);
        union(vec![result, Type::Nil])
    }
    pub(super) fn infer_anonymous(&mut self, node: syntax::FunctionExpr) -> Type {
        let outer_flow = self.flow_state();
        let outer_nil_aborts = std::mem::take(&mut self.nil_abort_states);
        let function_span = span(node.syntax());
        let capture_effects = self.function_captures(function_span);
        self.assignment_effect_frames
            .push((capture_effects.clone(), HashSet::new()));
        self.mutation_effect_frames.push(HashSet::new());
        let mut generics = HashMap::new();
        let constraints = support::child::<syntax::CallableTypeParamList>(node.syntax())
            .map(|header| self.parse_callable_constraints(header.syntax(), &mut generics))
            .unwrap_or_default();
        let parameters = support::child::<syntax::ParamList>(node.syntax())
            .map(|list| {
                support::children::<syntax::Param>(list.syntax())
                    .map(|parameter| {
                        let ty = support::child::<syntax::TypeAnnotation>(parameter.syntax())
                            .and_then(|annotation| {
                                support::child::<syntax::TypeExpr>(annotation.syntax())
                            })
                            .map(|annotation| self.parse_type(annotation.syntax(), &mut generics))
                            .unwrap_or_else(|| self.fresh());
                        if let Some(token) = direct_token(parameter.syntax(), K::IDENT)
                            && let Some(symbol) =
                                self.resolution.symbol_at(token_span(&token).start)
                        {
                            if support::child::<syntax::TypeAnnotation>(parameter.syntax())
                                .is_some()
                            {
                                self.annotated_symbols.insert(symbol);
                            }
                            self.symbol_types.insert(symbol, ty.clone());
                            self.symbol_bounds.insert(symbol, ty.clone());
                            if max_generic(&ty).is_some() {
                                self.monomorphic_symbols.insert(symbol);
                            }
                        }
                        let post = support::child::<syntax::PostType>(parameter.syntax())
                            .and_then(|post| support::child::<syntax::TypeExpr>(post.syntax()))
                            .map(|post| self.parse_type(post.syntax(), &mut generics));
                        CallableParameter {
                            name: direct_token(parameter.syntax(), K::IDENT)
                                .map(|token| token.text().to_owned()),
                            ty,
                            post,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let expected = support::child::<syntax::ReturnAnnotation>(node.syntax())
            .and_then(|annotation| support::child::<syntax::TypeExpr>(annotation.syntax()))
            .map(|annotation| self.parse_type(annotation.syntax(), &mut generics));
        let (declared_raised, raised_annotation) =
            self.parse_effect_annotation(node.syntax(), &mut generics);
        self.generic_bound_frames.push(
            constraints
                .iter()
                .filter_map(|constraint| match constraint.variable {
                    Type::Generic(id) => constraint.bound.clone().map(|bound| (id, bound)),
                    _ => None,
                })
                .collect(),
        );
        self.raised_exit_frames.push(Vec::new());
        let actual = support::child::<syntax::Block>(node.syntax())
            .map(|body| self.infer_block(&body))
            .unwrap_or(Type::Nil);
        let raised_exits = self.raised_exit_frames.pop().unwrap_or_default();
        self.generic_bound_frames.pop();
        let actual_raised = Self::raised_type(&raised_exits);
        let assignment_effects = self
            .assignment_effect_frames
            .pop()
            .map(|(_, assigned)| assigned)
            .unwrap_or_default();
        self.mutation_effect_frames.pop();
        let result = if let Some(expected) = expected {
            self.require_subtype(&actual, &expected, span(node.syntax()));
            expected
        } else {
            actual.clone()
        };
        if actual != Type::Never
            && let Some(list) = support::child::<syntax::ParamList>(node.syntax())
        {
            for (parameter_node, parameter) in
                support::children::<syntax::Param>(list.syntax()).zip(&parameters)
            {
                let Some(post) = &parameter.post else {
                    continue;
                };
                if !valid_post_transition(&parameter.ty, post) {
                    self.diagnostic(
                        AnalysisDiagnosticCode::InvalidType,
                        "Invalid post-type",
                        format!(
                            "Post-type `{}` is not a valid transition from parameter type `{}`.",
                            post.display(),
                            parameter.ty.display()
                        ),
                        span(parameter_node.syntax()),
                    );
                    continue;
                }
                if let Some(token) = direct_token(parameter_node.syntax(), K::IDENT)
                    && let Some(symbol) = self.resolution.symbol_at(token_span(&token).start)
                    && let Some(established) = self.symbol_types.get(&symbol).cloned()
                {
                    let established = self.resolve_type(established);
                    let promised = self.resolve_type(post.clone());
                    if !is_subtype(&established, &promised) {
                        self.diagnostic(
                            AnalysisDiagnosticCode::TypeMismatch,
                            "Post-type is not established",
                            format!(
                                "Parameter has type `{}` on normal return, which does not satisfy `{}`.",
                                established.display(),
                                promised.display()
                            ),
                            span(parameter_node.syntax()),
                        );
                    }
                }
            }
        }
        let raised = match raised_annotation {
            RaisedAnnotation::Inferred => actual_raised,
            RaisedAnnotation::Explicit | RaisedAnnotation::NoRaise => {
                self.require_subtype(&actual_raised, &declared_raised, function_span);
                declared_raised
            }
        };
        let callable = CallableType {
            constraints,
            parameters,
            result: Box::new(result),
            raised: Box::new(raised),
            raised_annotation,
        };
        let function_ty = self.generalize(Type::Function(Box::new(callable)));
        self.restore_outer_flow(&outer_flow);
        self.nil_abort_states = outer_nil_aborts;
        self.anonymous_capture_effects
            .push((function_span, capture_effects));
        self.anonymous_assignment_effects
            .push((function_span, assignment_effects));
        function_ty
    }
}
