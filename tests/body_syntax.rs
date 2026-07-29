use simi::eval;

fn assert_eval(source: &str, expected: &str) {
    let value = eval(source)
        .expect("source parses")
        .expect("source evaluates");
    assert_eq!(value.render(), expected);
}

#[test]
fn direct_function_and_case_bodies_are_whitespace_independent() {
    assert_eval(
        r#"
            fn add(left, right)
                left + right
            let label = case 2 of
            1 =>
                "one"
            value =>
                add(value, 1)
            end
            label
        "#,
        "3",
    );
    assert_eval("case 1 of value => value + 1 _ => 0 end", "2");
}

#[test]
fn protected_do_and_direct_catch_body_preserve_raise_semantics() {
    assert_eval(
        "fn recover()\n    do raise \"x\" catch _ => 1 end\nrecover()",
        "1",
    );
    assert_eval("case 1 of _ => do raise \"x\" catch _ => 2 end end", "2");
    assert_eval("case 1 of 1 => do let value = 2 value end _ => 0 end", "2");
    assert_eval(
        r#"
            do
                raise "missing"
            catch
                "missing" =>
                    "recovered"
            end
        "#,
        "\"recovered\"",
    );
}

#[test]
fn bang_never_direct_bodies_are_runtime_erased_for_varied_expressions() {
    assert_eval(
        r#"
            fn identity(value: integer) -> integer ! never value
            fn text() -> string ! never "ok"
            fn values() -> [integer] ! never [1, 2]
            fn nothing() -> nil ! never nil
            fn grouped() -> integer ! never (40 + 2)
            [identity(7), text(), values(), nothing(), grouped()]
        "#,
        "[7, \"ok\", [1, 2], nil, 42]",
    );
}

#[test]
fn direct_body_nil_propagation_stops_at_its_own_boundary() {
    assert_eval(
        r#"
            fn optional(value)
                value?
            [optional(nil), optional(1)]
        "#,
        "[nil, 1]",
    );
}

#[test]
fn legacy_try_and_catch_arm_spelling_report_migration_diagnostics() {
    let Err(try_error) = eval("try raise 1 catch _ do 0 end") else {
        panic!("legacy try succeeds");
    };
    assert!(try_error.to_string().contains("`try` was removed"));
    let Err(catch_error) = eval("do raise 1 catch _ do 0 end") else {
        panic!("legacy catch succeeds");
    };
    assert!(
        catch_error
            .to_string()
            .contains("`=>` after arm pattern and guard")
    );
}
