use simi::{Engine, SimiError, eval};

#[test]
fn legacy_loop_and_label_forms_are_rejected() {
    for source in ["loop break 1 end", "@outer loop break 1 end"] {
        assert!(eval(source).is_err(), "{source} should be rejected");
    }

    let value = eval("let loop = 7 loop").unwrap().unwrap();
    assert_eq!(value.render(), "7");
}

#[test]
fn iterator_control_members_remain_callable() {
    let value = eval("let iter = require(\"std/iter\") [iter.break(7), iter.continue(nil)]")
        .unwrap()
        .unwrap();
    assert_eq!(
        value.render(),
        "[{control=\"break\", value=7}, {control=\"continue\"}]"
    );
}

#[test]
fn iterator_loop_returns_break_payload_and_ignores_continue_payloads() {
    let value = eval(
        r#"
        let iter = require("std/iter")
        let state = {iterations = 0}
        let result = iter.loop(fn() do
            state.iterations = state.iterations + 1
            if state.iterations == 3 then iter.break({ result = state.iterations })
            else iter.continue("ignored")
            end
        end)
        [result, state.iterations]
        "#,
    )
    .unwrap()
    .unwrap();
    assert_eq!(value.render(), "[{result=3}, 3]");
}

#[test]
fn iterator_loop_propagates_raises_and_rejects_malformed_controls() {
    let raised = match eval(
        r#"
        let iter = require("std/iter")
        iter.loop(fn()
            raise { error = "loop_failed" })
        "#,
    )
    .expect("raise should not be a hard diagnostic")
    {
        Err(raised) => raised,
        Ok(value) => panic!("expected loop callback raise, got {}", value.render()),
    };
    assert_eq!(raised.value.render(), "{error=\"loop_failed\"}");

    for control in ["1", "{}", "{ control = 1 }", "{ control = \"stop\" }"] {
        let source = format!(
            "let iter = require(\"std/iter\") iter.loop(fn()
    {control})"
        );
        assert!(matches!(
            Engine::with_stdlib().eval(&source),
            Err(SimiError::Runtime(_))
        ));
    }
}

#[test]
fn native_iterator_loop_is_stack_safe_for_a_million_iterations() {
    let value = eval(
        r#"
        let iter = require("std/iter")
        let state = {count = 0}
        iter.loop(fn() do
            state.count = state.count + 1
            if state.count == 1000000 then iter.break(state.count)
            else iter.continue(nil)
            end
        end)
        "#,
    )
    .unwrap()
    .unwrap();
    assert_eq!(value.render(), "1000000");
}
