use simiscript::span::Span;
use simiscript::{SimiError, Value, eval};

fn value(source: &str) -> Value {
    eval(source)
        .expect("source should have no hard diagnostic")
        .expect("source should not leave an uncaught raise")
}

#[test]
fn is_recognizes_every_runtime_category_and_unifies_functions() {
    let result = value(
        r#"
        fn user_function() do nil end
        let native_function = type
        [
            nil is "nil",
            true is "boolean",
            1 is "integer",
            1.0 is "float",
            "text" is "string",
            [] is "list",
            {} is "map",
            user_function is "function",
            native_function is "function",
            1 is "float",
            native_function is "map",
            type(user_function),
            type(native_function),
        ]
        "#,
    );

    assert_eq!(
        result.render(),
        "[true, true, true, true, true, true, true, true, true, false, false, \"function\", \"function\"]"
    );
}

#[test]
fn is_uses_a_dedicated_path_and_composes_with_short_circuiting() {
    let result = value(
        r#"
        let evaluations = 0
        fn once() do
            evaluations = evaluations + 1
            1
        end
        let type = "shadowed"
        [
            once() is "integer",
            evaluations,
            type,
            false and missing is "nil",
            true or missing is "nil",
        ]
        "#,
    );

    assert_eq!(result.render(), "[true, 1, \"shadowed\", false, true]");
}

#[test]
fn is_has_comparison_precedence() {
    let result = value(
        r#"[
            1 + 2 is "integer" == true,
            1 < 2 is "boolean",
            false or 1 is "integer" and true,
        ]"#,
    );
    assert_eq!(result.render(), "[true, true, true]");
}

#[test]
fn is_rejects_dynamic_unknown_and_missing_labels_with_precise_spans() {
    for (source, expected_message, expected_span) in [
        (
            "value is label",
            "expected runtime type string literal after `is`, found `identifier`",
            Span::new(9, 14),
        ),
        (
            "nil is type(nil)",
            "expected runtime type string literal after `is`, found `identifier`",
            Span::new(7, 11),
        ),
        (
            "\"é\" is \"number\"",
            "unknown runtime type label `number`",
            Span::new(8, 16),
        ),
        (
            "nil is",
            "expected runtime type string literal after `is`, found `end of file`",
            Span::new(6, 6),
        ),
    ] {
        let error = match eval(source) {
            Err(error) => error,
            Ok(_) => panic!("invalid is syntax should fail to parse"),
        };
        assert!(matches!(error, SimiError::Parse(_)));
        assert_eq!(error.to_string(), expected_message);
        assert_eq!(error.span(), expected_span);
    }
}
