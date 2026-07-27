use simi::{SimiError, eval};

#[test]
fn terminal_expressions_are_hard_diagnostics_with_exact_spans() {
    for (source, message) in [
        ("panic", "panic"),
        ("panic \"unreachable state\"", "panic: unreachable state"),
        ("panic \"first\\nsecond\"", "panic: first\nsecond"),
        ("todo", "todo"),
        (
            "todo \"finish the \\\"decoder\\\"\"",
            "todo: finish the \"decoder\"",
        ),
        ("todo \"finish the decoder\"", "todo: finish the decoder"),
    ] {
        match eval(source) {
            Err(SimiError::Runtime(error)) => {
                assert_eq!(error.message, message);
                assert_eq!(error.span.start, 0);
                assert_eq!(error.span.end, source.len());
            }
            _ => panic!("expected hard runtime diagnostic"),
        }
    }
}

#[test]
fn protected_expression_cannot_catch_terminal_expressions() {
    for source in [
        "do panic catch of _ \"not reached\" end",
        "do todo \"finish\" catch of _ \"not reached\" end",
    ] {
        assert!(matches!(eval(source), Err(SimiError::Runtime(_))));
    }
}
