use super::*;

const SPAN: Span = Span::new(3, 9);

fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}

fn result(result: NativeResult) -> Value {
    result.unwrap().unwrap()
}

fn hard_error(result: NativeResult) -> RuntimeError {
    match result {
        Err(error) => error,
        Ok(Ok(value)) => panic!("expected hard error, got {}", value.render()),
        Ok(Err(raised)) => panic!("expected hard error, got {raised}"),
    }
}

#[test]
fn from_string_uses_syntax_to_preserve_numeric_categories() {
    for text in ["0", "+17", "-42", "001"] {
        assert!(
            matches!(
                result(number_from_string(&[string(text)], SPAN)),
                Value::Int(_)
            ),
            "{text} should produce an integer"
        );
    }

    for text in ["0.0", "+1.25", "-2.5", "1e3", "2E-2", "3.0e+4"] {
        assert!(
            matches!(
                result(number_from_string(&[string(text)], SPAN)),
                Value::Float(_)
            ),
            "{text} should produce a float"
        );
    }
}

#[test]
fn from_string_handles_integer_boundaries_without_float_fallback() {
    assert!(matches!(
        result(number_from_string(&[string("9223372036854775807")], SPAN)),
        Value::Int(i64::MAX)
    ));
    assert!(matches!(
        result(number_from_string(&[string("-9223372036854775808")], SPAN)),
        Value::Int(i64::MIN)
    ));
    assert!(matches!(
        result(number_from_string(&[string("9223372036854775808")], SPAN)),
        Value::Nil
    ));
    assert!(matches!(
        result(number_from_string(&[string("-9223372036854775809")], SPAN)),
        Value::Nil
    ));
}

#[test]
fn from_string_rejects_non_finite_and_malformed_forms() {
    for text in [
        "",
        "+",
        "-",
        ".5",
        "1.",
        "1e",
        "1e+",
        "1e-",
        "1.2.3",
        "1x",
        " 1",
        "1 ",
        "1_000",
        "NaN",
        "inf",
        "Infinity",
        "0x10",
        "１２",
        "1.7976931348623159e308",
    ] {
        assert!(
            matches!(
                result(number_from_string(&[string(text)], SPAN)),
                Value::Nil
            ),
            "{text:?} should be rejected"
        );
    }

    assert!(matches!(
        result(number_from_string(
            &[string("1.7976931348623157e308")],
            SPAN
        )),
        Value::Float(value) if value.is_finite()
    ));
}

#[test]
fn to_string_uses_canonical_visible_float_rendering() {
    let cases = [
        (Value::Int(i64::MIN), "-9223372036854775808"),
        (Value::Int(i64::MAX), "9223372036854775807"),
        (Value::Float(1.0), "1.0"),
        (Value::Float(-0.0), "-0.0"),
        (Value::Float(12.5), "12.5"),
    ];

    for (value, expected) in cases {
        assert!(matches!(
            result(number_to_string(&[value], SPAN)),
            Value::String(rendered) if rendered == expected
        ));
    }
}

#[test]
fn invalid_arity_and_types_are_qualified_hard_errors() {
    let errors = [
        hard_error(number_from_string(&[], SPAN)),
        hard_error(number_from_string(&[Value::Int(1)], SPAN)),
        hard_error(number_to_string(&[], SPAN)),
        hard_error(number_to_string(&[string("1")], SPAN)),
    ];

    for error in errors {
        assert!(
            error.message.starts_with("std/number."),
            "{}",
            error.message
        );
        assert_eq!(error.span, SPAN);
    }
}
