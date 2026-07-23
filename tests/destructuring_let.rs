use simi::{SimiError, Value, eval};

fn evaluate(source: &str) -> Value {
    match eval(source) {
        Ok(Ok(value)) => value,
        Ok(Err(raised)) => panic!("program should succeed, got {raised}"),
        Err(error) => panic!("program should evaluate, got {error}"),
    }
}

#[test]
fn destructuring_let_reuses_nested_list_and_map_patterns() {
    let value = evaluate(
        r#"
        let [first, {name=name}, ..rest] = [1, {name="Simi"}, 3, 4]
        let {enabled=true, missing=nil, ..settings} = {
            enabled=true,
            extra=9,
        }
        [first, name, rest, settings]
        "#,
    );

    assert_eq!(value.render(), "[1, \"Simi\", [3, 4], {extra=9}]");
}

#[test]
fn destructuring_let_rest_is_cow_and_nested_values_remain_aliased() {
    let value = evaluate(
        r#"
        let list = require("std/list")
        let nested = [1]
        let source = [0, nested, 2, 3]
        let [zero, captured, ..tail] = source
        source[2] = 9
        list.append(nested, 4)
        [zero, captured, tail, source]
        "#,
    );

    assert_eq!(value.render(), "[0, [1, 4], [2, 3], [0, [1, 4], 9, 3]]");
}

#[test]
fn destructuring_rhs_is_evaluated_once() {
    let value = evaluate(
        r#"
        let list = require("std/list")
        let calls = []
        fn produce() do
            list.append(calls, 1)
            [10, 20]
        end
        let [left, right] = produce()
        [left, right, list.length(calls)]
        "#,
    );

    assert_eq!(value.render(), "[10, 20, 1]");
}

#[test]
fn mismatch_is_a_hard_runtime_error_at_the_pattern_span() {
    let source = "let [x, y] = [1]";
    match eval(source) {
        Err(SimiError::Runtime(error)) => {
            assert_eq!(error.message, "let pattern did not match");
            assert_eq!((error.span.start, error.span.end), (4, 10));
        }
        _ => panic!("expected a hard destructuring mismatch"),
    }

    match eval("let {x=x} = {x=1, y=2}") {
        Err(SimiError::Runtime(error)) => {
            assert_eq!(error.message, "let pattern did not match");
        }
        _ => panic!("closed map destructuring must reject extra fields"),
    }
    assert_eq!(evaluate("let {x=x, ..} = {x=1, y=2} x").render(), "1");

    let attempted_catch = r#"
        fn unpack(value) do
            let [x, y] = value
            x + y
        end
        try unpack([1])
            catch _ do 0
        end
    "#;
    match eval(attempted_catch) {
        Err(SimiError::Runtime(error)) => {
            assert_eq!(error.message, "let pattern did not match");
        }
        _ => panic!("destructuring mismatch must bypass language catches"),
    }
}

#[test]
fn duplicate_bindings_and_malformed_patterns_are_parse_errors() {
    let duplicate = "let [value, value] = [1, 2]";
    match eval(duplicate) {
        Err(SimiError::Parse(error)) => {
            assert_eq!(error.message, "duplicate binding `value` in pattern");
            let second = duplicate.rfind("value").unwrap();
            assert_eq!((error.span.start, error.span.end), (second, second + 5));
        }
        _ => panic!("expected duplicate binding parse error"),
    }

    match eval("let [] [1]") {
        Err(SimiError::Parse(error)) => {
            assert!(error.message.contains("`=` after let pattern"));
        }
        _ => panic!("expected missing equals parse error"),
    }
}
