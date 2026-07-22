use super::*;
use crate::span::Span;

const SPAN: Span = Span::new(2, 5);

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
fn length_and_slice_count_unicode_scalar_values() {
    assert_eq!(result(string_length(&[string("aé🦀")], SPAN)).render(), "3");
    assert_eq!(
        result(string_slice(
            &[string("aé🦀z"), Value::Int(1), Value::Int(3)],
            SPAN,
        ))
        .render(),
        "\"é🦀\""
    );
}

#[test]
fn slice_clamps_bounds_and_empty_ranges() {
    for (start, end, expected) in [(1, 20, "bc"), (20, 30, ""), (2, 2, ""), (3, 1, "")] {
        assert_eq!(
            result(string_slice(
                &[string("abc"), Value::Int(start), Value::Int(end)],
                SPAN,
            ))
            .render(),
            format!("\"{expected}\"")
        );
    }
}

#[test]
fn search_and_case_operations_handle_unicode_strings() {
    assert!(matches!(
        result(string_contains(&[string("café"), string("fé")], SPAN)),
        Value::Bool(true)
    ));
    assert!(matches!(
        result(string_starts_with(&[string("🦀acean"), string("🦀")], SPAN)),
        Value::Bool(true)
    ));
    assert!(matches!(
        result(string_ends_with(&[string("naïve"), string("ïve")], SPAN)),
        Value::Bool(true)
    ));
    assert_eq!(
        result(string_trim(&[string(" \n\té \u{2003}")], SPAN)).render(),
        "\"é\""
    );
    assert_eq!(
        result(string_lower(&[string("ÄBC")], SPAN)).render(),
        "\"äbc\""
    );
    assert_eq!(
        result(string_upper(&[string("Straße")], SPAN)).render(),
        "\"STRASSE\""
    );
}

#[test]
fn split_preserves_regular_empty_fields_but_not_empty_separator_sentinels() {
    assert_eq!(
        result(string_split(&[string(",a,,b,"), string(",")], SPAN)).render(),
        "[\"\", \"a\", \"\", \"b\", \"\"]"
    );
    assert_eq!(
        result(string_split(&[string("aé🦀"), string("")], SPAN)).render(),
        "[\"a\", \"é\", \"🦀\"]"
    );
    assert_eq!(
        result(string_split(&[string(""), string("")], SPAN)).render(),
        "[]"
    );
}

#[test]
fn invalid_arity_types_and_indices_are_qualified_hard_errors() {
    let errors = [
        hard_error(string_length(&[], SPAN)),
        hard_error(string_contains(&[Value::Int(1), string("1")], SPAN)),
        hard_error(string_split(&[string("abc"), Value::Nil], SPAN)),
        hard_error(string_slice(
            &[string("abc"), Value::Int(-1), Value::Int(2)],
            SPAN,
        )),
        hard_error(string_slice(
            &[string("abc"), Value::Int(0), Value::Float(2.0)],
            SPAN,
        )),
    ];

    for error in errors {
        assert!(
            error.message.starts_with("std/string."),
            "{}",
            error.message
        );
        assert_eq!(error.span, SPAN);
    }
}
