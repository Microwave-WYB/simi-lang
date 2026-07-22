use simiscript::{SimiError, Value, eval};

fn value(source: &str) -> Value {
    eval(source)
        .expect("source should have no hard diagnostic")
        .expect("source should not leave an uncaught raise")
}

#[test]
fn type_comparisons_cover_every_stable_label_and_unify_functions() {
    let result = value(
        r#"
        fn user_function() do nil end
        let native_function = type
        [
            type(nil) == "nil",
            type(true) == "boolean",
            type(1) == "integer",
            type(1.0) == "float",
            type("text") == "string",
            type([]) == "list",
            type({}) == "map",
            type(user_function) == "function",
            type(native_function) == "function",
            type(1) == "float",
            type(native_function) == "map",
        ]
        "#,
    );

    assert_eq!(
        result.render(),
        "[true, true, true, true, true, true, true, true, true, false, false]"
    );
}

#[test]
fn type_call_evaluates_its_argument_once() {
    let result = value(
        r#"
        let evaluations = 0
        fn once() do
            evaluations = evaluations + 1
            1
        end
        [type(once()) == "integer", evaluations]
        "#,
    );

    assert_eq!(result.render(), "[true, 1]");
}

#[test]
fn type_comparisons_use_ordinary_equality_and_boolean_composition() {
    let result = value(
        r#"[
            type(1 + 2) == "integer" == true,
            type(1 < 2) == "boolean",
            false or type(1) == "integer" and true,
            false and type(missing) == "nil",
            true or type(missing) == "nil",
        ]"#,
    );
    assert_eq!(result.render(), "[true, true, true, false, true]");
}

#[test]
fn type_is_shadowable_and_labels_are_ordinary_values() {
    let result = value(
        r#"
        let label = "integer"
        let builtin_result = type(1) == label
        let unknown_label = type(1) == "number"
        let type = fn(_) do "shadowed" end
        [builtin_result, unknown_label, type(1) == "shadowed", type(1) == label]
        "#,
    );

    assert_eq!(result.render(), "[true, false, true, false]");
}

#[test]
fn is_is_an_ordinary_identifier_and_legacy_infix_fails_in_an_expression_context() {
    assert_eq!(value("let is = 42 is").render(), "42");

    let error = match eval("[1 is \"integer\"]") {
        Err(error) => error,
        Ok(_) => panic!("legacy infix syntax should fail in a list expression"),
    };
    assert!(matches!(error, SimiError::Parse(_)));
    assert_eq!(
        error.to_string(),
        "expected `]` after list elements, found `identifier`"
    );
}
