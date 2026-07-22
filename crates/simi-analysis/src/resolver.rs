use std::collections::{BTreeSet, HashMap};

use crate::model::{Capture, DocumentSymbol, Hir, Resolution, ScopeId, SymbolKind};

pub(crate) fn resolve(hir: Hir) -> Resolution {
    let mut result = Resolution {
        hir,
        occurrence_symbols: Vec::new(),
        symbol_references: HashMap::new(),
        captures: BTreeSet::new(),
    };
    for occurrence in &result.hir.occurrences {
        let symbol = result.resolve_name(occurrence.scope, occurrence.span.start, &occurrence.name);
        result.occurrence_symbols.push(symbol);
        let Some(symbol) = symbol else {
            continue;
        };
        result
            .symbol_references
            .entry(symbol)
            .or_default()
            .push(occurrence.span);
        let symbol_depth = result.hir.scopes[result.hir.symbols[symbol].scope].function_depth;
        let occurrence_depth = result.hir.scopes[occurrence.scope].function_depth;
        if occurrence_depth > symbol_depth
            && let Some(function_scope) = enclosing_function(&result, occurrence.scope)
        {
            result.captures.insert(Capture {
                function_scope,
                symbol,
            });
        }
    }
    result
}

pub(crate) fn document_symbols(resolution: &Resolution) -> Vec<DocumentSymbol> {
    resolution
        .hir
        .symbols
        .iter()
        .filter_map(|(id, symbol)| {
            let span = symbol.declaration?;
            matches!(
                symbol.kind,
                SymbolKind::Function | SymbolKind::Let | SymbolKind::Pattern
            )
            .then(|| DocumentSymbol {
                symbol: id,
                name: symbol.name.clone(),
                kind: symbol.kind,
                span,
            })
        })
        .collect()
}

fn enclosing_function(resolution: &Resolution, mut scope: ScopeId) -> Option<ScopeId> {
    loop {
        let data = &resolution.hir.scopes[scope];
        match data.parent {
            Some(parent) if resolution.hir.scopes[parent].function_depth == data.function_depth => {
                scope = parent;
            }
            Some(_) if data.function_depth > 0 => return Some(scope),
            Some(parent) => scope = parent,
            None => return None,
        }
    }
}
