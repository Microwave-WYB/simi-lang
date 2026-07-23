use std::collections::BTreeMap;
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
    let parsed = parse(db, file);
    let mut diagnostics = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| AnalysisDiagnostic {
            span: diagnostic.span,
            message: diagnostic.message.clone(),
        })
        .collect::<Vec<_>>();
    // Recovered HIR can contain incomplete declarations, so resolver diagnostics are
    // emitted only after syntax succeeds. Sequential declarations in one runtime frame
    // are a guaranteed hard error; prelude symbols are intentionally shadowable.
    if diagnostics.is_empty() {
        let resolution = resolve(db, file);
        for (_, scope) in resolution.hir.scopes.iter() {
            let mut declarations = BTreeMap::<&str, Span>::new();
            for symbol in scope.symbols.iter().map(|id| &resolution.hir.symbols[*id]) {
                let Some(span) = symbol.declaration else {
                    continue;
                };
                if let Some(previous) = declarations.get(symbol.name.as_str()) {
                    diagnostics.push(AnalysisDiagnostic {
                        span,
                        message: format!(
                            "name `{}` is already defined in this scope (first defined at byte {})",
                            symbol.name, previous.start
                        ),
                    });
                } else {
                    declarations.insert(&symbol.name, span);
                }
            }
        }
    }
    Arc::new(diagnostics)
}

#[salsa::tracked(returns(clone))]
pub fn document_symbols(db: &dyn salsa::Database, file: FileId) -> Arc<Vec<DocumentSymbol>> {
    Arc::new(crate::resolver::document_symbols(&resolve(db, file)))
}

#[salsa::tracked(returns(clone))]
pub fn references(db: &dyn salsa::Database, file: FileId, symbol: SymbolId) -> Arc<Vec<Span>> {
    Arc::new(resolve(db, file).references(symbol).to_vec())
}
