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
    Todo,
    AmbiguousLoopControl,
    DestructuringLetMayFail,
    DestructuringLetNeverMatches,
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
            Self::Todo => "todo",
            Self::AmbiguousLoopControl => "ambiguous_loop_control",
            Self::DestructuringLetMayFail => "destructuring_let_may_fail",
            Self::DestructuringLetNeverMatches => "destructuring_let_never_matches",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisDiagnosticSeverity {
    Error,
    Warning,
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
    pub fields: Vec<ExportField>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleShape {
    pub documentation: Option<String>,
    pub ty: Option<Type>,
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
}

impl PartialEq for CallableParameter {
    fn eq(&self, other: &Self) -> bool {
        self.ty == other.ty
    }
}

impl Eq for CallableParameter {}

impl Hash for CallableParameter {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ty.hash(state);
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
                .map(|ty| CallableParameter { name: None, ty })
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LiteralFloat(u64);

impl LiteralFloat {
    pub fn new(mut value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        if value == 0.0 {
            value = 0.0;
        }
        Some(Self(value.to_bits()))
    }

    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
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
    LiteralFloat(LiteralFloat),
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

    /// Render this type for a presentation surface with a maximum line width.
    ///
    /// Unlike [`Type::display`], this formatter is intentionally presentation-only:
    /// it preserves the same type syntax while introducing deterministic breaks and
    /// four-space indentation for large structural types.
    pub fn pretty_display(&self, width: usize) -> String {
        pretty_type(self, false, 0, 0, width.max(1))
    }
}

fn display_float_literal(value: f64) -> String {
    let rendered = value.to_string();
    if rendered.contains(['.', 'e', 'E']) {
        rendered
    } else {
        format!("{rendered}.0")
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
        Type::LiteralFloat(value) => display_float_literal(value.value()),
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
            if parts.is_empty() {
                "{}".to_owned()
            } else {
                format!("{{ {} }}", parts.join(", "))
            }
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
                    if let Some(name) = &parameter.name {
                        value = format!("{name}: {value}");
                    }
                    value
                })
                .collect::<Vec<_>>();
            let left = match callable.parameters.as_slice() {
                [parameter] if parameter.name.is_none() => display_type(&parameter.ty, true),
                _ => format!("({})", rendered_parameters.join(", ")),
            };
            let mut value = format!(
                "{constraints}{left} -> {}",
                display_type(&callable.result, false)
            );
            let orphan_inferred_effect = match callable.raised.as_ref() {
                Type::Generic(id) if callable.raised_annotation == RaisedAnnotation::Inferred => {
                    !callable
                        .parameters
                        .iter()
                        .any(|parameter| contains_generic(&parameter.ty, *id))
                        && !contains_generic(&callable.result, *id)
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

fn pretty_type(
    ty: &Type,
    nested: bool,
    column: usize,
    continuation_indent: usize,
    width: usize,
) -> String {
    let compact = display_type(ty, nested);
    if !compact.contains('\n') && column + compact.len() <= width {
        return compact;
    }

    match ty {
        Type::Map {
            fields,
            index,
            open,
        } => pretty_map(fields, index.as_ref(), *open, continuation_indent, width),
        Type::ListExact(items) => {
            let indent = continuation_indent + 4;
            let items = items
                .iter()
                .map(|item| {
                    format!(
                        "{}{},",
                        " ".repeat(indent),
                        pretty_type(item, false, indent, indent, width)
                    )
                })
                .collect::<Vec<_>>();
            if items.is_empty() {
                "[]".to_owned()
            } else {
                format!(
                    "[\n{}\n{}]",
                    items.join("\n"),
                    " ".repeat(continuation_indent)
                )
            }
        }
        Type::Function(callable) => {
            let value = pretty_function(callable, continuation_indent, width);
            if nested { format!("({value})") } else { value }
        }
        Type::Union(items) => pretty_union(items, nested, continuation_indent, width),
        _ => compact,
    }
}

fn pretty_map(
    fields: &[(String, Type)],
    index: Option<&(Box<Type>, Box<Type>)>,
    open: bool,
    continuation_indent: usize,
    width: usize,
) -> String {
    let indent = continuation_indent + 4;
    let mut lines = Vec::new();
    for (name, ty) in fields {
        let prefix = format!("{}{}: ", " ".repeat(indent), name);
        let value = pretty_type(ty, false, indent + name.len() + 2, indent, width);
        lines.push(format!("{prefix}{value},"));
    }
    if let Some((key, value)) = index {
        let key = pretty_type(key, false, indent + 1, indent, width);
        let value = pretty_type(value, false, indent + key.len() + 5, indent, width);
        lines.push(format!("{}[{}]: {},", " ".repeat(indent), key, value));
    }
    if open {
        lines.push(format!("{}..", " ".repeat(indent)));
    }
    if lines.is_empty() {
        "{}".to_owned()
    } else {
        format!(
            "{{\n{}\n{}}}",
            lines.join("\n"),
            " ".repeat(continuation_indent)
        )
    }
}

fn pretty_function(callable: &CallableType, continuation_indent: usize, width: usize) -> String {
    let indent = continuation_indent + 4;
    let parameters = callable
        .parameters
        .iter()
        .map(|parameter| {
            let name = parameter
                .name
                .as_deref()
                .map_or(String::new(), |name| format!("{name}: "));
            let type_column = indent + name.len();
            let value = format!(
                "{}{}{}",
                " ".repeat(indent),
                name,
                pretty_type(&parameter.ty, false, type_column, indent, width)
            );
            format!("{value},")
        })
        .collect::<Vec<_>>();
    let parameters = if parameters.is_empty() {
        "()".to_owned()
    } else {
        format!(
            "(\n{}\n{})",
            parameters.join("\n"),
            " ".repeat(continuation_indent)
        )
    };

    let constraints = if callable.constraints.is_empty() {
        String::new()
    } else {
        let constraints = callable
            .constraints
            .iter()
            .map(|constraint| {
                let variable = constraint.variable.display();
                constraint.bound.as_ref().map_or(variable.clone(), |bound| {
                    format!(
                        "{variable}: {}",
                        pretty_type(bound, false, indent + variable.len() + 2, indent, width,)
                    )
                })
            })
            .collect::<Vec<_>>();
        if constraints.join(", ").len() + continuation_indent + 3 <= width {
            format!("<{}> ", constraints.join(", "))
        } else {
            format!(
                "<\n{}\n{}> ",
                constraints
                    .iter()
                    .map(|constraint| format!("{}{},", " ".repeat(indent), constraint))
                    .collect::<Vec<_>>()
                    .join("\n"),
                " ".repeat(continuation_indent)
            )
        }
    };
    let result = pretty_type(
        &callable.result,
        false,
        continuation_indent + 4,
        continuation_indent,
        width,
    );
    let mut value = format!("{constraints}{parameters} -> {result}");
    match (&*callable.raised, callable.raised_annotation) {
        (Type::Never, RaisedAnnotation::Inferred) => {}
        (Type::Never, _) => value.push_str(" noraise"),
        (raised, _) => {
            value.push_str(" raises ");
            let raised = if value.len() + raised.display().len() <= width {
                raised.display()
            } else {
                pretty_type(
                    raised,
                    true,
                    continuation_indent + 8,
                    continuation_indent,
                    width,
                )
            };
            value.push_str(&raised);
        }
    }
    value
}

fn pretty_union(items: &[Type], nested: bool, continuation_indent: usize, width: usize) -> String {
    let indent = if nested {
        continuation_indent + 4
    } else {
        continuation_indent
    };
    let lines = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let line_indent = if nested || index > 0 { indent } else { 0 };
            format!(
                "{}| {}",
                " ".repeat(line_indent),
                pretty_type(item, true, indent + 2, indent, width)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if nested {
        format!("(\n{}\n{})", lines, " ".repeat(continuation_indent))
    } else {
        lines
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
            }) || callable
                .parameters
                .iter()
                .any(|parameter| contains_generic(&parameter.ty, id))
                || contains_generic(&callable.result, id)
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeInference {
    pub result_type: Option<Type>,
    pub symbol_types: HashMap<SymbolId, Type>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_display_keeps_compact_types_compact() {
        let ty = Type::Union(vec![Type::Int, Type::String]);
        assert_eq!(ty.pretty_display(80), "integer | string");
        assert_eq!(ty.display(), "integer | string");
    }

    #[test]
    fn pretty_display_wraps_maps_and_callable_parameters() {
        let ty = Type::Map {
            fields: vec![
                (
                    "concat".to_owned(),
                    Type::Function(Box::new(CallableType {
                        constraints: Vec::new(),
                        parameters: vec![
                            CallableParameter {
                                name: Some("left".to_owned()),
                                ty: Type::String,
                            },
                            CallableParameter {
                                name: Some("right".to_owned()),
                                ty: Type::String,
                            },
                        ],
                        result: Box::new(Type::String),
                        raised: Box::new(Type::Never),
                        raised_annotation: RaisedAnnotation::NoRaise,
                    })),
                ),
                ("length".to_owned(), Type::Int),
            ],
            index: None,
            open: true,
        };
        assert_eq!(
            ty.pretty_display(40),
            "{\n    concat: (\n        left: string,\n        right: string,\n    ) -> string noraise,\n    length: integer,\n    ..\n}"
        );
        assert_eq!(
            ty.display(),
            "{ concat: (left: string, right: string) -> string noraise, length: integer, .. }"
        );
    }

    #[test]
    fn pretty_display_puts_multiline_unions_on_marked_lines() {
        let ty = Type::Union(vec![
            Type::Map {
                fields: vec![("name".to_owned(), Type::String)],
                index: None,
                open: false,
            },
            Type::ListExact(vec![Type::Int, Type::String]),
        ]);
        assert_eq!(
            ty.pretty_display(20),
            "| { name: string }\n| [integer, string]"
        );
    }

    #[test]
    fn pretty_display_preserves_generics_and_raised_effects() {
        let ty = Type::Function(Box::new(CallableType {
            constraints: vec![GenericConstraint {
                variable: Type::Generic(0),
                bound: Some(Type::Union(vec![Type::Int, Type::String])),
            }],
            parameters: vec![CallableParameter {
                name: Some("value".to_owned()),
                ty: Type::Generic(0),
            }],
            result: Box::new(Type::String),
            raised: Box::new(Type::Union(vec![Type::String, Type::Int])),
            raised_annotation: RaisedAnnotation::Explicit,
        }));
        let rendered = ty.pretty_display(40);
        assert!(rendered.contains("<"));
        assert!(rendered.contains("raises"));
        assert!(rendered.lines().all(|line| line.len() <= 40), "{rendered}");
    }
}
