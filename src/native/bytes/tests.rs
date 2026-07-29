use super::*;
use crate::span::Span;

const SPAN: Span = Span::new(2, 5);

fn bytes(values: Vec<u8>) -> Value {
    Value::Bytes(Bytes::new(values))
}

fn list(values: Vec<Value>) -> Value {
    Value::List(List::shared(values))
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
fn immutable_byte_operations_return_expected_values() {
    let source = bytes(vec![0, 127, 255]);

    assert_eq!(
        result(bytes_length(std::slice::from_ref(&source), SPAN)).render(),
        "3"
    );
    assert_eq!(
        result(bytes_get(&[source.clone(), Value::Int(2)], SPAN)).render(),
        "255"
    );
    assert!(matches!(
        result(bytes_get(&[source.clone(), Value::Int(3)], SPAN)),
        Value::Nil
    ));
    assert_eq!(
        result(bytes_slice(
            &[source.clone(), Value::Int(1), Value::Int(20)],
            SPAN,
        ))
        .render(),
        "bytes[7f ff]"
    );
    assert_eq!(
        result(bytes_slice(
            &[source.clone(), Value::Int(3), Value::Int(1)],
            SPAN
        ))
        .render(),
        "bytes[]"
    );
    assert_eq!(
        result(bytes_concat(&[source, bytes(vec![1, 2])], SPAN)).render(),
        "bytes[00 7f ff 01 02]"
    );
}

#[test]
fn list_conversions_validate_before_constructing_bytes() {
    let values = list(vec![Value::Int(0), Value::Int(127), Value::Int(255)]);
    assert_eq!(
        result(bytes_from_list(std::slice::from_ref(&values), SPAN)).render(),
        "bytes[00 7f ff]"
    );
    assert_eq!(
        result(bytes_to_list(&[bytes(vec![0, 127, 255])], SPAN)).render(),
        "[0, 127, 255]"
    );

    let invalid = list(vec![Value::Int(1), Value::Int(256)]);
    let error = hard_error(bytes_from_list(std::slice::from_ref(&invalid), SPAN));
    assert_eq!(
        error.message,
        "std/bytes.from_list values[1] must be between 0 and 255, got 256"
    );
    assert_eq!(invalid.render(), "[1, 256]");
}

#[test]
fn invalid_arity_categories_and_indices_are_qualified_hard_errors() {
    let errors = [
        hard_error(bytes_length(&[], SPAN)),
        hard_error(bytes_get(&[bytes(vec![]), Value::Int(-1)], SPAN)),
        hard_error(bytes_slice(
            &[bytes(vec![]), Value::Float(0.0), Value::Int(1)],
            SPAN,
        )),
        hard_error(bytes_concat(&[bytes(vec![]), Value::Nil], SPAN)),
        hard_error(bytes_from_list(&[Value::Nil], SPAN)),
        hard_error(bytes_from_list(
            &[list(vec![Value::String("x".to_owned())])],
            SPAN,
        )),
        hard_error(bytes_to_list(&[Value::Int(1)], SPAN)),
    ];

    for error in errors {
        assert!(error.message.starts_with("std/bytes."), "{}", error.message);
        assert_eq!(error.span, SPAN);
    }
}
