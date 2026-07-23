mod db;
mod lower;
mod model;
mod modules;
mod resolver;

pub use db::{
    AnalysisDatabase, FileId, ParsedFile, diagnostics, document_symbols, hir, parse, references,
    resolve, source_text,
};
pub use simi_syntax::span::Span;

pub use model::{
    AnalysisDiagnostic, AnalysisDiagnosticCode, AnalysisDiagnosticSeverity, Capture,
    DocumentSymbol, ExportField, ExprData, ExprId, Hir, HoverFacts, ModuleMember, ModuleShape,
    ModuleValue, NameOccurrence, OccurrenceKind, PatternData, PatternId, RelatedDiagnostic,
    RenameError, Resolution, ScopeData, ScopeId, SymbolData, SymbolId, SymbolKind,
    display_signature,
};
pub use modules::{
    imported_members, imported_modules, member_at, member_completions, module_at, module_shape,
};
