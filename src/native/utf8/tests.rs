use super::*;
use crate::span::Span;

const SPAN: Span = Span::new(2, 5);

fn bytes(values: Vec<u8>) -> Value {
    Value::Bytes(Bytes::new(values))
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
fn encode_converts_string_to_bytes() {
    assert_eq!(
        result(utf8_encode(&[Value::String("abc".to_owned())], SPAN)).render(),
        "bytes[61 62 63]"
    );
    assert_eq!(
        result(utf8_encode(&[Value::String("".to_owned())], SPAN)).render(),
        "bytes[]"
    );
}

#[test]
fn decode_converts_valid_bytes_to_string() {
    assert_eq!(
        result(utf8_decode(&[bytes(vec![0x61, 0x62, 0x63])], SPAN)).render(),
        "\"abc\""
    );
    assert_eq!(result(utf8_decode(&[bytes(vec![])], SPAN)).render(), "\"\"");
    assert!(matches!(
        result(utf8_decode(&[bytes(vec![])], SPAN)),
        Value::String(_)
    ));
}

#[test]
fn decode_returns_nil_for_invalid_utf8() {
    // Invalid continuation byte
    assert!(matches!(
        result(utf8_decode(&[bytes(vec![0xFF])], SPAN)),
        Value::Nil
    ));
    // Overlong encoding
    assert!(matches!(
        result(utf8_decode(&[bytes(vec![0xC0, 0x80])], SPAN)),
        Value::Nil
    ));
    // Surrogate half encoded in UTF-8
    assert!(matches!(
        result(utf8_decode(&[bytes(vec![0xED, 0xA0, 0x80])], SPAN)),
        Value::Nil
    ));
    // Truncated multi-byte
    assert!(matches!(
        result(utf8_decode(&[bytes(vec![0xE2, 0x80])], SPAN)),
        Value::Nil
    ));
}

#[test]
fn unicode_codepoint_roundtrip() {
    let text = "aé🦀";
    let encoded = result(utf8_encode(&[Value::String(text.to_owned())], SPAN));
    let decoded = result(utf8_decode(&[encoded], SPAN));
    assert_eq!(decoded.render(), format!("\"{text}\""));
}

#[test]
fn non_bmp_codepoints_encode_and_decode() {
    // U+1F600 grinning face, U+10FFFF max codepoint
    let text = "\u{1F600}\u{10FFFF}";
    let encoded = result(utf8_encode(&[Value::String(text.to_owned())], SPAN));
    assert!(matches!(
        result(utf8_decode(
            &[std::slice::from_ref(&encoded)[0].clone()],
            SPAN
        )),
        Value::String(_)
    ));
    let decoded = result(utf8_decode(&[encoded], SPAN));
    assert_eq!(decoded.render(), format!("\"{text}\""));
}

#[test]
fn invalid_arity_and_types_are_qualified_hard_errors() {
    let errors = [
        hard_error(utf8_encode(&[], SPAN)),
        hard_error(utf8_encode(
            &[Value::String("a".to_owned()), Value::String("b".to_owned())],
            SPAN,
        )),
        hard_error(utf8_encode(&[Value::Int(1)], SPAN)),
        hard_error(utf8_decode(&[], SPAN)),
        hard_error(utf8_decode(&[bytes(vec![]), bytes(vec![])], SPAN)),
        hard_error(utf8_decode(&[Value::Nil], SPAN)),
    ];

    for error in errors {
        assert!(error.message.starts_with("std/utf8."), "{}", error.message);
        assert_eq!(error.span, SPAN);
    }
}
