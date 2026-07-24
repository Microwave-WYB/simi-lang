use super::*;

impl Context<'_> {
    pub(super) fn constrain_pattern_domain(&mut self, source: &Type, pattern: &syntax::Pattern) {
        let resolved = self.resolve_type(source.clone());
        if matches!(resolved, Type::Infer(_)) {
            if let Some(domain) = self.pattern_domain(pattern) {
                self.constrain(source, &domain, span(pattern.syntax()));
            }
            return;
        }
        match (resolved, pattern) {
            (Type::Union(items), _) => {
                for item in items {
                    self.constrain_pattern_domain(&item, pattern);
                }
            }
            (Type::ListExact(items), syntax::Pattern::List(list)) => {
                for (item, child) in items
                    .iter()
                    .zip(support::children::<syntax::Pattern>(list.syntax()))
                {
                    self.constrain_pattern_domain(item, &child);
                }
            }
            (Type::ListRest(item), syntax::Pattern::List(list)) => {
                for child in support::children::<syntax::Pattern>(list.syntax()) {
                    self.constrain_pattern_domain(&item, &child);
                }
            }
            (Type::Map { fields, .. }, syntax::Pattern::Map(map)) => {
                for field in support::children::<syntax::MapPatternField>(map.syntax()) {
                    let Some(name) = direct_token(field.syntax(), K::IDENT) else {
                        continue;
                    };
                    let Some(child) = support::child::<syntax::Pattern>(field.syntax()) else {
                        continue;
                    };
                    if let Some((_, field_type)) =
                        fields.iter().find(|(field, _)| field == name.text())
                    {
                        self.constrain_pattern_domain(field_type, &child);
                    }
                }
            }
            _ => {}
        }
    }
    pub(super) fn pattern_domain(&mut self, pattern: &syntax::Pattern) -> Option<Type> {
        match pattern {
            syntax::Pattern::Binding(_) | syntax::Pattern::Wildcard(_) => Some(self.fresh()),
            syntax::Pattern::Literal(node) => Some(literal_type(node.syntax())),
            syntax::Pattern::List(_) => Some(Type::ListRest(Box::new(self.fresh()))),
            syntax::Pattern::Map(map) => {
                let mut fields = Vec::new();
                for field in support::children::<syntax::MapPatternField>(map.syntax()) {
                    let Some(name) = direct_token(field.syntax(), K::IDENT) else {
                        continue;
                    };
                    let Some(child) = support::child::<syntax::Pattern>(field.syntax()) else {
                        continue;
                    };
                    let field_type = self.pattern_domain(&child).unwrap_or(Type::Unknown);
                    fields.push((name.text().to_owned(), field_type));
                }
                Some(Type::Map {
                    fields,
                    index: None,
                    open: true,
                })
            }
        }
    }
    pub(super) fn unresolved_case_domain(&mut self, node: &syntax::CaseExpr) -> Option<Type> {
        let patterns = support::children::<syntax::CaseClause>(node.syntax())
            .filter_map(|clause| support::child::<syntax::Pattern>(clause.syntax()))
            .collect::<Vec<_>>();
        if patterns.is_empty() {
            return None;
        }
        if patterns
            .iter()
            .all(|pattern| matches!(pattern, syntax::Pattern::List(_)))
        {
            return Some(Type::ListRest(Box::new(self.fresh())));
        }
        if patterns
            .iter()
            .all(|pattern| matches!(pattern, syntax::Pattern::Map(_)))
        {
            return Some(union(
                patterns
                    .iter()
                    .filter_map(|pattern| self.pattern_domain(pattern))
                    .collect(),
            ));
        }
        None
    }
    pub(super) fn refine_condition(&mut self, expression: &syntax::Expr, truth: bool) -> bool {
        match expression {
            syntax::Expr::Literal(node) if direct_token(node.syntax(), K::TRUE_KW).is_some() => {
                truth
            }
            syntax::Expr::Literal(node) if direct_token(node.syntax(), K::FALSE_KW).is_some() => {
                !truth
            }
            syntax::Expr::Paren(node) => child_expr(node.syntax(), 0)
                .is_none_or(|inner| self.refine_condition(&inner, truth)),
            syntax::Expr::Unary(node) if direct_token(node.syntax(), K::NOT_KW).is_some() => {
                child_expr(node.syntax(), 0)
                    .is_none_or(|inner| self.refine_condition(&inner, !truth))
            }
            syntax::Expr::Binary(node) => {
                let children = expr_children(node.syntax()).collect::<Vec<_>>();
                let Some(left) = children.first() else {
                    return true;
                };
                let Some(right) = children.get(1) else {
                    return true;
                };
                let operator = binary_operator(node.syntax());
                match (operator, truth) {
                    (Some(K::AND_KW), true) => {
                        self.refine_condition(left, true) && self.refine_condition(right, true)
                    }
                    (Some(K::OR_KW), false) => {
                        self.refine_condition(left, false) && self.refine_condition(right, false)
                    }
                    (Some(K::AND_KW), false) => {
                        let entry = self.flow_state();
                        let mut alternatives = Vec::new();
                        self.restore_flow(&entry);
                        if self.refine_condition(left, false) {
                            alternatives.push(self.flow_state());
                        }
                        self.restore_flow(&entry);
                        if self.refine_condition(left, true) && self.refine_condition(right, false)
                        {
                            alternatives.push(self.flow_state());
                        }
                        if let Some(joined) = self.joined_flow(alternatives) {
                            self.restore_flow(&joined);
                            true
                        } else {
                            false
                        }
                    }
                    (Some(K::OR_KW), true) => {
                        let entry = self.flow_state();
                        let mut alternatives = Vec::new();
                        self.restore_flow(&entry);
                        if self.refine_condition(left, true) {
                            alternatives.push(self.flow_state());
                        }
                        self.restore_flow(&entry);
                        if self.refine_condition(left, false) && self.refine_condition(right, true)
                        {
                            alternatives.push(self.flow_state());
                        }
                        if let Some(joined) = self.joined_flow(alternatives) {
                            self.restore_flow(&joined);
                            true
                        } else {
                            false
                        }
                    }
                    (Some(K::EQ_EQ), _) | (Some(K::BANG_EQ), _) => {
                        let equality = operator == Some(K::EQ_EQ);
                        self.refine_comparison(left, right, truth == equality)
                    }
                    _ => self.refine_place(
                        expression,
                        &TypeMatcher::Exact(Type::LiteralBoolean(true)),
                        truth,
                    ),
                }
            }
            _ => self.refine_place(
                expression,
                &TypeMatcher::Exact(Type::LiteralBoolean(true)),
                truth,
            ),
        }
    }
    pub(super) fn refine_comparison(
        &mut self,
        left: &syntax::Expr,
        right: &syntax::Expr,
        equal: bool,
    ) -> bool {
        if let Some((place, category)) = self.type_test(left, right) {
            return self.refine_place(&place, &TypeMatcher::Category(category), equal);
        }
        if let Some((place, category)) = self.type_test(right, left) {
            return self.refine_place(&place, &TypeMatcher::Category(category), equal);
        }
        if let Some(matcher) = comparison_matcher(right) {
            return self.refine_place(left, &matcher, equal);
        }
        if let Some(matcher) = comparison_matcher(left) {
            return self.refine_place(right, &matcher, equal);
        }
        true
    }
    pub(super) fn type_test(
        &self,
        call: &syntax::Expr,
        label: &syntax::Expr,
    ) -> Option<(syntax::Expr, &'static str)> {
        let syntax::Expr::Call(call) = call else {
            return None;
        };
        let callee = child_expr(call.syntax(), 0)?;
        let syntax::Expr::Name(name) = callee else {
            return None;
        };
        let token = direct_token(name.syntax(), K::IDENT)?;
        let symbol = self.resolution.symbol_at(token_span(&token).start)?;
        let data = &self.resolution.hir.symbols[symbol];
        if data.name != "type" || !self.trusted_builtin_symbols.contains(&symbol) {
            return None;
        }
        let argument = support::child::<syntax::ArgList>(call.syntax())
            .and_then(|args| expr_children(args.syntax()).next())?;
        let label = literal_string(label)?;
        let category = match label.as_str() {
            "nil" => "nil",
            "boolean" => "boolean",
            "integer" => "integer",
            "float" => "float",
            "string" => "string",
            "list" => "list",
            "map" => "map",
            "function" => "function",
            _ => return None,
        };
        Some((argument, category))
    }
    pub(super) fn refine_place(
        &mut self,
        expression: &syntax::Expr,
        matcher: &TypeMatcher,
        keep: bool,
    ) -> bool {
        match expression {
            syntax::Expr::Paren(node) => child_expr(node.syntax(), 0)
                .is_none_or(|inner| self.refine_place(&inner, matcher, keep)),
            syntax::Expr::Name(name) => {
                let Some(token) = direct_token(name.syntax(), K::IDENT) else {
                    return true;
                };
                let Some(symbol) = self.resolution.symbol_at(token_span(&token).start) else {
                    return true;
                };
                let current = self
                    .symbol_types
                    .get(&symbol)
                    .cloned()
                    .unwrap_or(Type::Unknown);
                let narrowed = narrow_type(current, matcher, keep);
                if narrowed == Type::Never {
                    return false;
                }
                self.set_refined_symbol(symbol, narrowed);
                true
            }
            syntax::Expr::Field(field) => {
                let Some(owner) = child_expr(field.syntax(), 0) else {
                    return true;
                };
                let Some(symbol) = expression_symbol(&owner, self.resolution) else {
                    return true;
                };
                let Some(name) = direct_token(field.syntax(), K::IDENT) else {
                    return true;
                };
                let current = self
                    .symbol_types
                    .get(&symbol)
                    .cloned()
                    .unwrap_or(Type::Unknown);
                let narrowed = narrow_map_field(current, name.text(), matcher, keep);
                if narrowed == Type::Never {
                    return false;
                }
                self.set_refined_symbol(symbol, narrowed);
                true
            }
            _ => true,
        }
    }
    pub(super) fn refine_place_to(&mut self, expression: &syntax::Expr, ty: Type) -> bool {
        let syntax::Expr::Name(name) = expression else {
            return true;
        };
        let Some(token) = direct_token(name.syntax(), K::IDENT) else {
            return true;
        };
        let Some(symbol) = self.resolution.symbol_at(token_span(&token).start) else {
            return true;
        };
        if ty == Type::Never {
            false
        } else {
            self.set_refined_symbol(symbol, ty);
            true
        }
    }
    pub(super) fn set_refined_symbol(&mut self, symbol: SymbolId, ty: Type) {
        if let Some(region) = self.symbol_regions.get(&symbol).copied() {
            let aliases = self
                .symbol_regions
                .iter()
                .filter_map(|(alias, candidate)| (*candidate == region).then_some(*alias))
                .collect::<Vec<_>>();
            for alias in aliases {
                self.symbol_types.insert(alias, ty.clone());
            }
        } else {
            self.symbol_types.insert(symbol, ty);
        }
    }
}
