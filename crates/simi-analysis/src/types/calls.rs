use super::*;

impl Context<'_> {
    pub(super) fn pipeline_stage_active(
        &mut self,
        stage: &syntax::PipelineStage,
        incoming: Type,
        origin: &Option<syntax::Expr>,
        tap: bool,
    ) -> Type {
        let Some(callee_node) = child_expr(stage.syntax(), 0) else {
            return Type::Unknown;
        };
        let member = self.call_member(&callee_node);
        let callee = self.expression(callee_node.clone());
        let callee = self.instantiate_callee(&callee_node, callee);
        let post_scheme = self
            .call_post_scheme(&callee_node)
            .or_else(|| callable_post_scheme(&callee));
        let callable = type_may_be_callable(&self.resolve_type(callee.clone()));
        let mut argument_nodes = support::child::<syntax::ArgList>(stage.syntax())
            .map(|list| expr_children(list.syntax()).collect::<Vec<_>>())
            .unwrap_or_default();
        argument_nodes.extend(expr_children(stage.syntax()).skip(1));
        let mut arguments = vec![incoming.clone()];
        arguments.extend(
            argument_nodes
                .iter()
                .cloned()
                .map(|argument| self.expression(argument)),
        );
        let (result, raised) = self.apply_call_type(callee, &arguments, span(stage.syntax()));
        let raised_entry = self.flow_state();
        let known_tap_result = member
            .as_ref()
            .and_then(|(module, field)| {
                (tap && module == "std/list" && field == "append" && arguments.len() == 2).then(
                    || {
                        list_append_result(
                            self.resolve_type(incoming.clone()),
                            arguments[1].clone(),
                        )
                    },
                )
            })
            .or_else(|| {
                tap.then(|| {
                    post_scheme.as_ref().and_then(|(parameters, posts)| {
                        self.instantiate_parameter_posts(parameters, posts, &arguments)
                            .into_iter()
                            .find_map(|(index, becomes)| (index == 0).then_some(becomes))
                    })
                })
                .flatten()
            });
        let mut effect_nodes = origin.clone().into_iter().collect::<Vec<_>>();
        effect_nodes.extend(argument_nodes.clone());
        if result != Type::Never {
            if let Some((parameters, posts)) = &post_scheme {
                self.apply_parameter_posts(parameters, posts, &effect_nodes, &arguments);
            }
            if let Some((module, field)) = &member {
                self.apply_call_effect(module, field, &effect_nodes, &arguments);
            }
        }
        let contract_complete = self.callable_parameter_contract_complete(&callee_node);
        self.invalidate_unmodeled_call_arguments(
            member.as_ref(),
            post_scheme.as_ref(),
            contract_complete,
            &effect_nodes,
            &arguments,
        );
        if callable {
            self.apply_callable_effects(
                &callee_node,
                &effect_nodes,
                member.is_some() || contract_complete,
            );
        }
        self.record_call_raised_effects(
            raised,
            raised_entry,
            &callee_node,
            &effect_nodes,
            &arguments,
            member.as_ref(),
            post_scheme.as_ref(),
            contract_complete,
            callable,
        );
        if result == Type::Never {
            Type::Never
        } else if tap {
            known_tap_result.unwrap_or_else(|| {
                origin
                    .as_ref()
                    .and_then(|place| expression_symbol(place, self.resolution))
                    .and_then(|symbol| self.symbol_types.get(&symbol).cloned())
                    .unwrap_or(incoming)
            })
        } else {
            result
        }
    }
    pub(super) fn trailing_call(&mut self, node: syntax::TrailingArgumentExpr) -> Type {
        let mut expressions = expr_children(node.syntax());
        let Some(call) = expressions.next() else {
            return Type::Unknown;
        };
        let Some(trailing) = expressions.next() else {
            return self.expression(call);
        };
        let syntax::Expr::Call(call) = call else {
            return Type::Unknown;
        };
        let Some(callee_node) = child_expr(call.syntax(), 0) else {
            return Type::Unknown;
        };
        let member = self.call_member(&callee_node);
        let callee = self.expression(callee_node.clone());
        let callee = self.instantiate_callee(&callee_node, callee);
        let post_scheme = self
            .call_post_scheme(&callee_node)
            .or_else(|| callable_post_scheme(&callee));
        let callable = type_may_be_callable(&self.resolve_type(callee.clone()));
        let mut argument_nodes = support::child::<syntax::ArgList>(call.syntax())
            .map(|list| expr_children(list.syntax()).collect::<Vec<_>>())
            .unwrap_or_default();
        argument_nodes.push(trailing);
        let arguments = argument_nodes
            .iter()
            .cloned()
            .map(|argument| self.expression(argument))
            .collect::<Vec<_>>();
        let (result, raised) = self.apply_call_type(callee, &arguments, span(node.syntax()));
        let raised_entry = self.flow_state();
        if result != Type::Never {
            if let Some((parameters, posts)) = &post_scheme {
                self.apply_parameter_posts(parameters, posts, &argument_nodes, &arguments);
            }
            if let Some((module, field)) = &member {
                self.apply_call_effect(module, field, &argument_nodes, &arguments);
            }
        }
        let contract_complete = self.callable_parameter_contract_complete(&callee_node);
        self.invalidate_unmodeled_call_arguments(
            member.as_ref(),
            post_scheme.as_ref(),
            contract_complete,
            &argument_nodes,
            &arguments,
        );
        if callable {
            self.apply_callable_effects(
                &callee_node,
                &argument_nodes,
                member.is_some() || contract_complete,
            );
        }
        self.record_call_raised_effects(
            raised,
            raised_entry,
            &callee_node,
            &argument_nodes,
            &arguments,
            member.as_ref(),
            post_scheme.as_ref(),
            contract_complete,
            callable,
        );
        result
    }
    pub(super) fn apply_call_type(
        &mut self,
        callee: Type,
        arguments: &[Type],
        at: Span,
    ) -> (Type, Type) {
        match self.resolve_type(callee) {
            Type::Function(callable) => {
                if callable.parameters.len() != arguments.len() {
                    self.diagnostic(
                        AnalysisDiagnosticCode::WrongArity,
                        "Wrong number of arguments",
                        format!(
                            "Expected {} arguments, but received {}.",
                            callable.parameters.len(),
                            arguments.len()
                        ),
                        at,
                    );
                }
                for (actual, expected) in arguments.iter().zip(&callable.parameters) {
                    self.constrain(&expected.ty, actual, at);
                }
                let raised = if callable.parameters.len() == arguments.len() {
                    (*callable.raised).clone()
                } else {
                    Type::Never
                };
                (self.resolve_type(*callable.result), raised)
            }
            Type::Any => (Type::Any, Type::Any),
            Type::Unknown | Type::Infer(_) => (Type::Unknown, Type::Any),
            other => {
                self.diagnostic(
                    AnalysisDiagnosticCode::NotCallable,
                    "Value is not callable",
                    format!("A value of type `{}` cannot be called.", other.display()),
                    at,
                );
                (Type::Unknown, Type::Never)
            }
        }
    }
    pub(super) fn call(&mut self, node: syntax::CallExpr) -> Type {
        let Some(callee_node) = child_expr(node.syntax(), 0) else {
            return Type::Unknown;
        };
        let member = self.call_member(&callee_node);
        let callee = self.expression(callee_node.clone());
        let callee = self.instantiate_callee(&callee_node, callee);
        let post_scheme = self
            .call_post_scheme(&callee_node)
            .or_else(|| callable_post_scheme(&callee));
        let callable = type_may_be_callable(&self.resolve_type(callee.clone()));
        let arguments = support::child::<syntax::ArgList>(node.syntax())
            .map(|list| expr_children(list.syntax()).collect::<Vec<_>>())
            .unwrap_or_default();
        let argument_types = arguments
            .iter()
            .cloned()
            .map(|argument| self.expression(argument))
            .collect::<Vec<_>>();
        let (result, raised) = self.apply_call_type(callee, &argument_types, span(node.syntax()));
        let raised_entry = self.flow_state();
        if result != Type::Never {
            if let Some((parameters, posts)) = &post_scheme {
                self.apply_parameter_posts(parameters, posts, &arguments, &argument_types);
            }
            if let Some((module, field)) = &member {
                self.apply_call_effect(module, field, &arguments, &argument_types);
            }
        }
        let contract_complete = self.callable_parameter_contract_complete(&callee_node);
        self.invalidate_unmodeled_call_arguments(
            member.as_ref(),
            post_scheme.as_ref(),
            contract_complete,
            &arguments,
            &argument_types,
        );
        if callable {
            self.apply_callable_effects(
                &callee_node,
                &arguments,
                member.is_some() || contract_complete,
            );
        }
        self.record_call_raised_effects(
            raised,
            raised_entry,
            &callee_node,
            &arguments,
            &argument_types,
            member.as_ref(),
            post_scheme.as_ref(),
            contract_complete,
            callable,
        );
        result
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_call_raised_effects(
        &mut self,
        raised: Type,
        raised_entry: FlowState,
        callee: &syntax::Expr,
        arguments: &[syntax::Expr],
        argument_types: &[Type],
        member: Option<&(String, String)>,
        post_scheme: Option<&(Vec<Type>, Vec<ParameterPostType>)>,
        contract_complete: bool,
        callable: bool,
    ) {
        if self.resolve_type(raised.clone()) == Type::Never {
            return;
        }
        let normal_exit = self.flow_state();
        self.restore_flow(&raised_entry);
        self.invalidate_unmodeled_call_arguments(
            member,
            post_scheme,
            contract_complete,
            arguments,
            argument_types,
        );
        if callable {
            self.apply_callable_effects(callee, arguments, member.is_some() || contract_complete);
        }
        self.record_raised(raised);
        self.restore_flow(&normal_exit);
    }
    pub(super) fn function_captures(&self, function_span: Span) -> HashSet<SymbolId> {
        self.resolution
            .captures
            .iter()
            .filter(|capture| {
                self.resolution.hir.scopes[capture.function_scope].span == function_span
            })
            .map(|capture| capture.symbol)
            .collect()
    }
    pub(super) fn callable_capture_effects(
        &self,
        expression: &syntax::Expr,
    ) -> Option<HashSet<SymbolId>> {
        match expression {
            syntax::Expr::Name(name) => direct_token(name.syntax(), K::IDENT)
                .and_then(|token| self.resolution.symbol_at(token_span(&token).start))
                .and_then(|symbol| self.callable_capture_effects.get(&symbol).cloned()),
            syntax::Expr::Function(function) => {
                let at = span(function.syntax());
                self.anonymous_capture_effects
                    .iter()
                    .rev()
                    .find_map(|(candidate, effects)| (*candidate == at).then(|| effects.clone()))
            }
            syntax::Expr::Paren(paren) => child_expr(paren.syntax(), 0)
                .and_then(|inner| self.callable_capture_effects(&inner)),
            _ => None,
        }
    }
    pub(super) fn callable_assignment_effects(
        &self,
        expression: &syntax::Expr,
    ) -> Option<HashSet<SymbolId>> {
        match expression {
            syntax::Expr::Name(name) => direct_token(name.syntax(), K::IDENT)
                .and_then(|token| self.resolution.symbol_at(token_span(&token).start))
                .and_then(|symbol| self.callable_assignment_effects.get(&symbol).cloned()),
            syntax::Expr::Function(function) => {
                let at = span(function.syntax());
                self.anonymous_assignment_effects
                    .iter()
                    .rev()
                    .find_map(|(candidate, effects)| (*candidate == at).then(|| effects.clone()))
            }
            syntax::Expr::Paren(paren) => child_expr(paren.syntax(), 0)
                .and_then(|inner| self.callable_assignment_effects(&inner)),
            _ => None,
        }
    }
    pub(super) fn apply_callable_effects(
        &mut self,
        callee: &syntax::Expr,
        arguments: &[syntax::Expr],
        modeled: bool,
    ) {
        let callee_effects = self.callable_capture_effects(callee);
        let callee_assignments = self.callable_assignment_effects(callee);
        let known_builtin = expression_symbol(callee, self.resolution)
            .is_some_and(|symbol| self.trusted_builtin_symbols.contains(&symbol));
        if callee_effects.is_none() && !modeled && !known_builtin {
            self.widen_all_regions();
        }

        let mut affected = callee_effects.unwrap_or_default();
        let mut assigned = callee_assignments.unwrap_or_default();
        for argument in arguments {
            if let Some(captures) = self.callable_capture_effects(argument) {
                affected.extend(captures);
            }
            if let Some(assignments) = self.callable_assignment_effects(argument) {
                assigned.extend(assignments);
            }
        }
        let regions = affected
            .into_iter()
            .filter_map(|symbol| self.symbol_regions.get(&symbol).copied())
            .collect::<HashSet<_>>();
        for region in regions {
            self.widen_region_individually(region);
        }
        if let Some((_, caller_assignments)) = self.assignment_effect_frames.last_mut() {
            caller_assignments.extend(assigned.iter().copied());
        }
        for symbol in assigned {
            self.invalidate_assigned_binding(symbol);
        }
    }
    pub(super) fn invalidate_assigned_binding(&mut self, symbol: SymbolId) {
        self.symbol_types.insert(symbol, Type::Any);
        self.symbol_bounds.insert(symbol, Type::Any);
        self.symbol_posts.remove(&symbol);
        self.symbol_regions.remove(&symbol);
        self.callable_capture_effects.remove(&symbol);
        self.callable_assignment_effects.remove(&symbol);
        self.trusted_builtin_symbols.remove(&symbol);
    }
    pub(super) fn widen_all_regions(&mut self) {
        let regions = self
            .symbol_regions
            .values()
            .copied()
            .collect::<HashSet<_>>();
        for region in regions {
            self.widen_region_individually(region);
        }
    }
    pub(super) fn widen_region_individually(&mut self, region: u32) {
        let aliases = self
            .symbol_regions
            .iter()
            .filter_map(|(symbol, candidate)| (*candidate == region).then_some(*symbol))
            .collect::<Vec<_>>();
        for alias in aliases {
            let widened = self
                .symbol_types
                .get(&alias)
                .cloned()
                .map(widen_mutable_type)
                .unwrap_or(Type::Unknown);
            self.symbol_types.insert(alias, widened.clone());
            self.symbol_bounds.insert(alias, widened);
        }
    }
    pub(super) fn callable_parameter_contract_complete(&self, callee: &syntax::Expr) -> bool {
        match callee {
            syntax::Expr::Name(name) => direct_token(name.syntax(), K::IDENT)
                .and_then(|token| self.resolution.symbol_at(token_span(&token).start))
                .is_some_and(|symbol| {
                    self.symbol_posts.contains_key(&symbol)
                        && self.callable_capture_effects.contains_key(&symbol)
                        && self.callable_assignment_effects.contains_key(&symbol)
                }),
            syntax::Expr::Paren(paren) => child_expr(paren.syntax(), 0)
                .is_some_and(|inner| self.callable_parameter_contract_complete(&inner)),
            _ => false,
        }
    }
    pub(super) fn call_member(&self, callee: &syntax::Expr) -> Option<(String, String)> {
        match callee {
            syntax::Expr::Field(field) => {
                let name = direct_token(field.syntax(), K::IDENT)?;
                let member = member_at(
                    self.db,
                    self.file,
                    self.modules,
                    self.source,
                    token_span(&name).start,
                )?;
                Some((member.module, member.field.name))
            }
            syntax::Expr::Name(name) => {
                let token = direct_token(name.syntax(), K::IDENT)?;
                let symbol = self.resolution.symbol_at(token_span(&token).start)?;
                let members = crate::modules::imported_members(self.db, self.file, self.modules);
                let member = members.get(&symbol)?;
                Some((member.module.clone(), member.field.name.clone()))
            }
            _ => None,
        }
    }
    pub(super) fn call_post_scheme(
        &self,
        callee: &syntax::Expr,
    ) -> Option<(Vec<Type>, Vec<ParameterPostType>)> {
        match callee {
            syntax::Expr::Name(name) => {
                let token = direct_token(name.syntax(), K::IDENT)?;
                let symbol = self.resolution.symbol_at(token_span(&token).start)?;
                let ty = self.symbol_types.get(&symbol).cloned().or_else(|| {
                    crate::modules::imported_members(self.db, self.file, self.modules)
                        .get(&symbol)
                        .and_then(|member| member.field.ty.clone())
                })?;
                let Type::Function(callable) = ty else {
                    return None;
                };
                let parameters = callable
                    .parameters
                    .into_iter()
                    .map(|parameter| parameter.ty)
                    .collect();
                let posts = self.symbol_posts.get(&symbol).cloned().or_else(|| {
                    crate::modules::imported_members(self.db, self.file, self.modules)
                        .get(&symbol)
                        .map(|member| member.field.posts.clone())
                })?;
                Some((parameters, posts))
            }
            syntax::Expr::Field(field) => {
                let token = direct_token(field.syntax(), K::IDENT)?;
                let member = member_at(
                    self.db,
                    self.file,
                    self.modules,
                    self.source,
                    token_span(&token).start,
                )?;
                let Type::Function(callable) = member.field.ty.clone()? else {
                    return None;
                };
                let parameters = callable
                    .parameters
                    .into_iter()
                    .map(|parameter| parameter.ty)
                    .collect();
                Some((parameters, member.field.posts))
            }
            _ => None,
        }
    }
    pub(super) fn instantiate_parameter_posts(
        &mut self,
        parameters: &[Type],
        posts: &[ParameterPostType],
        argument_types: &[Type],
    ) -> Vec<(usize, Type)> {
        let mut replacements = HashMap::new();
        for (parameter, actual) in parameters.iter().zip(argument_types) {
            collect_generic_replacements(
                parameter,
                &self.resolve_type(actual.clone()),
                &mut replacements,
            );
        }
        posts
            .iter()
            .map(|post| {
                (
                    post.parameter_index,
                    self.resolve_type(substitute_post_generics(
                        post.becomes.clone(),
                        &replacements,
                    )),
                )
            })
            .collect()
    }
    pub(super) fn apply_parameter_posts(
        &mut self,
        parameters: &[Type],
        posts: &[ParameterPostType],
        arguments: &[syntax::Expr],
        argument_types: &[Type],
    ) {
        for (parameter_index, becomes) in
            self.instantiate_parameter_posts(parameters, posts, argument_types)
        {
            let Some(argument) = arguments.get(parameter_index) else {
                continue;
            };
            let Some(symbol) = expression_symbol(argument, self.resolution) else {
                continue;
            };
            self.record_mutation(symbol);
            if let Some(region) = self.symbol_regions.get(&symbol).copied() {
                if self.conservative_regions.contains(&region) {
                    self.widen_region_individually(region);
                    continue;
                }
                let aliases = self
                    .symbol_regions
                    .iter()
                    .filter_map(|(symbol, candidate)| (*candidate == region).then_some(*symbol))
                    .collect::<Vec<_>>();
                for alias in aliases {
                    self.symbol_types.insert(alias, becomes.clone());
                    self.symbol_bounds.insert(alias, becomes.clone());
                }
            } else {
                self.symbol_types.insert(symbol, becomes.clone());
                self.symbol_bounds.insert(symbol, becomes);
            }
        }
    }
    pub(super) fn apply_call_effect(
        &mut self,
        module: &str,
        field: &str,
        arguments: &[syntax::Expr],
        argument_types: &[Type],
    ) {
        if module != "std/list" || field != "append" || arguments.len() != 2 {
            return;
        }
        let Some(symbol) = expression_symbol(&arguments[0], self.resolution) else {
            return;
        };
        self.record_mutation(symbol);
        let Some(region) = self.symbol_regions.get(&symbol).copied() else {
            return;
        };
        if self.conservative_regions.contains(&region) {
            self.widen_region_individually(region);
            return;
        }
        let widened = argument_types
            .first()
            .cloned()
            .map(|ty| list_append_result(self.resolve_type(ty), argument_types[1].clone()))
            .unwrap_or_else(|| Type::ListRest(Box::new(Type::Unknown)));
        let aliases = self
            .symbol_regions
            .iter()
            .filter_map(|(symbol, candidate)| (*candidate == region).then_some(*symbol))
            .collect::<Vec<_>>();
        for alias in aliases {
            self.symbol_types.insert(alias, widened.clone());
            self.symbol_bounds.insert(alias, widened.clone());
        }
    }
    pub(super) fn invalidate_unmodeled_call_arguments(
        &mut self,
        member: Option<&(String, String)>,
        post_scheme: Option<&(Vec<Type>, Vec<ParameterPostType>)>,
        contract_complete: bool,
        arguments: &[syntax::Expr],
        argument_types: &[Type],
    ) {
        let posted = post_scheme
            .map(|(_, posts)| {
                posts
                    .iter()
                    .map(|post| post.parameter_index)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        for (index, (argument, ty)) in arguments.iter().zip(argument_types).enumerate() {
            let has_mutable_region = mutation_root_symbol(argument, self.resolution)
                .is_some_and(|symbol| self.symbol_regions.contains_key(&symbol));
            if contract_complete
                || posted.contains(&index)
                || member.is_some_and(|(module, field)| {
                    known_module_argument_is_pure(module, field, index)
                })
                || (!has_mutable_region && !has_mutable_category(&self.resolve_type(ty.clone())))
            {
                continue;
            }
            self.invalidate_mutated_owner(argument);
        }
    }
}
