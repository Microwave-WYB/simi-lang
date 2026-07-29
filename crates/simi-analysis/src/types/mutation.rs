use super::*;

impl Context<'_> {
    pub(super) fn field(&mut self, node: syntax::FieldExpr) -> Type {
        let Some(name) = direct_token(node.syntax(), K::IDENT) else {
            return Type::Unknown;
        };
        if let Some(member) = member_at(
            self.db,
            self.file,
            self.modules,
            self.source,
            token_span(&name).start,
        ) {
            return member
                .field
                .ty
                .map(|ty| self.instantiate(ty))
                .unwrap_or(Type::Unknown);
        }
        if let Some(object) = child_expr(node.syntax(), 0) {
            let object_ty = self.expression(object);
            let object_ty = self.resolve_type(object_ty);
            return field_lookup_type(object_ty, name.text());
        }
        Type::Unknown
    }
    pub(super) fn index(&mut self, node: syntax::IndexExpr) -> Type {
        let mut children = expr_children(node.syntax());
        let object = children
            .next()
            .map(|child| self.expression(child))
            .unwrap_or(Type::Unknown);
        let key_node = children.next();
        let key = key_node
            .clone()
            .map(|child| self.expression(child))
            .unwrap_or(Type::Unknown);
        if let Type::Map {
            fields,
            index: Some((key_hole, value_hole)),
            ..
        } = &object
        {
            if let Type::LiteralString(name) = &key
                && let Some((_, value)) = fields.iter().find(|(field, _)| field == name)
            {
                return value.clone();
            }
            if self.is_deferred_empty_map_index(key_hole, value_hole) {
                self.bind_infer((**key_hole).clone(), key.clone());
                return union(vec![self.resolve_type((**value_hole).clone()), Type::Nil]);
            }
        }
        match self.resolve_type(object) {
            Type::ListExact(items) => {
                self.require_subtype(&key, &Type::Int, span(node.syntax()));
                if let Some(syntax::Expr::Literal(literal)) = key_node
                    && let Some(token) = direct_token(literal.syntax(), K::INT)
                    && let Ok(index) = token.text().parse::<usize>()
                {
                    return items.get(index).cloned().unwrap_or(Type::Nil);
                }
                union(items.into_iter().chain([Type::Nil]).collect())
            }
            Type::ListRest(item) => {
                self.require_subtype(&key, &Type::Int, span(node.syntax()));
                union(vec![*item, Type::Nil])
            }
            Type::Bytes => {
                self.require_subtype(&key, &Type::Int, span(node.syntax()));
                union(vec![Type::Int, Type::Nil])
            }
            Type::Map {
                fields,
                index,
                open,
            } => {
                if let Type::LiteralString(name) = &key
                    && let Some((_, ty)) = fields.iter().find(|(field, _)| field == name)
                {
                    return ty.clone();
                }
                if let Some((expected_key, value)) = index {
                    self.constrain(&expected_key, &key, span(node.syntax()));
                    return union(vec![*value, Type::Nil]);
                }
                if open {
                    Type::Any
                } else {
                    union(
                        fields
                            .into_iter()
                            .map(|(_, ty)| ty)
                            .chain([Type::Nil])
                            .collect(),
                    )
                }
            }
            Type::Any => Type::Any,
            _ => Type::Unknown,
        }
    }
    pub(super) fn assignment(&mut self, node: syntax::AssignExpr) -> Type {
        let mut children = expr_children(node.syntax());
        let target = children.next();
        let value_node = children.next();
        let value_region = value_node
            .as_ref()
            .and_then(|expression| self.expression_region(expression));
        let value_trusted_builtin = value_node
            .as_ref()
            .and_then(|expression| expression_symbol(expression, self.resolution))
            .filter(|symbol| self.trusted_builtin_symbols.contains(symbol));
        let value = value_node
            .clone()
            .map(|child| self.expression(child))
            .unwrap_or(Type::Unknown);
        let value_capture_effects = value_node
            .as_ref()
            .and_then(|expression| self.callable_capture_effects(expression));
        let value_assignment_effects = value_node
            .as_ref()
            .and_then(|expression| self.callable_assignment_effects(expression));
        match target {
            Some(syntax::Expr::Name(name)) => {
                if let Some(token) = direct_token(name.syntax(), K::IDENT)
                    && let Some(symbol) = self.resolution.symbol_at(token_span(&token).start)
                {
                    self.record_mutation(symbol);
                    if let Some((captures, assigned)) = self.assignment_effect_frames.last_mut()
                        && captures.contains(&symbol)
                    {
                        assigned.insert(symbol);
                    }
                    if let Some(previous) = self.symbol_types.get(&symbol).cloned() {
                        self.expression_types.push((span(name.syntax()), previous));
                    }
                    self.symbol_types.insert(symbol, value.clone());
                    self.symbol_bounds.insert(symbol, value.clone());
                    if let Some(region) = value_region
                        && may_hold_mutable_value(&self.resolve_type(value.clone()))
                    {
                        self.symbol_regions.insert(symbol, region);
                        if value_node.as_ref().is_some_and(is_nested_read) {
                            self.conservative_regions.insert(region);
                        }
                    } else {
                        self.symbol_regions.remove(&symbol);
                    }
                    if let Some(effects) = value_capture_effects {
                        self.callable_capture_effects.insert(symbol, effects);
                    } else {
                        self.callable_capture_effects.remove(&symbol);
                    }
                    if let Some(effects) = value_assignment_effects {
                        self.callable_assignment_effects.insert(symbol, effects);
                    } else {
                        self.callable_assignment_effects.remove(&symbol);
                    }
                    if value_trusted_builtin == Some(symbol) {
                        self.trusted_builtin_symbols.insert(symbol);
                    } else {
                        self.trusted_builtin_symbols.remove(&symbol);
                    }
                }
            }
            Some(target) => {
                let sealed = mutation_owner_symbol(&target, self.resolution)
                    .is_some_and(|symbol| self.annotated_symbols.contains(&symbol));
                let unbound_deferred_empty_map_target = match &target {
                    syntax::Expr::Index(index) => child_expr(index.syntax(), 0)
                        .and_then(|owner| expression_symbol(&owner, self.resolution))
                        .and_then(|symbol| self.symbol_types.get(&symbol))
                        .is_some_and(|ty| self.is_unbound_deferred_empty_map(ty)),
                    _ => false,
                };
                let expected = if sealed || !unbound_deferred_empty_map_target {
                    self.expression(target.clone())
                } else {
                    Type::Unknown
                };
                let resolved_expected = self.resolve_type(expected.clone());
                let checked_value = value_node
                    .as_ref()
                    .and_then(direct_literal_type)
                    .filter(|literal| type_contains_exact(&resolved_expected, literal))
                    .unwrap_or_else(|| value.clone());
                if sealed {
                    self.constrain(&expected, &checked_value, span(node.syntax()));
                }
                let retain_singleton_union = sealed
                    && matches!(
                        &resolved_expected,
                        Type::Union(items)
                            if items.len() > 1 && items.iter().all(type_contains_singleton)
                    );
                let updated_value = if retain_singleton_union {
                    &resolved_expected
                } else {
                    &checked_value
                };
                match target {
                    syntax::Expr::Index(index) => {
                        self.apply_index_assignment(&index, updated_value)
                    }
                    syntax::Expr::Field(field) => {
                        self.apply_field_assignment(&field, updated_value)
                    }
                    _ => {}
                }
            }
            None => {}
        }
        value
    }

    pub(super) fn capture_mutation_is_compatible(
        &mut self,
        symbol: SymbolId,
        updated: &Type,
        at: Span,
    ) -> bool {
        let captured = self
            .assignment_effect_frames
            .last()
            .is_some_and(|(captures, _)| captures.contains(&symbol));
        if !captured {
            return true;
        }
        let bound = self
            .symbol_bounds
            .get(&symbol)
            .cloned()
            .unwrap_or(Type::Unknown);
        if self.is_unbound_deferred_empty_map(&bound)
            || !is_subtype(updated, &self.resolve_type(bound))
        {
            self.diagnostic(
                AnalysisDiagnosticCode::TypeMismatch,
                "Captured mutation exceeds declared type",
                "Structural widening is inferred only in a binding's defining scope; annotate the captured binding with a type that admits this mutation.".to_owned(),
                at,
            );
            return false;
        }
        true
    }
    fn is_unbound_deferred_empty_map(&self, ty: &Type) -> bool {
        let Type::Map {
            index: Some((key, value)),
            ..
        } = ty
        else {
            return false;
        };
        self.is_deferred_empty_map_index(key, value)
            && matches!(self.resolve_type((**key).clone()), Type::Infer(_))
            && matches!(self.resolve_type((**value).clone()), Type::Infer(_))
    }
    pub(super) fn apply_field_assignment(&mut self, field: &syntax::FieldExpr, value: &Type) {
        let Some(owner) = child_expr(field.syntax(), 0) else {
            return;
        };
        let Some(symbol) = expression_symbol(&owner, self.resolution) else {
            self.invalidate_mutated_owner(&owner);
            return;
        };
        let Some(name) = direct_token(field.syntax(), K::IDENT) else {
            return;
        };
        if self
            .symbol_regions
            .get(&symbol)
            .is_some_and(|region| self.conservative_regions.contains(region))
        {
            self.invalidate_mutated_owner(&owner);
            return;
        }
        let current = self
            .symbol_types
            .get(&symbol)
            .cloned()
            .map(|ty| self.resolve_type(ty))
            .unwrap_or(Type::Unknown);
        let updated = update_map_field(current, name.text(), value.clone());
        if !self.capture_mutation_is_compatible(symbol, &updated, span(field.syntax())) {
            return;
        }
        self.record_mutation(symbol);
        self.update_region_or_symbol(symbol, updated);
    }
    pub(super) fn apply_index_assignment(&mut self, index: &syntax::IndexExpr, value: &Type) {
        let mut children = expr_children(index.syntax());
        let Some(object) = children.next() else {
            return;
        };
        let key = children.next();
        let key_type = key
            .as_ref()
            .and_then(|key| {
                self.expression_types
                    .iter()
                    .rev()
                    .find(|(at, _)| *at == span(key.syntax()))
                    .map(|(_, ty)| ty.clone())
            })
            .unwrap_or_else(|| {
                key.clone()
                    .map(|key| self.expression(key))
                    .unwrap_or(Type::Unknown)
            });
        let Some(symbol) = expression_symbol(&object, self.resolution) else {
            self.invalidate_mutated_owner(&object);
            return;
        };
        if self
            .symbol_regions
            .get(&symbol)
            .is_some_and(|region| self.conservative_regions.contains(region))
        {
            self.invalidate_mutated_owner(&object);
            return;
        }
        let current = self
            .symbol_types
            .get(&symbol)
            .cloned()
            .unwrap_or(Type::Unknown);
        let deferred_empty_map_index = match &current {
            Type::Map {
                index: Some((key, value)),
                ..
            } if self.is_deferred_empty_map_index(key, value) => Some((key.clone(), value.clone())),
            _ => None,
        };
        let updated = if let Some((key_hole, value_hole)) = deferred_empty_map_index {
            let holes_are_unbound =
                matches!(self.resolve_type((*key_hole).clone()), Type::Infer(_))
                    && matches!(self.resolve_type((*value_hole).clone()), Type::Infer(_));
            if *value == Type::Nil && holes_are_unbound {
                return;
            }
            let Type::Map { fields, open, .. } = &current else {
                unreachable!("deferred empty map indexes belong to maps")
            };
            let updated = Type::Map {
                fields: fields.clone(),
                index: Some((Box::new(key_type.clone()), Box::new(value.clone()))),
                open: *open,
            };
            if !self.capture_mutation_is_compatible(symbol, &updated, span(index.syntax())) {
                return;
            }
            self.bind_infer(*key_hole, key_type);
            self.bind_infer(*value_hole, value.clone());
            current
        } else {
            match self.resolve_type(current) {
                Type::ListExact(mut items) => {
                    let literal_index = key.as_ref().and_then(|key| {
                        let syntax::Expr::Literal(literal) = key else {
                            return None;
                        };
                        direct_token(literal.syntax(), K::INT)?
                            .text()
                            .parse::<usize>()
                            .ok()
                    });
                    if let Some(index) = literal_index {
                        if let Some(item) = items.get_mut(index) {
                            *item = value.clone();
                        }
                    } else {
                        for item in &mut items {
                            *item = union(vec![item.clone(), value.clone()]);
                        }
                    }
                    Type::ListExact(items)
                }
                Type::ListRest(item) => Type::ListRest(Box::new(union(vec![*item, value.clone()]))),
                map @ Type::Map { .. } => {
                    if let Some(syntax::Expr::Literal(literal)) = key.as_ref()
                        && let Some(token) = direct_token(literal.syntax(), K::STRING)
                    {
                        update_map_field(map, &unquote(token.text()), value.clone())
                    } else {
                        widen_mutable_type(map)
                    }
                }
                _ => return,
            }
        };
        if !self.capture_mutation_is_compatible(symbol, &updated, span(index.syntax())) {
            return;
        }
        self.record_mutation(symbol);
        self.update_region_or_symbol(symbol, updated);
    }
    fn is_deferred_empty_map_index(&self, key: &Type, value: &Type) -> bool {
        matches!(key, Type::Infer(id) if self.deferred_empty_map_infers.contains(id))
            && matches!(value, Type::Infer(id) if self.deferred_empty_map_infers.contains(id))
    }
    pub(super) fn invalidate_mutated_owner(&mut self, owner: &syntax::Expr) {
        let Some(symbol) = mutation_root_symbol(owner, self.resolution) else {
            return;
        };
        if let Some(region) = self.symbol_regions.get(&symbol).copied()
            && self.conservative_regions.contains(&region)
        {
            self.widen_region_individually(region);
            return;
        }
        let current = self
            .symbol_regions
            .get(&symbol)
            .and_then(|region| {
                self.symbol_regions
                    .iter()
                    .filter(|(_, candidate)| *candidate == region)
                    .filter_map(|(alias, _)| self.symbol_types.get(alias))
                    .find(|ty| has_mutable_category(ty))
                    .cloned()
            })
            .or_else(|| self.symbol_types.get(&symbol).cloned())
            .map(widen_mutable_type)
            .unwrap_or(Type::Unknown);
        self.update_region_or_symbol(symbol, current);
    }
    pub(super) fn record_mutation(&mut self, symbol: SymbolId) {
        if let Some(frame) = self.mutation_effect_frames.last_mut() {
            frame.insert(symbol);
        }
    }
    pub(super) fn update_region_or_symbol(&mut self, symbol: SymbolId, ty: Type) {
        if let Some(region) = self.symbol_regions.get(&symbol).copied() {
            let aliases = self
                .symbol_regions
                .iter()
                .filter_map(|(symbol, candidate)| (*candidate == region).then_some(*symbol))
                .collect::<Vec<_>>();
            for alias in aliases {
                self.symbol_types.insert(alias, ty.clone());
                self.symbol_bounds.insert(alias, ty.clone());
            }
        } else {
            self.symbol_types.insert(symbol, ty.clone());
            self.symbol_bounds.insert(symbol, ty);
        }
    }
}
