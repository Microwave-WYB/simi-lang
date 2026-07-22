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

#[test]
fn ordinary_completion_threads_accumulator_and_break_returns_a_value() {
    assert_eval(
        r#"
            loop state = 0 do
                if state < 3 then
                    state + 1
                else
                    break state
                end
            end
        "#,
        "3",
    );
}

#[test]
fn initializer_runs_once_across_multiple_iterations() {
    assert_eval(
        r#"
            let calls = []
            fn initialize() do
                list.append(calls, "called")
                0
            end

            let result = loop state = initialize() do
                if state < 3 then
                    state + 1
                else
                    break state
                end
            end
            [list.length(calls), result]
        "#,
        "[1, 3]",
    );
}

#[test]
fn valued_continue_supplies_next_state_and_skips_remaining_items() {
    assert_eval(
        r#"
            let visited = []
            let result = loop state = 0 do
                if state < 3 then
                    continue state + 1
                end
                list.append(visited, state)
                break state
            end
            [visited, result]
        "#,
        "[[3], 3]",
    );
}

#[test]
fn bare_continue_supplies_nil_and_skips_remaining_items() {
    assert_eval(
        r#"
            let visited = []
            let result = loop state = 0 do
                if state == 0 then
                    continue 1
                end
                if state == 1 then
                    continue
                end
                list.append(visited, state)
                break state
            end
            [visited, result]
        "#,
        "[[nil], nil]",
    );
}

#[test]
fn loop_state_shadows_without_overwriting_an_outer_binding() {
    assert_eval(
        r#"
            let state = 99
            let result = loop state = 0 do
                if state < 2 then
                    state + 1
                else
                    break state
                end
            end
            [state, result]
        "#,
        "[99, 2]",
    );
}

#[test]
fn stateless_loop_binds_nil_to_underscore() {
    assert_eval("loop do break _ end", "nil");
}

#[test]
fn stateless_bare_continue_restarts_with_nil_state() {
    assert_eval(
        r#"
            let visits = []
            let result = loop do
                list.append(visits, _)
                if list.length(visits) < 2 then
                    continue
                else
                    break _
                end
            end
            [result, visits]
        "#,
        "[nil, [nil, nil]]",
    );
}

#[test]
fn closures_capture_a_fresh_state_binding_for_each_iteration() {
    assert_eval(
        r#"
            let captures = []
            loop state = 0 do
                fn capture() do state end
                list.append(captures, capture)
                if state < 2 then
                    state + 1
                else
                    break [list.get(captures, 0)(), list.get(captures, 1)(), list.get(captures, 2)()]
                end
            end
        "#,
        "[0, 1, 2]",
    );
}

#[test]
fn nested_loops_catch_only_the_nearest_continue_and_break() {
    assert_eval(
        r#"
            loop outer = 0 do
                if outer < 3 then
                    let inner_result = loop inner = 0 do
                        if inner == 0 then
                            inner + 1
                        elseif inner == 1 then
                            continue inner + 1
                        else
                            break inner
                        end
                    end
                    outer + inner_result
                else
                    break outer
                end
            end
        "#,
        "4",
    );
}

#[test]
fn rejects_loop_control_outside_a_loop() {
    assert_parse_error("break 1", (0, 5), "`break` outside of a loop");
    assert_parse_error("continue 1", (0, 8), "`continue` outside of a loop");
}

#[test]
fn rejects_break_without_a_value() {
    let source = "loop state = 0 do break end";
    let end = source.rfind("end").expect("source contains end");
    assert_parse_error(
        source,
        (end, end + "end".len()),
        "expected expression, found `end`",
    );
}

#[test]
fn reports_malformed_loop_headers_and_missing_delimiters() {
    let missing_state = "loop = 0 do break 0 end";
    let equal = missing_state.find('=').expect("source contains equals");
    assert_parse_error(
        missing_state,
        (equal, equal + 1),
        "expected loop state name, found `=`",
    );

    let missing_equal = "loop state 0 do break 0 end";
    let initial = missing_equal
        .find('0')
        .expect("source contains initial value");
    assert_parse_error(
        missing_equal,
        (initial, initial + 1),
        "expected `=` after loop state name, found `integer`",
    );

    let missing_do = "loop state = 0 end";
    let end = missing_do.rfind("end").expect("source contains end");
    assert_parse_error(
        missing_do,
        (end, end + "end".len()),
        "expected `do` before loop body, found `end`",
    );

    let missing_end = "loop state = 0 do break state";
    assert_parse_error(
        missing_end,
        (missing_end.len(), missing_end.len()),
        "expected `end` after loop body, found `end of file`",
    );
}
