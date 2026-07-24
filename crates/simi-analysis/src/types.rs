use std::collections::{HashMap, HashSet};

use simi_syntax::ast as support;
use simi_syntax::generated::{self as syntax, AstNode};
use simi_syntax::span::Span;
use simi_syntax::{SyntaxKind as K, SyntaxNode, SyntaxToken};

use crate::db::{FileId, parse, resolve, source_text};
use crate::model::{
    AnalysisDiagnostic, AnalysisDiagnosticCode, AnalysisDiagnosticSeverity, CallableParameter,
    CallableType, GenericConstraint, ModuleShape, ParameterPostType, RaisedAnnotation, Resolution,
    SymbolId, Type, TypeInference,
};
use crate::modules::member_at;

mod flow;

mod declarations;

mod expressions;

mod narrowing;

mod calls;

mod mutation;

mod annotations;

mod solver;

mod algebra;

mod pattern_model;

mod syntax_util;

mod mutation_model;

use algebra::*;
use mutation_model::*;
use pattern_model::*;
use syntax_util::*;

#[derive(Clone)]
struct AliasDef {
    parameters: Vec<String>,
    body: SyntaxNode,
}

#[derive(Clone, Default)]
struct VarState {
    binding: Option<Type>,
    bound: Option<Type>,
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
        mutation_effect_frames: Vec::new(),
        annotated_symbols: HashSet::new(),
        monomorphic_symbols: HashSet::new(),
        trusted_builtin_symbols,
        next_region: 0,
        expression_types: Vec::new(),
        pattern_types: Vec::new(),
        loops: Vec::new(),
        nil_abort_states: Vec::new(),
        raised_exit_frames: vec![Vec::new()],
        generic_bound_frames: Vec::new(),
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

fn callable_type(parameters: Vec<Type>, result: Type) -> Type {
    Type::Function(Box::new(CallableType::inferred(
        parameters,
        result,
        Type::Never,
    )))
}

fn builtin_types(resolution: &Resolution) -> HashMap<SymbolId, Type> {
    let mut types = HashMap::new();
    for (id, symbol) in resolution.hir.symbols.iter() {
        if !symbol.builtin {
            continue;
        }
        let ty = match symbol.name.as_str() {
            "require" => Type::Function(Box::new(CallableType::inferred(
                vec![Type::String],
                Type::Any,
                Type::Any,
            ))),
            "type" => callable_type(vec![Type::Any], Type::String),
            "inspect" => callable_type(vec![Type::Any], Type::String),
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
struct RaisedExit {
    raised: Type,
    flow: FlowState,
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
    mutation_effect_frames: Vec<HashSet<SymbolId>>,
    annotated_symbols: HashSet<SymbolId>,
    monomorphic_symbols: HashSet<SymbolId>,
    trusted_builtin_symbols: HashSet<SymbolId>,
    next_region: u32,
    expression_types: Vec<(Span, Type)>,
    pattern_types: Vec<(Span, Type)>,
    loops: Vec<LoopContext>,
    nil_abort_states: Vec<Vec<FlowState>>,
    raised_exit_frames: Vec<Vec<RaisedExit>>,
    generic_bound_frames: Vec<HashMap<u32, Type>>,
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

    fn record_raised(&mut self, raised: Type) {
        let raised = self.resolve_type(raised);
        if raised == Type::Never {
            return;
        }
        let flow = self.flow_state();
        if let Some(frame) = self.raised_exit_frames.last_mut() {
            frame.push(RaisedExit { raised, flow });
        }
    }

    fn raised_type(exits: &[RaisedExit]) -> Type {
        if exits.is_empty() {
            Type::Never
        } else {
            union(exits.iter().map(|exit| exit.raised.clone()).collect())
        }
    }

    fn upper_bound_view(&self, ty: Type) -> Type {
        map_type(ty, &mut |candidate| match candidate {
            Type::Generic(id) => self
                .generic_bound_frames
                .iter()
                .rev()
                .find_map(|frame| frame.get(&id).cloned())
                .unwrap_or(Type::Generic(id)),
            other => other,
        })
    }

    fn instantiate_callee(&mut self, node: &syntax::Expr, ty: Type) -> Type {
        if expression_symbol(node, self.resolution)
            .is_some_and(|symbol| self.monomorphic_symbols.contains(&symbol))
        {
            let Type::Function(callable) = &ty else {
                return ty;
            };
            let mut replacements = HashMap::new();
            for constraint in &callable.constraints {
                if let Type::Generic(id) = constraint.variable {
                    replacements.insert(id, self.fresh());
                }
            }
            let instantiated = map_type(ty, &mut |candidate| match candidate {
                Type::Generic(id) => replacements.get(&id).cloned().unwrap_or(Type::Generic(id)),
                other => other,
            });
            self.install_constraint_bounds(&instantiated);
            instantiated
        } else {
            self.instantiate(ty)
        }
    }

    fn install_constraint_bounds(&mut self, ty: &Type) {
        let mut bounds = Vec::new();
        collect_constraint_bounds(ty, &mut bounds);
        for (id, bound) in bounds {
            if let Some(state) = self.vars.get_mut(id as usize) {
                state.bound = Some(bound);
            }
        }
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

fn block_ends_in_direct_call(body: &syntax::Block) -> bool {
    let Some(syntax::Stmt::ExprStmt(statement)) = body.statements().last() else {
        return false;
    };
    support::child::<syntax::Expr>(statement.syntax()).is_some_and(|expression| {
        matches!(expression, syntax::Expr::Call(_))
            || matches!(expression, syntax::Expr::Paren(ref paren) if child_expr(paren.syntax(), 0).is_some_and(|inner| matches!(inner, syntax::Expr::Call(_))))
    })
}

fn is_host_wrapper(body: &syntax::Block, resolution: &Resolution) -> bool {
    if resolution
        .hir
        .occurrences
        .iter()
        .zip(&resolution.occurrence_symbols)
        .any(|(occurrence, symbol)| {
            occurrence.name == "host"
                && occurrence.kind == crate::model::OccurrenceKind::Assignment
                && symbol.is_none()
        })
    {
        return false;
    }
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
    let Some(syntax::Expr::Name(object)) = child_expr(field.syntax(), 0) else {
        return false;
    };
    direct_token(object.syntax(), K::IDENT).is_some_and(|token| {
        token.text() == "host" && resolution.symbol_at(token_span(&token).start).is_none()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::AnalysisDatabase;

    #[test]
    fn assigned_private_host_bindings_are_not_trusted_wrappers() {
        for source in [
            "host = replacement fn mutate(xs: [..integer] => [integer]) -> nil do host.mutate(xs) end",
            "fn mutate(xs: [..integer] => [integer]) -> nil do host.mutate(xs) end host = replacement",
        ] {
            let db = AnalysisDatabase::default();
            let file = db.add_file(source);
            let parsed = parse(&db, file);
            let resolution = resolve(&db, file);
            let root = syntax::Root::cast(parsed.syntax()).unwrap();
            let function = root
                .statements()
                .find_map(|statement| match statement {
                    syntax::Stmt::FunctionDecl(function) => Some(function),
                    _ => None,
                })
                .unwrap();
            let body = support::child::<syntax::Block>(function.syntax()).unwrap();
            assert!(!is_host_wrapper(&body, &resolution));
        }
    }
}
