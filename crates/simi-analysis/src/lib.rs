mod db;
mod lower;
mod model;
mod resolver;

pub use db::{
    AnalysisDatabase, FileId, ParsedFile, diagnostics, document_symbols, hir, parse, references,
    resolve, source_text,
};
pub use simi_syntax::span::Span;

pub use model::{
    AnalysisDiagnostic, AnalysisDiagnosticCode, AnalysisDiagnosticSeverity, Capture,
    DocumentSymbol, ExprData, ExprId, Hir, HoverFacts, NameOccurrence, OccurrenceKind, PatternData,
    PatternId, RelatedDiagnostic, RenameError, Resolution, ScopeData, ScopeId, SymbolData,
    SymbolId, SymbolKind,
};
