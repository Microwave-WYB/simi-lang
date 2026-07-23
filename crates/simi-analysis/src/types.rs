use std::collections::{HashMap, HashSet};

use simi_syntax::ast as support;
use simi_syntax::generated::{self as syntax, AstNode};
use simi_syntax::span::Span;
use simi_syntax::{SyntaxKind as K, SyntaxNode, SyntaxToken};

use crate::db::{FileId, parse, resolve, source_text};
use crate::model::{
    AnalysisDiagnostic, AnalysisDiagnosticCode, AnalysisDiagnosticSeverity, ModuleShape,
    ParameterPostType, Resolution, SymbolId, Type, TypeInference,
};
use crate::modules::member_at;

#[derive(Clone)]
struct AliasDef {
    parameters: Vec<String>,
    body: SyntaxNode,
}

#[derive(Clone, Default)]
struct VarState {
    binding: Option<Type>,
}

pub fn infer_types(
    db: &dyn salsa::Database,
    file: FileId,
    modules: &HashMap<String, ModuleShape>,
) -> TypeInference {
    let parsed = parse(db, file);
    if !parsed.diagnostics.is_empty() {
        return TypeInference::default();
    }
    let resolution = resolve(db, file);
    let source = source_text(db, file);
    let root = parsed.syntax();
    let aliases = collect_aliases(&root);
    let trusted_builtin_symbols = resolution
        .hir
        .symbols
        .iter()
        .filter_map(|(symbol, data)| data.builtin.then_some(symbol))
        .collect();
    let mut context = Context {
        db,
        file,
        resolution: &resolution,
        modules,
        source: &source,
        aliases,
        alias_stack: HashSet::new(),
        vars: Vec::new(),
        symbol_types: builtin_types(&resolution),
        symbol_bounds: builtin_types(&resolution),
        symbol_posts: HashMap::new(),
        symbol_regions: HashMap::new(),
        conservative_regions: HashSet::new(),
        callable_capture_effects: HashMap::new(),
        callable_assignment_effects: HashMap::new(),
        anonymous_capture_effects: Vec::new(),
        anonymous_assignment_effects: Vec::new(),
        assignment_effect_frames: Vec::new(),
        annotated_symbols: HashSet::new(),
        trusted_builtin_symbols,
        next_region: 0,
        expression_types: Vec::new(),
        pattern_types: Vec::new(),
        loops: Vec::new(),
        nil_abort_states: Vec::new(),
        diagnostics: Vec::new(),
    };
    let root = syntax::Root::cast(root).expect("parser produces root");
    context.predeclare(root.statements());
    let root = syntax::Root::cast(parsed.syntax()).expect("parser produces root");
    context.statements(root.statements());
    context.finish()
}

pub fn symbol_type_at(
    inference: &TypeInference,
    resolution: &Resolution,
    offset: usize,
) -> Option<Type> {
    let (symbol, occurrence_span) = resolution.symbol_span_at(offset)?;
    let occurrence_type = inference
        .expression_types
        .iter()
        .chain(&inference.pattern_types)
        .filter(|(span, _)| *span == occurrence_span)
        .min_by_key(|(span, _)| span.end - span.start)
        .map(|(_, ty)| ty.clone());
    occurrence_type.or_else(|| inference.symbol_types.get(&symbol).cloned())
}

pub fn expression_type_at(inference: &TypeInference, offset: usize) -> Option<(Span, Type)> {
    inference
        .expression_types
        .iter()
        .filter(|(span, _)| span.start <= offset && offset < span.end)
        .min_by_key(|(span, _)| span.end - span.start)
        .cloned()
}

pub fn wildcard_type_at(
    db: &dyn salsa::Database,
    file: FileId,
    inference: &TypeInference,
    offset: usize,
) -> Option<(Span, Type)> {
    let parsed = parse(db, file);
    parsed
        .syntax()
        .descendants()
        .filter_map(syntax::WildcardPattern::cast)
        .find_map(|wildcard| {
            let token = direct_token(wildcard.syntax(), K::IDENT)?;
            let token_span = token_span(&token);
            if offset < token_span.start || offset >= token_span.end {
                return None;
            }
            inference.pattern_types.iter().rev().find_map(|(at, ty)| {
                (*at == span(wildcard.syntax())).then(|| (token_span, ty.clone()))
            })
        })
}

pub fn field_type_at(
    db: &dyn salsa::Database,
    file: FileId,
    inference: &TypeInference,
    offset: usize,
) -> Option<(String, Span, Type)> {
    let parsed = parse(db, file);
    parsed
        .syntax()
        .descendants()
        .filter_map(syntax::FieldExpr::cast)
        .find_map(|field| {
            let token = direct_token(field.syntax(), K::IDENT)?;
            let token_span = token_span(&token);
            if offset < token_span.start || offset >= token_span.end {
                return None;
            }
            let expression_span = span(field.syntax());
            let ty = inference
                .expression_types
                .iter()
                .rev()
                .find_map(|(at, ty)| (*at == expression_span).then(|| ty.clone()))?;
            Some((token.text().to_owned(), token_span, ty))
        })
}

fn builtin_types(resolution: &Resolution) -> HashMap<SymbolId, Type> {
    let mut types = HashMap::new();
    for (id, symbol) in resolution.hir.symbols.iter() {
        if !symbol.builtin {
            continue;
        }
        let ty = match symbol.name.as_str() {
            "require" => Type::Function(vec![Type::String], Box::new(Type::Unknown)),
            "type" => Type::Function(vec![Type::Any], Box::new(Type::String)),
            "inspect" => Type::Function(vec![Type::Any], Box::new(Type::String)),
            _ => Type::Unknown,
        };
        types.insert(id, ty);
    }
    types
}

struct LoopContext {
    transitions: Vec<Type>,
    breaks: Vec<(Type, FlowState)>,
}

#[derive(Clone)]
struct FlowState {
    symbol_types: HashMap<SymbolId, Type>,
    symbol_bounds: HashMap<SymbolId, Type>,
    symbol_posts: HashMap<SymbolId, Vec<ParameterPostType>>,
    symbol_regions: HashMap<SymbolId, u32>,
    callable_capture_effects: HashMap<SymbolId, HashSet<SymbolId>>,
    callable_assignment_effects: HashMap<SymbolId, HashSet<SymbolId>>,
    trusted_builtin_symbols: HashSet<SymbolId>,
}

#[derive(Clone)]
enum TypeMatcher {
    Exact(Type),
    Category(&'static str),
}

struct Context<'a> {
    db: &'a dyn salsa::Database,
    file: FileId,
    resolution: &'a Resolution,
    modules: &'a HashMap<String, ModuleShape>,
    source: &'a str,
    aliases: HashMap<String, AliasDef>,
    alias_stack: HashSet<String>,
    vars: Vec<VarState>,
    symbol_types: HashMap<SymbolId, Type>,
    symbol_bounds: HashMap<SymbolId, Type>,
    symbol_posts: HashMap<SymbolId, Vec<ParameterPostType>>,
    symbol_regions: HashMap<SymbolId, u32>,
    conservative_regions: HashSet<u32>,
    callable_capture_effects: HashMap<SymbolId, HashSet<SymbolId>>,
    callable_assignment_effects: HashMap<SymbolId, HashSet<SymbolId>>,
    anonymous_capture_effects: Vec<(Span, HashSet<SymbolId>)>,
    anonymous_assignment_effects: Vec<(Span, HashSet<SymbolId>)>,
    assignment_effect_frames: Vec<(HashSet<SymbolId>, HashSet<SymbolId>)>,
    annotated_symbols: HashSet<SymbolId>,
    trusted_builtin_symbols: HashSet<SymbolId>,
    next_region: u32,
    expression_types: Vec<(Span, Type)>,
    pattern_types: Vec<(Span, Type)>,
    loops: Vec<LoopContext>,
    nil_abort_states: Vec<Vec<FlowState>>,
    diagnostics: Vec<AnalysisDiagnostic>,
}

impl Context<'_> {
    fn finish(mut self) -> TypeInference {
        let symbols = std::mem::take(&mut self.symbol_types);
        let symbol_types = symbols
            .into_iter()
            .map(|(symbol, ty)| (symbol, public_type(self.generalize(ty))))
            .collect();
        let expression_types = std::mem::take(&mut self.expression_types)
            .into_iter()
            .map(|(span, ty)| (span, public_type(self.generalize(ty))))
            .collect();
        let pattern_types = std::mem::take(&mut self.pattern_types)
            .into_iter()
            .map(|(span, ty)| (span, public_type(self.generalize(ty))))
            .collect();
        let posts = std::mem::take(&mut self.symbol_posts);
        let symbol_posts = posts
            .into_iter()
            .map(|(symbol, posts)| {
                let posts = posts
                    .into_iter()
                    .map(|post| ParameterPostType {
                        becomes: public_type(self.generalize(post.becomes)),
                        ..post
                    })
                    .collect();
                (symbol, posts)
            })
            .collect();
        TypeInference {
            symbol_types,
            symbol_posts,
            expression_types,
            pattern_types,
            diagnostics: self.diagnostics,
        }
    }

    fn fresh(&mut self) -> Type {
        let id = self.vars.len() as u32;
        self.vars.push(VarState::default());
        Type::Infer(id)
    }

    fn flow_state(&self) -> FlowState {
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

    fn restore_flow(&mut self, state: &FlowState) {
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

    fn restore_outer_flow(&mut self, state: &FlowState) {
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

    fn joined_flow(&mut self, states: Vec<FlowState>) -> Option<FlowState> {
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

    fn join_and_restore(&mut self, states: Vec<FlowState>) {
        if let Some(state) = self.joined_flow(states) {
            self.restore_flow(&state);
        }
    }

    fn predeclare(&mut self, statements: impl Iterator<Item = syntax::Stmt>) {
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
            let parameter_nodes = support::child::<syntax::ParamList>(function.syntax())
                .map(|list| support::children::<syntax::Param>(list.syntax()).collect::<Vec<_>>())
                .unwrap_or_default();
            let parameter_names = parameter_nodes
                .iter()
                .filter_map(|parameter| direct_token(parameter.syntax(), K::IDENT))
                .map(|token| token.text().to_owned())
                .collect::<Vec<_>>();
            let parameters = parameter_nodes
                .into_iter()
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
            let mut posts = Vec::new();
            let mut post_parameters = HashSet::new();
            for post in support::children::<syntax::PostCondition>(function.syntax()) {
                let Some(name) =
                    direct_token(post.syntax(), K::IDENT).map(|token| token.text().to_owned())
                else {
                    continue;
                };
                let Some(parameter_index) = parameter_names
                    .iter()
                    .position(|candidate| candidate == &name)
                else {
                    self.diagnostic(
                        AnalysisDiagnosticCode::InvalidType,
                        "Unknown post-type parameter",
                        format!("Function parameter `{name}` does not exist."),
                        span(post.syntax()),
                    );
                    continue;
                };
                if !post_parameters.insert(parameter_index) {
                    self.diagnostic(
                        AnalysisDiagnosticCode::InvalidType,
                        "Duplicate post-type",
                        format!("Parameter `{name}` already has a post-type."),
                        span(post.syntax()),
                    );
                    continue;
                }
                let Some(becomes) = support::child::<syntax::TypeExpr>(post.syntax())
                    .map(|ty| self.parse_type(ty.syntax(), &mut generics))
                else {
                    continue;
                };
                posts.push(ParameterPostType {
                    parameter_index,
                    parameter_name: name,
                    becomes,
                });
            }
            let function_ty = Type::Function(parameters, Box::new(result));
            self.symbol_types.insert(symbol, function_ty.clone());
            self.symbol_bounds.insert(symbol, function_ty);
            self.symbol_posts.insert(symbol, posts);
        }
    }

    fn statements(&mut self, statements: impl Iterator<Item = syntax::Stmt>) -> Type {
        let mut result = Type::Nil;
        for statement in statements {
            if result == Type::Never {
                break;
            }
            result = self.statement(statement);
        }
        result
    }

    fn statement(&mut self, statement: syntax::Stmt) -> Type {
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
                let annotation = support::child::<syntax::TypeAnnotation>(statement.syntax())
                    .and_then(|annotation| support::child::<syntax::TypeExpr>(annotation.syntax()))
                    .map(|ty| self.parse_type(ty.syntax(), &mut HashMap::new()));
                let explicitly_annotated = annotation.is_some();
                let final_ty = if let Some(expected) = annotation {
                    self.require_subtype(&value, &expected, span(statement.syntax()));
                    expected
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
                        if let Some(posts) = inherited_posts {
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

    fn expression_region(&mut self, expression: &syntax::Expr) -> Option<u32> {
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

    fn infer_function_decl(&mut self, function: syntax::FunctionDecl) {
        let Some(name) = direct_token(function.syntax(), K::IDENT) else {
            return;
        };
        let Some(symbol) = self.resolution.symbol_at(token_span(&name).start) else {
            return;
        };
        let Some(Type::Function(parameters, expected_result)) =
            self.symbol_types.get(&symbol).cloned()
        else {
            return;
        };
        let outer_flow = self.flow_state();
        let outer_nil_aborts = std::mem::take(&mut self.nil_abort_states);
        let function_span = span(function.syntax());
        let capture_effects = self.function_captures(function_span);
        self.assignment_effect_frames
            .push((capture_effects.clone(), HashSet::new()));
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
        let trusted_host_wrapper = body.as_ref().is_some_and(is_host_wrapper);
        let actual = body
            .as_ref()
            .map(|body| self.statements(body.statements()))
            .unwrap_or(Type::Nil);
        let assignment_effects = self
            .assignment_effect_frames
            .pop()
            .map(|(_, assigned)| assigned)
            .unwrap_or_default();
        let resolved_result = self.resolve_type((*expected_result).clone());
        let resolved_actual = self.resolve_type(actual.clone());
        if let Type::Infer(id) = resolved_result
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
        let post_nodes =
            support::children::<syntax::PostCondition>(function.syntax()).collect::<Vec<_>>();
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
                if declared.contains(&parameter_index) {
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

        let function_ty = Type::Function(parameters, Box::new(self.resolve_type(*expected_result)));
        let resolved_function = self.resolve_type(function_ty);
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

    fn expression(&mut self, expression: syntax::Expr) -> Type {
        let expression_span = span(expression.syntax());
        let ty = match expression {
            syntax::Expr::Literal(node) => literal_type(node.syntax()),
            syntax::Expr::Name(node) => direct_token(node.syntax(), K::IDENT)
                .and_then(|token| self.resolution.symbol_at(token_span(&token).start))
                .and_then(|symbol| self.symbol_types.get(&symbol))
                .cloned()
                .map(|ty| self.instantiate(ty))
                .unwrap_or(Type::Unknown),
            syntax::Expr::Function(node) => self.infer_anonymous(node),
            syntax::Expr::Block(node) => {
                self.nil_abort_states.push(Vec::new());
                let result = support::child::<syntax::Block>(node.syntax())
                    .map(|block| self.statements(block.statements()))
                    .unwrap_or(Type::Nil);
                let aborts = self.nil_abort_states.pop().unwrap_or_default();
                if aborts.is_empty() {
                    result
                } else {
                    let mut exits = aborts;
                    if result != Type::Never {
                        exits.push(self.flow_state());
                    }
                    self.join_and_restore(exits);
                    union(vec![result, Type::Nil])
                }
            }
            syntax::Expr::Paren(node) => child_expr(node.syntax(), 0)
                .map(|child| self.expression(child))
                .unwrap_or(Type::Unknown),
            syntax::Expr::List(node) => Type::ListExact(
                expr_children(node.syntax())
                    .map(|item| self.expression(item))
                    .collect(),
            ),
            syntax::Expr::Map(node) => {
                let mut fields = Vec::new();
                let mut keys = Vec::new();
                let mut values = Vec::new();
                let mut open = false;
                for entry in support::children::<syntax::MapEntry>(node.syntax()) {
                    let mut expressions = expr_children(entry.syntax());
                    if let Some(name) = direct_token(entry.syntax(), K::IDENT) {
                        if let Some(value) = expressions.next() {
                            let value = self.expression(value);
                            if value != Type::Nil && !type_may_be_nil(&value) {
                                fields.push((name.text().to_owned(), value));
                            } else if value != Type::Nil {
                                // The type model has no optional fields. An entry whose value may
                                // be nil may be omitted at runtime, so retain only an open-map fact.
                                open = true;
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
            syntax::Expr::Try(node) => {
                let mut results = Vec::new();
                if let Some(block) = support::child::<syntax::Block>(node.syntax()) {
                    results.push(self.statements(block.statements()));
                }
                for clause in support::children::<syntax::CatchClause>(node.syntax()) {
                    if let Some(pattern) = support::child::<syntax::Pattern>(clause.syntax()) {
                        self.bind_pattern(pattern, Type::Any);
                    }
                    if let Some(guard) = support::child::<syntax::Expr>(clause.syntax()) {
                        let guard = self.expression(guard);
                        self.constrain(&Type::Boolean, &guard, span(clause.syntax()));
                    }
                    if let Some(block) = support::child::<syntax::Block>(clause.syntax()) {
                        results.push(self.statements(block.statements()));
                    }
                }
                union(results)
            }
            syntax::Expr::Raise(node) => {
                if let Some(value) = support::child::<syntax::Expr>(node.syntax()) {
                    let _ = self.expression(value);
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
            syntax::Expr::Loop(node) => {
                const MAX_LOOP_PASSES: usize = 8;

                let initial = child_expr(node.syntax(), 0)
                    .map(|child| self.expression(child))
                    .unwrap_or(Type::Nil);
                let state_symbol = direct_token(node.syntax(), K::IDENT)
                    .and_then(|token| self.resolution.symbol_at(token_span(&token).start));
                if let Some(symbol) = state_symbol {
                    self.symbol_types.insert(symbol, initial.clone());
                }

                let entry_flow = self.flow_state();
                let entry_nil_aborts = self.nil_abort_states.clone();
                let entry_assignment_effects = self.assignment_effect_frames.clone();
                let entry_conservative_regions = self.conservative_regions.clone();
                let entry_next_region = self.next_region;
                let expression_count = self.expression_types.len();
                let pattern_count = self.pattern_types.len();
                let diagnostic_count = self.diagnostics.len();
                let anonymous_capture_count = self.anonymous_capture_effects.len();
                let anonymous_assignment_count = self.anonymous_assignment_effects.len();
                let mut state = self.resolve_type(initial);
                let mut breaks = Vec::new();

                for pass in 0..=MAX_LOOP_PASSES {
                    self.restore_flow(&entry_flow);
                    self.nil_abort_states = entry_nil_aborts.clone();
                    self.assignment_effect_frames = entry_assignment_effects.clone();
                    self.conservative_regions = entry_conservative_regions.clone();
                    self.next_region = entry_next_region;
                    self.expression_types.truncate(expression_count);
                    self.pattern_types.truncate(pattern_count);
                    self.diagnostics.truncate(diagnostic_count);
                    self.anonymous_capture_effects
                        .truncate(anonymous_capture_count);
                    self.anonymous_assignment_effects
                        .truncate(anonymous_assignment_count);
                    if let Some(symbol) = state_symbol {
                        self.symbol_types.insert(symbol, state.clone());
                    }

                    self.loops.push(LoopContext {
                        transitions: Vec::new(),
                        breaks: Vec::new(),
                    });
                    let ordinary = support::child::<syntax::Block>(node.syntax())
                        .map(|block| self.statements(block.statements()))
                        .unwrap_or(Type::Nil);
                    let mut context = self.loops.pop().expect("loop inference context");
                    if ordinary != Type::Never {
                        context.transitions.push(ordinary);
                    }
                    let evolved = context.transitions.into_iter().fold(
                        state.clone(),
                        |current, transition| {
                            join_loop_state(current, self.resolve_type(transition))
                        },
                    );
                    if evolved == state {
                        breaks = context.breaks;
                        if let Some(symbol) = state_symbol {
                            self.symbol_types.insert(symbol, state.clone());
                        }
                        break;
                    }
                    state = if pass + 1 == MAX_LOOP_PASSES {
                        widen_mutable_type(evolved)
                    } else {
                        evolved
                    };
                    if pass == MAX_LOOP_PASSES {
                        unreachable!("a conservatively widened loop state must stabilize")
                    }
                }

                if breaks.is_empty() {
                    Type::Never
                } else {
                    let (types, exits): (Vec<_>, Vec<_>) = breaks.into_iter().unzip();
                    self.join_and_restore(exits);
                    union(types)
                }
            }
            syntax::Expr::Continue(node) => {
                let value = child_expr(node.syntax(), 0)
                    .map(|child| self.expression(child))
                    .unwrap_or(Type::Nil);
                if let Some(context) = self.loops.last_mut() {
                    context.transitions.push(value);
                }
                Type::Never
            }
            syntax::Expr::Break(node) => {
                let value = child_expr(node.syntax(), 0)
                    .map(|child| self.expression(child))
                    .unwrap_or(Type::Nil);
                let exit = self.flow_state();
                if let Some(context) = self.loops.last_mut() {
                    context.breaks.push((value, exit));
                }
                Type::Never
            }
        };
        self.expression_types.push((expression_span, ty.clone()));
        ty
    }

    fn infer_anonymous(&mut self, node: syntax::FunctionExpr) -> Type {
        let outer_flow = self.flow_state();
        let outer_nil_aborts = std::mem::take(&mut self.nil_abort_states);
        let function_span = span(node.syntax());
        let capture_effects = self.function_captures(function_span);
        self.assignment_effect_frames
            .push((capture_effects.clone(), HashSet::new()));
        let mut generics = HashMap::new();
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
                        }
                        ty
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let expected = support::child::<syntax::ReturnAnnotation>(node.syntax())
            .and_then(|annotation| support::child::<syntax::TypeExpr>(annotation.syntax()))
            .map(|annotation| self.parse_type(annotation.syntax(), &mut generics));
        let actual = support::child::<syntax::Block>(node.syntax())
            .map(|body| self.statements(body.statements()))
            .unwrap_or(Type::Nil);
        let assignment_effects = self
            .assignment_effect_frames
            .pop()
            .map(|(_, assigned)| assigned)
            .unwrap_or_default();
        let result = if let Some(expected) = expected {
            self.require_subtype(&actual, &expected, span(node.syntax()));
            expected
        } else {
            actual
        };
        let function_ty = self.generalize(Type::Function(parameters, Box::new(result)));
        self.restore_outer_flow(&outer_flow);
        self.nil_abort_states = outer_nil_aborts;
        self.anonymous_capture_effects
            .push((function_span, capture_effects));
        self.anonymous_assignment_effects
            .push((function_span, assignment_effects));
        function_ty
    }

    fn infer_if(&mut self, node: syntax::IfExpr) -> Type {
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
                    .map(|block| self.statements(block.statements()))
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
                let result = self.statements(block.statements());
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

    fn infer_case(&mut self, node: syntax::CaseExpr) -> Type {
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
                        let result = support::child::<syntax::Block>(clause.syntax())
                            .map(|block| self.statements(block.statements()))
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
                    let result = support::child::<syntax::Block>(clause.syntax())
                        .map(|block| self.statements(block.statements()))
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

    fn constrain_pattern_domain(&mut self, source: &Type, pattern: &syntax::Pattern) {
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

    fn pattern_domain(&mut self, pattern: &syntax::Pattern) -> Option<Type> {
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

    fn unresolved_case_domain(&mut self, node: &syntax::CaseExpr) -> Option<Type> {
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

    fn refine_condition(&mut self, expression: &syntax::Expr, truth: bool) -> bool {
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

    fn refine_comparison(
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

    fn type_test(
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

    fn refine_place(
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

    fn refine_place_to(&mut self, expression: &syntax::Expr, ty: Type) -> bool {
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

    fn set_refined_symbol(&mut self, symbol: SymbolId, ty: Type) {
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

    fn unary(&mut self, node: syntax::UnaryExpr) -> Type {
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

    fn binary(&mut self, node: syntax::BinaryExpr) -> Type {
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

    fn numeric_operand(&mut self, ty: Type, at: Span) -> Type {
        let resolved = self.resolve_type(ty.clone());
        if matches!(resolved, Type::Infer(_)) {
            self.bind_infer(ty, numeric());
            return numeric();
        }
        if is_subtype(&resolved, &numeric()) || matches!(resolved, Type::Any | Type::Unknown) {
            resolved
        } else {
            self.invalid_operator(at, &resolved, None);
            Type::Unknown
        }
    }

    fn numeric_operands(&mut self, left: Type, right: Type, at: Span) -> Option<(Type, Type)> {
        let left = self.resolve_type(left);
        let right = self.resolve_type(right);
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

    fn numeric_binary(&mut self, left: Type, right: Type, operator: K, at: Span) -> Type {
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

    fn pipeline(&mut self, node: syntax::PipelineExpr) -> Type {
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

    fn pipeline_stage_active(
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
        let post_scheme = self.call_post_scheme(&callee_node);
        let callee = self.expression(callee_node.clone());
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
        let result = self.apply_call_type(callee, &arguments, span(stage.syntax()));
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
        if let Some((parameters, posts)) = &post_scheme {
            self.apply_parameter_posts(parameters, posts, &effect_nodes, &arguments);
        }
        if let Some((module, field)) = &member {
            self.apply_call_effect(module, field, &effect_nodes, &arguments);
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
                &argument_nodes,
                member.is_some() || contract_complete,
            );
        }
        if tap {
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

    fn trailing_call(&mut self, node: syntax::TrailingArgumentExpr) -> Type {
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
        let post_scheme = self.call_post_scheme(&callee_node);
        let callee = self.expression(callee_node.clone());
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
        let result = self.apply_call_type(callee, &arguments, span(node.syntax()));
        if let Some((parameters, posts)) = &post_scheme {
            self.apply_parameter_posts(parameters, posts, &argument_nodes, &arguments);
        }
        if let Some((module, field)) = &member {
            self.apply_call_effect(module, field, &argument_nodes, &arguments);
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
        result
    }

    fn apply_call_type(&mut self, callee: Type, arguments: &[Type], at: Span) -> Type {
        match self.resolve_type(callee) {
            Type::Function(parameters, result) => {
                if parameters.len() != arguments.len() {
                    self.diagnostic(
                        AnalysisDiagnosticCode::WrongArity,
                        "Wrong number of arguments",
                        format!(
                            "Expected {} arguments, but received {}.",
                            parameters.len(),
                            arguments.len()
                        ),
                        at,
                    );
                }
                for (actual, expected) in arguments.iter().zip(&parameters) {
                    self.constrain(expected, actual, at);
                }
                self.resolve_type(*result)
            }
            Type::Any => Type::Any,
            Type::Unknown | Type::Infer(_) => Type::Unknown,
            other => {
                self.diagnostic(
                    AnalysisDiagnosticCode::NotCallable,
                    "Value is not callable",
                    format!("A value of type `{}` cannot be called.", other.display()),
                    at,
                );
                Type::Unknown
            }
        }
    }

    fn call(&mut self, node: syntax::CallExpr) -> Type {
        let Some(callee_node) = child_expr(node.syntax(), 0) else {
            return Type::Unknown;
        };
        let member = self.call_member(&callee_node);
        let post_scheme = self.call_post_scheme(&callee_node);
        let callee = self.expression(callee_node.clone());
        let callable = type_may_be_callable(&self.resolve_type(callee.clone()));
        let arguments = support::child::<syntax::ArgList>(node.syntax())
            .map(|list| expr_children(list.syntax()).collect::<Vec<_>>())
            .unwrap_or_default();
        let argument_types = arguments
            .iter()
            .cloned()
            .map(|argument| self.expression(argument))
            .collect::<Vec<_>>();
        let result = self.apply_call_type(callee, &argument_types, span(node.syntax()));
        if let Some((parameters, posts)) = &post_scheme {
            self.apply_parameter_posts(parameters, posts, &arguments, &argument_types);
        }
        if let Some((module, field)) = &member {
            self.apply_call_effect(module, field, &arguments, &argument_types);
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
        result
    }

    fn function_captures(&self, function_span: Span) -> HashSet<SymbolId> {
        self.resolution
            .captures
            .iter()
            .filter(|capture| {
                self.resolution.hir.scopes[capture.function_scope].span == function_span
            })
            .map(|capture| capture.symbol)
            .collect()
    }

    fn callable_capture_effects(&self, expression: &syntax::Expr) -> Option<HashSet<SymbolId>> {
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

    fn callable_assignment_effects(&self, expression: &syntax::Expr) -> Option<HashSet<SymbolId>> {
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

    fn apply_callable_effects(
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

    fn invalidate_assigned_binding(&mut self, symbol: SymbolId) {
        self.symbol_types.insert(symbol, Type::Any);
        self.symbol_bounds.insert(symbol, Type::Any);
        self.symbol_posts.remove(&symbol);
        self.symbol_regions.remove(&symbol);
        self.callable_capture_effects.remove(&symbol);
        self.callable_assignment_effects.remove(&symbol);
        self.trusted_builtin_symbols.remove(&symbol);
    }

    fn widen_all_regions(&mut self) {
        let regions = self
            .symbol_regions
            .values()
            .copied()
            .collect::<HashSet<_>>();
        for region in regions {
            self.widen_region_individually(region);
        }
    }

    fn widen_region_individually(&mut self, region: u32) {
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

    fn callable_parameter_contract_complete(&self, callee: &syntax::Expr) -> bool {
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

    fn call_member(&self, callee: &syntax::Expr) -> Option<(String, String)> {
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

    fn call_post_scheme(
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
                let Type::Function(parameters, _) = ty else {
                    return None;
                };
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
                let Type::Function(parameters, _) = member.field.ty.clone()? else {
                    return None;
                };
                Some((parameters, member.field.posts))
            }
            _ => None,
        }
    }

    fn instantiate_parameter_posts(
        &self,
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
                    substitute_post_generics(post.becomes.clone(), &replacements),
                )
            })
            .collect()
    }

    fn apply_parameter_posts(
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

    fn apply_call_effect(
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

    fn invalidate_unmodeled_call_arguments(
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

    fn field(&mut self, node: syntax::FieldExpr) -> Type {
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

    fn index(&mut self, node: syntax::IndexExpr) -> Type {
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

    fn assignment(&mut self, node: syntax::AssignExpr) -> Type {
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

    fn apply_field_assignment(&mut self, field: &syntax::FieldExpr, value: &Type) {
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
        self.update_region_or_symbol(symbol, updated);
    }

    fn apply_index_assignment(&mut self, index: &syntax::IndexExpr, value: &Type) {
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
        self.update_region_or_symbol(symbol, updated);
    }

    fn invalidate_mutated_owner(&mut self, owner: &syntax::Expr) {
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

    fn update_region_or_symbol(&mut self, symbol: SymbolId, ty: Type) {
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

    fn bind_pattern(&mut self, pattern: syntax::Pattern, ty: Type) {
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

    fn parse_type(&mut self, node: &SyntaxNode, generics: &mut HashMap<String, u32>) -> Type {
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
                let mut children = node.children();
                let left = children
                    .next()
                    .map(|child| self.parse_type(&child, generics))
                    .unwrap_or(Type::Unknown);
                if let Some(right) = children.find(|child| child.kind() == K::TYPE_FUNCTION) {
                    let parameters = match left {
                        Type::FunctionArgs(items) => items,
                        other => vec![other],
                    };
                    Type::Function(parameters, Box::new(self.parse_type(&right, generics)))
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
                let next = generics.len() as u32;
                Type::Generic(*generics.entry(name).or_insert(next))
            }
            K::TYPE_LITERAL => literal_type(node),
            K::TYPE_PAREN => {
                let items = support::children::<syntax::TypeExpr>(node)
                    .map(|ty| self.parse_type(ty.syntax(), generics))
                    .collect::<Vec<_>>();
                match items.as_slice() {
                    [one] => one.clone(),
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

    fn expand_alias(
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
        for parameter in &alias.parameters {
            let next = alias_generics.len() as u32;
            alias_generics.insert(parameter.clone(), next);
        }
        let mut expanded = self.parse_type(&alias.body, &mut alias_generics);
        let replacements = arguments
            .into_iter()
            .enumerate()
            .map(|(index, ty)| (index as u32, ty))
            .collect::<HashMap<_, _>>();
        expanded = substitute_generics(expanded, &replacements);
        self.alias_stack.remove(name);
        let _ = outer;
        expanded
    }

    fn constrain(&mut self, expected: &Type, actual: &Type, at: Span) {
        let expected = self.resolve_type(expected.clone());
        let actual = self.resolve_type(actual.clone());
        match (&expected, &actual) {
            (Type::Any | Type::Unknown, _) | (_, Type::Any | Type::Unknown) => {}
            (Type::Infer(_), _) => self.bind_infer(expected, actual),
            (_, Type::Infer(_)) => self.bind_infer(actual, expected),
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
            (
                Type::Function(expected_parameters, expected_result),
                Type::Function(actual_parameters, actual_result),
            ) if expected_parameters.len() == actual_parameters.len() => {
                for (expected, actual) in expected_parameters.iter().zip(actual_parameters) {
                    if contains_infer(expected) {
                        self.constrain(expected, actual, at);
                    } else if !is_subtype(expected, actual) {
                        self.require_subtype(expected, actual, at);
                    }
                }
                self.constrain(expected_result, actual_result, at);
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

    fn bind_infer(&mut self, variable: Type, ty: Type) {
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

    fn resolve_type(&self, ty: Type) -> Type {
        self.resolve_type_inner(ty, &mut HashSet::new())
    }

    fn resolve_type_inner(&self, ty: Type, resolving: &mut HashSet<u32>) -> Type {
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
            Type::Function(parameters, result) => Type::Function(
                parameters
                    .into_iter()
                    .map(|parameter| self.resolve_type_inner(parameter, resolving))
                    .collect(),
                Box::new(self.resolve_type_inner(*result, resolving)),
            ),
            Type::FunctionArgs(items) => Type::FunctionArgs(
                items
                    .into_iter()
                    .map(|item| self.resolve_type_inner(item, resolving))
                    .collect(),
            ),
            Type::Union(items) => union(
                items
                    .into_iter()
                    .map(|item| self.resolve_type_inner(item, resolving))
                    .collect(),
            ),
            other => other,
        }
    }

    fn generalize(&self, ty: Type) -> Type {
        let resolved = self.resolve_type(ty);
        let mut next = max_generic(&resolved).map_or(0, |index| index + 1);
        let mut variables = HashMap::new();
        generalize_type(resolved, &mut variables, &mut next)
    }

    fn instantiate(&mut self, ty: Type) -> Type {
        let mut variables = HashMap::new();
        instantiate_type(ty, &mut variables, self)
    }

    fn require_subtype(&mut self, actual: &Type, expected: &Type, at: Span) {
        let actual = self.resolve_type(actual.clone());
        let expected = self.resolve_type(expected.clone());
        if !is_subtype(&actual, &expected) {
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

    fn invalid_operator(&mut self, at: Span, left: &Type, right: Option<&Type>) {
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

    fn diagnostic(
        &mut self,
        code: AnalysisDiagnosticCode,
        title: &str,
        detail: String,
        span: Span,
    ) {
        self.diagnostics.push(AnalysisDiagnostic {
            span,
            code,
            title: title.to_owned(),
            detail,
            severity: AnalysisDiagnosticSeverity::Error,
            related: Vec::new(),
        });
    }
}

fn restore_map_entry<K, V>(target: &mut HashMap<K, V>, source: &HashMap<K, V>, key: K)
where
    K: Copy + Eq + std::hash::Hash,
    V: Clone,
{
    if let Some(value) = source.get(&key) {
        target.insert(key, value.clone());
    } else {
        target.remove(&key);
    }
}

fn join_callable_effects(
    common: &mut HashMap<SymbolId, HashSet<SymbolId>>,
    other: &HashMap<SymbolId, HashSet<SymbolId>>,
) {
    common.retain(|symbol, effects| {
        let Some(other_effects) = other.get(symbol) else {
            return false;
        };
        effects.extend(other_effects);
        true
    });
}

fn collect_aliases(root: &SyntaxNode) -> HashMap<String, AliasDef> {
    let Some(root) = syntax::Root::cast(root.clone()) else {
        return HashMap::new();
    };
    root.statements()
        .filter_map(|statement| {
            let syntax::Stmt::AliasDecl(alias) = statement else {
                return None;
            };
            let contextual = direct_token(alias.syntax(), K::ALIAS_KW).is_none();
            let name = support::tokens(alias.syntax(), K::IDENT)
                .nth(usize::from(contextual))?
                .text()
                .to_owned();
            let parameters = support::child::<syntax::TypeParamList>(alias.syntax())
                .map(|list| {
                    support::children::<syntax::TypeVariable>(list.syntax())
                        .filter_map(|variable| direct_token(variable.syntax(), K::IDENT))
                        .map(|token| token.text().to_owned())
                        .collect()
                })
                .unwrap_or_default();
            let body = support::child::<syntax::TypeExpr>(alias.syntax())?
                .syntax()
                .clone();
            Some((name, AliasDef { parameters, body }))
        })
        .collect()
}

fn public_type(ty: Type) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Unknown | Type::Infer(_) => Type::Any,
        Type::FunctionArgs(_) => Type::Any,
        other => other,
    })
}

fn generalize_type(ty: Type, variables: &mut HashMap<u32, u32>, next: &mut u32) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Infer(id) => {
            let generic = *variables.entry(id).or_insert_with(|| {
                let generic = *next;
                *next += 1;
                generic
            });
            Type::Generic(generic)
        }
        other => other,
    })
}

fn max_generic(ty: &Type) -> Option<u32> {
    match ty {
        Type::Generic(id) => Some(*id),
        Type::ListExact(items) | Type::FunctionArgs(items) | Type::Union(items) => {
            items.iter().filter_map(max_generic).max()
        }
        Type::ListRest(item) => max_generic(item),
        Type::Map { fields, index, .. } => fields
            .iter()
            .filter_map(|(_, ty)| max_generic(ty))
            .chain(
                index
                    .iter()
                    .flat_map(|(key, value)| [max_generic(key), max_generic(value)])
                    .flatten(),
            )
            .max(),
        Type::Function(parameters, result) => parameters
            .iter()
            .filter_map(max_generic)
            .chain(max_generic(result))
            .max(),
        _ => None,
    }
}

fn instantiate_type(
    ty: Type,
    variables: &mut HashMap<u32, Type>,
    context: &mut Context<'_>,
) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Generic(id) => variables
            .entry(id)
            .or_insert_with(|| context.fresh())
            .clone(),
        other => other,
    })
}

fn collect_generic_replacements(
    parameter: &Type,
    actual: &Type,
    replacements: &mut HashMap<u32, Type>,
) {
    match (parameter, actual) {
        (Type::Generic(id), actual) => {
            replacements
                .entry(*id)
                .and_modify(|existing| *existing = union(vec![existing.clone(), actual.clone()]))
                .or_insert_with(|| actual.clone());
        }
        (Type::ListRest(parameter), Type::ListExact(actuals)) => {
            for actual in actuals {
                collect_generic_replacements(parameter, actual, replacements);
            }
        }
        (Type::ListRest(_), Type::ListRest(actual)) if **actual == Type::Never => {}
        (Type::ListRest(parameter), Type::ListRest(actual)) => {
            collect_generic_replacements(parameter, actual, replacements);
        }
        (Type::ListExact(parameters), Type::ListExact(actuals)) => {
            for (parameter, actual) in parameters.iter().zip(actuals) {
                collect_generic_replacements(parameter, actual, replacements);
            }
        }
        (Type::Function(parameters, result), Type::Function(actuals, actual_result)) => {
            for (parameter, actual) in parameters.iter().zip(actuals) {
                collect_generic_replacements(parameter, actual, replacements);
            }
            collect_generic_replacements(result, actual_result, replacements);
        }
        _ => {}
    }
}

fn substitute_generics(ty: Type, replacements: &HashMap<u32, Type>) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Generic(id) => replacements.get(&id).cloned().unwrap_or(Type::Generic(id)),
        other => other,
    })
}

fn substitute_post_generics(ty: Type, replacements: &HashMap<u32, Type>) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Generic(id) => replacements.get(&id).cloned().unwrap_or(Type::Never),
        other => other,
    })
}

fn map_type(ty: Type, mapper: &mut impl FnMut(Type) -> Type) -> Type {
    let mapped = match ty {
        Type::ListExact(items) => Type::ListExact(
            items
                .into_iter()
                .map(|item| map_type(item, mapper))
                .collect(),
        ),
        Type::ListRest(item) => Type::ListRest(Box::new(map_type(*item, mapper))),
        Type::Map {
            fields,
            index,
            open,
        } => Type::Map {
            fields: fields
                .into_iter()
                .map(|(name, ty)| (name, map_type(ty, mapper)))
                .collect(),
            index: index.map(|(key, value)| {
                (
                    Box::new(map_type(*key, mapper)),
                    Box::new(map_type(*value, mapper)),
                )
            }),
            open,
        },
        Type::Function(parameters, result) => Type::Function(
            parameters
                .into_iter()
                .map(|parameter| map_type(parameter, mapper))
                .collect(),
            Box::new(map_type(*result, mapper)),
        ),
        Type::FunctionArgs(items) => Type::FunctionArgs(
            items
                .into_iter()
                .map(|item| map_type(item, mapper))
                .collect(),
        ),
        Type::Union(items) => union(
            items
                .into_iter()
                .map(|item| map_type(item, mapper))
                .collect(),
        ),
        other => other,
    };
    mapper(mapped)
}

fn contains_specific_infer(ty: &Type, target: u32) -> bool {
    match ty {
        Type::Infer(id) => *id == target,
        Type::ListExact(items) | Type::FunctionArgs(items) | Type::Union(items) => items
            .iter()
            .any(|item| contains_specific_infer(item, target)),
        Type::ListRest(item) => contains_specific_infer(item, target),
        Type::Map { fields, index, .. } => {
            fields
                .iter()
                .any(|(_, ty)| contains_specific_infer(ty, target))
                || index.as_ref().is_some_and(|(key, value)| {
                    contains_specific_infer(key, target) || contains_specific_infer(value, target)
                })
        }
        Type::Function(parameters, result) => {
            parameters
                .iter()
                .any(|parameter| contains_specific_infer(parameter, target))
                || contains_specific_infer(result, target)
        }
        _ => false,
    }
}

fn remove_recursive_alternatives(ty: Type, target: u32) -> Type {
    match ty {
        Type::Union(items) => union(
            items
                .into_iter()
                .map(|item| {
                    if contains_specific_infer(&item, target) {
                        Type::Never
                    } else {
                        item
                    }
                })
                .collect(),
        ),
        other if contains_specific_infer(&other, target) => Type::Never,
        other => other,
    }
}

fn contains_infer(ty: &Type) -> bool {
    match ty {
        Type::Infer(_) => true,
        Type::ListExact(items) | Type::FunctionArgs(items) | Type::Union(items) => {
            items.iter().any(contains_infer)
        }
        Type::ListRest(item) => contains_infer(item),
        Type::Map { fields, index, .. } => {
            fields.iter().any(|(_, ty)| contains_infer(ty))
                || index
                    .as_ref()
                    .is_some_and(|(key, value)| contains_infer(key) || contains_infer(value))
        }
        Type::Function(parameters, result) => {
            parameters.iter().any(contains_infer) || contains_infer(result)
        }
        _ => false,
    }
}

fn list_append_result(list: Type, value: Type) -> Type {
    match list {
        Type::ListExact(mut items) => {
            items.push(value);
            Type::ListExact(items)
        }
        Type::ListRest(item) => Type::ListRest(Box::new(union(vec![*item, value]))),
        _ => Type::ListRest(Box::new(Type::Unknown)),
    }
}

fn join_loop_state(current: Type, transition: Type) -> Type {
    if current == transition {
        return current;
    }
    match (current, transition) {
        (Type::Never, other) | (other, Type::Never) => other,
        (Type::ListExact(left), Type::ListExact(right)) if left.len() == right.len() => {
            Type::ListExact(
                left.into_iter()
                    .zip(right)
                    .map(|(left, right)| union(vec![left, right]))
                    .collect(),
            )
        }
        (Type::ListExact(left), Type::ListExact(right)) => {
            Type::ListRest(Box::new(union(left.into_iter().chain(right).collect())))
        }
        (Type::ListRest(left), Type::ListRest(right)) => {
            Type::ListRest(Box::new(union(vec![*left, *right])))
        }
        (Type::ListExact(items), Type::ListRest(item))
        | (Type::ListRest(item), Type::ListExact(items)) => Type::ListRest(Box::new(union(
            items.into_iter().chain(std::iter::once(*item)).collect(),
        ))),
        (
            Type::Map {
                fields: left_fields,
                index: left_index,
                open: left_open,
            },
            Type::Map {
                fields: right_fields,
                index: right_index,
                open: right_open,
            },
        ) => {
            let fields = left_fields
                .iter()
                .filter_map(|(name, left)| {
                    right_fields
                        .iter()
                        .find(|(field, _)| field == name)
                        .map(|(_, right)| {
                            (name.clone(), join_loop_state(left.clone(), right.clone()))
                        })
                })
                .collect();
            let same_fields = left_fields
                .iter()
                .all(|(name, _)| right_fields.iter().any(|(field, _)| field == name))
                && right_fields
                    .iter()
                    .all(|(name, _)| left_fields.iter().any(|(field, _)| field == name));
            let index = match (left_index, right_index) {
                (Some((left_key, left_value)), Some((right_key, right_value))) => Some((
                    Box::new(union(vec![*left_key, *right_key])),
                    Box::new(join_loop_state(*left_value, *right_value)),
                )),
                (Some(index), None) | (None, Some(index)) => Some(index),
                (None, None) => None,
            };
            Type::Map {
                fields,
                index,
                open: left_open || right_open || !same_fields,
            }
        }
        (left, right) => union(vec![left, right]),
    }
}

fn union(items: Vec<Type>) -> Type {
    let mut flattened = Vec::new();
    let mut terminated = false;
    let mut pending = items.into_iter().rev().collect::<Vec<_>>();
    while let Some(item) = pending.pop() {
        match item {
            Type::Union(items) => pending.extend(items.into_iter().rev()),
            Type::Never => terminated = true,
            Type::Any => return Type::Any,
            item => flattened.push(item),
        }
    }
    let mut unique = Vec::new();
    for item in flattened {
        if !unique.contains(&item) {
            unique.push(item);
        }
    }
    unique.sort_by_key(type_order);
    let snapshot = unique.clone();
    unique.retain(|item| {
        !snapshot
            .iter()
            .any(|other| item != other && is_subtype(item, other))
    });
    match unique.as_slice() {
        [] if terminated => Type::Never,
        [] => Type::Unknown,
        [one] => one.clone(),
        _ => Type::Union(unique),
    }
}

fn type_order(ty: &Type) -> u8 {
    match ty {
        Type::Never => 0,
        Type::Boolean | Type::LiteralBoolean(_) => 1,
        Type::Int | Type::LiteralInt(_) => 2,
        Type::Float => 3,
        Type::String | Type::LiteralString(_) => 4,
        Type::ListExact(_) | Type::ListRest(_) => 5,
        Type::Map { .. } => 7,
        Type::Function(_, _) => 8,
        Type::Generic(_) | Type::Infer(_) => 9,
        Type::Nil => 10,
        Type::Unknown => 11,
        Type::Any => 12,
        Type::FunctionArgs(_) | Type::Union(_) => 13,
    }
}

fn equality_type(ty: &Type) -> bool {
    match ty {
        Type::Unknown
        | Type::Any
        | Type::Nil
        | Type::Boolean
        | Type::Int
        | Type::Float
        | Type::String
        | Type::LiteralInt(_)
        | Type::LiteralString(_)
        | Type::LiteralBoolean(_)
        | Type::Infer(_)
        | Type::Generic(_) => true,
        Type::Union(items) => items.iter().all(equality_type),
        _ => false,
    }
}

fn numeric() -> Type {
    union(vec![Type::Int, Type::Float])
}

fn numeric_atoms(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Int => vec![Type::Int],
        Type::Float => vec![Type::Float],
        Type::Union(items) => items.iter().flat_map(numeric_atoms).collect(),
        _ => Vec::new(),
    }
}

fn is_subtype(actual: &Type, expected: &Type) -> bool {
    if matches!(expected, Type::Any | Type::Unknown)
        || matches!(actual, Type::Never | Type::Unknown | Type::Any)
        || actual == expected
    {
        return true;
    }
    match (actual, expected) {
        (Type::LiteralInt(_), Type::Int) => true,
        (Type::LiteralString(_), Type::String) => true,
        (Type::LiteralBoolean(_), Type::Boolean) => true,
        (Type::Union(items), expected) => items.iter().all(|item| is_subtype(item, expected)),
        (actual, Type::Union(items)) => items.iter().any(|item| is_subtype(actual, item)),
        (Type::ListExact(actual), Type::ListRest(expected)) => {
            actual.iter().all(|actual| is_subtype(actual, expected))
        }
        (Type::ListRest(actual), Type::ListRest(expected)) => is_subtype(actual, expected),
        (Type::ListExact(actual), Type::ListExact(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| is_subtype(actual, expected))
        }
        (
            Type::Map {
                fields: actual,
                index: actual_index,
                open: actual_open,
            },
            Type::Map {
                fields: expected,
                index: expected_index,
                open,
            },
        ) => {
            let fields_match = expected.iter().all(|(name, expected)| {
                actual
                    .iter()
                    .find(|(field, _)| field == name)
                    .is_some_and(|(_, actual)| is_subtype(actual, expected))
            });
            let index_matches = expected_index.as_ref().is_none_or(|(key, value)| {
                actual
                    .iter()
                    .all(|(_, actual)| is_subtype(&Type::String, key) && is_subtype(actual, value))
                    && actual_index
                        .as_ref()
                        .is_none_or(|(actual_key, actual_value)| {
                            is_subtype(actual_key, key) && is_subtype(actual_value, value)
                        })
            });
            fields_match
                && index_matches
                && (*open
                    || expected_index.is_some()
                    || (!*actual_open && actual.len() == expected.len()))
        }
        (
            Type::Function(actual_parameters, actual_result),
            Type::Function(expected_parameters, expected_result),
        ) => {
            actual_parameters.len() == expected_parameters.len()
                && actual_parameters
                    .iter()
                    .zip(expected_parameters)
                    .all(|(actual, expected)| is_subtype(expected, actual))
                && is_subtype(actual_result, expected_result)
        }
        _ => false,
    }
}

fn known_module_argument_is_pure(module: &str, field: &str, index: usize) -> bool {
    match module {
        "std/list" => {
            matches!(
                field,
                "length" | "copy" | "get" | "slice" | "contains" | "append" | "iter"
            ) || (field != "fold" && index != 0)
        }
        "std/map" => index != 0 || field != "clear",
        "std/iter" | "std/number" | "std/string" | "std/io" => true,
        _ => false,
    }
}

fn widen_mutable_type(ty: Type) -> Type {
    match ty {
        Type::ListExact(_) | Type::ListRest(_) => Type::ListRest(Box::new(Type::Any)),
        Type::Map { .. } => Type::Map {
            fields: Vec::new(),
            index: None,
            open: true,
        },
        Type::Union(items) => union(items.into_iter().map(widen_mutable_type).collect()),
        other => other,
    }
}

fn update_map_field(ty: Type, field: &str, value: Type) -> Type {
    match ty {
        Type::Union(items) => union(
            items
                .into_iter()
                .map(|item| update_map_field(item, field, value.clone()))
                .collect(),
        ),
        Type::Map {
            mut fields,
            index,
            open,
        } => {
            let definitely_nil = value == Type::Nil;
            let may_delete = type_may_be_nil(&value);
            if !definitely_nil && !may_delete {
                if let Some((_, existing)) = fields.iter_mut().find(|(name, _)| name == field) {
                    *existing = value;
                } else {
                    fields.push((field.to_owned(), value));
                }
            } else {
                fields.retain(|(name, _)| name != field);
            }
            Type::Map {
                fields,
                index,
                open: open || (may_delete && !definitely_nil),
            }
        }
        other => other,
    }
}

fn has_mutable_category(ty: &Type) -> bool {
    match ty {
        Type::ListExact(_) | Type::ListRest(_) | Type::Map { .. } => true,
        Type::Union(items) => items.iter().any(has_mutable_category),
        _ => false,
    }
}

fn valid_post_transition(pre: &Type, post: &Type) -> bool {
    match post {
        Type::Union(items) => items.iter().all(|item| valid_post_transition(pre, item)),
        _ => match pre {
            Type::Any | Type::Unknown | Type::Generic(_) | Type::Infer(_) => true,
            Type::Union(items) => items.iter().any(|item| valid_post_transition(item, post)),
            Type::ListExact(_) | Type::ListRest(_) => {
                matches!(post, Type::ListExact(_) | Type::ListRest(_))
            }
            Type::Map { .. } => matches!(post, Type::Map { .. }),
            _ => is_subtype(post, pre),
        },
    }
}

fn block_ends_in_direct_call(body: &syntax::Block) -> bool {
    let Some(syntax::Stmt::ExprStmt(statement)) = body.statements().last() else {
        return false;
    };
    support::child::<syntax::Expr>(statement.syntax()).is_some_and(|expression| {
        matches!(expression, syntax::Expr::Call(_))
            || matches!(expression, syntax::Expr::Paren(ref paren) if child_expr(paren.syntax(), 0).is_some_and(|inner| matches!(inner, syntax::Expr::Call(_))))
    })
}

fn is_host_wrapper(body: &syntax::Block) -> bool {
    let mut statements = body.statements();
    let Some(syntax::Stmt::ExprStmt(statement)) = statements.next() else {
        return false;
    };
    if statements.next().is_some() {
        return false;
    }
    let Some(syntax::Expr::Call(call)) = support::child::<syntax::Expr>(statement.syntax()) else {
        return false;
    };
    let Some(syntax::Expr::Field(field)) = child_expr(call.syntax(), 0) else {
        return false;
    };
    let Some(name) = direct_token(field.syntax(), K::IDENT) else {
        return false;
    };
    let Some(syntax::Expr::Name(object)) = child_expr(field.syntax(), 0) else {
        return false;
    };
    name.text() == "call"
        && direct_token(object.syntax(), K::IDENT).is_some_and(|token| token.text() == "host")
}

fn binary_operator(node: &SyntaxNode) -> Option<K> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .map(|token| token.kind())
        .find(|kind| {
            matches!(
                kind,
                K::PLUS
                    | K::MINUS
                    | K::STAR
                    | K::SLASH
                    | K::SLASH_SLASH
                    | K::PERCENT
                    | K::LESS_GREATER
                    | K::EQ_EQ
                    | K::BANG_EQ
                    | K::LESS
                    | K::LESS_EQ
                    | K::GREATER
                    | K::GREATER_EQ
                    | K::AND_KW
                    | K::OR_KW
            )
        })
}

fn literal_string(expression: &syntax::Expr) -> Option<String> {
    let syntax::Expr::Literal(literal) = expression else {
        return None;
    };
    direct_token(literal.syntax(), K::STRING).map(|token| unquote(token.text()))
}

fn comparison_matcher(expression: &syntax::Expr) -> Option<TypeMatcher> {
    let syntax::Expr::Literal(literal) = expression else {
        return None;
    };
    if direct_token(literal.syntax(), K::NIL_KW).is_some() {
        return Some(TypeMatcher::Exact(Type::Nil));
    }
    if let Some(token) = direct_token(literal.syntax(), K::STRING) {
        return Some(TypeMatcher::Exact(Type::LiteralString(unquote(
            token.text(),
        ))));
    }
    if let Some(token) = direct_token(literal.syntax(), K::INT)
        && let Ok(value) = token.text().parse()
    {
        return Some(TypeMatcher::Exact(Type::LiteralInt(value)));
    }
    if direct_token(literal.syntax(), K::TRUE_KW).is_some() {
        return Some(TypeMatcher::Exact(Type::LiteralBoolean(true)));
    }
    if direct_token(literal.syntax(), K::FALSE_KW).is_some() {
        return Some(TypeMatcher::Exact(Type::LiteralBoolean(false)));
    }
    None
}

fn matcher_relation(ty: &Type, matcher: &TypeMatcher) -> Option<bool> {
    if matches!(
        ty,
        Type::Any | Type::Unknown | Type::Infer(_) | Type::Generic(_)
    ) {
        return None;
    }
    match matcher {
        TypeMatcher::Category(category) => Some(match *category {
            "nil" => matches!(ty, Type::Nil),
            "boolean" => matches!(ty, Type::Boolean | Type::LiteralBoolean(_)),
            "integer" => matches!(ty, Type::Int | Type::LiteralInt(_)),
            "float" => matches!(ty, Type::Float),
            "string" => matches!(ty, Type::String | Type::LiteralString(_)),
            "list" => matches!(ty, Type::ListExact(_) | Type::ListRest(_)),
            "map" => matches!(ty, Type::Map { .. }),
            "function" => matches!(ty, Type::Function(_, _)),
            _ => false,
        }),
        TypeMatcher::Exact(expected) => match (ty, expected) {
            (Type::String, Type::LiteralString(_))
            | (Type::Int, Type::LiteralInt(_))
            | (Type::Boolean, Type::LiteralBoolean(_)) => None,
            (Type::LiteralString(left), Type::LiteralString(right)) => Some(left == right),
            (Type::LiteralInt(left), Type::LiteralInt(right)) => Some(left == right),
            (Type::LiteralBoolean(left), Type::LiteralBoolean(right)) => Some(left == right),
            _ => Some(ty == expected),
        },
    }
}

fn narrow_type(ty: Type, matcher: &TypeMatcher, keep: bool) -> Type {
    if let Type::Union(items) = ty {
        return union(
            items
                .into_iter()
                .map(|item| narrow_type(item, matcher, keep))
                .collect(),
        );
    }
    match matcher_relation(&ty, matcher) {
        Some(matches) if matches == keep => ty,
        Some(_) => Type::Never,
        None if keep => match (&ty, matcher) {
            (Type::String, TypeMatcher::Exact(Type::LiteralString(value))) => {
                Type::LiteralString(value.clone())
            }
            _ => ty,
        },
        None => ty,
    }
}

fn narrow_map_field(ty: Type, field: &str, matcher: &TypeMatcher, keep: bool) -> Type {
    if let Type::Union(items) = ty {
        return union(
            items
                .into_iter()
                .map(|item| narrow_map_field(item, field, matcher, keep))
                .collect(),
        );
    }
    let Type::Map {
        mut fields,
        index,
        open,
    } = ty
    else {
        return ty;
    };
    if let Some((_, field_ty)) = fields.iter_mut().find(|(name, _)| name == field) {
        let narrowed = narrow_type(field_ty.clone(), matcher, keep);
        if narrowed == Type::Never {
            return Type::Never;
        }
        *field_ty = narrowed;
        return Type::Map {
            fields,
            index,
            open,
        };
    }
    if open || index.is_some() {
        return Type::Map {
            fields,
            index,
            open,
        };
    }
    let nil_matches = matcher_relation(&Type::Nil, matcher).unwrap_or(false);
    if nil_matches == keep {
        Type::Map {
            fields,
            index,
            open,
        }
    } else {
        Type::Never
    }
}

fn field_lookup_type(ty: Type, field: &str) -> Type {
    match ty {
        Type::Union(items) => union(
            items
                .into_iter()
                .map(|item| field_lookup_type(item, field))
                .collect(),
        ),
        Type::Map {
            fields,
            index,
            open,
        } => fields
            .into_iter()
            .find(|(name, _)| name == field)
            .map(|(_, ty)| ty)
            .or_else(|| index.map(|(_, value)| union(vec![*value, Type::Nil])))
            .unwrap_or(if open { Type::Any } else { Type::Nil }),
        Type::Any => Type::Any,
        _ => Type::Unknown,
    }
}

fn type_may_be_nil(ty: &Type) -> bool {
    match ty {
        Type::Nil | Type::Any | Type::Unknown | Type::Infer(_) | Type::Generic(_) => true,
        Type::Union(items) => items.iter().any(type_may_be_nil),
        _ => false,
    }
}

fn type_may_be_callable(ty: &Type) -> bool {
    match ty {
        Type::Function(_, _) | Type::Any | Type::Unknown | Type::Infer(_) => true,
        Type::Union(items) => items.iter().any(type_may_be_callable),
        _ => false,
    }
}

fn pattern_partition(source: Type, pattern: &syntax::Pattern) -> (Type, Type) {
    match pattern {
        syntax::Pattern::Binding(_) | syntax::Pattern::Wildcard(_) => (source, Type::Never),
        syntax::Pattern::Literal(node) => {
            let expression = if direct_token(node.syntax(), K::NIL_KW).is_some() {
                TypeMatcher::Exact(Type::Nil)
            } else if let Some(token) = direct_token(node.syntax(), K::STRING) {
                TypeMatcher::Exact(Type::LiteralString(unquote(token.text())))
            } else if let Some(token) = direct_token(node.syntax(), K::INT) {
                TypeMatcher::Exact(Type::LiteralInt(token.text().parse().unwrap_or_default()))
            } else if direct_token(node.syntax(), K::TRUE_KW).is_some() {
                TypeMatcher::Exact(Type::LiteralBoolean(true))
            } else if direct_token(node.syntax(), K::FALSE_KW).is_some() {
                TypeMatcher::Exact(Type::LiteralBoolean(false))
            } else {
                return (source.clone(), source);
            };
            (
                narrow_type(source.clone(), &expression, true),
                narrow_type(source, &expression, false),
            )
        }
        syntax::Pattern::List(node) => partition_list_pattern(source, node),
        syntax::Pattern::Map(node) => partition_map_pattern(source, node),
    }
}

fn unresolved_pattern_shape(pattern: &syntax::Pattern) -> Type {
    match pattern {
        syntax::Pattern::Binding(_) | syntax::Pattern::Wildcard(_) => Type::Unknown,
        syntax::Pattern::Literal(node) => literal_type(node.syntax()),
        syntax::Pattern::List(list) => {
            let items = support::children::<syntax::Pattern>(list.syntax())
                .map(|child| unresolved_pattern_shape(&child))
                .collect::<Vec<_>>();
            if support::child::<syntax::RestPattern>(list.syntax()).is_some() {
                Type::ListRest(Box::new(union(items)))
            } else {
                Type::ListExact(items)
            }
        }
        syntax::Pattern::Map(map) => Type::Map {
            fields: support::children::<syntax::MapPatternField>(map.syntax())
                .filter_map(|field| {
                    let name = direct_token(field.syntax(), K::IDENT)?;
                    let child = support::child::<syntax::Pattern>(field.syntax())?;
                    Some((name.text().to_owned(), unresolved_pattern_shape(&child)))
                })
                .collect(),
            index: None,
            open: true,
        },
    }
}

fn partition_list_pattern(source: Type, pattern: &syntax::ListPattern) -> (Type, Type) {
    if let Type::Union(items) = source {
        let (matched, unmatched): (Vec<_>, Vec<_>) = items
            .into_iter()
            .map(|item| partition_list_pattern(item, pattern))
            .unzip();
        return (union(matched), union(unmatched));
    }
    let children = support::children::<syntax::Pattern>(pattern.syntax()).collect::<Vec<_>>();
    let has_rest = support::child::<syntax::RestPattern>(pattern.syntax()).is_some();
    match source {
        Type::ListExact(mut items)
            if (has_rest && items.len() >= children.len())
                || (!has_rest && items.len() == children.len()) =>
        {
            let original = Type::ListExact(items.clone());
            for (index, child) in children.iter().enumerate() {
                let (matched, _) = pattern_partition(items[index].clone(), child);
                if matched == Type::Never {
                    return (Type::Never, original);
                }
                items[index] = matched;
            }
            let matched = Type::ListExact(items);
            if matched == original {
                (matched, Type::Never)
            } else {
                (matched, original)
            }
        }
        Type::ListExact(items) => (Type::Never, Type::ListExact(items)),
        Type::ListRest(item) => {
            let original = Type::ListRest(item.clone());
            if !has_rest {
                return (original.clone(), original);
            }
            for child in children {
                let (matched, _) = pattern_partition((*item).clone(), &child);
                if matched == Type::Never {
                    return (Type::Never, original);
                }
            }
            (original.clone(), original)
        }
        unresolved @ (Type::Infer(_) | Type::Unknown | Type::Any) => {
            let shape = unresolved_pattern_shape(&syntax::Pattern::List(pattern.clone()));
            (shape, unresolved)
        }
        other => (Type::Never, other),
    }
}

fn partition_map_pattern(source: Type, pattern: &syntax::MapPattern) -> (Type, Type) {
    if let Type::Union(items) = source {
        let (matched, unmatched): (Vec<_>, Vec<_>) = items
            .into_iter()
            .map(|item| partition_map_pattern(item, pattern))
            .unzip();
        return (union(matched), union(unmatched));
    }
    if matches!(source, Type::Infer(_) | Type::Unknown | Type::Any) {
        let shape = unresolved_pattern_shape(&syntax::Pattern::Map(pattern.clone()));
        return (shape, source);
    }
    let mut matched = source;
    let mut failures = Vec::new();
    for field in support::children::<syntax::MapPatternField>(pattern.syntax()) {
        let Some(name) = direct_token(field.syntax(), K::IDENT) else {
            continue;
        };
        let Some(child) = support::child::<syntax::Pattern>(field.syntax()) else {
            continue;
        };
        let (field_match, field_failure) =
            partition_required_map_field(matched, name.text(), &child);
        failures.push(field_failure);
        matched = field_match;
        if matched == Type::Never {
            break;
        }
    }
    let unmatched = if failures.is_empty() {
        Type::Never
    } else {
        union(failures)
    };
    (matched, unmatched)
}

fn partition_required_map_field(
    source: Type,
    field: &str,
    pattern: &syntax::Pattern,
) -> (Type, Type) {
    if let syntax::Pattern::Literal(literal) = pattern
        && direct_token(literal.syntax(), K::NIL_KW).is_some()
    {
        let matcher = TypeMatcher::Exact(Type::Nil);
        return (
            narrow_map_field(source.clone(), field, &matcher, true),
            narrow_map_field(source, field, &matcher, false),
        );
    }
    let Type::Map {
        fields,
        index,
        open,
    } = &source
    else {
        return (Type::Never, source);
    };
    if let Some((_, field_ty)) = fields.iter().find(|(name, _)| name == field) {
        let (matched_field, unmatched_field) = pattern_partition(field_ty.clone(), pattern);
        let matched = replace_required_map_field(source.clone(), field, matched_field);
        let unmatched = replace_required_map_field(source, field, unmatched_field);
        return (matched, unmatched);
    }
    if *open || index.is_some() {
        let possible = index
            .as_ref()
            .map(|(_, value)| (**value).clone())
            .unwrap_or(Type::Any);
        let (matched_field, _) = pattern_partition(possible, pattern);
        // Open/index maps do not establish presence. Even if a present value can
        // match, absence must remain in the failure partition.
        let matched = if matched_field == Type::Never {
            Type::Never
        } else {
            source.clone()
        };
        return (matched, source);
    }
    (Type::Never, source)
}

fn replace_required_map_field(source: Type, field: &str, value: Type) -> Type {
    if value == Type::Never {
        return Type::Never;
    }
    let Type::Map {
        mut fields,
        index,
        open,
    } = source
    else {
        return Type::Never;
    };
    if let Some((_, current)) = fields.iter_mut().find(|(name, _)| name == field) {
        *current = value;
    }
    Type::Map {
        fields,
        index,
        open,
    }
}

fn remove_nil(ty: Type) -> Type {
    match ty {
        Type::Nil => Type::Never,
        Type::Union(items) => union(
            items
                .into_iter()
                .map(|item| if item == Type::Nil { Type::Never } else { item })
                .collect(),
        ),
        other => other,
    }
}

fn literal_type(node: &SyntaxNode) -> Type {
    if let Some(token) = direct_token(node, K::INT) {
        return token
            .text()
            .parse::<i64>()
            .map(|_| Type::Int)
            .unwrap_or(Type::Int);
    }
    if direct_token(node, K::FLOAT).is_some() {
        return Type::Float;
    }
    if let Some(token) = direct_token(node, K::STRING) {
        return Type::LiteralString(unquote(token.text()));
    }
    if direct_token(node, K::TRUE_KW).is_some() || direct_token(node, K::FALSE_KW).is_some() {
        return Type::Boolean;
    }
    Type::Nil
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(text)
        .to_owned()
}

fn may_hold_mutable_value(ty: &Type) -> bool {
    matches!(ty, Type::Any | Type::Unknown | Type::Infer(_)) || has_mutable_category(ty)
}

fn is_nested_read(expression: &syntax::Expr) -> bool {
    match expression {
        syntax::Expr::Field(_) | syntax::Expr::Index(_) => true,
        syntax::Expr::Paren(paren) => child_expr(paren.syntax(), 0)
            .as_ref()
            .is_some_and(is_nested_read),
        _ => false,
    }
}

fn expression_symbol(expression: &syntax::Expr, resolution: &Resolution) -> Option<SymbolId> {
    match expression {
        syntax::Expr::Name(name) => {
            let token = direct_token(name.syntax(), K::IDENT)?;
            resolution.symbol_at(token_span(&token).start)
        }
        syntax::Expr::Paren(paren) => {
            let inner = child_expr(paren.syntax(), 0)?;
            expression_symbol(&inner, resolution)
        }
        _ => None,
    }
}

fn mutation_owner_symbol(expression: &syntax::Expr, resolution: &Resolution) -> Option<SymbolId> {
    mutation_root_symbol(expression, resolution)
}

fn mutation_root_symbol(expression: &syntax::Expr, resolution: &Resolution) -> Option<SymbolId> {
    match expression {
        syntax::Expr::Name(_) => expression_symbol(expression, resolution),
        syntax::Expr::Paren(paren) => {
            let inner = child_expr(paren.syntax(), 0)?;
            mutation_root_symbol(&inner, resolution)
        }
        syntax::Expr::Field(field) => {
            let owner = child_expr(field.syntax(), 0)?;
            mutation_root_symbol(&owner, resolution)
        }
        syntax::Expr::Index(index) => {
            let owner = child_expr(index.syntax(), 0)?;
            mutation_root_symbol(&owner, resolution)
        }
        _ => None,
    }
}

fn pattern_symbol(pattern: &syntax::Pattern, resolution: &Resolution) -> Option<SymbolId> {
    let syntax::Pattern::Binding(binding) = pattern else {
        return None;
    };
    let token = direct_token(binding.syntax(), K::IDENT)?;
    resolution.symbol_at(token_span(&token).start)
}

fn expr_children(node: &SyntaxNode) -> impl Iterator<Item = syntax::Expr> + '_ {
    node.children().filter_map(syntax::Expr::cast)
}

fn child_expr(node: &SyntaxNode, index: usize) -> Option<syntax::Expr> {
    expr_children(node).nth(index)
}

fn child_node(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children().next()
}

fn direct_token(node: &SyntaxNode, kind: K) -> Option<SyntaxToken> {
    support::token(node, kind)
}

fn span(node: &SyntaxNode) -> Span {
    let range = node.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}

fn token_span(token: &SyntaxToken) -> Span {
    let range = token.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}
