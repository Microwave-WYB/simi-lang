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
        let value_posts = value_node
            .as_ref()
            .and_then(|expression| self.call_post_scheme(expression).map(|(_, posts)| posts));
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
                    if let Some(posts) = value_posts {
                        self.symbol_posts.insert(symbol, posts);
                    } else {
                        self.symbol_posts.remove(&symbol);
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
                let expected = self.expression(target.clone());
                if mutation_owner_symbol(&target, self.resolution)
                    .is_some_and(|symbol| self.annotated_symbols.contains(&symbol))
                {
                    self.constrain(&expected, &value, span(node.syntax()));
                }
                match target {
                    syntax::Expr::Index(index) => self.apply_index_assignment(&index, &value),
                    syntax::Expr::Field(field) => self.apply_field_assignment(&field, &value),
                    _ => {}
                }
            }
            None => {}
        }
        value
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
        self.record_mutation(symbol);
        self.update_region_or_symbol(symbol, updated);
    }
    pub(super) fn apply_index_assignment(&mut self, index: &syntax::IndexExpr, value: &Type) {
        let mut children = expr_children(index.syntax());
        let Some(object) = children.next() else {
            return;
        };
        let key = children.next();
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
            .map(|ty| self.resolve_type(ty))
            .unwrap_or(Type::Unknown);
        let updated = match current {
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
                let updated = if let Some(syntax::Expr::Literal(literal)) = key.as_ref()
                    && let Some(token) = direct_token(literal.syntax(), K::STRING)
                {
                    update_map_field(map, &unquote(token.text()), value.clone())
                } else {
                    widen_mutable_type(map)
                };
                self.update_region_or_symbol(symbol, updated);
                return;
            }
            _ => return,
        };
        self.record_mutation(symbol);
        self.update_region_or_symbol(symbol, updated);
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
