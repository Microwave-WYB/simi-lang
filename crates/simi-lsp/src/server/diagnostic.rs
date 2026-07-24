use super::*;

impl Backend {
    pub(super) fn diagnostics_notification(&self, uri: &Url) -> Notification {
        let Some(document) = self.documents.get(uri) else {
            return Notification::new(
                PublishDiagnostics::METHOD.to_owned(),
                PublishDiagnosticsParams::new(uri.clone(), Vec::new(), None),
            );
        };
        let text = source_text(&self.db, document.file);
        let mut analysis_diagnostics = diagnostics(&self.db, document.file).as_ref().clone();
        analysis_diagnostics
            .extend(infer_types(&self.db, document.file, &self.module_shapes).diagnostics);
        analysis_diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.span.start,
                diagnostic.span.end,
                diagnostic.code.as_str(),
            )
        });
        analysis_diagnostics.dedup_by(|left, right| {
            left.span == right.span && left.code == right.code && left.detail == right.detail
        });
        let items = analysis_diagnostics
            .iter()
            .filter_map(|diagnostic| {
                let related_information = diagnostic
                    .related
                    .iter()
                    .filter_map(|related| {
                        Some(DiagnosticRelatedInformation {
                            location: Location::new(
                                uri.clone(),
                                position::range(&text, related.span).ok()?,
                            ),
                            message: related.message.clone(),
                        })
                    })
                    .collect::<Vec<_>>();
                Some(Diagnostic {
                    range: position::range(&text, diagnostic.span).ok()?,
                    severity: Some(match diagnostic.severity {
                        AnalysisDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
                    }),
                    code: Some(NumberOrString::String(diagnostic.code.as_str().to_owned())),
                    source: Some("simi".to_owned()),
                    message: diagnostic.message(),
                    related_information: (!related_information.is_empty())
                        .then_some(related_information),
                    ..Diagnostic::default()
                })
            })
            .collect();
        Notification::new(
            PublishDiagnostics::METHOD.to_owned(),
            PublishDiagnosticsParams::new(uri.clone(), items, Some(document.version)),
        )
    }
}
