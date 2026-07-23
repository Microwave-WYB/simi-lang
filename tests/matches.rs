use simi::{SimiError, eval};

fn assert_eval(source: &str, expected: &str) {
    match eval(source) {
        Ok(Ok(value)) => assert_eq!(value.render(), expected),
        Ok(Err(raised)) => panic!("program should evaluate, got {raised}"),
        Err(error) => panic!("program should evaluate, got {error}"),
    }
}

fn assert_parse_error(source: &str, expected_span: (usize, usize), expected_message: &str) {
    match eval(source) {
        Err(SimiError::Parse(error)) => {
            assert_eq!((error.span.start, error.span.end), expected_span);
            assert_eq!(error.message, expected_message);
        }
        Err(error) => panic!("expected parse error, got {error}"),
        Ok(Ok(value)) => panic!("expected parse error, got {}", value.render()),
        Ok(Err(raised)) => panic!("expected parse error, got {raised}"),
    }
}

fn assert_runtime_error(source: &str, expected_span: (usize, usize), expected_message: &str) {
    match eval(source) {
        Err(SimiError::Runtime(error)) => {
            assert_eq!((error.span.start, error.span.end), expected_span);
            assert_eq!(error.message, expected_message);
        }
        Err(error) => panic!("expected runtime error, got {error}"),
        Ok(Ok(value)) => panic!("expected runtime error, got {}", value.render()),
        Ok(Err(raised)) => panic!("expected runtime error, got {raised}"),
    }
}

#[test]
fn wildcard_discard_bindings_and_primitive_literals_match() {
    assert_eval(
        r#"
            let _ignored = 7
            let _tail = "outer"
            let discarded = case [1, 2]
                of [_value, _value] do "discarded"
            end
            let discarded_rest = case [1, 2, 3]
                of [head, .._tail] do [head, _tail]
            end
            let result = case 2
                of nil do "nil"
                of true do "true"
                of false do "false"
                of 1 do "one"
                of "two" do "string"
                of n when n < 2 do "small"
                of _ignored when false do "unreachable"
                of n do [n, _ignored]
            end
            [discarded, discarded_rest, result, _ignored]
        "#,
        "[\"discarded\", [1, \"outer\"], [2, 7], 7]",
    );
}

#[test]
fn nested_exact_and_rest_list_patterns_match_structurally() {
    assert_eval(
        r#"
            let exact = case [1, 2, 3]
                of [a, b] do "wrong"
                of [a, b, c] do [a, b, c]
            end
            let nested = case [1, [2, 3, 4], 5, 6]
                of [head, [middle, ..inner], ..outer] do [head, middle, inner, outer]
            end
            let empty = case []
                of [] do true
                of _ do false
            end
            let literals = case [nil, true, false, 42, "ok"]
                of [nil, true, false, 42, "ok"] do "all"
                of _ do "wrong"
            end
            [exact, nested, empty, literals]
        "#,
        "[[1, 2, 3], [1, 2, [3, 4], [5, 6]], true, \"all\"]",
    );
}

#[test]
fn list_rest_is_a_new_container_with_existing_element_aliases() {
    assert_eval(
        r#"
            let list = require("std/list")
            let shared = []
            let original = [0, shared, 2, 3]
            let tail = case original
                of [_, ..rest] do rest
            end
            list.set(tail, 1, 9)
            list.append(shared, 7)
            [original, tail]
        "#,
        "[[0, [7], 2, 3], [[7], 9, 3]]",
    );
}

#[test]
fn list_rest_uses_independent_cow_views_while_preserving_alias_groups() {
    assert_eval(
        r#"
        let nested = [2]
        let source = [1, nested, 3]
        let source_alias = source
        let rest = case source
            of [_, ..tail] do tail
        end
        let rest_alias = rest

        nested[0] = 7
        source[2] = 4
        rest[1] = 9

        [source, source_alias, rest, rest_alias]
        "#,
        "[[1, [7], 4], [1, [7], 4], [[7], 9], [[7], 9]]",
    );
}

#[test]
fn recursive_head_tail_matching_handles_longer_lists() {
    assert_eval(
        r#"
        fn count(values) do
            case values
                of [] do 0
                of [_, ..rest] do 1 + count(rest)
            end
        end
        count([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
        "#,
        "16",
    );
}

#[test]
fn map_patterns_are_closed_by_default_and_map_rest_preserves_order_and_aliases() {
    assert_eval(
        r#"
            let list = require("std/list")
            let shared = []
            let source = {take=1, first=shared, [true]=shared, last=3}
            let captured = case source
                of {take=1, last=n, ..rest} do [n, rest]
            end
            list.append(shared, 7)
            let rejects_extra_field = case {x=1, extra=2}
                of {x=1} do false
                of _ do true
            end
            let rejects_extra_computed_key = case {x=1, [true]=2}
                of {x=1} do false
                of _ do true
            end
            let permits_extras = case {x=1, extra=2}
                of {x=1, ..} do true
                of _ do false
            end
            let rest_only = case {a=1, b=2}
                of {..all} do all
            end
            [captured, rejects_extra_field, rejects_extra_computed_key, permits_extras, rest_only]
        "#,
        "[[3, {first=[7], [true]=[7]}], true, true, true, {a=1, b=2}]",
    );
}

#[test]
fn nil_map_fields_match_absence_while_other_patterns_require_presence() {
    assert_eval(
        r#"
        let absent_nil = case {}
            of {missing=nil} do true
            of _ do false
        end
        let absent_binding = case {}
            of {missing=value} do false
            of _ do true
        end
        let absent_nil_with_extra = case {other=1}
            of {missing=nil} do false
            of {missing=nil, ..} do true
        end
        let omitted_literal = case {missing=nil}
            of {missing=nil, ..rest} do rest
        end
        [absent_nil, absent_binding, absent_nil_with_extra, omitted_literal]
        "#,
        "[true, true, true, {}]",
    );
}

#[test]
fn guards_run_in_order_only_after_a_match_and_share_the_selected_case_scope() {
    assert_eval(
        r#"
            let list = require("std/list")
            fn record(events, label, outcome) do
                list.append(events, label)
                outcome
            end

            fn produce(events) do
                list.append(events, "scrutinee")
                [2]
            end

            let events = []
            let n = 99
            let local = 50
            let selected = case produce(events)
                of [value, extra] when record(events, "failed-pattern", true) do 0
                of [n] when record(events, "false", false) do
                    list.append(events, "false-body")
                    1

                of [n] when record(events, "true", n == 2) do
                    let local = n + 1
                    [n, local]

            end
            [n, local, selected, events]
        "#,
        "[99, 50, [2, 3], [\"scrutinee\", \"false\", \"true\"]]",
    );
}

#[test]
fn match_is_expression_valued_and_supports_postfix_operations() {
    assert_eval(
        r#"
            let selected = case "yes"
                of "yes" do 40
                of _ do 0
            end
            let indexed = case true
                of true do [10, selected + 2]
                of false do []
            end[1]
            indexed
        "#,
        "42",
    );
}

#[test]
fn nested_match_and_if_bodies_do_not_consume_an_outer_case() {
    assert_eval(
        r#"
            case 2
                of 1 do
                    if true then "wrong" else "also wrong" end

                of 2 do
                    let inner = case [3]
                        of [value] do value
                    end
                    if inner == 3 then "right" else "wrong" end

                of _ do "fallback"
            end
        "#,
        "\"right\"",
    );
}

#[test]
fn no_selected_case_reports_the_complete_match_span() {
    let source = "let prefix = \"é\"\ncase 1\nof value when false do 0\nend";
    let start = source.find("case").expect("source contains case");
    assert_runtime_error(source, (start, source.len()), "no case clause matched");
}

#[test]
fn a_non_boolean_guard_reports_the_guard_span() {
    let source = "case 1 of value when 123 do value end";
    let start = source.find("123").expect("source contains guard");
    assert_runtime_error(
        source,
        (start, start + 3),
        "case guard must be boolean, got integer",
    );
}

#[test]
fn duplicate_bindings_are_rejected_at_the_second_identifier() {
    let nested = "case [] of [x, {field=x}] do nil end";
    let second_x = nested.rfind('x').expect("source contains duplicate x");
    assert_parse_error(
        nested,
        (second_x, second_x + 1),
        "duplicate binding `x` in pattern",
    );

    let rest = "case [] of [item, ..item] do nil end";
    let second_item = rest.rfind("item").expect("source contains duplicate item");
    assert_parse_error(
        rest,
        (second_item, second_item + "item".len()),
        "duplicate binding `item` in pattern",
    );
}

#[test]
fn duplicate_map_pattern_fields_are_rejected_at_the_second_key() {
    let source = "case {} of {value=1, value=2} do nil end";
    let second_value = source
        .rfind("value")
        .expect("source contains duplicate field");
    assert_parse_error(
        source,
        (second_value, second_value + "value".len()),
        "duplicate map pattern field `value`",
    );
}

#[test]
fn match_inside_a_functional_loop_propagates_continue_and_break() {
    assert_eval(
        r#"
            let list = require("std/list")
            let visited = []
            let result = loop state = 0 do
                case state
                    of 0 do
                        list.append(visited, state)
                        continue 1

                    of n when n < 3 do
                        list.append(visited, n)
                        n + 1

                    of n do break [n, visited]
                end
            end
            result
        "#,
        "[3, [0, 1, 2]]",
    );
}
