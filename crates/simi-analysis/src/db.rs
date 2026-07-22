use std::sync::Arc;

use salsa::Setter;
use simi_syntax::span::Span;
use simi_syntax::{SyntaxDiagnostic, SyntaxNode};

use crate::model::{AnalysisDiagnostic, DocumentSymbol, Hir, Resolution, SymbolId};

#[salsa::input(debug)]
pub struct FileId {
    #[returns(deref)]
    text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedFile {
    green: rowan::GreenNode,
    pub diagnostics: Vec<SyntaxDiagnostic>,
}

impl ParsedFile {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
}

#[salsa::db]
#[derive(Clone, Default)]
pub struct AnalysisDatabase {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for AnalysisDatabase {}

impl AnalysisDatabase {
    pub fn add_file(&self, text: impl Into<String>) -> FileId {
        FileId::new(self, text.into())
    }

    pub fn set_source(&mut self, file: FileId, text: impl Into<String>) {
        file.set_text(self).to(text.into());
    }
}

#[salsa::tracked(returns(clone))]
pub fn source_text(db: &dyn salsa::Database, file: FileId) -> Arc<String> {
    Arc::new(file.text(db).to_owned())
}

#[salsa::tracked(returns(clone))]
pub fn parse(db: &dyn salsa::Database, file: FileId) -> Arc<ParsedFile> {
    let parsed = simi_syntax::parse_source(file.text(db));
    Arc::new(ParsedFile {
        green: parsed.syntax().green().into_owned(),
        diagnostics: parsed.diagnostics().to_vec(),
    })
}

#[salsa::tracked(returns(clone))]
pub fn hir(db: &dyn salsa::Database, file: FileId) -> Arc<Hir> {
    Arc::new(crate::lower::lower(parse(db, file).syntax()))
}

#[salsa::tracked(returns(clone))]
pub fn resolve(db: &dyn salsa::Database, file: FileId) -> Arc<Resolution> {
    Arc::new(crate::resolver::resolve((*hir(db, file)).clone()))
}

#[salsa::tracked(returns(clone))]
pub fn diagnostics(db: &dyn salsa::Database, file: FileId) -> Arc<Vec<AnalysisDiagnostic>> {
    Arc::new(
        parse(db, file)
            .diagnostics
            .iter()
            .map(|diagnostic| AnalysisDiagnostic {
                span: diagnostic.span,
                message: diagnostic.message.clone(),
            })
            .collect(),
    )
}

#[salsa::tracked(returns(clone))]
pub fn document_symbols(db: &dyn salsa::Database, file: FileId) -> Arc<Vec<DocumentSymbol>> {
    Arc::new(crate::resolver::document_symbols(&resolve(db, file)))
}

#[salsa::tracked(returns(clone))]
pub fn references(db: &dyn salsa::Database, file: FileId, symbol: SymbolId) -> Arc<Vec<Span>> {
    Arc::new(resolve(db, file).references(symbol).to_vec())
}
