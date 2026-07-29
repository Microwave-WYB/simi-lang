use simi::{Engine, SimiError, eval};

fn assert_eval(source: &str, expected: &str) {
    let value = eval(source)
        .expect("program should have no hard diagnostic")
        .expect("program should not raise");
    assert_eq!(value.render(), expected);
}

#[test]
fn iterator_sources_are_finite_half_open_and_sticky() {
    assert_eval(
        r#"
        let empty = iter.empty()
        let once = iter.once(nil)
        let repeated = iter.repeat("x", 3)
        let ascending = iter.range(-2, 3)
        let descending = iter.range(3, 1)
        [
            iter.next(empty), iter.next(empty),
            iter.next(once), iter.next(once), iter.next(once),
            iter.to_list(repeated), iter.next(repeated),
            iter.to_list(ascending), iter.next(ascending),
            iter.to_list(descending),
        ]
        "#,
        "[{done=true}, {done=true}, {done=false}, {done=true}, {done=true}, [\"x\", \"x\", \"x\"], {done=true}, [-2, -1, 0, 1, 2], {done=true}, []]",
    );
}

#[test]
fn repeat_once_and_zip_longest_preserve_alias_identity() {
    assert_eval(
        r#"
        let shared = []
        let repeated = iter.to_list(iter.repeat(shared, 2))
        list.append(repeated[0], 1)

        let once_value = []
        let yielded_once = iter.to_list(iter.once(once_value))
        list.append(yielded_once[0], 2)

        let fill = []
        let longest = iter.to_list(iter.zip_longest(
            list.iter([10, 20]),
            list.iter([30]),
            fill,
        ))
        list.append(longest[1][1], 3)
        [shared, repeated, once_value, yielded_once, fill, longest]
        "#,
        "[[1], [[1], [1]], [2], [[2]], [3], [[10, 30], [20, [3]]]]",
    );
}

#[test]
fn take_and_drop_are_lazy_and_preserve_source_remainders() {
    assert_eval(
        r#"
        let state = {calls = 0}
        let source = iter.repeat_with(fn() do
            state.calls = state.calls + 1
            state.calls
        end)
        let taken = iter.take(source, 2)
        let before_take = state.calls
        let taken_values = iter.to_list(taken)
        let after_take = state.calls
        let source_next = iter.next(source)

        let dropped_source = list.iter([1, 2, 3, 4])
        let dropped = iter.drop(dropped_source, 2)
        let before_drop = iter.next(iter.take(dropped_source, 0))
        let first_after_drop = iter.next(dropped)
        let dropped_remainder = iter.to_list(dropped)

        [
            before_take, taken_values, after_take, source_next,
            before_drop, first_after_drop, dropped_remainder,
        ]
        "#,
        "[0, [1, 2], 2, {done=false, value=3}, {done=true}, {done=false, value=3}, [4]]",
    );
}

#[test]
fn enumerate_zip_and_zip_longest_yield_exact_list_pairs() {
    assert_eval(
        r#"
        let right = list.iter(["a", "b", "c"])
        let zipped = iter.zip(list.iter([1, 2]), right)
        let pairs = iter.to_list(zipped)
        let right_remainder = iter.to_list(right)
        [
            iter.to_list(iter.enumerate(list.iter([nil, 8]))),
            pairs,
            right_remainder,
            iter.to_list(iter.zip_longest(
                list.iter([1]),
                list.iter([2, 3, 4]),
                nil,
            )),
            type(pairs[0]), list.length(pairs[0]),
        ]
        "#,
        "[[[0, nil], [1, 8]], [[1, \"a\"], [2, \"b\"]], [\"c\"], [[1, 2], [nil, 3], [nil, 4]], \"list\", 2]",
    );
}

#[test]
fn pair_adapters_do_not_pull_sources_when_constructed() {
    assert_eval(
        r#"
        let state = {calls = 0}
        fn source() do
            iter.repeat_with(fn() do
                state.calls = state.calls + 1
                state.calls
            end)
        end
        let enumerated = iter.enumerate(source())
        let zipped = iter.zip(source(), iter.once(1))
        let longest = iter.zip_longest(iter.empty(), source(), nil)
        let before = state.calls
        let values = [iter.next(enumerated), iter.next(zipped), iter.next(longest)]
        [before, state.calls, values]
        "#,
        "[0, 3, [{done=false, value=[0, 1]}, {done=false, value=[2, 1]}, {done=false, value=[nil, 3]}]]",
    );
}

#[test]
fn source_raises_propagate_through_new_adapters() {
    for expression in [
        "iter.next(iter.take(source, 1))",
        "iter.next(iter.drop(source, 1))",
        "iter.next(iter.enumerate(source))",
        "iter.next(iter.zip(source, iter.once(1)))",
        "iter.next(iter.zip_longest(iter.empty(), source, nil))",
    ] {
        let source = format!(
            r#"
            let source = iter.from(fn() do raise {{ error = "source_failed" }} end)
            {expression}
            "#
        );
        let raised = match eval(&source).expect("raise should not be a hard diagnostic") {
            Err(raised) => raised,
            Ok(value) => panic!("expected source raise, got {}", value.render()),
        };
        assert_eq!(raised.value.render(), "{error=\"source_failed\"}");
    }
}

#[test]
fn invalid_counts_and_range_bounds_are_hard_diagnostics() {
    for expression in [
        "iter.repeat(1, -1)",
        "iter.repeat(1, 1.0)",
        "iter.take(iter.empty(), -1)",
        "iter.take(iter.empty(), \"1\")",
        "iter.drop(iter.empty(), -1)",
        "iter.drop(iter.empty(), nil)",
        "iter.range(1.0, 2)",
        "iter.range(1, true)",
    ] {
        assert!(
            matches!(
                Engine::with_stdlib().eval(expression),
                Err(SimiError::Runtime(_))
            ),
            "expected hard diagnostic for {expression}"
        );
    }
}

#[test]
fn dropping_a_million_items_is_stack_safe() {
    assert_eval(
        r#"
        let state = {current = 0}
        let source = iter.repeat_with(fn() do
            state.current = state.current + 1
            state.current
        end)
        iter.next(iter.drop(source, 1000000))
        "#,
        "{done=false, value=1000001}",
    );
}
