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
        let list = require("std/list")
        let map = require("std/map")
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
fn iterators_are_lazy_single_pass_and_sticky_after_exhaustion() {
    assert_eval(
        r#"
        let list = require("std/list")
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
        let list = require("std/list")
        let iter = require("std/iter")
        let calls = 0
        let source = iter.from(fn() do
            calls = calls + 1
            if calls == 1 then { done = true }
            else { done = false, value = 1 }
            end
        end)
        [
            iter.next(source),
            iter.next(source),
            calls,
            iter.contains(list.iter([nil]), nil),
            iter.any(list.iter([nil]), fn(value) do value == nil end),
        ]
        "#,
        "[{done=true}, {done=true}, 1, true, true]",
    );
}

#[test]
fn map_and_filter_are_lazy_and_filter_predicates_are_strict() {
    assert_eval(
        r#"
        let list = require("std/list")
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
        r#"let list = require("std/list") let iter = require("std/iter") iter.to_list(iter.filter(list.iter([1]), fn(value) do value end))"#,
    );
    assert!(error.is_err());
}

#[test]
fn consumers_fold_search_queries_and_each_have_contracts() {
    assert_eval(
        r#"
        let list = require("std/list")
        let iter = require("std/iter")
        let values = [1, 2, 3, 4]
        [
            iter.fold(list.iter(values), 0, fn(total, value) do total + value end),
            iter.find(list.iter(values), fn(value) do value >= 3 end),
            iter.find_index(list.iter(values), fn(value) do value >= 3 end),
            iter.contains(list.iter(values), 2),
            iter.any(list.iter(values), fn(value) do value == 4 end),
            iter.all(list.iter(values), fn(value) do value < 5 end),
            iter.count(list.iter(values), fn(value) do value % 2 == 0 end),
            iter.each(list.iter(values), fn(value) do value end),
        ]
        "#,
        "[10, 3, 2, true, true, true, 2, nil]",
    );
}

#[test]
fn consumers_short_circuit_and_leave_the_remainder_unconsumed() {
    assert_eval(
        r#"
        let list = require("std/list")
        let iter = require("std/iter")
        let source = list.iter([1, 2, 3])
        let found = iter.find(source, fn(value) do value == 2 end)
        [found, iter.to_list(source)]
        "#,
        "[2, [3]]",
    );

    assert_eval(
        r#"
        let list = require("std/list")
        let iter = require("std/iter")
        let source = list.iter([1, 2, 3])
        let result = iter.all(source, fn(value) do value < 2 end)
        [result, iter.to_list(source)]
        "#,
        "[false, [3]]",
    );
}

#[test]
fn list_iterator_snapshots_structural_mutation() {
    assert_eval(
        r#"
        let list = require("std/list")
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
        let list = require("std/list")
        let iter = require("std/iter")
        iter.to_list(iter.map(list.iter([1]), fn(value) do
            raise { error = "callback_failed", value = value }
        end))
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
            "let iter = require(\"std/iter\") iter.to_list(iter.from(fn() do {expression} end))"
        );
        assert!(matches!(
            Engine::with_stdlib().eval(&source),
            Err(SimiError::Runtime(_))
        ));
    }
}

#[test]
fn removed_collection_hofs_and_map_views_are_not_exports() {
    let source = r#"
        let list = require("std/list")
        let map = require("std/map")
        [type(list.map), type(list.filter), type(list.fold), type(map.keys), type(map.values), type(map.entries)]
    "#;
    assert_eval(
        source,
        "[\"nil\", \"nil\", \"nil\", \"nil\", \"nil\", \"nil\"]",
    );
}
