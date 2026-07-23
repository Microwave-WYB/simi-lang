use simi::{SimiError, Value, eval};

fn value(source: &str) -> Value {
    eval(source)
        .expect("source should have no hard diagnostic")
        .expect("source should not leave an uncaught raise")
}

fn outer_error(source: &str) -> SimiError {
    match eval(source) {
        Err(error) => error,
        Ok(Ok(value)) => panic!("source should fail, got {}", value.render()),
        Ok(Err(raised)) => panic!("source should fail, got {raised}"),
    }
}

#[test]
fn nil_aware_pipelines_are_lazy_left_associative_and_stage_local() {
    let result = value(
        r#"
        fn add(value, amount) do value + amount end
        fn classify(value) do type(value) end
        [
            1 + 1 ?> add(2) ?> add(3),
            nil ?> missing_callee(missing_argument),
            nil ?> missing() |> classify(),
        ]
        "#,
    );
    assert_eq!(result.render(), "[7, nil, \"nil\"]");
}

#[test]
fn nil_aware_pipelines_evaluate_input_and_active_stage_parts_once() {
    let result = value(
        r#"
        let calls = 0
        fn next() do
            calls = calls + 1
            calls
        end
        fn none() do
            calls = calls + 1
            nil
        end
        fn add(value, amount) do value + amount end
        let piped = next() ?> add(next())
        let skipped = none() ?> missing(next())
        [piped, skipped, calls]
        "#,
    );
    assert_eq!(result.render(), "[3, nil, 3]");
}

#[test]
fn nil_aware_tap_preserves_nonnil_input_and_skips_nil_stages() {
    let result = value(
        r#"
        let list = require("std/list")
        let events = []
        fn record(value) do list.append(events, value) end
        let kept = 4 ?> tap record()
        let skipped = nil ?> tap missing(events)
        [kept, skipped, events]
        "#,
    );
    assert_eq!(result.render(), "[4, nil, [4]]");
}

#[test]
fn standalone_blocks_are_scoped_expression_values_and_compose_postfix() {
    let result = value(
        r#"
        let outer = "outer"
        let empty = do end
        let called = do
            let outer = "inner"
            fn(value) do [outer, value] end
        end(3)
        let indexed = do [10, 20] end[1]
        let piped = do 2 end ?> type()
        [empty, called, indexed, piped, outer]
        "#,
    );
    assert_eq!(
        result.render(),
        "[nil, [\"inner\", 3], 20, \"integer\", \"outer\"]"
    );
}

#[test]
fn nil_propagation_passes_values_and_aborts_only_the_nearest_block() {
    let result = value(
        r#"
        let list = require("std/list")
        let events = []
        let passed = do 4? + 1 end
        let nested = do
            let inner = do
                list.append(events, "inner-before")
                nil?
                list.append(events, "inner-after")
            end
            list.append(events, "outer-after")
            [inner, events]
        end
        [passed, nested]
        "#,
    );
    assert_eq!(
        result.render(),
        "[5, [nil, [\"inner-before\", \"outer-after\"]]]"
    );
}

#[test]
fn nil_propagation_evaluates_each_kind_of_current_block_as_nil() {
    let result = value(
        r#"
        fn from_function() do
            nil?
            "unreachable"
        end
        let from_if = do
            let selected = if true then nil? else 1 end
            [selected, "outer continued"]
        end
        let from_else = do
            let selected = if false then 1 else nil? end
            [selected, "outer continued"]
        end
        let from_case = do
            let selected = case 1
            of 1 do nil?
            end
            [selected, "outer continued"]
        end
        let from_protected = do
            let selected = try
                nil?
            catch _ do "must not catch"
            end
            [selected, "outer continued"]
        end
        let from_catch = do
            let selected = try
                raise "failure"
            catch "failure" do nil?
            catch _ do "must not run"
            end
            [selected, "outer continued"]
        end
        [from_function(), from_if, from_else, from_case, from_protected, from_catch]
        "#,
    );
    assert_eq!(
        result.render(),
        "[nil, [nil, \"outer continued\"], [nil, \"outer continued\"], [nil, \"outer continued\"], [nil, \"outer continued\"], [nil, \"outer continued\"]]"
    );
}

#[test]
fn nil_propagation_from_a_loop_body_is_a_nil_state_transition() {
    let result = value(
        r#"
        let direct = loop state = 0 do
            if state == nil then break "continued with nil" end
            nil?
            "unreachable"
        end
        let before_break = loop state = 0 do
            if state == nil then break "break was skipped" end
            break nil?
        end
        let plain_break = loop do break nil end
        [direct, before_break, plain_break]
        "#,
    );
    assert_eq!(
        result.render(),
        "[\"continued with nil\", \"break was skipped\", nil]"
    );
}

#[test]
fn nil_propagation_requires_an_enclosing_block() {
    let error = outer_error("nil?");
    assert!(matches!(error, SimiError::Parse(_)));
    assert!(error.to_string().contains("outside of a block"));

    assert_eq!(value("fn named() do nil? end named()").render(), "nil");
    assert_eq!(
        value("let anonymous = fn() do nil? end anonymous()").render(),
        "nil"
    );
}

#[test]
fn multi_item_try_returns_last_value_and_uses_a_fresh_scope() {
    let result = value(
        r#"
        let local = "outer"
        let success = try
            let local = "protected"
            local
            42
        catch _ do "wrong"
        end
        let raised = try
            let hidden = "protected"
            raise hidden
            "unreachable"
        catch value do value
        end
        let hidden = "outside"
        [success, raised, local, hidden]
        "#,
    );
    assert_eq!(
        result.render(),
        "[42, \"protected\", \"outer\", \"outside\"]"
    );
}

#[test]
fn try_does_not_catch_hard_diagnostics_or_nil_propagation() {
    let propagated = value(
        r#"
        do
            let selected = try
                nil?
            catch _ do "caught"
            end
            [selected, "enclosing block continued"]
        end
        "#,
    );
    assert_eq!(propagated.render(), "[nil, \"enclosing block continued\"]");

    assert!(matches!(
        eval("try let local = 1 missing catch _ do nil end"),
        Err(SimiError::Runtime(_))
    ));
}

#[test]
fn raised_nil_and_active_stage_errors_are_not_converted_to_absence() {
    let raised = match eval("raise nil ?> missing()").expect("raised nil is not a hard diagnostic")
    {
        Err(raised) => raised,
        Ok(value) => panic!("raised nil must not be skipped, got {}", value.render()),
    };
    assert_eq!(raised.value.render(), "nil");

    assert!(matches!(eval("1 ?> missing()"), Err(SimiError::Runtime(_))));
    assert_eq!(value("nil ?> missing()").render(), "nil");
}

#[test]
fn repeated_case_and_catch_markers_own_bodies_until_the_next_marker() {
    let result = value(
        r#"
        let selected = case 2 of 1 do "one" of n when n == 2 do
            let local = n + 1
            local
        of _ do "other" end
        let handled = try
            let first = "protected"
            raise 2
        catch 1 do "one"
        catch n when n == 2 do [n, selected]
        catch _ do "other"
        end
        [selected, handled]
        "#,
    );
    assert_eq!(result.render(), "[3, [2, 3]]");
}

#[test]
fn legacy_per_branch_ends_and_unmarked_siblings_are_rejected() {
    for source in [
        "case 1 of 1 do nil end end",
        "case 1 of 1 do nil 2 do nil end",
        "try raise 1 catch 1 do nil end end",
        "try raise 1 catch 1 do nil _ do nil end",
    ] {
        assert!(
            matches!(outer_error(source), SimiError::Parse(_)),
            "{source}"
        );
    }
}

#[test]
fn try_requires_protected_items_and_precise_delimiters() {
    for (source, message) in [
        (
            "try catch _ do nil end",
            "expected at least one protected block item",
        ),
        ("try 1 end", "expected `catch` after protected block"),
        (
            "do 1 catch _ do nil end",
            "expected `end` after standalone block",
        ),
    ] {
        let error = outer_error(source);
        assert!(matches!(error, SimiError::Parse(_)));
        assert!(error.to_string().contains(message), "{error}");
    }
}
