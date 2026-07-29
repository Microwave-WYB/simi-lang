use simi::{SimiError, eval};

fn value(source: &str) -> simi::Value {
    eval(source)
        .expect("source should have no hard diagnostic")
        .expect("source should not raise")
}

// ---------------------------------------------------------------------------
// require works; bare names fail
// ---------------------------------------------------------------------------

#[test]
fn integer_module_is_require_only() {
    assert!(matches!(
        eval("integer.encode(0, \"i8le\")"),
        Err(SimiError::Runtime(_))
    ));
    let _ = value("let integer = require(\"std/integer\") integer.encode(0, \"i8le\")");
}

#[test]
fn float_module_is_require_only() {
    assert!(matches!(
        eval("float.encode(0.0, \"f64le\")"),
        Err(SimiError::Runtime(_))
    ));
    let _ = value("let float = require(\"std/float\") float.encode(0.0, \"f64le\")");
}

#[test]
fn utf8_module_is_require_only() {
    assert!(matches!(
        eval("utf8.encode(\"a\")"),
        Err(SimiError::Runtime(_))
    ));
    let _ = value("let utf8 = require(\"std/utf8\") utf8.encode(\"a\")");
}

#[test]
fn utf16_module_is_require_only() {
    assert!(matches!(
        eval("utf16.encode_le(\"a\")"),
        Err(SimiError::Runtime(_))
    ));
    let _ = value("let utf16 = require(\"std/utf16\") utf16.encode_le(\"a\")");
}

// ---------------------------------------------------------------------------
// integer: representative operations, range errors (hard), wrong length (nil)
// ---------------------------------------------------------------------------

#[test]
fn integer_encode_decode_roundtrip() {
    let result = value(
        r#"
        let integer = require("std/integer")
        let encoded = integer.encode(42, "i32le")
        let decoded = integer.decode(encoded, "i32le")
        [
            inspect(encoded),
            decoded,
            integer.decode(integer.encode(-1, "i64le"), "i64le"),
            integer.decode(integer.encode(255, "u8le"), "u8le"),
        ]
        "#,
    );
    assert_eq!(result.render(), "[\"bytes[2a 00 00 00]\", 42, -1, 255]");
}

#[test]
fn integer_decode_wrong_length_returns_nil() {
    let result = value(
        r#"
        let integer = require("std/integer")
        [
            integer.decode(#[], "i8le"),
            integer.decode(#[0], "i16le"),
            integer.decode(#[0, 0, 0], "i32le"),
            integer.decode(#[0, 0, 0, 0, 0, 0, 0], "i64le"),
        ]
        "#,
    );
    assert_eq!(result.render(), "[nil, nil, nil, nil]");
}

#[test]
fn integer_encode_range_errors_are_hard_diagnostics() {
    for source in [
        r#"let integer = require("std/integer") integer.encode(128, "i8le")"#,
        r#"let integer = require("std/integer") integer.encode(-129, "i8le")"#,
        r#"let integer = require("std/integer") integer.encode(-1, "u8le")"#,
        r#"let integer = require("std/integer") integer.encode(256, "u8le")"#,
        r#"let integer = require("std/integer") integer.encode(-1, "u16le")"#,
        r#"let integer = require("std/integer") integer.encode(65536, "u16le")"#,
        r#"let integer = require("std/integer") integer.encode(-1, "u32le")"#,
        r#"let integer = require("std/integer") integer.encode(-1, "u64le")"#,
    ] {
        let error = match eval(source) {
            Err(error) => error,
            Ok(_) => panic!("expected hard diagnostic for {source}"),
        };
        assert!(matches!(error, SimiError::Runtime(_)), "{error}");
        assert!(error.to_string().contains("std/integer."), "{error}");
    }
}

#[test]
fn integer_decode_u64_out_of_range_is_hard_diagnostic() {
    for source in [
        // u64::MAX exceeds i64::MAX
        r#"let integer = require("std/integer") integer.decode(#[255, 255, 255, 255, 255, 255, 255, 255], "u64le")"#,
        // i64::MAX + 1 as u64
        r#"let integer = require("std/integer") integer.decode(#[0, 0, 0, 0, 0, 0, 0, 128], "u64le")"#,
    ] {
        let error = match eval(source) {
            Err(error) => error,
            Ok(_) => panic!("expected hard diagnostic for {source}"),
        };
        assert!(matches!(error, SimiError::Runtime(_)), "{error}");
        assert!(error.to_string().contains("std/integer."), "{error}");
    }
}

#[test]
fn integer_all_formats_encode_decode_successfully() {
    let result = value(
        r#"
        let integer = require("std/integer")
        let format = "i16be"
        let encoded = integer.encode(0x1234, format)
        [
            integer.encode(0, "i8le") == #[0],
            integer.encode(0, "i8be") == #[0],
            integer.encode(127, "i8le") == #[127],
            integer.encode(-128, "i8le") == #[128],
            integer.encode(-1, "i8le") == #[255],
            integer.encode(0, "u8le") == #[0],
            integer.encode(255, "u8le") == #[255],
            integer.encode(0, "i16le") == #[0, 0],
            integer.encode(32767, "i16le") == #[255, 127],
            integer.encode(32767, "i16be") == #[127, 255],
            integer.encode(-32768, "i16le") == #[0, 128],
            integer.encode(-1, "i16le") == #[255, 255],
            integer.encode(0, "u16le") == #[0, 0],
            integer.encode(65535, "u16le") == #[255, 255],
            integer.decode(integer.encode(0, "i32le"), "i32le"),
            integer.decode(integer.encode(0, "i64le"), "i64le"),
        ]
        "#,
    );
    assert_eq!(
        result.render(),
        "[true, true, true, true, true, true, true, true, true, true, true, true, true, true, 0, 0]"
    );
}

// ---------------------------------------------------------------------------
// float: special strings, narrowing nil, wrong length nil
// ---------------------------------------------------------------------------

#[test]
fn float_finite_encode_decode_roundtrip() {
    let result = value(
        r#"
        let float = require("std/float")
        [
            float.decode(float.encode(0.0, "f64le"), "f64le"),
            float.decode(float.encode(1.5, "f64le"), "f64le"),
            float.decode(float.encode(-2.25, "f64le"), "f64le"),
            float.decode(float.encode(1.0, "f32le"), "f32le"),
            float.decode(float.encode(-1.0, "f32be"), "f32be"),
        ]
        "#,
    );
    assert_eq!(result.render(), "[0.0, 1.5, -2.25, 1.0, -1.0]");
}

#[test]
fn float_special_strings_encode_decode() {
    let result = value(
        r#"
        let float = require("std/float")
        let inf_encoded = float.encode("inf", "f64le")
        let neg_inf_encoded = float.encode("-inf", "f64le")
        let nan_encoded = float.encode("nan", "f64le")
        [
            float.decode(inf_encoded, "f64le"),
            float.decode(neg_inf_encoded, "f64le"),
            float.decode(nan_encoded, "f64le"),
            float.decode(float.encode("inf", "f32le"), "f32le"),
            float.decode(float.encode("-inf", "f32be"), "f32be"),
            float.decode(float.encode("nan", "f32le"), "f32le"),
        ]
        "#,
    );
    assert_eq!(
        result.render(),
        "[\"inf\", \"-inf\", \"nan\", \"inf\", \"-inf\", \"nan\"]"
    );
}

#[test]
fn float_narrowing_to_f32_returns_nil_for_nonfinite() {
    let result = value(
        r#"
        let float = require("std/float")
        float.encode(3.4028235e39, "f32le")
        "#,
    );
    assert_eq!(result.render(), "nil");
}

#[test]
fn float_decode_wrong_length_returns_nil() {
    let result = value(
        r#"
        let float = require("std/float")
        [
            float.decode(#[], "f32le"),
            float.decode(#[0, 0, 0], "f32le"),
            float.decode(#[0, 0, 0, 0], "f64le"),
            float.decode(#[0, 0, 0, 0, 0, 0, 0, 0, 0], "f64le"),
        ]
        "#,
    );
    assert_eq!(result.render(), "[nil, nil, nil, nil]");
}

#[test]
fn float_invalid_special_string_is_hard_diagnostic() {
    let error = match eval(r#"let float = require("std/float") float.encode("infinite", "f64le")"#)
    {
        Err(error) => error,
        Ok(_) => panic!("expected hard diagnostic"),
    };
    assert!(matches!(error, SimiError::Runtime(_)), "{error}");
    assert!(error.to_string().contains("std/float."), "{error}");
}

#[test]
fn float_decode_ieee_nonfinite_returns_strings() {
    let result = value(
        r#"
        let float = require("std/float")
        -- f32 inf LE: 0x7f800000
        -- f32 -inf LE: 0xff800000
        -- canonical NaN f32 LE: 0x7fc00000
        -- f64 inf LE
        [
            float.decode(#[0, 0, 128, 127], "f32le"),
            float.decode(#[0, 0, 128, 255], "f32le"),
            float.decode(#[0, 0, 192, 127], "f32le"),
            float.decode(#[0, 0, 0, 0, 0, 0, 240, 127], "f64le"),
            float.decode(#[0, 0, 0, 0, 0, 0, 240, 255], "f64le"),
            float.decode(#[0, 0, 0, 0, 0, 0, 248, 127], "f64le"),
        ]
        "#,
    );
    assert_eq!(
        result.render(),
        "[\"inf\", \"-inf\", \"nan\", \"inf\", \"-inf\", \"nan\"]"
    );
}

// ---------------------------------------------------------------------------
// utf8: strict decoding, malformed → nil
// ---------------------------------------------------------------------------

#[test]
fn utf8_encode_decode_roundtrip() {
    let result = value(
        r#"
        let utf8 = require("std/utf8")
        let text = "aé🦀"
        let encoded = utf8.encode(text)
        [
            inspect(encoded),
            utf8.decode(encoded),
        ]
        "#,
    );
    assert_eq!(
        result.render(),
        "[\"bytes[61 c3 a9 f0 9f a6 80]\", \"aé🦀\"]"
    );
}

#[test]
fn utf8_decode_malformed_returns_nil() {
    let result = value(
        r#"
        let utf8 = require("std/utf8")
        [
            utf8.decode(#[255]),
            utf8.decode(#[192, 128]),
            utf8.decode(#[237, 160, 128]),
            utf8.decode(#[226, 128]),
        ]
        "#,
    );
    assert_eq!(result.render(), "[nil, nil, nil, nil]");
}

#[test]
fn utf8_empty_roundtrip() {
    let result = value(
        r#"
        let utf8 = require("std/utf8")
        [
            inspect(utf8.encode("")),
            utf8.decode(utf8.encode("")),
        ]
        "#,
    );
    assert_eq!(result.render(), "[\"bytes[]\", \"\"]");
}

// ---------------------------------------------------------------------------
// utf16: explicit endian, malformed → nil, BOM preserved
// ---------------------------------------------------------------------------

#[test]
fn utf16_encode_decode_roundtrip() {
    let result = value(
        r#"
        let utf16 = require("std/utf16")
        let text = "aé🦀"
        [
            utf16.decode_le(utf16.encode_le(text)),
            utf16.decode_be(utf16.encode_be(text)),
        ]
        "#,
    );
    assert_eq!(result.render(), "[\"aé🦀\", \"aé🦀\"]");
}

#[test]
fn utf16_endianness_is_preserved() {
    let result = value(
        r#"
        let utf16 = require("std/utf16")
        let le = utf16.encode_le("AB")
        let be = utf16.encode_be("AB")
        [
            le != be,
            inspect(le),
            inspect(be),
        ]
        "#,
    );
    assert_eq!(
        result.render(),
        "[true, \"bytes[41 00 42 00]\", \"bytes[00 41 00 42]\"]"
    );
}

#[test]
fn utf16_decode_malformed_returns_nil() {
    let result = value(
        r#"
        let utf16 = require("std/utf16")
        [
            utf16.decode_le(#[65]),
            utf16.decode_be(#[65]),
            -- lone high surrogate D800
            utf16.decode_le(#[0, 216]),
            -- lone low surrogate DC00
            utf16.decode_le(#[0, 220]),
            -- high surrogate followed by ASCII instead of low surrogate
            utf16.decode_le(#[0, 216, 65, 0]),
        ]
        "#,
    );
    assert_eq!(result.render(), "[nil, nil, nil, nil, nil]");
}

#[test]
fn utf16_bom_preserved_as_regular_codepoint() {
    let result = value(
        r#"
        let utf16 = require("std/utf16")
        let text = "﻿"
        [
            inspect(utf16.encode_le(text)),
            utf16.decode_le(utf16.encode_le(text)),
        ]
        "#,
    );
    assert_eq!(result.render(), "[\"bytes[ff fe]\", \"﻿\"]");
}

// ---------------------------------------------------------------------------
// invalid argument categories → hard diagnostics
// ---------------------------------------------------------------------------

#[test]
fn codec_invalid_argument_categories_are_hard_diagnostics() {
    for source in [
        // integer wrong arg categories
        r#"let integer = require("std/integer") integer.encode(1.0, "i8le")"#,
        r#"let integer = require("std/integer") integer.encode(0, 1)"#,
        r#"let integer = require("std/integer") integer.decode(1, "i8le")"#,
        // float wrong arg categories
        r#"let float = require("std/float") float.encode(1, "f64le")"#,
        r#"let float = require("std/float") float.encode(1.0, 1)"#,
        r#"let float = require("std/float") float.decode(1, "f64le")"#,
        // utf8 wrong arg categories
        r#"let utf8 = require("std/utf8") utf8.encode(1)"#,
        r#"let utf8 = require("std/utf8") utf8.decode("x")"#,
        // utf16 wrong arg categories
        r#"let utf16 = require("std/utf16") utf16.encode_le(1)"#,
        r#"let utf16 = require("std/utf16") utf16.decode_le("x")"#,
    ] {
        let error = match eval(source) {
            Err(error) => error,
            Ok(_) => panic!("expected hard diagnostic for {source}"),
        };
        assert!(matches!(error, SimiError::Runtime(_)), "{error}");
    }
}

// ---------------------------------------------------------------------------
// wrong arity → hard diagnostics
// ---------------------------------------------------------------------------

#[test]
fn codec_wrong_arity_is_hard_diagnostic() {
    for source in [
        r#"let integer = require("std/integer") integer.encode(0)"#,
        r#"let integer = require("std/integer") integer.decode()"#,
        r#"let float = require("std/float") float.encode(1.0)"#,
        r#"let float = require("std/float") float.decode()"#,
        r#"let utf8 = require("std/utf8") utf8.encode()"#,
        r#"let utf8 = require("std/utf8") utf8.decode()"#,
        r#"let utf16 = require("std/utf16") utf16.encode_le()"#,
        r#"let utf16 = require("std/utf16") utf16.decode_le()"#,
    ] {
        let error = match eval(source) {
            Err(error) => error,
            Ok(_) => panic!("expected hard diagnostic for {source}"),
        };
        assert!(matches!(error, SimiError::Runtime(_)), "{error}");
    }
}
