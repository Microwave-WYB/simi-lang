use super::*;

impl Context<'_> {
    pub(super) fn expression(&mut self, expression: syntax::Expr) -> Type {
        let expression_span = span(expression.syntax());
        let ty = match expression {
            syntax::Expr::Literal(node) => literal_type(node.syntax()),
            syntax::Expr::Name(node) => direct_token(node.syntax(), K::IDENT)
                .and_then(|token| self.resolution.symbol_at(token_span(&token).start))
                .and_then(|symbol| {
                    let ty = self.symbol_types.get(&symbol)?.clone();
                    Some(if self.monomorphic_symbols.contains(&symbol) {
                        ty
                    } else {
                        self.instantiate(ty)
                    })
                })
                .unwrap_or(Type::Unknown),
            syntax::Expr::Function(node) => self.infer_anonymous(node),
            syntax::Expr::Block(node) => support::child::<syntax::Block>(node.syntax())
                .map(|block| self.infer_block(&block))
                .unwrap_or(Type::Nil),
            syntax::Expr::Paren(node) => child_expr(node.syntax(), 0)
                .map(|child| self.expression(child))
                .unwrap_or(Type::Unknown),
            syntax::Expr::List(node) => {
                let mut items = Vec::new();
                let mut rest = Vec::new();
                for element in support::children::<syntax::ListElement>(node.syntax()) {
                    let Some(value) = child_expr(element.syntax(), 0) else {
                        continue;
                    };
                    let value_type = self.expression(value);
                    if direct_token(element.syntax(), K::DOT_DOT).is_none() {
                        items.push(value_type);
                        continue;
                    }
                    let item = self.fresh();
                    self.constrain(
                        &Type::ListRest(Box::new(item)),
                        &value_type,
                        span(element.syntax()),
                    );
                    match self.resolve_type(value_type) {
                        Type::ListExact(values) => items.extend(values),
                        Type::ListRest(item) => rest.push(*item),
                        Type::Unknown | Type::Any | Type::Infer(_) => rest.push(Type::Unknown),
                        _ => rest.push(Type::Unknown),
                    }
                }
                if rest.is_empty() {
                    Type::ListExact(items)
                } else {
                    rest.extend(items);
                    Type::ListRest(Box::new(union(rest)))
                }
            }
            syntax::Expr::Bytes(node) => {
                for segment in expr_children(node.syntax()) {
                    let literal_string = matches!(
                        &segment,
                        syntax::Expr::Literal(literal)
                            if direct_token(literal.syntax(), K::STRING).is_some()
                    );
                    let segment_span = span(segment.syntax());
                    let segment_type = self.expression(segment);
                    if !literal_string {
                        self.constrain(
                            &Type::Union(vec![Type::Int, Type::Bytes]),
                            &segment_type,
                            segment_span,
                        );
                    }
                }
                Type::Bytes
            }
            syntax::Expr::Map(node) => {
                let mut fields = Vec::new();
                let mut keys = Vec::new();
                let mut values = Vec::new();
                let mut open = false;
                for entry in support::children::<syntax::MapEntry>(node.syntax()) {
                    let mut expressions = expr_children(entry.syntax());
                    if let Some(name) = direct_token(entry.syntax(), K::IDENT) {
                        let value = if let Some(value) = expressions.next() {
                            let discriminant = direct_boolean_literal_type(&value);
                            let inferred = self.expression(value);
                            discriminant.unwrap_or(inferred)
                        } else if let Some(symbol) =
                            self.resolution.symbol_at(token_span(&name).start)
                            && let Some(ty) = self.symbol_types.get(&symbol).cloned()
                        {
                            if self.monomorphic_symbols.contains(&symbol) {
                                ty
                            } else {
                                self.instantiate(ty)
                            }
                        } else {
                            Type::Unknown
                        };
                        if value != Type::Nil {
                            if type_has_explicit_nil(&value) {
                                // An explicit `T | nil` record field is optional at runtime:
                                // map insertion deletes nil, but the union records that
                                // absence precisely without opening the whole map.
                                fields.push((name.text().to_owned(), value));
                            } else if type_may_be_nil(&value) {
                                // `any` and unresolved values may be nil without declaring an
                                // optional field. Preserve soundness by widening the literal.
                                open = true;
                            } else {
                                fields.push((name.text().to_owned(), value));
                            }
                        }
                    } else if let (Some(key), Some(value)) =
                        (expressions.next(), expressions.next())
                    {
                        keys.push(self.expression(key));
                        values.push(self.expression(value));
                    }
                }
                Type::Map {
                    fields,
                    index: (!keys.is_empty())
                        .then(|| (Box::new(union(keys)), Box::new(union(values)))),
                    open,
                }
            }
            syntax::Expr::Unary(node) => self.unary(node),
            syntax::Expr::Binary(node) => self.binary(node),
            syntax::Expr::Call(node) => self.call(node),
            syntax::Expr::Field(node) => self.field(node),
            syntax::Expr::Index(node) => self.index(node),
            syntax::Expr::Assign(node) => self.assignment(node),
            syntax::Expr::If(node) => self.infer_if(node),
            syntax::Expr::Case(node) => self.infer_case(node),
            syntax::Expr::Protected(node) => self.infer_protected(node),
            syntax::Expr::Panic(_) => Type::Never,
            syntax::Expr::Todo(node) => {
                self.warning(
                    AnalysisDiagnosticCode::Todo,
                    "unfinished code",
                    "`todo` always stops evaluation with a hard diagnostic".to_owned(),
                    span(node.syntax()),
                );
                Type::Never
            }
            syntax::Expr::Raise(node) => {
                if let Some(value) = support::child::<syntax::Expr>(node.syntax()) {
                    let raised = self.expression(value);
                    self.record_raised(raised);
                }
                Type::Never
            }
            syntax::Expr::NilPropagate(node) => {
                let Some(child) = child_expr(node.syntax(), 0) else {
                    return Type::Unknown;
                };
                let value = self.expression(child.clone());
                let before = self.flow_state();
                if type_may_be_nil(&self.resolve_type(value.clone())) {
                    let mut abort = before.clone();
                    self.restore_flow(&abort);
                    if self.refine_place(&child, &TypeMatcher::Exact(Type::Nil), true) {
                        abort = self.flow_state();
                        if let Some(boundary) = self.nil_abort_states.last_mut() {
                            boundary.push(abort);
                        }
                    }
                    self.restore_flow(&before);
                }
                let _ = self.refine_place(&child, &TypeMatcher::Exact(Type::Nil), false);
                remove_nil(value)
            }
            syntax::Expr::Pipeline(node) => self.pipeline(node),
            syntax::Expr::TrailingArgument(node) => self.trailing_call(node),
        };
        self.expression_types.push((expression_span, ty.clone()));
        ty
    }
    pub(super) fn infer_if(&mut self, node: syntax::IfExpr) -> Type {
        let mut pending = Some(self.flow_state());
        let mut exits = Vec::new();
        let mut results = Vec::new();

        for branch in support::children::<syntax::IfBranch>(node.syntax()) {
            let Some(entry) = pending.take() else {
                break;
            };
            self.restore_flow(&entry);
            let Some(condition) = support::child::<syntax::Expr>(branch.syntax()) else {
                continue;
            };
            let condition_ty = self.expression(condition.clone());
            self.constrain(&Type::Boolean, &condition_ty, span(branch.syntax()));
            let after_condition = self.flow_state();

            self.restore_flow(&after_condition);
            if self.refine_condition(&condition, true) {
                let result = support::child::<syntax::Block>(branch.syntax())
                    .map(|block| self.infer_block(&block))
                    .unwrap_or(Type::Nil);
                if result != Type::Never {
                    results.push(result);
                    exits.push(self.flow_state());
                }
            }

            self.restore_flow(&after_condition);
            pending = self
                .refine_condition(&condition, false)
                .then(|| self.flow_state());
        }

        if let Some(entry) = pending {
            self.restore_flow(&entry);
            if let Some(branch) = support::child::<syntax::ElseBranch>(node.syntax())
                && let Some(block) = support::child::<syntax::Block>(branch.syntax())
            {
                let result = self.infer_block(&block);
                if result != Type::Never {
                    results.push(result);
                    exits.push(self.flow_state());
                }
            } else {
                results.push(Type::Nil);
                exits.push(self.flow_state());
            }
        }

        self.join_and_restore(exits);
        if results.is_empty() {
            Type::Never
        } else {
            union(results)
        }
    }
    pub(super) fn infer_protected(&mut self, node: syntax::ProtectedExpr) -> Type {
        self.raised_exit_frames.push(Vec::new());
        let protected = support::child::<syntax::Block>(node.syntax())
            .map(|block| self.infer_block(&block))
            .unwrap_or(Type::Nil);
        let mut remaining = self.raised_exit_frames.pop().unwrap_or_default();
        let mut results = Vec::new();
        let mut normal_exits = Vec::new();
        if protected != Type::Never {
            results.push(protected);
            normal_exits.push(self.flow_state());
        }

        for clause in support::children::<syntax::CatchArm>(node.syntax()) {
            let Some(pattern) = support::child::<syntax::Pattern>(clause.syntax()) else {
                continue;
            };
            let mut matched = Vec::new();
            let mut unmatched = Vec::new();
            for exit in remaining.drain(..) {
                let (matched_type, unmatched_type) =
                    pattern_partition(exit.raised.clone(), &pattern);
                if matched_type != Type::Never {
                    matched.push(RaisedExit {
                        raised: matched_type,
                        flow: exit.flow.clone(),
                    });
                }
                if unmatched_type != Type::Never {
                    unmatched.push(RaisedExit {
                        raised: unmatched_type,
                        flow: exit.flow,
                    });
                }
            }
            remaining = unmatched;
            if matched.is_empty() {
                continue;
            }
            let matched_type = Self::raised_type(&matched);
            let Some(entry) =
                self.joined_flow(matched.iter().map(|exit| exit.flow.clone()).collect())
            else {
                continue;
            };
            self.restore_flow(&entry);
            self.bind_pattern(pattern, matched_type.clone());

            let mut handler_reachable = true;
            if let Some(guard) = support::child::<syntax::Expr>(clause.syntax()) {
                let guard_type = self.expression(guard.clone());
                self.constrain(&Type::Boolean, &guard_type, span(clause.syntax()));
                let after_guard = self.flow_state();

                self.restore_flow(&after_guard);
                handler_reachable = self.refine_condition(&guard, true);
                let handler_entry = self.flow_state();

                self.restore_flow(&after_guard);
                if self.refine_condition(&guard, false) {
                    remaining.push(RaisedExit {
                        raised: matched_type,
                        flow: self.flow_state(),
                    });
                }
                if handler_reachable {
                    self.restore_flow(&handler_entry);
                }
            }

            if handler_reachable {
                let result = support::child::<syntax::Body>(clause.syntax())
                    .map(|body| self.infer_body(body.syntax()))
                    .unwrap_or(Type::Nil);
                if result != Type::Never {
                    results.push(result);
                    normal_exits.push(self.flow_state());
                }
            }
        }

        if let Some(frame) = self.raised_exit_frames.last_mut() {
            frame.extend(remaining);
        }
        self.join_and_restore(normal_exits);
        if results.is_empty() {
            Type::Never
        } else {
            union(results)
        }
    }
    pub(super) fn infer_case(&mut self, node: syntax::CaseExpr) -> Type {
        let value_node = support::child::<syntax::Expr>(node.syntax());
        let value = value_node
            .as_ref()
            .map(|value| self.expression(value.clone()))
            .unwrap_or(Type::Unknown);
        let resolved = self.resolve_type(value.clone());
        let mut remaining = if matches!(resolved, Type::Infer(_) | Type::Unknown | Type::Any)
            && let Some(domain) = self.unresolved_case_domain(&node)
        {
            if matches!(resolved, Type::Infer(_)) {
                self.constrain(&value, &domain, span(node.syntax()));
                self.resolve_type(value.clone())
            } else {
                domain
            }
        } else {
            resolved
        };
        if !matches!(remaining, Type::Infer(_) | Type::Unknown | Type::Any) {
            for pattern in support::children::<syntax::CaseClause>(node.syntax())
                .filter_map(|clause| support::child::<syntax::Pattern>(clause.syntax()))
            {
                self.constrain_pattern_domain(&remaining, &pattern);
            }
            remaining = self.resolve_type(remaining);
        }
        let mut pending = Some(self.flow_state());
        let mut exits = Vec::new();
        let mut results = Vec::new();

        for clause in support::children::<syntax::CaseClause>(node.syntax()) {
            let Some(entry) = pending.take() else {
                break;
            };
            self.restore_flow(&entry);
            let Some(pattern) = support::child::<syntax::Pattern>(clause.syntax()) else {
                continue;
            };
            let (matched, unmatched) = pattern_partition(remaining.clone(), &pattern);
            let mut clause_remaining = unmatched.clone();
            if matched != Type::Never {
                if let Some(scrutinee) = &value_node {
                    let _ = self.refine_place_to(scrutinee, matched.clone());
                }
                self.bind_pattern(pattern, matched.clone());
                if let Some(guard) = support::child::<syntax::Expr>(clause.syntax()) {
                    let guard_ty = self.expression(guard.clone());
                    self.constrain(&Type::Boolean, &guard_ty, span(clause.syntax()));
                    let after_guard = self.flow_state();
                    self.restore_flow(&after_guard);
                    if self.refine_condition(&guard, true) {
                        let result = support::child::<syntax::Body>(clause.syntax())
                            .map(|body| self.infer_body(body.syntax()))
                            .unwrap_or(Type::Nil);
                        if result != Type::Never {
                            results.push(result);
                            exits.push(self.flow_state());
                        }
                    }
                    self.restore_flow(&after_guard);
                    let mut next = Vec::new();
                    if self.refine_condition(&guard, false) {
                        if let Some(scrutinee) = &value_node
                            && let Some(symbol) = expression_symbol(scrutinee, self.resolution)
                            && let Some(guard_failed) = self.symbol_types.get(&symbol).cloned()
                        {
                            clause_remaining = union(vec![clause_remaining, guard_failed]);
                        } else {
                            clause_remaining = union(vec![clause_remaining, matched.clone()]);
                        }
                        next.push(self.flow_state());
                    }
                    self.restore_flow(&entry);
                    if unmatched != Type::Never {
                        if let Some(scrutinee) = &value_node {
                            let _ = self.refine_place_to(scrutinee, unmatched.clone());
                        }
                        next.push(self.flow_state());
                    }
                    pending = self.joined_flow(next);
                } else {
                    let result = support::child::<syntax::Body>(clause.syntax())
                        .map(|body| self.infer_body(body.syntax()))
                        .unwrap_or(Type::Nil);
                    if result != Type::Never {
                        results.push(result);
                        exits.push(self.flow_state());
                    }
                    self.restore_flow(&entry);
                    if unmatched != Type::Never {
                        if let Some(scrutinee) = &value_node {
                            let _ = self.refine_place_to(scrutinee, unmatched.clone());
                        }
                        pending = Some(self.flow_state());
                    }
                }
            } else {
                pending = Some(entry);
            }
            remaining = clause_remaining;
        }

        self.join_and_restore(exits);
        if results.is_empty() {
            Type::Never
        } else {
            union(results)
        }
    }
    pub(super) fn unary(&mut self, node: syntax::UnaryExpr) -> Type {
        let operand = child_expr(node.syntax(), 0)
            .map(|child| self.expression(child))
            .unwrap_or(Type::Unknown);
        if direct_token(node.syntax(), K::NOT_KW).is_some() {
            self.constrain(&Type::Boolean, &operand, span(node.syntax()));
            Type::Boolean
        } else {
            self.numeric_operand(operand, span(node.syntax()))
        }
    }
    pub(super) fn binary(&mut self, node: syntax::BinaryExpr) -> Type {
        let children = expr_children(node.syntax()).collect::<Vec<_>>();
        let Some(left_node) = children.first().cloned() else {
            return Type::Unknown;
        };
        let Some(right_node) = children.get(1).cloned() else {
            return Type::Unknown;
        };
        let Some(op_kind) = binary_operator(node.syntax()) else {
            return Type::Unknown;
        };
        if matches!(op_kind, K::AND_KW | K::OR_KW) {
            let left = self.expression(left_node.clone());
            self.constrain(&Type::Boolean, &left, span(left_node.syntax()));
            let after_left = self.flow_state();
            let rhs_truth = op_kind == K::AND_KW;
            let mut exits = Vec::new();
            self.restore_flow(&after_left);
            if self.refine_condition(&left_node, !rhs_truth) {
                exits.push(self.flow_state());
            }
            self.restore_flow(&after_left);
            if self.refine_condition(&left_node, rhs_truth) {
                let right = self.expression(right_node.clone());
                self.constrain(&Type::Boolean, &right, span(right_node.syntax()));
                exits.push(self.flow_state());
            }
            self.join_and_restore(exits);
            return Type::Boolean;
        }
        let left = self.expression(left_node);
        let right = self.expression(right_node);
        let op_span = node
            .syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == op_kind)
            .map(|token| token_span(&token))
            .unwrap_or_else(|| span(node.syntax()));
        match op_kind {
            K::PLUS | K::MINUS | K::STAR | K::SLASH | K::SLASH_SLASH | K::PERCENT => {
                self.numeric_binary(left, right, op_kind, op_span)
            }
            K::LESS_GREATER => {
                self.constrain(&Type::String, &left, op_span);
                self.constrain(&Type::String, &right, op_span);
                Type::String
            }
            K::LESS | K::LESS_EQ | K::GREATER | K::GREATER_EQ => {
                let _ = self.numeric_operands(left, right, op_span);
                Type::Boolean
            }
            K::EQ_EQ | K::BANG_EQ => {
                let left = self.resolve_type(left);
                let right = self.resolve_type(right);
                if !equality_type(&left) || !equality_type(&right) {
                    self.invalid_operator(op_span, &left, Some(&right));
                }
                Type::Boolean
            }
            _ => Type::Boolean,
        }
    }
    pub(super) fn numeric_operand(&mut self, ty: Type, at: Span) -> Type {
        let resolved = self.resolve_type(ty.clone());
        if matches!(resolved, Type::Infer(_)) {
            self.bind_infer(ty, numeric());
            return numeric();
        }
        let checked = self.upper_bound_view(resolved.clone());
        if is_subtype(&checked, &numeric()) || matches!(checked, Type::Any | Type::Unknown) {
            resolved
        } else {
            self.invalid_operator(at, &resolved, None);
            Type::Unknown
        }
    }
    pub(super) fn numeric_operands(
        &mut self,
        left: Type,
        right: Type,
        at: Span,
    ) -> Option<(Type, Type)> {
        let left = self.upper_bound_view(self.resolve_type(left));
        let right = self.upper_bound_view(self.resolve_type(right));
        let left = if matches!(left, Type::Infer(_)) {
            self.bind_infer(left, numeric());
            numeric()
        } else {
            left
        };
        let right = if matches!(right, Type::Infer(_)) {
            self.bind_infer(right, numeric());
            numeric()
        } else {
            right
        };
        let valid =
            |ty: &Type| is_subtype(ty, &numeric()) || matches!(ty, Type::Any | Type::Unknown);
        if valid(&left) && valid(&right) {
            Some((left, right))
        } else {
            self.invalid_operator(at, &left, Some(&right));
            None
        }
    }
    pub(super) fn numeric_binary(
        &mut self,
        left: Type,
        right: Type,
        operator: K,
        at: Span,
    ) -> Type {
        let Some((left, right)) = self.numeric_operands(left, right, at) else {
            return Type::Unknown;
        };
        if matches!(left, Type::Unknown | Type::Any) || matches!(right, Type::Unknown | Type::Any) {
            return if matches!(left, Type::Any) || matches!(right, Type::Any) {
                Type::Any
            } else {
                Type::Unknown
            };
        }
        let left_atoms = numeric_atoms(&left);
        let right_atoms = numeric_atoms(&right);
        let mut results = Vec::new();
        for left in &left_atoms {
            for right in &right_atoms {
                let result = match operator {
                    K::SLASH => Type::Float,
                    K::PLUS | K::MINUS | K::STAR | K::SLASH_SLASH | K::PERCENT
                        if matches!((left, right), (Type::Int, Type::Int)) =>
                    {
                        Type::Int
                    }
                    _ => Type::Float,
                };
                results.push(result);
            }
        }
        union(results)
    }
    pub(super) fn pipeline(&mut self, node: syntax::PipelineExpr) -> Type {
        let Some(input) = child_expr(node.syntax(), 0) else {
            return Type::Unknown;
        };
        let mut origin = Some(input.clone());
        let mut current = self.expression(input);
        for stage in support::children::<syntax::PipelineStage>(node.syntax()) {
            let nil_aware = direct_token(stage.syntax(), K::QUESTION_GREATER).is_some();
            let tap = direct_token(stage.syntax(), K::TAP_KW).is_some();
            if !nil_aware {
                current = self.pipeline_stage_active(&stage, current, &origin, tap);
                if !tap {
                    origin = None;
                }
                continue;
            }

            let before = self.flow_state();
            let active_input = remove_nil(current.clone());
            let active_possible = active_input != Type::Never;
            let skipped_possible = type_may_be_nil(&self.resolve_type(current.clone()));
            let mut exits = Vec::new();
            let mut active_result = Type::Never;

            if active_possible {
                self.restore_flow(&before);
                let active_reachable = origin.as_ref().is_none_or(|place| {
                    self.refine_place(place, &TypeMatcher::Exact(Type::Nil), false)
                });
                if active_reachable {
                    active_result = self.pipeline_stage_active(&stage, active_input, &origin, tap);
                    exits.push(self.flow_state());
                }
            }
            if skipped_possible {
                self.restore_flow(&before);
                let skipped_reachable = origin.as_ref().is_none_or(|place| {
                    self.refine_place(place, &TypeMatcher::Exact(Type::Nil), true)
                });
                if skipped_reachable {
                    exits.push(self.flow_state());
                }
            }
            self.join_and_restore(exits);

            current = if tap {
                origin
                    .as_ref()
                    .and_then(|place| expression_symbol(place, self.resolution))
                    .and_then(|symbol| self.symbol_types.get(&symbol).cloned())
                    .unwrap_or_else(|| {
                        union(
                            [active_result]
                                .into_iter()
                                .chain(skipped_possible.then_some(Type::Nil))
                                .collect(),
                        )
                    })
            } else {
                origin = None;
                union(
                    [active_result]
                        .into_iter()
                        .chain(skipped_possible.then_some(Type::Nil))
                        .collect(),
                )
            };
        }
        current
    }
}
