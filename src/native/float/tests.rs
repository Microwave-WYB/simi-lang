use super::*;
use crate::span::Span;

const SPAN: Span = Span::new(2, 5);

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

fn bytes(data: &[u8]) -> Value {
    Value::Bytes(Bytes::new(data.to_vec()))
}

fn f(value: f64) -> Value {
    Value::Float(value)
}

fn s(value: &str) -> Value {
    Value::String(value.to_owned())
}

// ---- encode ---- //

#[test]
fn finite_f64_encode_round_trip() {
    for &(value, format_str) in &[
        (0.0, "f64le"),
        (-0.0, "f64le"),
        (0.0, "f64be"),
        (-0.0, "f64be"),
        (1.0, "f64le"),
        (-1.0, "f64le"),
        (std::f64::consts::PI, "f64be"),
        (f64::MAX, "f64le"),
    ] {
        let encoded = result(float_encode(&[f(value), s(format_str)], SPAN));
        let decoded = result(float_decode(&[encoded, s(format_str)], SPAN));
        assert!(
            matches!(&decoded, Value::Float(v) if v.to_bits() == value.to_bits()),
            "round-trip {value} {format_str}: got {decoded}",
            decoded = decoded.render(),
        );
    }
}

#[test]
fn signed_zero_encode_round_trip() {
    let pos_zero = f(0.0);
    let neg_zero = f(-0.0);

    for format_str in &["f32le", "f32be", "f64le", "f64be"] {
        let fmt = s(format_str);

        let encoded_pos = result(float_encode(&[pos_zero.clone(), fmt.clone()], SPAN));
        let decoded_pos = result(float_decode(&[encoded_pos, fmt.clone()], SPAN));
        assert!(matches!(&decoded_pos, Value::Float(v) if v.to_bits() == 0u64));
        assert!(decoded_pos.render().parse::<f64>().unwrap() >= 0.0);

        let encoded_neg = result(float_encode(&[neg_zero.clone(), fmt.clone()], SPAN));
        let decoded_neg = result(float_decode(&[encoded_neg, fmt.clone()], SPAN));
        assert!(matches!(&decoded_neg, Value::Float(v) if v.to_bits() == (-0.0f64).to_bits()));
        assert!(decoded_neg.render().parse::<f64>().unwrap() <= 0.0);
    }
}

#[test]
fn f64_to_f32_narrowing_returns_nil_for_nonfinite() {
    // f64::MAX narrowed to f32 becomes inf – should return nil
    let encoded = float_encode(&[f(f64::MAX), s("f32le")], SPAN);
    assert!(matches!(encoded, Ok(Ok(Value::Nil))));
}

#[test]
fn finite_f32_encode_round_trip() {
    let values = [
        0.0f64,
        -0.0f64,
        std::f32::consts::PI as f64,
        -(std::f32::consts::PI as f64),
        1.0f64,
        f32::MAX as f64,
    ];
    for &value in &values {
        for format_str in &["f32le", "f32be"] {
            let fmt = s(format_str);
            let encoded = result(float_encode(&[f(value), fmt.clone()], SPAN));
            let decoded = result(float_decode(&[encoded, fmt], SPAN));
            match decoded {
                Value::Float(v) => {
                    let expected = value as f32 as f64;
                    assert_eq!(
                        v.to_bits(),
                        expected.to_bits(),
                        "narrow {value} {format_str}"
                    );
                }
                other => panic!(
                    "{format_str} decode of {value}: expected float, got {}",
                    other.render()
                ),
            }
        }
    }
}

// ---- special encode ---- //

#[test]
fn special_strings_encode_decode() {
    for special in &["inf", "-inf", "nan"] {
        for format_str in &["f32le", "f32be", "f64le", "f64be"] {
            let fmt = s(format_str);
            let encoded = result(float_encode(&[s(special), fmt.clone()], SPAN));
            let decoded = result(float_decode(&[encoded, fmt], SPAN));
            assert!(
                matches!(&decoded, Value::String(s) if s == *special),
                "encode/decode {special} {format_str}: got {}",
                decoded.render(),
            );
        }
    }
}

#[test]
fn invalid_special_string_hard_error() {
    let error = hard_error(float_encode(&[s("infinite"), s("f64le")], SPAN));
    assert!(error.message.starts_with("std/float.encode"));
    assert!(error.message.contains("inf"));
}

// ---- decode ---- //

#[test]
fn wrong_byte_length_decode_returns_nil() {
    for format_str in &["f32le", "f32be"] {
        let fmt = s(format_str);
        let encoded = bytes(&[0, 0, 0, 0, 0, 0, 0, 0]); // 8 bytes for f32
        assert!(matches!(
            float_decode(&[encoded, fmt], SPAN),
            Ok(Ok(Value::Nil))
        ));
    }
    for format_str in &["f64le", "f64be"] {
        let fmt = s(format_str);
        let encoded = bytes(&[0, 0, 0, 0]); // 4 bytes for f64
        assert!(matches!(
            float_decode(&[encoded, fmt], SPAN),
            Ok(Ok(Value::Nil))
        ));
    }
}

#[test]
fn decode_returns_special_strings_for_ieee_nonfinite() {
    // f32 infinity LE: 0x00007f7f (big-endian bytes: 7f 80 00 00)
    let inf_bytes_le = bytes(&[0x00, 0x00, 0x80, 0x7f]);
    let decoded = result(float_decode(&[inf_bytes_le, s("f32le")], SPAN));
    assert!(matches!(&decoded, Value::String(s) if s == "inf"));

    // -inf f32 LE
    let neg_inf_le = bytes(&[0x00, 0x00, 0x80, 0xff]);
    let decoded = result(float_decode(&[neg_inf_le, s("f32le")], SPAN));
    assert!(matches!(&decoded, Value::String(s) if s == "-inf"));

    // canonical NaN f32 LE
    let nan_le = bytes(&[0x00, 0x00, 0xc0, 0x7f]);
    let decoded = result(float_decode(&[nan_le, s("f32le")], SPAN));
    assert!(matches!(&decoded, Value::String(s) if s == "nan"));

    // silent NaN f32 LE (different payload)
    let snan_le = bytes(&[0x00, 0x00, 0xc0, 0xff]);
    let decoded = result(float_decode(&[snan_le, s("f32le")], SPAN));
    assert!(matches!(&decoded, Value::String(s) if s == "nan"));

    // f64 infinity LE
    let inf64_le = bytes(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x7f]);
    let decoded = result(float_decode(&[inf64_le, s("f64le")], SPAN));
    assert!(matches!(&decoded, Value::String(s) if s == "inf"));

    // f64 -inf LE
    let neg_inf64_le = bytes(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0xff]);
    let decoded = result(float_decode(&[neg_inf64_le, s("f64le")], SPAN));
    assert!(matches!(&decoded, Value::String(s) if s == "-inf"));

    // f64 NaN LE
    let nan64_le = bytes(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf8, 0x7f]);
    let decoded = result(float_decode(&[nan64_le, s("f64le")], SPAN));
    assert!(matches!(&decoded, Value::String(s) if s == "nan"));
}

#[test]
fn big_endian_decode_correct() {
    // f32 PI ~3.14 in BE
    let pi_f32_be = bytes(&[0x40, 0x48, 0xf5, 0xc3]); // 3.14...
    let decoded = result(float_decode(&[pi_f32_be, s("f32be")], SPAN));
    assert!(matches!(&decoded, Value::Float(v) if (*v - std::f32::consts::PI as f64).abs() < 0.01));

    // f32 inf BE
    let inf_be = bytes(&[0x7f, 0x80, 0x00, 0x00]);
    let decoded = result(float_decode(&[inf_be, s("f32be")], SPAN));
    assert!(matches!(&decoded, Value::String(s) if s == "inf"));

    // f32 -inf BE
    let neg_inf_be = bytes(&[0xff, 0x80, 0x00, 0x00]);
    let decoded = result(float_decode(&[neg_inf_be, s("f32be")], SPAN));
    assert!(matches!(&decoded, Value::String(s) if s == "-inf"));

    // f64 PI BE
    let pi_f64_be = bytes(&[0x40, 0x09, 0x21, 0xfb, 0x54, 0x44, 0x2d, 0x18]);
    let decoded = result(float_decode(&[pi_f64_be, s("f64be")], SPAN));
    assert!(matches!(&decoded, Value::Float(v) if (*v - std::f64::consts::PI).abs() < 1e-14));
}

#[test]
fn decode_empty_bytes_returns_nil() {
    assert!(matches!(
        float_decode(&[bytes(&[]), s("f32le")], SPAN),
        Ok(Ok(Value::Nil))
    ));
    assert!(matches!(
        float_decode(&[bytes(&[]), s("f64le")], SPAN),
        Ok(Ok(Value::Nil))
    ));
}

// ---- diagnostics ---- //

#[test]
fn wrong_arity_hard_error() {
    let error = hard_error(float_encode(&[], SPAN));
    assert!(
        error.message.starts_with("std/float.encode"),
        "{}",
        error.message
    );
    assert!(error.message.contains("expects 2"), "{}", error.message);

    let error = hard_error(float_decode(&[f(1.0)], SPAN));
    assert!(
        error.message.starts_with("std/float.decode"),
        "{}",
        error.message
    );
    assert!(error.message.contains("expects 2"), "{}", error.message);
}

#[test]
fn wrong_category_hard_errors() {
    // encode with wrong first arg
    let error = hard_error(float_encode(&[Value::Int(1), s("f64le")], SPAN));
    assert!(error.message.starts_with("std/float.encode"));
    assert!(error.message.contains("integer"));

    // decode with wrong first arg
    let error = hard_error(float_decode(&[Value::Int(1), s("f64le")], SPAN));
    assert!(error.message.starts_with("std/float.decode"));
    assert!(error.message.contains("bytes"));

    // unknown format for encode
    let error = hard_error(float_encode(&[f(1.0), s("f16le")], SPAN));
    assert!(error.message.starts_with("std/float.encode"));
    assert!(error.message.contains("f16le"));

    // format is not a string
    let error = hard_error(float_encode(&[f(1.0), Value::Int(1)], SPAN));
    assert!(error.message.starts_with("std/float.encode"));
    assert!(error.message.contains("integer"));

    let error = hard_error(float_decode(&[bytes(&[0, 0, 0, 0]), Value::Int(1)], SPAN));
    assert!(error.message.starts_with("std/float.decode"));
}

#[test]
fn all_diagnostics_use_qualified_std_float() {
    let cases: Vec<NativeResult> = vec![
        float_encode(&[], SPAN),
        float_encode(&[Value::Int(1), s("f64le")], SPAN),
        float_encode(&[s("garbage"), s("f64le")], SPAN),
        float_encode(&[f(1.0), Value::Int(1)], SPAN),
        float_decode(&[], SPAN),
        float_decode(&[f(1.0), s("f64le")], SPAN),
        float_decode(&[bytes(&[1, 2, 3]), Value::Int(1)], SPAN),
    ];
    for case in cases {
        let error = hard_error(case);
        assert!(
            error.message.starts_with("std/float."),
            "expected std/float prefix: {}",
            error.message,
        );
    }
}

#[test]
fn nil_return_cases() {
    // f64 narrowing of really large value to f32 returns nil
    assert!(matches!(
        float_encode(&[f(f64::MAX), s("f32be")], SPAN),
        Ok(Ok(Value::Nil))
    ));

    // decode wrong length returns nil (not error)
    assert!(matches!(
        float_decode(&[bytes(&[0, 0, 0, 0, 0, 0]), s("f32le")], SPAN),
        Ok(Ok(Value::Nil))
    ));
}

#[test]
fn zero_length_round_trips() {
    for format_str in &["f32le", "f32be", "f64le", "f64be"] {
        let encoded = result(float_encode(&[f(0.0), s(format_str)], SPAN));
        // 0.0 should have all zero bits
        if let Value::Bytes(b) = &encoded {
            assert!(
                b.as_slice().iter().all(|&x| x == 0),
                "0.0 in {format_str}: got {encoded}",
                encoded = encoded.render(),
            );
        } else {
            panic!("expected bytes for 0.0 in {format_str}");
        }

        // negative zero should have sign bit set
        let encoded_neg = result(float_encode(&[f(-0.0), s(format_str)], SPAN));
        if let Value::Bytes(b) = &encoded_neg {
            match format_str {
                &"f64le" | &"f32le" => assert_eq!(b.as_slice().last(), Some(&0x80)),
                &"f64be" | &"f32be" => assert_eq!(b.as_slice().first(), Some(&0x80)),
                _ => unreachable!(),
            }
        } else {
            panic!("expected bytes for -0.0 in {format_str}");
        }
    }
}
