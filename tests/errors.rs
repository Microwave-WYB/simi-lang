use gc::Gc;

use simiscript::{Raised, SimiError, Value, eval};

fn assert_eval(source: &str, expected: &str) {
    match eval(source) {
        Ok(Ok(value)) => assert_eq!(value.render(), expected),
        Ok(Err(raised)) => panic!("program should succeed, got {raised}"),
        Err(error) => panic!("program should evaluate, got {error}"),
    }
}

fn assert_raised(source: &str) -> Raised {
    match eval(source) {
        Ok(Err(raised)) => raised,
        Ok(Ok(value)) => panic!("program should raise, got {}", value.render()),
        Err(error) => panic!("program should raise a value, got {error}"),
    }
}

#[test]
fn every_value_category_can_be_raised_and_caught() {
    assert_eval(
        r#"
            let list = require("std/list")
            fn identity(value) do value end
            [
                try raise nil catch case nil -> "nil" end,
                try raise true catch case true -> "true" end,
                try raise false catch case false -> "false" end,
                try raise 42 catch case 42 -> "integer" end,
                try raise 4.5 catch case 4.5 -> "float" end,
                try raise "boom" catch case "boom" -> "string" end,
                try raise [1, 2, 3] catch case [head, ..tail] -> [head, tail] end,
                try raise {kind="missing", payload=[1, 2]} catch
                    case {kind="missing", payload=payload} -> payload
                end,
                try raise identity catch case callable -> callable("function") end,
                try raise list.length catch case callable -> callable([1, 2]) end
            ]
        "#,
        "[\"nil\", \"true\", \"false\", \"integer\", \"float\", \"string\", [1, [2, 3]], [1, 2], \"function\", 2]",
    );
}

#[test]
fn successful_try_is_a_transparent_passthrough() {
    assert_eval(
        r#"
            let list = require("std/list")
            let shared = []
            let result = try shared catch
                case _ -> missing_handler_must_not_run
            end
            list.append(shared, "after")
            result
        "#,
        "[\"after\"]",
    );
}

#[test]
fn catch_bindings_and_handler_locals_are_case_scoped() {
    assert_eval(
        r#"
            let error = "outer error"
            let local = "outer local"
            let handled = try raise "inner" catch
                case error ->
                    let local = "inner local"
                    [error, local]
            end
            [handled, error, local]
        "#,
        "[[\"inner\", \"inner local\"], \"outer error\", \"outer local\"]",
    );
}

#[test]
fn catch_cases_use_structural_patterns_and_ordered_guards() {
    assert_eval(
        r#"
            let list = require("std/list")
            let events = []
            try raise {kind="missing", payload=[1, 2]} catch
                case {kind="other"} when missing_guard_must_not_run -> nil
                case {kind="missing", payload=[head, ..tail]} when head == 0 -> "wrong"
                case {kind="missing", payload=[head, ..tail]} when head == 1 ->
                    list.append(events, "selected")
                    [tail, events]
                case _ -> "fallback"
            end
        "#,
        "[[2], [\"selected\"]]",
    );
}

#[test]
fn unmatched_catch_propagates_the_original_raise_unchanged() {
    let source = "try raise [1, 2] catch case [3, ..rest] -> rest end";
    let raised = assert_raised(source);
    let origin_start = source.find("raise").expect("source contains raise");
    let origin_end = source.find(" catch").expect("source contains catch");

    assert_eq!(raised.value.render(), "[1, 2]");
    assert_eq!(
        (raised.origin.start, raised.origin.end),
        (origin_start, origin_end)
    );
    assert!(raised.frames.is_empty());
    assert!(raised.cause.is_none());
}

#[test]
fn nested_tries_catch_propagated_and_handler_raised_values() {
    assert_eval(
        r#"
            let propagated = try
                try raise "old" catch
                    case "different" -> "wrong"
                end
            catch
                case "old" -> "outer caught unmatched"
            end

            let handler_raised = try
                try raise "old" catch
                    case error -> raise "new"
                end
            catch
                case "old" -> "wrong cause"
                case "new" -> "outer caught current"
            end
            [propagated, handler_raised]
        "#,
        "[\"outer caught unmatched\", \"outer caught current\"]",
    );
}

#[test]
fn raises_cross_function_boundaries_with_innermost_first_frames_and_exact_spans() {
    let source = concat!(
        "fn leaf() do\n",
        "    raise \"boom\"\n",
        "end\n",
        "let alias = leaf\n",
        "fn middle() do\n",
        "    alias()\n",
        "end\n",
        "middle()",
    );
    let raised = assert_raised(source);
    let origin_start = source.find("raise").expect("source contains raise");
    let origin_end = origin_start + "raise \"boom\"".len();
    let alias_call_start = source.find("alias()").expect("source contains alias call");
    let middle_call_start = source
        .rfind("middle()")
        .expect("source contains middle call");

    assert_eq!(raised.value.render(), "\"boom\"");
    assert_eq!(
        (raised.origin.start, raised.origin.end),
        (origin_start, origin_end)
    );
    assert_eq!(raised.frames.len(), 2);
    assert_eq!(raised.frames[0].function, "leaf");
    assert_eq!(
        (
            raised.frames[0].call_span.start,
            raised.frames[0].call_span.end
        ),
        (alias_call_start, alias_call_start + "alias()".len()),
    );
    assert_eq!(raised.frames[1].function, "middle");
    assert_eq!(
        (
            raised.frames[1].call_span.start,
            raised.frames[1].call_span.end
        ),
        (middle_call_start, middle_call_start + "middle()".len()),
    );
}

#[test]
fn handler_mutation_is_observable_and_reraise_preserves_the_caught_context() {
    let source = concat!(
        "let list = require(\"std/list\")\n",
        "try raise [\"old\"] catch\n",
        "    case error ->\n",
        "        list.append(error, \"mutated\")\n",
        "        raise error\n",
        "end",
    );
    let raised = assert_raised(source);
    let cause = raised
        .cause
        .as_deref()
        .expect("handler raise retains its cause");
    let first_raise_start = source.find("raise").expect("source contains first raise");
    let first_raise_end = source.find(" catch").expect("source contains catch");
    let reraised_start = source
        .rfind("raise error")
        .expect("source contains re-raise");

    assert_eq!(raised.value.render(), "[\"old\", \"mutated\"]");
    assert_eq!(
        (raised.origin.start, raised.origin.end),
        (reraised_start, reraised_start + "raise error".len()),
    );
    assert_eq!(
        (cause.origin.start, cause.origin.end),
        (first_raise_start, first_raise_end),
    );
    assert!(raised.frames.is_empty());
    assert!(cause.frames.is_empty());
    assert!(cause.cause.is_none());

    let (Value::List(current), Value::List(original)) = (&raised.value, &cause.value) else {
        panic!("both raised contexts should retain the raised list value");
    };
    assert!(Gc::ptr_eq(current, original));
}

#[test]
fn a_catch_does_not_catch_a_raise_from_its_own_handler() {
    let raised = assert_raised(
        r#"
            try raise "first" catch
                case error -> raise "second"
                case _ -> "must not run"
            end
        "#,
    );
    let cause = raised
        .cause
        .as_deref()
        .expect("handler raise retains its cause");

    assert_eq!(raised.value.render(), "\"second\"");
    assert_eq!(cause.value.render(), "\"first\"");
    assert!(cause.cause.is_none());
}

#[test]
fn raises_propagate_through_loop_initialization_and_iterations() {
    assert_eval(
        r#"
            [
                try loop state = raise "initial" do break state end catch
                    case value -> value
                end,
                try loop state = 0 do raise "iteration" end catch
                    case value -> value
                end
            ]
        "#,
        "[\"initial\", \"iteration\"]",
    );
}

#[test]
fn hard_runtime_errors_bypass_language_catches() {
    for (source, expected_message) in [
        (
            "try missing_name catch case _ -> \"must not catch\" end",
            "undefined name `missing_name`",
        ),
        (
            "try raise missing_name catch case _ -> \"must not catch\" end",
            "undefined name `missing_name`",
        ),
        (
            "try raise 1 catch case _ when 2 -> \"must not catch\" end",
            "catch guard must be boolean, got integer",
        ),
    ] {
        match eval(source) {
            Err(SimiError::Runtime(error)) => assert_eq!(error.message, expected_message),
            Err(error) => panic!("expected hard runtime error, got {error}"),
            Ok(Ok(value)) => panic!("expected hard runtime error, got {}", value.render()),
            Ok(Err(raised)) => panic!("hard runtime error became catchable: {raised}"),
        }
    }
}

#[test]
fn lex_and_parse_failures_remain_outer_diagnostics() {
    assert!(matches!(eval("@"), Err(SimiError::Lex(_))));
    assert!(matches!(eval("try 1 end"), Err(SimiError::Parse(_))));
}
