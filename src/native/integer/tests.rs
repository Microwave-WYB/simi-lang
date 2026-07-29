use super::*;
use crate::runtime::RuntimeError;
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

// ---------------------------------------------------------------------------
// encode tests
// ---------------------------------------------------------------------------

#[test]
fn encode_signed_8bit_extrema() {
    assert_eq!(
        result(integer_encode(
            &[Value::Int(0), Value::String("i8le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[00]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(0), Value::String("i8be".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[00]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(127), Value::String("i8le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[7f]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(-128), Value::String("i8le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[80]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(-1), Value::String("i8le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[ff]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(-1), Value::String("i8be".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[ff]"
    );
}

#[test]
fn encode_unsigned_8bit_extrema() {
    assert_eq!(
        result(integer_encode(
            &[Value::Int(0), Value::String("u8le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[00]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(255), Value::String("u8le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[ff]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(255), Value::String("u8be".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[ff]"
    );
}

#[test]
fn encode_signed_16bit_extrema() {
    assert_eq!(
        result(integer_encode(
            &[Value::Int(0), Value::String("i16le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[00 00]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(32767), Value::String("i16le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[ff 7f]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(32767), Value::String("i16be".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[7f ff]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(-32768), Value::String("i16le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[00 80]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(-1), Value::String("i16le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[ff ff]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(-1), Value::String("i16be".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[ff ff]"
    );
}

#[test]
fn encode_unsigned_16bit_extrema() {
    assert_eq!(
        result(integer_encode(
            &[Value::Int(0), Value::String("u16le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[00 00]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(65535), Value::String("u16le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[ff ff]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(65535), Value::String("u16be".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[ff ff]"
    );
}

#[test]
fn encode_signed_32bit_extrema() {
    assert_eq!(
        result(integer_encode(
            &[Value::Int(0), Value::String("i32le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[00 00 00 00]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(2_147_483_647), Value::String("i32le".to_owned()),],
            SPAN,
        ))
        .render(),
        "bytes[ff ff ff 7f]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(2_147_483_647), Value::String("i32be".to_owned()),],
            SPAN,
        ))
        .render(),
        "bytes[7f ff ff ff]"
    );
    assert_eq!(
        result(integer_encode(
            &[
                Value::Int(-2_147_483_648),
                Value::String("i32le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "bytes[00 00 00 80]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(-1), Value::String("i32le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[ff ff ff ff]"
    );
}

#[test]
fn encode_unsigned_32bit_extrema() {
    assert_eq!(
        result(integer_encode(
            &[Value::Int(0), Value::String("u32le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[00 00 00 00]"
    );
    assert_eq!(
        result(integer_encode(
            &[
                Value::Int(4_294_967_295_i64),
                Value::String("u32le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "bytes[ff ff ff ff]"
    );
    assert_eq!(
        result(integer_encode(
            &[
                Value::Int(4_294_967_295_i64),
                Value::String("u32be".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "bytes[ff ff ff ff]"
    );
}

#[test]
fn encode_signed_64bit_extrema() {
    assert_eq!(
        result(integer_encode(
            &[Value::Int(0), Value::String("i64le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[00 00 00 00 00 00 00 00]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(i64::MAX), Value::String("i64le".to_owned()),],
            SPAN,
        ))
        .render(),
        "bytes[ff ff ff ff ff ff ff 7f]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(i64::MAX), Value::String("i64be".to_owned()),],
            SPAN,
        ))
        .render(),
        "bytes[7f ff ff ff ff ff ff ff]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(i64::MIN), Value::String("i64le".to_owned()),],
            SPAN,
        ))
        .render(),
        "bytes[00 00 00 00 00 00 00 80]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(-1), Value::String("i64le".to_owned()),],
            SPAN,
        ))
        .render(),
        "bytes[ff ff ff ff ff ff ff ff]"
    );
}

#[test]
fn encode_unsigned_64bit_values() {
    // u64 values within i64 range
    assert_eq!(
        result(integer_encode(
            &[Value::Int(0), Value::String("u64le".to_owned())],
            SPAN,
        ))
        .render(),
        "bytes[00 00 00 00 00 00 00 00]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(i64::MAX), Value::String("u64le".to_owned()),],
            SPAN,
        ))
        .render(),
        "bytes[ff ff ff ff ff ff ff 7f]"
    );
    assert_eq!(
        result(integer_encode(
            &[Value::Int(i64::MAX), Value::String("u64be".to_owned()),],
            SPAN,
        ))
        .render(),
        "bytes[7f ff ff ff ff ff ff ff]"
    );
}

#[test]
fn encode_range_errors() {
    let error = hard_error(integer_encode(
        &[Value::Int(128), Value::String("i8le".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.encode value 128 is out of range"),
        "{}",
        error.message
    );

    let error = hard_error(integer_encode(
        &[Value::Int(-129), Value::String("i8le".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.encode value -129 is out of range"),
        "{}",
        error.message
    );

    let error = hard_error(integer_encode(
        &[Value::Int(-1), Value::String("u8le".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.encode value -1 is out of range"),
        "{}",
        error.message
    );

    let error = hard_error(integer_encode(
        &[Value::Int(256), Value::String("u8le".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.encode value 256 is out of range"),
        "{}",
        error.message
    );

    let error = hard_error(integer_encode(
        &[Value::Int(65536), Value::String("u16le".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.encode value 65536 is out of range"),
        "{}",
        error.message
    );

    let error = hard_error(integer_encode(
        &[Value::Int(-1), Value::String("u32le".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.encode value -1 is out of range"),
        "{}",
        error.message
    );

    let error = hard_error(integer_encode(
        &[Value::Int(-1), Value::String("u64le".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.encode value -1 is out of range"),
        "{}",
        error.message
    );
}

// ---------------------------------------------------------------------------
// decode tests
// ---------------------------------------------------------------------------

#[test]
fn decode_signed_8bit_extrema() {
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0x00]), Value::String("i8le".to_owned())],
            SPAN,
        ))
        .render(),
        "0"
    );
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0x7f]), Value::String("i8le".to_owned())],
            SPAN,
        ))
        .render(),
        "127"
    );
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0x80]), Value::String("i8le".to_owned())],
            SPAN,
        ))
        .render(),
        "-128"
    );
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0xff]), Value::String("i8le".to_owned())],
            SPAN,
        ))
        .render(),
        "-1"
    );
}

#[test]
fn decode_unsigned_8bit_extrema() {
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0x00]), Value::String("u8le".to_owned())],
            SPAN,
        ))
        .render(),
        "0"
    );
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0xff]), Value::String("u8le".to_owned())],
            SPAN,
        ))
        .render(),
        "255"
    );
}

#[test]
fn decode_signed_16bit_le() {
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0x00, 0x00]), Value::String("i16le".to_owned()),],
            SPAN,
        ))
        .render(),
        "0"
    );
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0xff, 0x7f]), Value::String("i16le".to_owned()),],
            SPAN,
        ))
        .render(),
        "32767"
    );
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0x00, 0x80]), Value::String("i16le".to_owned()),],
            SPAN,
        ))
        .render(),
        "-32768"
    );
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0xff, 0xff]), Value::String("i16le".to_owned()),],
            SPAN,
        ))
        .render(),
        "-1"
    );
}

#[test]
fn decode_signed_16bit_be() {
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0x7f, 0xff]), Value::String("i16be".to_owned()),],
            SPAN,
        ))
        .render(),
        "32767"
    );
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0xff, 0xff]), Value::String("i16be".to_owned()),],
            SPAN,
        ))
        .render(),
        "-1"
    );
}

#[test]
fn decode_unsigned_16bit() {
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0xff, 0xff]), Value::String("u16le".to_owned()),],
            SPAN,
        ))
        .render(),
        "65535"
    );
    assert_eq!(
        result(integer_decode(
            &[bytes(vec![0xff, 0xff]), Value::String("u16be".to_owned()),],
            SPAN,
        ))
        .render(),
        "65535"
    );
}

#[test]
fn decode_signed_32bit_extrema() {
    assert_eq!(
        result(integer_decode(
            &[
                bytes(vec![0x00, 0x00, 0x00, 0x00]),
                Value::String("i32le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "0"
    );
    // i32 MAX (2_147_483_647) → bytes[ff ff ff 7f] le, [7f ff ff ff] be
    assert_eq!(
        result(integer_decode(
            &[
                bytes(vec![0xff, 0xff, 0xff, 0x7f]),
                Value::String("i32le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "2147483647"
    );
    assert_eq!(
        result(integer_decode(
            &[
                bytes(vec![0x7f, 0xff, 0xff, 0xff]),
                Value::String("i32be".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "2147483647"
    );
    // i32 MIN (-2_147_483_648) → bytes[00 00 00 80] le
    assert_eq!(
        result(integer_decode(
            &[
                bytes(vec![0x00, 0x00, 0x00, 0x80]),
                Value::String("i32le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "-2147483648"
    );
    assert_eq!(
        result(integer_decode(
            &[
                bytes(vec![0xff, 0xff, 0xff, 0xff]),
                Value::String("i32le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "-1"
    );
}

#[test]
fn decode_unsigned_32bit() {
    assert_eq!(
        result(integer_decode(
            &[
                bytes(vec![0xff, 0xff, 0xff, 0xff]),
                Value::String("u32le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "4294967295"
    );
}

#[test]
fn decode_signed_64bit_extrema() {
    assert_eq!(
        result(integer_decode(
            &[
                bytes(vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
                Value::String("i64le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "0"
    );
    // i64 MAX
    assert_eq!(
        result(integer_decode(
            &[
                bytes(vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f]),
                Value::String("i64le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "9223372036854775807"
    );
    // i64 MIN
    assert_eq!(
        result(integer_decode(
            &[
                bytes(vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]),
                Value::String("i64le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "-9223372036854775808"
    );
    assert_eq!(
        result(integer_decode(
            &[
                bytes(vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
                Value::String("i64le".to_owned()),
            ],
            SPAN,
        ))
        .render(),
        "-1"
    );
}

#[test]
fn decode_unsigned_64bit_boundary() {
    // u64 value = u64::MAX (which exceeds i64::MAX) → hard diagnostic
    let error = hard_error(integer_decode(
        &[
            bytes(vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            Value::String("u64le".to_owned()),
        ],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.decode decoded value 18446744073709551615 exceeds i64"),
        "{}",
        error.message
    );

    // u64 value = i64::MAX + 1 → hard diagnostic
    let error = hard_error(integer_decode(
        &[
            bytes(vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]),
            Value::String("u64le".to_owned()),
        ],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.decode decoded value"),
        "{}",
        error.message
    );
}

#[test]
fn decode_wrong_byte_length_returns_nil() {
    // i8le expects 1 byte; 0 bytes → nil
    assert!(matches!(
        result(integer_decode(
            &[bytes(vec![]), Value::String("i8le".to_owned())],
            SPAN,
        )),
        Value::Nil
    ));
    // i16le expects 2 bytes; 1 byte → nil
    assert!(matches!(
        result(integer_decode(
            &[bytes(vec![0x00]), Value::String("i16le".to_owned())],
            SPAN,
        )),
        Value::Nil
    ));
    // i32le expects 4 bytes; 3 bytes → nil
    assert!(matches!(
        result(integer_decode(
            &[
                bytes(vec![0x00, 0x00, 0x00]),
                Value::String("i32le".to_owned()),
            ],
            SPAN,
        )),
        Value::Nil
    ));
    // i64le expects 8 bytes; 9 bytes → nil
    assert!(matches!(
        result(integer_decode(
            &[
                bytes(vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
                Value::String("i64le".to_owned()),
            ],
            SPAN,
        )),
        Value::Nil
    ));
}

// ---------------------------------------------------------------------------
// diagnostic tests
// ---------------------------------------------------------------------------

#[test]
fn unknown_format_is_hard_error() {
    let error = hard_error(integer_encode(
        &[Value::Int(0), Value::String("bad".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.encode unknown format"),
        "{}",
        error.message
    );

    let error = hard_error(integer_decode(
        &[bytes(vec![]), Value::String("bad".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.decode unknown format"),
        "{}",
        error.message
    );
}

#[test]
fn wrong_arity_is_hard_error() {
    let error = hard_error(integer_encode(&[], SPAN));
    assert_eq!(
        error.message,
        "std/integer.encode expects 2 arguments, got 0"
    );

    let error = hard_error(integer_encode(&[Value::Int(0)], SPAN));
    assert_eq!(
        error.message,
        "std/integer.encode expects 2 arguments, got 1"
    );

    let error = hard_error(integer_decode(&[], SPAN));
    assert_eq!(
        error.message,
        "std/integer.decode expects 2 arguments, got 0"
    );

    let error = hard_error(integer_decode(
        &[bytes(vec![]), Value::String("i8le".to_owned()), Value::Nil],
        SPAN,
    ));
    assert_eq!(
        error.message,
        "std/integer.decode expects 2 arguments, got 3"
    );
}

#[test]
fn invalid_argument_categories_are_hard_errors() {
    let error = hard_error(integer_encode(
        &[Value::Float(0.0), Value::String("i8le".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.encode value must be an integer"),
        "{}",
        error.message
    );

    let error = hard_error(integer_encode(&[Value::Int(0), Value::Nil], SPAN));
    assert!(
        error
            .message
            .starts_with("std/integer.encode format must be a string"),
        "{}",
        error.message
    );

    let error = hard_error(integer_decode(
        &[Value::Int(0), Value::String("i8le".to_owned())],
        SPAN,
    ));
    assert!(
        error
            .message
            .starts_with("std/integer.decode data must be bytes"),
        "{}",
        error.message
    );

    let error = hard_error(integer_decode(&[bytes(vec![]), Value::Float(0.0)], SPAN));
    assert!(
        error
            .message
            .starts_with("std/integer.decode format must be a string"),
        "{}",
        error.message
    );
}

#[test]
fn all_error_messages_are_qualified() {
    // encode range errors
    let error = hard_error(integer_encode(
        &[Value::Int(256), Value::String("u8le".to_owned())],
        SPAN,
    ));
    assert!(
        error.message.starts_with("std/integer."),
        "{}",
        error.message
    );

    // unknown format
    let error = hard_error(integer_encode(
        &[Value::Int(0), Value::String("nope".to_owned())],
        SPAN,
    ));
    assert!(
        error.message.starts_with("std/integer."),
        "{}",
        error.message
    );

    // wrong arity
    let error = hard_error(integer_decode(&[Value::Int(0)], SPAN));
    assert!(
        error.message.starts_with("std/integer."),
        "{}",
        error.message
    );

    // wrong category
    let error = hard_error(integer_decode(
        &[Value::Nil, Value::String("i8le".to_owned())],
        SPAN,
    ));
    assert!(
        error.message.starts_with("std/integer."),
        "{}",
        error.message
    );

    // u64 decode boundary
    let error = hard_error(integer_decode(
        &[
            bytes(vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            Value::String("u64le".to_owned()),
        ],
        SPAN,
    ));
    assert!(
        error.message.starts_with("std/integer."),
        "{}",
        error.message
    );
}
