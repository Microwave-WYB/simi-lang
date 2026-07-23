mod db;
mod lower;
mod model;
mod modules;
mod resolver;
mod types;

pub use db::{
    AnalysisDatabase, FileId, ParsedFile, diagnostics, document_symbols, hir, parse, references,
    resolve, source_text, type_inference,
};
pub use simi_syntax::span::Span;

pub use model::{
    AnalysisDiagnostic, AnalysisDiagnosticCode, AnalysisDiagnosticSeverity, Capture,
    DocumentSymbol, ExportField, ExprData, ExprId, Hir, HoverFacts, ModuleMember, ModuleShape,
    ModuleValue, NameOccurrence, OccurrenceKind, ParameterPostType, PatternData, PatternId,
    RelatedDiagnostic, RenameError, Resolution, ScopeData, ScopeId, SymbolData, SymbolId,
    SymbolKind, Type, TypeInference, display_signature,
};
pub use modules::{
    imported_members, imported_modules, member_at, member_completions, module_at, module_shape,
};
pub use types::{expression_type_at, field_type_at, infer_types, symbol_type_at, wildcard_type_at};
