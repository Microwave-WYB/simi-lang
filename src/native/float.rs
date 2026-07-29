use crate::runtime::{Bytes, NativeResult, RuntimeError, RuntimeResult, Value};
use crate::span::Span;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FloatFormat {
    F32Le,
    F32Be,
    F64Le,
    F64Be,
}

impl FloatFormat {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "f32le" => Some(Self::F32Le),
            "f32be" => Some(Self::F32Be),
            "f64le" => Some(Self::F64Le),
            "f64be" => Some(Self::F64Be),
            _ => None,
        }
    }

    fn byte_count(self) -> usize {
        match self {
            Self::F32Le | Self::F32Be => 4,
            Self::F64Le | Self::F64Be => 8,
        }
    }
}

pub fn float_encode(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "encode", span)?;
    let format = expect_format(&args[1], "encode", span)?;
    let bytes = match &args[0] {
        Value::Float(value) => encode_finite(*value, format, span)?,
        Value::String(name) => encode_special(name, format, span)?,
        value => {
            return Err(RuntimeError::new(
                span,
                format!(
                    "std/float.encode first argument must be a float or one of the exact strings \"inf\", \"-inf\", \"nan\", got {}",
                    value.type_name()
                ),
            ));
        }
    };
    Ok(Ok(bytes))
}

pub fn float_decode(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "decode", span)?;
    let bytes = expect_bytes(&args[0], span)?;
    let format = expect_format(&args[1], "decode", span)?;
    if bytes.len() != format.byte_count() {
        return Ok(Ok(Value::Nil));
    }
    Ok(Ok(decode_bytes(bytes.as_slice(), format)))
}

fn encode_finite(value: f64, format: FloatFormat, _span: Span) -> RuntimeResult<Value> {
    match format {
        FloatFormat::F32Le | FloatFormat::F32Be => {
            let narrowed = value as f32;
            if !narrowed.is_finite() {
                return Ok(Value::Nil);
            }
            let bits = match format {
                FloatFormat::F32Le => narrowed.to_le_bytes(),
                FloatFormat::F32Be => narrowed.to_be_bytes(),
                _ => unreachable!(),
            };
            Ok(Value::Bytes(Bytes::new(bits.to_vec())))
        }
        FloatFormat::F64Le | FloatFormat::F64Be => {
            let bits = match format {
                FloatFormat::F64Le => value.to_le_bytes(),
                FloatFormat::F64Be => value.to_be_bytes(),
                _ => unreachable!(),
            };
            Ok(Value::Bytes(Bytes::new(bits.to_vec())))
        }
    }
}

fn encode_special(name: &str, format: FloatFormat, span: Span) -> RuntimeResult<Value> {
    match format {
        FloatFormat::F32Le | FloatFormat::F32Be => {
            let f: f32 = match name {
                "inf" => f32::INFINITY,
                "-inf" => f32::NEG_INFINITY,
                "nan" => f32::NAN,
                other => {
                    return Err(RuntimeError::new(
                        span,
                        format!(
                            "std/float.encode expected a float or one of \"inf\", \"-inf\", \"nan\", got \"{other}\"",
                        ),
                    ));
                }
            };
            let bits = match format {
                FloatFormat::F32Le => f.to_le_bytes(),
                FloatFormat::F32Be => f.to_be_bytes(),
                _ => unreachable!(),
            };
            Ok(Value::Bytes(Bytes::new(bits.to_vec())))
        }
        FloatFormat::F64Le | FloatFormat::F64Be => {
            let f: f64 = match name {
                "inf" => f64::INFINITY,
                "-inf" => f64::NEG_INFINITY,
                "nan" => f64::NAN,
                other => {
                    return Err(RuntimeError::new(
                        span,
                        format!(
                            "std/float.encode expected a float or one of \"inf\", \"-inf\", \"nan\", got \"{other}\"",
                        ),
                    ));
                }
            };
            let bits = match format {
                FloatFormat::F64Le => f.to_le_bytes(),
                FloatFormat::F64Be => f.to_be_bytes(),
                _ => unreachable!(),
            };
            Ok(Value::Bytes(Bytes::new(bits.to_vec())))
        }
    }
}

fn decode_bytes(bytes: &[u8], format: FloatFormat) -> Value {
    match format {
        FloatFormat::F32Le => decode_f32(&array4(bytes)),
        FloatFormat::F32Be => decode_f32(&array4_be(bytes)),
        FloatFormat::F64Le => decode_f64(&array8(bytes)),
        FloatFormat::F64Be => decode_f64(&array8_be(bytes)),
    }
}

fn array4(bytes: &[u8]) -> [u8; 4] {
    [bytes[0], bytes[1], bytes[2], bytes[3]]
}

fn array4_be(bytes: &[u8]) -> [u8; 4] {
    [bytes[3], bytes[2], bytes[1], bytes[0]]
}

fn array8(bytes: &[u8]) -> [u8; 8] {
    [
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]
}

fn array8_be(bytes: &[u8]) -> [u8; 8] {
    [
        bytes[7], bytes[6], bytes[5], bytes[4], bytes[3], bytes[2], bytes[1], bytes[0],
    ]
}

fn decode_f32(bits: &[u8; 4]) -> Value {
    let value = f32::from_le_bytes(*bits);
    float32_to_wire(value)
}

fn decode_f64(bits: &[u8; 8]) -> Value {
    let value = f64::from_le_bytes(*bits);
    float64_to_wire(value)
}

fn float32_to_wire(value: f32) -> Value {
    if value.is_finite() {
        // Preserve signed zero via the float value itself.
        Value::Float(value as f64)
    } else if value.is_infinite() {
        if value.is_sign_positive() {
            Value::String("inf".to_owned())
        } else {
            Value::String("-inf".to_owned())
        }
    } else {
        Value::String("nan".to_owned())
    }
}

fn float64_to_wire(value: f64) -> Value {
    if value.is_finite() {
        Value::Float(value)
    } else if value.is_infinite() {
        if value.is_sign_positive() {
            Value::String("inf".to_owned())
        } else {
            Value::String("-inf".to_owned())
        }
    } else {
        Value::String("nan".to_owned())
    }
}

fn expect_arity(args: &[Value], expected: usize, name: &str, span: Span) -> RuntimeResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!(
                "std/float.{name} expects {expected} arguments, got {}",
                args.len()
            ),
        ))
    }
}

fn expect_format(value: &Value, caller: &str, span: Span) -> RuntimeResult<FloatFormat> {
    match value {
        Value::String(name) => FloatFormat::parse(name).ok_or_else(|| {
            RuntimeError::new(
                span,
                format!(
                    "std/float.{caller} format must be one of \"f32le\", \"f32be\", \"f64le\", \"f64be\", got \"{name}\""
                ),
            )
        }),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/float.{caller} format must be a string, got {}",
                value.type_name()
            ),
        )),
    }
}

fn expect_bytes(value: &Value, span: Span) -> RuntimeResult<&Bytes> {
    match value {
        Value::Bytes(bytes) => Ok(bytes),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/float.decode first argument must be bytes, got {}",
                value.type_name()
            ),
        )),
    }
}

#[cfg(test)]
mod tests;
