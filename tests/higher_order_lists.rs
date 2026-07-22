use simiscript::{Engine, SimiError, eval};

fn assert_eval(source: &str, expected: &str) {
    let value = eval(source)
        .expect("program should have no hard diagnostic")
        .expect("program should not raise");
    assert_eq!(value.render(), expected);
}

#[test]
fn map_filter_and_fold_accept_anonymous_closures() {
    assert_eval(
        r#"
        let list = require("list")
        let factor = 3
        let mapped = list.map([1, 2, 3, 4], fn(value) do value * factor end)
        let filtered = list.filter(mapped, fn(value) do value >= 6 end)
        let total = list.fold(filtered, 0, fn(sum, value) do sum + value end)
        [mapped, filtered, total]
        "#,
        "[[3, 6, 9, 12], [6, 9, 12], 27]",
    );
}

#[test]
fn higher_order_list_calls_compose_with_pipelines_and_native_callbacks() {
    assert_eval(
        r#"
        let list = require("list")
        let core = require("core")
        let doubled = [1, 2, 3] |> list.map(fn(value) do value * 2 end)
        [doubled, list.map([1, "two", true], core.type)]
        "#,
        "[[2, 4, 6], [\"integer\", \"string\", \"boolean\"]]",
    );
}

#[test]
fn iteration_uses_a_snapshot_when_callbacks_mutate_the_source() {
    assert_eval(
        r#"
        let list = require("list")
        let values = [1, 2]
        let mapped = list.map(values, fn(value) do
            list.append(values, value + 10)
            value
        end)
        [mapped, values]
        "#,
        "[[1, 2], [1, 2, 11, 12]]",
    );
}

#[test]
fn fold_returns_its_initial_value_for_an_empty_list() {
    assert_eval(
        r#"
        let list = require("list")
        list.fold([], "initial", fn(left, right) do left + right end)
        "#,
        "\"initial\"",
    );
}

#[test]
fn empty_map_and_filter_return_empty_lists_after_validating_callbacks() {
    assert_eval(
        r#"
        let list = require("list")
        [
            list.map([], fn(value) do value end),
            list.filter([], fn(value) do true end),
        ]
        "#,
        "[[], []]",
    );
}

#[test]
fn callback_raises_propagate_through_higher_order_calls() {
    assert_eval(
        r#"
        let list = require("list")
        try list.map([1], fn(value) do
            raise {error="callback_failed", value=value}
        end) catch
            case {error="callback_failed", value=value} -> value
        end
        "#,
        "1",
    );
}

#[test]
fn callback_raise_frames_include_the_anonymous_callback_and_caller() {
    let source = r#"
        fn outer() do
            let list = require("list")
            list.map([1], fn(value) do raise value end)
        end
        outer()
    "#;
    let raised = match eval(source).expect("callback raise should have no hard diagnostic") {
        Err(raised) => raised,
        Ok(value) => panic!("callback should raise, got {}", value.render()),
    };
    assert_eq!(raised.value.render(), "1");
    assert_eq!(raised.frames.len(), 2);
    assert_eq!(raised.frames[0].function, "<anonymous>");
    assert_eq!(raised.frames[1].function, "outer");
}

#[test]
fn invalid_callbacks_and_filter_results_are_hard_diagnostics() {
    let invalid = match Engine::with_stdlib().eval("let list = require(\"list\") list.map([], 1)") {
        Err(error) => error,
        Ok(_) => panic!("non-callable callback should be a hard diagnostic"),
    };
    assert!(
        invalid
            .to_string()
            .contains("cannot call value of type integer")
    );

    let wrong_arity = match Engine::with_stdlib()
        .eval("let list = require(\"list\") list.map([], fn(left, right) do left end)")
    {
        Err(error) => error,
        Ok(_) => panic!("wrong callback arity should be a hard diagnostic"),
    };
    assert!(
        wrong_arity
            .to_string()
            .contains("expects 2 arguments, got 1")
    );

    let predicate = match Engine::with_stdlib()
        .eval("let list = require(\"list\") list.filter([1], fn(value) do value end)")
    {
        Err(error) => error,
        Ok(_) => panic!("non-boolean predicate should be a hard diagnostic"),
    };
    assert!(matches!(predicate, SimiError::Runtime(_)));
    assert!(
        predicate
            .to_string()
            .contains("list.filter callback must return a boolean, got integer")
    );
}
