use simi::{Engine, SimiError, eval};

fn assert_eval(source: &str, expected: &str) {
    let value = eval(source)
        .expect("program should have no hard diagnostic")
        .expect("program should not raise");
    assert_eq!(value.render(), expected);
}

#[test]
fn list_and_map_producers_are_public_iterators() {
    assert_eval(
        r#"


        let iter = require("std/iter")
        [
            iter.to_list(list.iter([1, nil, 3])),
            iter.to_list(map.iter({ first = 1, [10] = nil, last = 3 })),
        ]
        "#,
        "[[1, nil, 3], [{key=\"first\", value=1}, {key=\"last\", value=3}]]",
    );
}

#[test]
fn erased_iterator_item_annotations_preserve_fold_and_nil_items() {
    assert_eval(
        r#"

        let iter = require("std/iter")
        let number = require("std/number")
        let total =
            [1, 2, 3, 4]
            |> list.iter()
            |> iter.fold(0, fn(acc, item)
                acc + item)
        let nil_items = iter.to_list(
            iter.map(list.iter([1, nil, 3]), fn(item)
                item)
        )
        [total |> number.to_string(), nil_items]
        "#,
        "[\"10\", [1, nil, 3]]",
    );
}

#[test]
fn iterators_are_lazy_single_pass_and_sticky_after_exhaustion() {
    assert_eval(
        r#"

        let iter = require("std/iter")
        let calls = []
        let source = list.iter([1, 2])
        let mapped = iter.map(source, fn(value) do
            list.append(calls, value)
            value * 2
        end)
        let before = calls
        let first = iter.next(mapped)
        let second = iter.next(mapped)
        let done = iter.next(mapped)
        let again = iter.next(mapped)
        [before, first, second, done, again, calls]
        "#,
        "[[1, 2], {done=false, value=2}, {done=false, value=4}, {done=true}, {done=true}, [1, 2]]",
    );
}

#[test]
fn custom_iterators_stay_exhausted_and_nil_queries_do_not_use_nil_as_a_sentinel() {
    assert_eval(
        r#"

        let iter = require("std/iter")
        let state = {calls = 0}
        let source = iter.from(fn() do
            state.calls = state.calls + 1
            if state.calls == 1 then { done = true }
            else { done = false, value = 1 }
            end
        end)
        [
            iter.next(source),
            iter.next(source),
            state.calls,
            iter.contains(list.iter([nil]), nil),
            iter.any(list.iter([nil]), fn(value)
                value == nil),
        ]
        "#,
        "[{done=true}, {done=true}, 1, true, true]",
    );
}

#[test]
fn map_and_filter_are_lazy_and_filter_predicates_are_strict() {
    assert_eval(
        r#"

        let iter = require("std/iter")
        let seen = []
        let filtered = iter.filter(list.iter([1, 2, 3]), fn(value) do
            list.append(seen, value)
            value >= 2
        end)
        let first = iter.next(filtered)
        [first, seen, iter.to_list(filtered)]
        "#,
        "[{done=false, value=2}, [1, 2, 3], [3]]",
    );

    let error = Engine::with_stdlib().eval(
        r#" let iter = require("std/iter") iter.to_list(iter.filter(list.iter([1]), fn(value)
     value))"#,
    );
    assert!(error.is_err());
}

#[test]
fn consumers_fold_search_queries_and_each_have_contracts() {
    assert_eval(
        r#"

        let iter = require("std/iter")
        let values = [1, 2, 3, 4]
        [
            iter.fold(list.iter(values), 0, fn(total, value)
                total + value),
            iter.find(list.iter(values), fn(value)
                value >= 3),
            iter.find_index(list.iter(values), fn(value)
                value >= 3),
            iter.contains(list.iter(values), 2),
            iter.any(list.iter(values), fn(value)
                value == 4),
            iter.all(list.iter(values), fn(value)
                value < 5),
            iter.count(list.iter(values), fn(value)
                value % 2 == 0),
            iter.each(list.iter(values), fn(value)
                value),
        ]
        "#,
        "[10, 3, 2, true, true, true, 2, nil]",
    );
}

#[test]
fn consumers_short_circuit_and_leave_the_remainder_unconsumed() {
    assert_eval(
        r#"

        let iter = require("std/iter")
        let source = list.iter([1, 2, 3])
        let found = iter.find(source, fn(value)
            value == 2)
        [found, iter.to_list(source)]
        "#,
        "[2, [3]]",
    );

    assert_eval(
        r#"

        let iter = require("std/iter")
        let source = list.iter([1, 2, 3])
        let result = iter.all(source, fn(value)
            value < 2)
        [result, iter.to_list(source)]
        "#,
        "[false, [3]]",
    );
}

#[test]
fn list_iterator_snapshots_structural_mutation() {
    assert_eval(
        r#"

        let iter = require("std/iter")
        let values = [1, 2]
        let source = list.iter(values)
        list.append(values, 3)
        [iter.to_list(source), values]
        "#,
        "[[1, 2], [1, 2, 3]]",
    );
}

#[test]
fn raises_propagate_through_iterator_adapters_and_consumers() {
    let raised = match eval(
        r#"

        let iter = require("std/iter")
        iter.to_list(iter.map(list.iter([1]), fn(value)
            raise { error = "callback_failed", value = value }))
        "#,
    )
    .expect("raise should not be a hard diagnostic")
    {
        Err(value) => value,
        Ok(value) => panic!("expected raise, got {}", value.render()),
    };
    assert_eq!(
        raised.value.render(),
        "{error=\"callback_failed\", value=1}"
    );
}

#[test]
fn malformed_steps_are_hard_contract_diagnostics() {
    for expression in ["1", "{}", "{ done = 1 }"] {
        let source = format!(
            "let iter = require(\"std/iter\") iter.to_list(iter.from(fn()
    {expression}))"
        );
        assert!(matches!(
            Engine::with_stdlib().eval(&source),
            Err(SimiError::Runtime(_))
        ));
    }
}

#[test]
fn iterator_control_helpers_are_structural_and_require_one_argument() {
    assert_eval(
        r#"
        let iter = require("std/iter")
        [iter.break(7), iter.break(nil), iter.continue("next"), iter.continue(nil)]
        "#,
        "[{control=\"break\", value=7}, {control=\"break\"}, {control=\"continue\", value=\"next\"}, {control=\"continue\"}]",
    );

    for expression in ["iter.break()", "iter.continue(1, 2)"] {
        let source = format!("let iter = require(\"std/iter\") {expression}");
        assert!(matches!(
            Engine::with_stdlib().eval(&source),
            Err(SimiError::Runtime(_))
        ));
    }
}

#[test]
fn while_drivers_handle_exhaustion_controls_aliases_and_remainders() {
    assert_eval(
        r#"
        let iter = require("std/iter")
        let source = list.iter([1, 2, 3])
        let stopped = iter.each_while(source, fn(value)
            if value == 2 then iter.break(value * 10)
            else iter.continue(nil)
            end)
        let exhausted = iter.each_while(list.iter([]), fn(value)
            iter.break(value))
        let missing_break_value = iter.each_while(
            list.iter([1]),
            fn(value)
                { control = "break" },
        )
        [stopped, iter.to_list(source), exhausted, missing_break_value]
        "#,
        "[20, [3], nil, nil]",
    );

    assert_eval(
        r#"
        let iter = require("std/iter")
        let state = []
        let result = iter.fold_while(list.iter([1, 2]), state, fn(current, value) do
            list.append(current, value)
            iter.continue(current)
        end)
        list.append(result, 3)
        let nil_state = iter.fold_while(
            list.iter([1]),
            10,
            fn(current, value)
                { control = "continue" },
        )
        let broken = iter.fold_while(
            list.iter([4, 5]),
            0,
            fn(current, value)
                iter.break(current + value),
        )
        [state, result, nil_state, broken]
        "#,
        "[[1, 2, 3], [1, 2, 3], nil, 4]",
    );
}

#[test]
fn repeat_with_is_lazy_emits_nil_and_propagates_raises() {
    assert_eval(
        r#"
        let iter = require("std/iter")
        let state = {calls = 0}
        let repeated = iter.repeat_with(fn() do
            state.calls = state.calls + 1
            if state.calls == 1 then nil else state.calls end
        end)
        let before = state.calls
        [before, iter.next(repeated), iter.next(repeated), state.calls]
        "#,
        "[0, {done=false}, {done=false, value=2}, 2]",
    );

    let raised = match eval(
        r#"
        let iter = require("std/iter")
        let repeated = iter.repeat_with(fn()
            raise "producer_failed")
        iter.next(repeated)
        "#,
    )
    .expect("producer raise should not be a hard diagnostic")
    {
        Err(raised) => raised,
        Ok(value) => panic!("expected producer raise, got {}", value.render()),
    };
    assert_eq!(raised.value.render(), "\"producer_failed\"");
}

#[test]
fn while_driver_source_and_callback_raises_propagate() {
    for source in [
        r#"
        let iter = require("std/iter")
        let source = fn()
            raise { error = "source_failed" }
        iter.each_while(source, fn(value)
            iter.continue(value))
        "#,
        r#"
        let iter = require("std/iter")
        iter.fold_while(list.iter([1]), 0, fn(state, value)
            raise { error = "callback_failed", value = value })
        "#,
    ] {
        let raised = match eval(source).expect("raise should not be a hard diagnostic") {
            Err(raised) => raised,
            Ok(value) => panic!("expected operation raise, got {}", value.render()),
        };
        assert!(
            [
                "{error=\"source_failed\"}",
                "{error=\"callback_failed\", value=1}"
            ]
            .contains(&raised.value.render().as_str())
        );
    }
}

#[test]
fn malformed_while_controls_are_hard_contract_diagnostics() {
    for control in ["1", "{}", "{ control = 1 }", "{ control = \"stop\" }"] {
        let source = format!(
            "let iter = require(\"std/iter\") iter.each_while(list.iter([1]), fn(value)
    {control})"
        );
        assert!(matches!(
            Engine::with_stdlib().eval(&source),
            Err(SimiError::Runtime(_))
        ));
    }
    assert!(matches!(
        Engine::with_stdlib().eval(
            r#"let iter = require("std/iter")
               iter.fold_while(list.iter([1]), 0, fn(state, value)
                   {})"#,
        ),
        Err(SimiError::Runtime(_))
    ));
}

#[test]
fn public_iterator_controls_support_stateful_case_and_catch_workflows() {
    assert_eval(
        r#"
        let iter = require("std/iter")
        let state = {attempts = 0}
        let readings = iter.repeat_with(fn() do
            state.attempts = state.attempts + 1
            if state.attempts == 4 then
                raise { error = "sensor_failed", attempt = state.attempts }
            else
                state.attempts * 3
            end
        end)
        let summary = iter.fold_while(
            readings,
            { sum = 0, count = 0 },
            fn(state, value)
                case value of
                    value when value >= 9 =>
                        iter.break({ status = "threshold", sum = state.sum, value = value })
                    value =>
                        iter.continue({ sum = state.sum + value, count = state.count + 1 })
                end,
        )
        let recovered = do
            iter.next(readings)
        catch
            { error = "sensor_failed", attempt = attempt } =>
                { status = "recovered", attempt = attempt }
        end
        [summary, recovered, state.attempts]
        "#,
        "[{status=\"threshold\", sum=9, value=9}, {status=\"recovered\", attempt=4}, 4]",
    );
}

#[test]
fn native_filter_driver_is_stack_safe_for_a_million_rejections() {
    assert_eval(
        r#"
        let iter = require("std/iter")
        let state = {current = 0}
        let source = iter.repeat_with(fn() do
            state.current = state.current + 1
            state.current
        end)
        let filtered = iter.filter(source, fn(value)
            value > 1000000)
        iter.next(filtered)
        "#,
        "{done=false, value=1000001}",
    );
}

#[test]
fn removed_collection_hofs_and_map_views_are_not_exports() {
    let source = r#"


        [type(list.map), type(list.filter), type(list.fold), type(map.keys), type(map.values), type(map.entries)]
    "#;
    assert_eval(
        source,
        "[\"nil\", \"nil\", \"nil\", \"nil\", \"nil\", \"nil\"]",
    );
}
