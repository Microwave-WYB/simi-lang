use super::*;

impl Backend {
    pub(super) fn diagnostics_notification(&self, result: AnalysisResult) -> Notification {
        let mut analysis_diagnostics = result.diagnostics;
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
                                result.uri.clone(),
                                position::range(&result.source, related.span).ok()?,
                            ),
                            message: related.message.clone(),
                        })
                    })
                    .collect::<Vec<_>>();
                Some(Diagnostic {
                    range: position::range(&result.source, diagnostic.span).ok()?,
                    severity: Some(match diagnostic.severity {
                        AnalysisDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
                        AnalysisDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
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
            PublishDiagnosticsParams::new(result.uri, items, Some(result.version)),
        )
    }
}

pub(super) fn background_analysis_worker(
    wakeups: Receiver<()>,
    pending: Arc<Mutex<HashMap<Url, AnalysisJob>>>,
    completed_wakeups: Sender<()>,
    completed: Arc<Mutex<HashMap<Url, AnalysisResult>>>,
) {
    loop {
        if wakeups.recv().is_err() {
            return;
        }
        while wakeups.try_recv().is_ok() {}
        let jobs = std::mem::take(
            &mut *pending
                .lock()
                .expect("analysis scheduler lock should not be poisoned"),
        );
        for (_, job) in jobs {
            let database = AnalysisDatabase::default();
            let file = database.add_file(job.source.clone());
            let mut diagnostics = diagnostics(&database, file).as_ref().clone();
            diagnostics.extend(
                simi_analysis::infer_types(&database, file, &job.module_shapes)
                    .diagnostics
                    .iter()
                    .cloned(),
            );
            let uri = job.uri.clone();
            completed
                .lock()
                .expect("completed analysis lock should not be poisoned")
                .insert(
                    uri,
                    AnalysisResult {
                        uri: job.uri,
                        generation: job.generation,
                        version: job.version,
                        source: job.source,
                        diagnostics,
                    },
                );
            match completed_wakeups.try_send(()) {
                Ok(()) | Err(TrySendError::Full(())) => {}
                Err(TrySendError::Disconnected(())) => return,
            }
        }
    }
}
