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

// --- encode / decode roundtrip ---

#[test]
fn encode_le_converts_string_to_little_endian_bytes() {
    let encoded = result(utf16_encode_le(&[Value::String("a".to_owned())], SPAN));
    assert_eq!(encoded.render(), "bytes[61 00]");
}

#[test]
fn encode_be_converts_string_to_big_endian_bytes() {
    let encoded = result(utf16_encode_be(&[Value::String("a".to_owned())], SPAN));
    assert_eq!(encoded.render(), "bytes[00 61]");
}

#[test]
fn empty_string_encodes_to_empty_bytes() {
    let encoded = result(utf16_encode_le(&[Value::String("".to_owned())], SPAN));
    assert_eq!(encoded.render(), "bytes[]");
    let encoded = result(utf16_encode_be(&[Value::String("".to_owned())], SPAN));
    assert_eq!(encoded.render(), "bytes[]");
}

#[test]
fn decode_valid_utf16_both_endiannesses() {
    // "abc" little-endian
    let le = bytes(vec![0x61, 0x00, 0x62, 0x00, 0x63, 0x00]);
    assert_eq!(result(utf16_decode_le(&[le], SPAN)).render(), "\"abc\"");

    // "abc" big-endian
    let be = bytes(vec![0x00, 0x61, 0x00, 0x62, 0x00, 0x63]);
    assert_eq!(result(utf16_decode_be(&[be], SPAN)).render(), "\"abc\"");
}

#[test]
fn empty_bytes_decode_to_empty_string() {
    let decoded = result(utf16_decode_le(&[bytes(vec![])], SPAN));
    assert_eq!(decoded.render(), "\"\"");
    let decoded = result(utf16_decode_be(&[bytes(vec![])], SPAN));
    assert_eq!(decoded.render(), "\"\"");
}

// --- unicode roundtrip ---

#[test]
fn unicode_roundtrip_both_endiannesses() {
    let text = "aé🦀";
    // Little-endian roundtrip
    let encoded = result(utf16_encode_le(&[Value::String(text.to_owned())], SPAN));
    let decoded = result(utf16_decode_le(&[encoded], SPAN));
    assert_eq!(decoded.render(), format!("\"{text}\""));

    // Big-endian roundtrip
    let encoded = result(utf16_encode_be(&[Value::String(text.to_owned())], SPAN));
    let decoded = result(utf16_decode_be(&[encoded], SPAN));
    assert_eq!(decoded.render(), format!("\"{text}\""));
}

#[test]
fn non_bmp_surrogate_pair_roundtrips() {
    // U+1F600 grinning face
    let text = "\u{1F600}";
    // '😀' encodes as surrogate pair D83D DE00 in UTF-16
    let encoded = result(utf16_encode_le(&[Value::String(text.to_owned())], SPAN));
    // Little-endian: 3D D8 00 DE
    assert_eq!(encoded.render(), "bytes[3d d8 00 de]");
    let decoded = result(utf16_decode_le(&[encoded], SPAN));
    assert_eq!(decoded.render(), "\"😀\"");

    // Big-endian
    let encoded_be = result(utf16_encode_be(&[Value::String(text.to_owned())], SPAN));
    assert_eq!(encoded_be.render(), "bytes[d8 3d de 00]");
    let decoded_be = result(utf16_decode_be(&[encoded_be], SPAN));
    assert_eq!(decoded_be.render(), "\"😀\"");
}

// --- endianness independence ---

#[test]
fn endianness_distinction_is_preserved() {
    let text = "\u{0041}\u{0042}"; // AB in BMP
    let le = result(utf16_encode_le(&[Value::String(text.to_owned())], SPAN));
    let be = result(utf16_encode_be(&[Value::String(text.to_owned())], SPAN));
    // Little-endian "AB": 41 00 42 00
    // Big-endian "AB":    00 41 00 42
    assert_ne!(le.render(), be.render());

    // Cross-decoding endianness produces garbage or nil
    let cross_le = utf16_decode_le(&[be], SPAN);
    let cross_be = utf16_decode_be(&[le], SPAN);

    // Each cross-decode either returns nil or some wrong string
    if let Ok(Ok(v)) = cross_le {
        assert_ne!(v.render(), "\"AB\"");
    }
    if let Ok(Ok(v)) = cross_be {
        assert_ne!(v.render(), "\"AB\"");
    }
}

// --- malformed input ---

#[test]
fn odd_length_bytes_returns_nil() {
    let odd = bytes(vec![0x41]); // single byte, not a complete u16
    assert!(matches!(
        result(utf16_decode_le(std::slice::from_ref(&odd), SPAN)),
        Value::Nil
    ));
    assert!(matches!(result(utf16_decode_be(&[odd], SPAN)), Value::Nil));
}

#[test]
fn unpaired_surrogates_return_nil() {
    // Lone high surrogate D800 (little-endian)
    let lone_high_le = bytes(vec![0x00, 0xD8]);
    assert!(matches!(
        result(utf16_decode_le(&[lone_high_le], SPAN)),
        Value::Nil
    ));

    // Lone high surrogate D800 (big-endian)
    let lone_high_be = bytes(vec![0xD8, 0x00]);
    assert!(matches!(
        result(utf16_decode_be(&[lone_high_be], SPAN)),
        Value::Nil
    ));

    // Lone low surrogate DC00 (little-endian)
    let lone_low_le = bytes(vec![0x00, 0xDC]);
    assert!(matches!(
        result(utf16_decode_le(&[lone_low_le], SPAN)),
        Value::Nil
    ));

    // High surrogate followed by non-surrogate (broken pair)
    let broken_pair_le = bytes(vec![0x00, 0xD8, 0x41, 0x00]); // D800 + 'A'
    assert!(matches!(
        result(utf16_decode_le(&[broken_pair_le], SPAN)),
        Value::Nil
    ));
}

// --- BOM preservation ---

#[test]
fn bom_is_preserved_as_regular_codepoint() {
    // U+FEFF BOM
    let text = "\u{FEFF}";
    let encoded_le = result(utf16_encode_le(&[Value::String(text.to_owned())], SPAN));
    // Little-endian: FF FE
    assert_eq!(encoded_le.render(), "bytes[ff fe]");

    let decoded = result(utf16_decode_le(&[encoded_le], SPAN));
    assert_eq!(decoded.render(), "\"\u{FEFF}\"");

    // Same for big-endian
    let encoded_be = result(utf16_encode_be(&[Value::String(text.to_owned())], SPAN));
    assert_eq!(encoded_be.render(), "bytes[fe ff]");
    let decoded_be = result(utf16_decode_be(&[encoded_be], SPAN));
    assert_eq!(decoded_be.render(), "\"\u{FEFF}\"");
}

#[test]
fn bom_in_middle_of_string_is_preserved() {
    let text = "a\u{FEFF}b";
    let encoded = result(utf16_encode_le(&[Value::String(text.to_owned())], SPAN));
    let decoded = result(utf16_decode_le(&[encoded], SPAN));
    assert_eq!(decoded.render(), "\"a\u{FEFF}b\"");
}

// --- hard diagnostics ---

#[test]
fn invalid_arity_and_types_are_qualified_hard_errors() {
    let errors = [
        hard_error(utf16_encode_le(&[], SPAN)),
        hard_error(utf16_encode_le(
            &[Value::String("a".to_owned()), Value::String("b".to_owned())],
            SPAN,
        )),
        hard_error(utf16_encode_le(&[Value::Int(1)], SPAN)),
        hard_error(utf16_encode_be(&[], SPAN)),
        hard_error(utf16_encode_be(&[Value::Nil], SPAN)),
        hard_error(utf16_decode_le(&[], SPAN)),
        hard_error(utf16_decode_le(&[Value::String("x".to_owned())], SPAN)),
        hard_error(utf16_decode_be(&[], SPAN)),
        hard_error(utf16_decode_be(&[Value::Int(0)], SPAN)),
    ];

    for error in errors {
        assert!(error.message.starts_with("std/utf16."), "{}", error.message);
        assert_eq!(error.span, SPAN);
    }
}
