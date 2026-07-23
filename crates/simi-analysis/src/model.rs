use std::collections::{BTreeSet, HashMap};

use la_arena::{Arena, Idx};
use simi_syntax::{lexer::is_identifier, span::Span};

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
    DuplicateBinding,
}

impl AnalysisDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSyntax => "invalid_syntax",
            Self::SyntaxError => "syntax_error",
            Self::DuplicateBinding => "duplicate_binding",
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
    pub fn symbol_at(&self, offset: usize) -> Option<SymbolId> {
        self.symbol_span_at(offset).map(|(symbol, _)| symbol)
    }

    pub fn symbol_span_at(&self, offset: usize) -> Option<(SymbolId, Span)> {
        self.hir
            .symbols
            .iter()
            .find_map(|(id, symbol)| {
                symbol
                    .declaration
                    .filter(|span| contains(*span, offset))
                    .map(|span| (id, span))
            })
            .or_else(|| {
                self.hir
                    .occurrences
                    .iter()
                    .zip(&self.occurrence_symbols)
                    .find_map(|(occurrence, symbol)| {
                        contains(occurrence.span, offset)
                            .then(|| symbol.map(|symbol| (symbol, occurrence.span)))
                            .flatten()
                    })
            })
    }

    pub fn definition_span(&self, symbol: SymbolId) -> Option<Span> {
        self.symbol_data(symbol)?.declaration
    }

    pub fn references(&self, symbol: SymbolId) -> &[Span] {
        self.symbol_references
            .get(&symbol)
            .map_or(&[], Vec::as_slice)
    }

    pub fn rename_spans(&self, symbol: SymbolId) -> Vec<Span> {
        let Some(data) = self.symbol_data(symbol) else {
            return Vec::new();
        };
        let mut spans = self.references(symbol).to_vec();
        if let Some(declaration) = data.declaration {
            spans.push(declaration);
        }
        spans.sort_by_key(|span| (span.start, span.end));
        spans.dedup();
        spans
    }

    pub fn hover(&self, offset: usize) -> Option<HoverFacts> {
        let id = self.symbol_at(offset)?;
        let symbol = self.symbol_data(id)?;
        Some(HoverFacts {
            symbol: id,
            name: symbol.name.clone(),
            kind: symbol.kind,
            arity: symbol.arity,
            parameters: symbol.parameters.clone(),
            documentation: symbol.documentation.clone(),
            declaration: symbol.declaration,
        })
    }

    pub fn visible_symbols(&self, offset: usize) -> Vec<SymbolId> {
        let mut scope = self.scope_at(offset);
        let Some(start_scope) = scope else {
            return Vec::new();
        };
        let occurrence_depth = self.hir.scopes[start_scope].function_depth;
        let mut names = BTreeSet::new();
        let mut result = Vec::new();
        while let Some(id) = scope {
            let data = &self.hir.scopes[id];
            let mut scope_names = BTreeSet::new();
            for symbol in data.symbols.iter().rev().copied() {
                let name = self.hir.symbols[symbol].name.clone();
                if scope_names.insert(name.clone())
                    && !names.contains(&name)
                    && let Some(symbol) = self.symbol_in_scope(id, occurrence_depth, offset, &name)
                {
                    names.insert(name);
                    result.push(symbol);
                }
            }
            scope = data.parent;
        }
        result
    }

    pub fn check_rename(&self, symbol: SymbolId, new_name: &str) -> Result<(), RenameError> {
        if !is_identifier(new_name) {
            return Err(RenameError::InvalidName);
        }
        let target = self.symbol_data(symbol).ok_or(RenameError::Unresolved)?;
        if target.builtin {
            return Err(RenameError::Builtin);
        }
        if target.name == new_name {
            return Ok(());
        }
        if let Some(other) = self.hir.scopes[target.scope]
            .symbols
            .iter()
            .copied()
            .find(|other| *other != symbol && self.hir.symbols[*other].name == new_name)
        {
            return Err(RenameError::Collision {
                name: new_name.to_owned(),
                at: self.hir.symbols[other]
                    .declaration
                    .unwrap_or(Span::new(0, 0)),
            });
        }
        for (occurrence, resolved) in self.hir.occurrences.iter().zip(&self.occurrence_symbols) {
            if *resolved == Some(symbol) {
                if let Some(other) = self.resolve_name_after_rename(
                    occurrence.scope,
                    occurrence.span.start,
                    new_name,
                    symbol,
                    new_name,
                ) && other != symbol
                {
                    return Err(RenameError::Collision {
                        name: new_name.to_owned(),
                        at: self.hir.symbols[other]
                            .declaration
                            .unwrap_or(Span::new(0, 0)),
                    });
                }
            } else if occurrence.name == new_name
                && self.resolve_name_after_rename(
                    occurrence.scope,
                    occurrence.span.start,
                    new_name,
                    symbol,
                    new_name,
                ) == Some(symbol)
            {
                return Err(RenameError::Collision {
                    name: new_name.to_owned(),
                    at: occurrence.span,
                });
            }
        }
        Ok(())
    }

    fn resolve_name_after_rename(
        &self,
        mut scope: ScopeId,
        offset: usize,
        name: &str,
        renamed: SymbolId,
        new_name: &str,
    ) -> Option<SymbolId> {
        let occurrence_depth = self.hir.scopes[scope].function_depth;
        loop {
            if let Some(symbol) = self.symbol_in_scope_with_name(
                scope,
                occurrence_depth,
                offset,
                name,
                Some((renamed, new_name)),
            ) {
                return Some(symbol);
            }
            scope = self.hir.scopes[scope].parent?;
        }
    }

    pub fn symbol_data(&self, id: SymbolId) -> Option<&SymbolData> {
        self.hir
            .symbols
            .iter()
            .find_map(|(candidate, symbol)| (candidate == id).then_some(symbol))
    }

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

    fn symbol_in_scope(
        &self,
        scope: ScopeId,
        occurrence_depth: u32,
        offset: usize,
        name: &str,
    ) -> Option<SymbolId> {
        self.symbol_in_scope_with_name(scope, occurrence_depth, offset, name, None)
    }

    fn symbol_in_scope_with_name(
        &self,
        scope: ScopeId,
        occurrence_depth: u32,
        offset: usize,
        name: &str,
        renamed: Option<(SymbolId, &str)>,
    ) -> Option<SymbolId> {
        let scope_data = &self.hir.scopes[scope];
        let has_name = |id: SymbolId| {
            let symbol = &self.hir.symbols[id];
            let effective = renamed
                .filter(|(renamed, _)| *renamed == id)
                .map_or(symbol.name.as_str(), |(_, new_name)| new_name);
            effective == name
        };
        let preceding_user = scope_data.symbols.iter().copied().find(|id| {
            let symbol = &self.hir.symbols[*id];
            has_name(*id) && symbol.activation <= offset && symbol.declaration.is_some()
        });
        let preceding_builtin = scope_data.symbols.iter().copied().find(|id| {
            let symbol = &self.hir.symbols[*id];
            has_name(*id) && symbol.activation <= offset && symbol.builtin
        });
        let preceding = preceding_user.or(preceding_builtin);
        // Closures capture shared outer frames, so a declaration installed later in an
        // outer function can still be the lexical target when the closure is invoked. A
        // user declaration also replaces the prelude binding in that shared frame.
        let following = (occurrence_depth > scope_data.function_depth)
            .then(|| {
                scope_data.symbols.iter().copied().find(|id| {
                    let symbol = &self.hir.symbols[*id];
                    has_name(*id) && symbol.activation > offset && symbol.declaration.is_some()
                })
            })
            .flatten();
        match preceding {
            Some(symbol) if self.hir.symbols[symbol].builtin => following.or(Some(symbol)),
            Some(symbol) => Some(symbol),
            None => following,
        }
    }

    fn scope_at(&self, offset: usize) -> Option<ScopeId> {
        self.hir
            .scopes
            .iter()
            .filter(|(_, scope)| contains_inclusive(scope.span, offset))
            .min_by_key(|(id, scope)| {
                (
                    scope.span.end.saturating_sub(scope.span.start),
                    std::cmp::Reverse(self.scope_depth(*id)),
                )
            })
            .map(|(id, _)| id)
    }

    fn scope_depth(&self, mut scope: ScopeId) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.hir.scopes[scope].parent {
            depth += 1;
            scope = parent;
        }
        depth
    }
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset < span.end
}

fn contains_inclusive(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}
