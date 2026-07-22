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
                try raise nil catch nil do "nil" end end,
                try raise true catch true do "true" end end,
                try raise false catch false do "false" end end,
                try raise 42 catch 42 do "integer" end end,
                try raise 4.5 catch 4.5 do "float" end end,
                try raise "boom" catch "boom" do "string" end end,
                try raise [1, 2, 3] catch [head, ..tail] do [head, tail] end end,
                try raise {kind="missing", payload=[1, 2]} catch
                    {kind="missing", payload=payload} do payload end
                end,
                try raise identity catch callable do callable("function") end end,
                try raise list.length catch callable do callable([1, 2]) end end
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
                _ do missing_handler_must_not_run end
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
                error do
                    let local = "inner local"
                    [error, local]
                end
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
                {kind="other"} when missing_guard_must_not_run do nil end
                {kind="missing", payload=[head, ..tail]} when head == 0 do "wrong" end
                {kind="missing", payload=[head, ..tail]} when head == 1 do
                    list.append(events, "selected")
                    [tail, events]
                end
                _ do "fallback" end
            end
        "#,
        "[[2], [\"selected\"]]",
    );
}

#[test]
fn unmatched_catch_propagates_the_original_raise_unchanged() {
    let source = "try raise [1, 2] catch [3, ..rest] do rest end end";
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
                    "different" do "wrong" end
                end
            catch
                "old" do "outer caught unmatched" end
            end

            let handler_raised = try
                try raise "old" catch
                    error do raise "new" end
                end
            catch
                "old" do "wrong cause" end
                "new" do "outer caught current" end
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
        "    error do\n",
        "        list.append(error, \"mutated\")\n",
        "        raise error\n",
        "    end\n",
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
                error do raise "second" end
                _ do "must not run" end
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
                    value do value end
                end,
                try loop state = 0 do raise "iteration" end catch
                    value do value end
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
            "try missing_name catch _ do \"must not catch\" end end",
            "undefined name `missing_name`",
        ),
        (
            "try raise missing_name catch _ do \"must not catch\" end end",
            "undefined name `missing_name`",
        ),
        (
            "try raise 1 catch _ when 2 do \"must not catch\" end end",
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
