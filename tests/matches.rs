use simiscript::{SimiError, eval};

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
            let discarded = match [1, 2] with
                case [_value, _value] -> "discarded"
            end
            let discarded_rest = match [1, 2, 3] with
                case [head, .._tail] -> [head, _tail]
            end
            let result = match 2 with
                case nil -> "nil"
                case true -> "true"
                case false -> "false"
                case 1 -> "one"
                case "two" -> "string"
                case n when n < 2 -> "small"
                case _ignored when false -> "unreachable"
                case n -> [n, _ignored]
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
            let exact = match [1, 2, 3] with
                case [a, b] -> "wrong"
                case [a, b, c] -> [a, b, c]
            end
            let nested = match [1, [2, 3, 4], 5, 6] with
                case [head, [middle, ..inner], ..outer] -> [head, middle, inner, outer]
            end
            let empty = match [] with
                case [] -> true
                case _ -> false
            end
            let literals = match [nil, true, false, 42, "ok"] with
                case [nil, true, false, 42, "ok"] -> "all"
                case _ -> "wrong"
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
            let list = require("list")
            let shared = []
            let original = [0, shared, 2, 3]
            let tail = match original with
                case [_, ..rest] -> rest
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
        let rest = match source with
            case [_, ..tail] -> tail
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
            match values with
                case [] -> 0
                case [_, ..rest] -> 1 + count(rest)
            end
        end
        count([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
        "#,
        "16",
    );
}

#[test]
fn map_patterns_are_structural_and_map_rest_preserves_order_and_aliases() {
    assert_eval(
        r#"
            let list = require("list")
            let shared = []
            let source = {take=1, first=shared, [true]=shared, last=3}
            let captured = match source with
                case {take=1, last=n, ..rest} -> [n, rest]
            end
            list.append(shared, 7)
            let permits_extras = match {x=1, extra=2} with
                case {x=1} -> true
                case _ -> false
            end
            let rest_only = match {a=1, b=2} with
                case {..all} -> all
            end
            [captured, permits_extras, rest_only]
        "#,
        "[[3, {first=[7], [true]=[7]}], true, {a=1, b=2}]",
    );
}

#[test]
fn nil_map_fields_match_absence_while_other_patterns_require_presence() {
    assert_eval(
        r#"
        let absent_nil = match {} with
            case {missing=nil} -> true
            case _ -> false
        end
        let absent_binding = match {} with
            case {missing=value} -> false
            case _ -> true
        end
        let omitted_literal = match {missing=nil} with
            case {missing=nil, ..rest} -> rest
        end
        [absent_nil, absent_binding, omitted_literal]
        "#,
        "[true, true, {}]",
    );
}

#[test]
fn guards_run_in_order_only_after_a_match_and_share_the_selected_case_scope() {
    assert_eval(
        r#"
            let list = require("list")
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
            let selected = match produce(events) with
                case [value, extra] when record(events, "failed-pattern", true) -> 0
                case [n] when record(events, "false", false) ->
                    list.append(events, "false-body")
                    1
                case [n] when record(events, "true", n == 2) ->
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
            let selected = match "yes" with
                case "yes" -> 40
                case _ -> 0
            end
            let indexed = match true with
                case true -> [10, selected + 2]
                case false -> []
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
            match 2 with
                case 1 ->
                    if true then "wrong" else "also wrong" end
                case 2 ->
                    let inner = match [3] with
                        case [value] -> value
                    end
                    if inner == 3 then "right" else "wrong" end
                case _ -> "fallback"
            end
        "#,
        "\"right\"",
    );
}

#[test]
fn no_selected_case_reports_the_complete_match_span() {
    let source = "let prefix = \"é\"\nmatch 1 with\n    case value when false -> 0\nend";
    let start = source.find("match").expect("source contains match");
    assert_runtime_error(source, (start, source.len()), "no match case matched");
}

#[test]
fn a_non_boolean_guard_reports_the_guard_span() {
    let source = "match 1 with case value when 123 -> value end";
    let start = source.find("123").expect("source contains guard");
    assert_runtime_error(
        source,
        (start, start + 3),
        "match guard must be boolean, got integer",
    );
}

#[test]
fn duplicate_bindings_are_rejected_at_the_second_identifier() {
    let nested = "match [] with case [x, {field=x}] -> nil end";
    let second_x = nested.rfind('x').expect("source contains duplicate x");
    assert_parse_error(
        nested,
        (second_x, second_x + 1),
        "duplicate binding `x` in pattern",
    );

    let rest = "match [] with case [item, ..item] -> nil end";
    let second_item = rest.rfind("item").expect("source contains duplicate item");
    assert_parse_error(
        rest,
        (second_item, second_item + "item".len()),
        "duplicate binding `item` in pattern",
    );
}

#[test]
fn duplicate_map_pattern_fields_are_rejected_at_the_second_key() {
    let source = "match {} with case {value=1, value=2} -> nil end";
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
            let list = require("list")
            let visited = []
            let result = loop state = 0 do
                match state with
                    case 0 ->
                        list.append(visited, state)
                        continue 1
                    case n when n < 3 ->
                        list.append(visited, n)
                        n + 1
                    case n -> break [n, visited]
                end
            end
            result
        "#,
        "[3, [0, 1, 2]]",
    );
}
