use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use la_arena::{Arena, Idx};
use simi_syntax::{lexer::is_identifier, span::Span};

mod resolution;

pub type ScopeId = Idx<ScopeData>;
pub type SymbolId = Idx<SymbolData>;
pub type ExprId = Idx<ExprData>;
pub type PatternId = Idx<PatternData>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeData {
    pub parent: Option<ScopeId>,
    pub span: Span,
    pub function_depth: u32,
    pub symbols: Vec<SymbolId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Parameter,
    Let,
    Pattern,
    LoopState,
    Builtin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolData {
    pub name: String,
    pub kind: SymbolKind,
    pub declaration: Option<Span>,
    pub scope: ScopeId,
    pub arity: Option<usize>,
    pub parameters: Option<Vec<String>>,
    pub documentation: Option<String>,
    pub builtin: bool,
    pub activation: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprData {
    pub span: Span,
    pub scope: ScopeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternData {
    pub span: Span,
    pub scope: ScopeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OccurrenceKind {
    Read,
    Assignment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameOccurrence {
    pub name: String,
    pub span: Span,
    pub scope: ScopeId,
    pub kind: OccurrenceKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hir {
    pub scopes: Arena<ScopeData>,
    pub symbols: Arena<SymbolData>,
    pub expressions: Arena<ExprData>,
    pub patterns: Arena<PatternData>,
    pub occurrences: Vec<NameOccurrence>,
    pub root_scope: ScopeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Capture {
    pub function_scope: ScopeId,
    pub symbol: SymbolId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub hir: Hir,
    pub occurrence_symbols: Vec<Option<SymbolId>>,
    pub symbol_references: HashMap<SymbolId, Vec<Span>>,
    pub captures: BTreeSet<Capture>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisDiagnosticCode {
    InvalidSyntax,
    SyntaxError,
    TypeMismatch,
    InvalidOperator,
    NotCallable,
    WrongArity,
    UnknownType,
    WrongTypeArity,
    CyclicTypeAlias,
    InvalidType,
}

impl AnalysisDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSyntax => "invalid_syntax",
            Self::SyntaxError => "syntax_error",
            Self::TypeMismatch => "type_mismatch",
            Self::InvalidOperator => "invalid_operator",
            Self::NotCallable => "not_callable",
            Self::WrongArity => "wrong_arity",
            Self::UnknownType => "unknown_type",
            Self::WrongTypeArity => "wrong_type_arity",
            Self::CyclicTypeAlias => "cyclic_type_alias",
            Self::InvalidType => "invalid_type",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisDiagnosticSeverity {
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedDiagnostic {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisDiagnostic {
    pub span: Span,
    pub code: AnalysisDiagnosticCode,
    pub title: String,
    pub detail: String,
    pub severity: AnalysisDiagnosticSeverity,
    pub related: Vec<RelatedDiagnostic>,
}

impl AnalysisDiagnostic {
    pub fn message(&self) -> String {
        format!("{}\n\n{}", self.title, self.detail)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSymbol {
    pub symbol: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportField {
    pub name: String,
    pub span: Span,
    pub parameters: Option<Vec<String>>,
    pub documentation: Option<String>,
    pub ty: Option<Type>,
    pub posts: Vec<ParameterPostType>,
    pub fields: Vec<ExportField>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleShape {
    pub documentation: Option<String>,
    pub fields: Vec<ExportField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleValue {
    pub module: String,
    pub documentation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleMember {
    pub module: String,
    pub field: ExportField,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RaisedAnnotation {
    Inferred,
    Explicit,
    NoRaise,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GenericConstraint {
    pub variable: Type,
    pub bound: Option<Type>,
}

#[derive(Clone, Debug)]
pub struct CallableParameter {
    pub name: Option<String>,
    pub ty: Type,
    pub post: Option<Type>,
}

impl PartialEq for CallableParameter {
    fn eq(&self, other: &Self) -> bool {
        self.ty == other.ty && self.post == other.post
    }
}

impl Eq for CallableParameter {}

impl Hash for CallableParameter {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ty.hash(state);
        self.post.hash(state);
    }
}

#[derive(Clone, Debug)]
pub struct CallableType {
    pub constraints: Vec<GenericConstraint>,
    pub parameters: Vec<CallableParameter>,
    pub result: Box<Type>,
    pub raised: Box<Type>,
    pub raised_annotation: RaisedAnnotation,
}

impl CallableType {
    pub fn inferred(parameters: Vec<Type>, result: Type, raised: Type) -> Self {
        Self {
            constraints: Vec::new(),
            parameters: parameters
                .into_iter()
                .map(|ty| CallableParameter {
                    name: None,
                    ty,
                    post: None,
                })
                .collect(),
            result: Box::new(result),
            raised: Box::new(raised),
            raised_annotation: RaisedAnnotation::Inferred,
        }
    }
}

impl PartialEq for CallableType {
    fn eq(&self, other: &Self) -> bool {
        self.constraints == other.constraints
            && self.parameters == other.parameters
            && self.result == other.result
            && self.raised == other.raised
    }
}

impl Eq for CallableType {}

impl Hash for CallableType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.constraints.hash(state);
        self.parameters.hash(state);
        self.result.hash(state);
        self.raised.hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    #[doc(hidden)]
    Never,
    Unknown,
    Any,
    Nil,
    Boolean,
    Int,
    Float,
    String,
    LiteralInt(i64),
    LiteralString(String),
    LiteralBoolean(bool),
    ListExact(Vec<Type>),
    ListRest(Box<Type>),
    Map {
        fields: Vec<(String, Type)>,
        index: Option<(Box<Type>, Box<Type>)>,
        open: bool,
    },
    Function(Box<CallableType>),
    #[doc(hidden)]
    FunctionArgs(Vec<CallableParameter>),
    Union(Vec<Type>),
    Generic(u32),
    Infer(u32),
}

impl Type {
    pub fn display(&self) -> String {
        display_type(self, false)
    }
}

fn display_type(ty: &Type, nested: bool) -> String {
    match ty {
        Type::Never => "never".to_owned(),
        Type::Unknown => "any".to_owned(),
        Type::Any => "any".to_owned(),
        Type::Nil => "nil".to_owned(),
        Type::Boolean => "boolean".to_owned(),
        Type::Int => "integer".to_owned(),
        Type::Float => "float".to_owned(),
        Type::String => "string".to_owned(),
        Type::LiteralInt(value) => value.to_string(),
        Type::LiteralString(value) => format!("{value:?}"),
        Type::LiteralBoolean(value) => value.to_string(),
        Type::ListExact(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|item| display_type(item, false))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::ListRest(item) => format!("[..{}]", display_type(item, true)),
        Type::Map {
            fields,
            index,
            open,
        } => {
            let mut parts = fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", display_type(ty, false)))
                .collect::<Vec<_>>();
            if let Some((key, value)) = index {
                parts.push(format!(
                    "[{}]: {}",
                    display_type(key, false),
                    display_type(value, false)
                ));
            }
            if *open {
                parts.push("..".to_owned());
            }
            format!("{{ {} }}", parts.join(", "))
        }
        Type::Function(callable) => {
            let constraints = if callable.constraints.is_empty() {
                String::new()
            } else {
                let values = callable
                    .constraints
                    .iter()
                    .map(|constraint| {
                        let variable = display_type(&constraint.variable, false);
                        constraint.bound.as_ref().map_or(variable.clone(), |bound| {
                            format!("{variable}: {}", display_type(bound, false))
                        })
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("<{values}> ")
            };
            let rendered_parameters = callable
                .parameters
                .iter()
                .map(|parameter| {
                    let mut value = display_type(&parameter.ty, false);
                    if let Some(post) = &parameter.post {
                        value = format!("{value} => {}", display_type(post, false));
                    }
                    if let Some(name) = &parameter.name {
                        value = format!("{name}: {value}");
                    }
                    value
                })
                .collect::<Vec<_>>();
            let left = match callable.parameters.as_slice() {
                [parameter] if parameter.name.is_none() && parameter.post.is_none() => {
                    display_type(&parameter.ty, true)
                }
                _ => format!("({})", rendered_parameters.join(", ")),
            };
            let mut value = format!(
                "{constraints}{left} -> {}",
                display_type(&callable.result, false)
            );
            let orphan_inferred_effect = match callable.raised.as_ref() {
                Type::Generic(id) if callable.raised_annotation == RaisedAnnotation::Inferred => {
                    !callable.parameters.iter().any(|parameter| {
                        contains_generic(&parameter.ty, *id)
                            || parameter
                                .post
                                .as_ref()
                                .is_some_and(|post| contains_generic(post, *id))
                    }) && !contains_generic(&callable.result, *id)
                        && !callable.constraints.iter().any(|constraint| {
                            contains_generic(&constraint.variable, *id)
                                || constraint
                                    .bound
                                    .as_ref()
                                    .is_some_and(|bound| contains_generic(bound, *id))
                        })
                }
                _ => false,
            };
            match (&*callable.raised, callable.raised_annotation) {
                (Type::Never, RaisedAnnotation::Inferred) => {}
                (Type::Never, _) => value.push_str(" noraise"),
                (raised, _) if !orphan_inferred_effect => {
                    value.push_str(" raises ");
                    value.push_str(&display_type(raised, false));
                }
                _ => {}
            }
            if nested { format!("({value})") } else { value }
        }
        Type::FunctionArgs(items) => format!(
            "({})",
            items
                .iter()
                .map(|item| display_type(&item.ty, false))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Union(items) => {
            let value = items
                .iter()
                .map(|item| display_type(item, true))
                .collect::<Vec<_>>()
                .join(" | ");
            if nested { format!("({value})") } else { value }
        }
        Type::Generic(index) => format!("'{}", generic_name(*index)),
        Type::Infer(index) => format!("?{index}"),
    }
}

fn contains_generic(ty: &Type, id: u32) -> bool {
    match ty {
        Type::Generic(candidate) => *candidate == id,
        Type::Union(items) | Type::ListExact(items) => {
            items.iter().any(|item| contains_generic(item, id))
        }
        Type::ListRest(item) => contains_generic(item, id),
        Type::Map { fields, index, .. } => {
            fields.iter().any(|(_, value)| contains_generic(value, id))
                || index.as_ref().is_some_and(|(key, value)| {
                    contains_generic(key, id) || contains_generic(value, id)
                })
        }
        Type::Function(callable) => {
            callable.constraints.iter().any(|constraint| {
                contains_generic(&constraint.variable, id)
                    || constraint
                        .bound
                        .as_ref()
                        .is_some_and(|bound| contains_generic(bound, id))
            }) || callable.parameters.iter().any(|parameter| {
                contains_generic(&parameter.ty, id)
                    || parameter
                        .post
                        .as_ref()
                        .is_some_and(|post| contains_generic(post, id))
            }) || contains_generic(&callable.result, id)
                || contains_generic(&callable.raised, id)
        }
        _ => false,
    }
}

fn generic_name(mut index: u32) -> String {
    let mut name = String::new();
    loop {
        name.insert(0, (b'a' + (index % 26) as u8) as char);
        if index < 26 {
            return name;
        }
        index = index / 26 - 1;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterPostType {
    pub parameter_index: usize,
    pub parameter_name: String,
    pub becomes: Type,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeInference {
    pub symbol_types: HashMap<SymbolId, Type>,
    pub symbol_posts: HashMap<SymbolId, Vec<ParameterPostType>>,
    pub expression_types: Vec<(Span, Type)>,
    pub pattern_types: Vec<(Span, Type)>,
    pub diagnostics: Vec<AnalysisDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoverFacts {
    pub symbol: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub arity: Option<usize>,
    pub parameters: Option<Vec<String>>,
    pub documentation: Option<String>,
    pub declaration: Option<Span>,
}

pub fn display_signature(name: &str, parameters: &[String]) -> String {
    format!("fn {name}({})", parameters.join(", "))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameError {
    Builtin,
    Unresolved,
    InvalidName,
    Collision { name: String, at: Span },
}

impl Resolution {
    pub(crate) fn resolve_name(
        &self,
        mut scope: ScopeId,
        offset: usize,
        name: &str,
    ) -> Option<SymbolId> {
        let occurrence_depth = self.hir.scopes[scope].function_depth;
        loop {
            if let Some(symbol) = self.symbol_in_scope(scope, occurrence_depth, offset, name) {
                return Some(symbol);
            }
            scope = self.hir.scopes[scope].parent?;
        }
    }
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset < span.end
}

fn contains_inclusive(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}
