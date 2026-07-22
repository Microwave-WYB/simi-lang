use simiscript::{SimiError, Value, eval};

fn evaluate(source: &str) -> Value {
    match eval(source) {
        Ok(Ok(value)) => value,
        Ok(Err(raised)) => panic!("program should succeed, got {raised}"),
        Err(error) => panic!("program should evaluate, got {error}"),
    }
}

#[test]
fn anonymous_functions_support_closures_immediate_calls_and_nesting() {
    let value = evaluate(
        r#"
            let make_adder = fn(base) do
                fn(value) do base + value end
            end
            let add_two = make_adder(2)
            [add_two(5), fn(value) do value * 2 end(6)]
        "#,
    );

    assert_eq!(value.render(), "[7, 12]");
}

#[test]
fn anonymous_functions_recurse_through_let_and_compose_as_primary_expressions() {
    let value = evaluate(
        r#"
            let factorial = fn(n) do
                if n == 0 then 1 else n * factorial(n - 1) end
            end
            fn apply(callable, value) do callable(value) end
            let piped = fn(value) do value + 1 end |> apply(4)
            let indexed = [fn(value) do value * 3 end][0](5)
            [factorial(6), piped, indexed]
        "#,
    );

    assert_eq!(value.render(), "[720, 5, 15]");
}

#[test]
fn anonymous_function_rendering_and_diagnostics_use_the_anonymous_name() {
    assert_eq!(evaluate("fn() do nil end").render(), "<fn <anonymous>>");

    match eval("fn(value) do value end()") {
        Err(SimiError::Runtime(error)) => assert_eq!(
            error.message,
            "function `<anonymous>` expects 1 arguments, got 0"
        ),
        _ => panic!("expected anonymous function arity error"),
    }

    let source = "fn() do raise \"boom\" end()";
    let raised = match eval(source) {
        Ok(Err(raised)) => raised,
        _ => panic!("expected raised value"),
    };
    assert_eq!(raised.frames.len(), 1);
    assert_eq!(raised.frames[0].function, "<anonymous>");
    assert_eq!(raised.frames[0].call_span.start, 0);
    assert_eq!(raised.frames[0].call_span.end, source.len());
}

#[test]
fn malformed_anonymous_functions_report_exact_parse_spans() {
    let source = "let f = fn(value, value) do value end";
    match eval(source) {
        Err(SimiError::Parse(error)) => {
            assert_eq!(error.message, "duplicate parameter `value`");
            assert_eq!((error.span.start, error.span.end), (18, 23));
        }
        _ => panic!("expected duplicate parameter error"),
    }

    let source = "let f = fn(value) do value";
    match eval(source) {
        Err(SimiError::Parse(error)) => {
            assert_eq!(
                error.message,
                "expected `end` after function body, found `end of file`"
            );
            assert_eq!(
                (error.span.start, error.span.end),
                (source.len(), source.len())
            );
        }
        _ => panic!("expected missing end error"),
    }
}
