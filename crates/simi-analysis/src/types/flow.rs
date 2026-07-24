use super::*;

impl Context<'_> {
    pub(super) fn flow_state(&self) -> FlowState {
        FlowState {
            symbol_types: self.symbol_types.clone(),
            symbol_bounds: self.symbol_bounds.clone(),
            symbol_posts: self.symbol_posts.clone(),
            symbol_regions: self.symbol_regions.clone(),
            callable_capture_effects: self.callable_capture_effects.clone(),
            callable_assignment_effects: self.callable_assignment_effects.clone(),
            trusted_builtin_symbols: self.trusted_builtin_symbols.clone(),
        }
    }
    pub(super) fn restore_flow(&mut self, state: &FlowState) {
        self.symbol_types.clone_from(&state.symbol_types);
        self.symbol_bounds.clone_from(&state.symbol_bounds);
        self.symbol_posts.clone_from(&state.symbol_posts);
        self.symbol_regions.clone_from(&state.symbol_regions);
        self.callable_capture_effects
            .clone_from(&state.callable_capture_effects);
        self.callable_assignment_effects
            .clone_from(&state.callable_assignment_effects);
        self.trusted_builtin_symbols
            .clone_from(&state.trusted_builtin_symbols);
    }
    pub(super) fn restore_outer_flow(&mut self, state: &FlowState) {
        let outer_symbols = state
            .symbol_types
            .keys()
            .chain(state.symbol_bounds.keys())
            .chain(state.symbol_posts.keys())
            .chain(state.symbol_regions.keys())
            .chain(state.callable_capture_effects.keys())
            .chain(state.callable_assignment_effects.keys())
            .copied()
            .collect::<HashSet<_>>();
        self.trusted_builtin_symbols
            .clone_from(&state.trusted_builtin_symbols);
        for symbol in outer_symbols {
            restore_map_entry(&mut self.symbol_types, &state.symbol_types, symbol);
            restore_map_entry(&mut self.symbol_bounds, &state.symbol_bounds, symbol);
            restore_map_entry(&mut self.symbol_posts, &state.symbol_posts, symbol);
            restore_map_entry(&mut self.symbol_regions, &state.symbol_regions, symbol);
            restore_map_entry(
                &mut self.callable_capture_effects,
                &state.callable_capture_effects,
                symbol,
            );
            restore_map_entry(
                &mut self.callable_assignment_effects,
                &state.callable_assignment_effects,
                symbol,
            );
        }
    }
    pub(super) fn joined_flow(&mut self, states: Vec<FlowState>) -> Option<FlowState> {
        let mut states = states.into_iter();
        let first = states.next()?;
        let mut symbol_types = first.symbol_types;
        let mut symbol_bounds = first.symbol_bounds;
        let mut common_posts = first.symbol_posts;
        let mut common_regions = first.symbol_regions;
        let mut common_capture_effects = first.callable_capture_effects;
        let mut common_assignment_effects = first.callable_assignment_effects;
        let mut trusted_builtin_symbols = first.trusted_builtin_symbols;
        for state in states {
            for (symbol, ty) in state.symbol_types {
                symbol_types
                    .entry(symbol)
                    .and_modify(|current| *current = union(vec![current.clone(), ty.clone()]))
                    .or_insert(ty);
            }
            for (symbol, ty) in state.symbol_bounds {
                symbol_bounds
                    .entry(symbol)
                    .and_modify(|current| *current = union(vec![current.clone(), ty.clone()]))
                    .or_insert(ty);
            }
            common_posts.retain(|symbol, posts| state.symbol_posts.get(symbol) == Some(posts));
            common_regions.retain(|symbol, region| {
                state
                    .symbol_regions
                    .get(symbol)
                    .is_some_and(|other| other == region)
            });
            join_callable_effects(&mut common_capture_effects, &state.callable_capture_effects);
            join_callable_effects(
                &mut common_assignment_effects,
                &state.callable_assignment_effects,
            );
            trusted_builtin_symbols.retain(|symbol| state.trusted_builtin_symbols.contains(symbol));
        }
        Some(FlowState {
            symbol_types,
            symbol_bounds,
            symbol_posts: common_posts,
            symbol_regions: common_regions,
            callable_capture_effects: common_capture_effects,
            callable_assignment_effects: common_assignment_effects,
            trusted_builtin_symbols,
        })
    }
    pub(super) fn join_and_restore(&mut self, states: Vec<FlowState>) {
        if let Some(state) = self.joined_flow(states) {
            self.restore_flow(&state);
        }
    }
}
