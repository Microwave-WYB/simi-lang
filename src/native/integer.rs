use crate::runtime::{Bytes, NativeResult, RuntimeError, RuntimeResult, Value};
use crate::span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Signedness {
    Signed,
    Unsigned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Endian {
    Little,
    Big,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Format {
    signedness: Signedness,
    width: usize,
    endian: Endian,
}

fn parse_format(name: &str, format_str: &str, span: Span) -> RuntimeResult<Format> {
    match format_str {
        "i8le" => Ok(Format {
            signedness: Signedness::Signed,
            width: 1,
            endian: Endian::Little,
        }),
        "i8be" => Ok(Format {
            signedness: Signedness::Signed,
            width: 1,
            endian: Endian::Big,
        }),
        "u8le" => Ok(Format {
            signedness: Signedness::Unsigned,
            width: 1,
            endian: Endian::Little,
        }),
        "u8be" => Ok(Format {
            signedness: Signedness::Unsigned,
            width: 1,
            endian: Endian::Big,
        }),
        "i16le" => Ok(Format {
            signedness: Signedness::Signed,
            width: 2,
            endian: Endian::Little,
        }),
        "i16be" => Ok(Format {
            signedness: Signedness::Signed,
            width: 2,
            endian: Endian::Big,
        }),
        "u16le" => Ok(Format {
            signedness: Signedness::Unsigned,
            width: 2,
            endian: Endian::Little,
        }),
        "u16be" => Ok(Format {
            signedness: Signedness::Unsigned,
            width: 2,
            endian: Endian::Big,
        }),
        "i32le" => Ok(Format {
            signedness: Signedness::Signed,
            width: 4,
            endian: Endian::Little,
        }),
        "i32be" => Ok(Format {
            signedness: Signedness::Signed,
            width: 4,
            endian: Endian::Big,
        }),
        "u32le" => Ok(Format {
            signedness: Signedness::Unsigned,
            width: 4,
            endian: Endian::Little,
        }),
        "u32be" => Ok(Format {
            signedness: Signedness::Unsigned,
            width: 4,
            endian: Endian::Big,
        }),
        "i64le" => Ok(Format {
            signedness: Signedness::Signed,
            width: 8,
            endian: Endian::Little,
        }),
        "i64be" => Ok(Format {
            signedness: Signedness::Signed,
            width: 8,
            endian: Endian::Big,
        }),
        "u64le" => Ok(Format {
            signedness: Signedness::Unsigned,
            width: 8,
            endian: Endian::Little,
        }),
        "u64be" => Ok(Format {
            signedness: Signedness::Unsigned,
            width: 8,
            endian: Endian::Big,
        }),
        _ => Err(RuntimeError::new(
            span,
            format!("std/integer.{name} unknown format {format_str:?}"),
        )),
    }
}

pub fn integer_encode(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "encode", span)?;
    let value = expect_integer(&args[0], "encode", "value", span)?;
    let format_str = expect_string(&args[1], "encode", "format", span)?;
    let format = parse_format("encode", format_str, span)?;

    let max: i64 = match format.signedness {
        Signedness::Signed => match format.width {
            1 => i8::MAX as i64,
            2 => i16::MAX as i64,
            4 => i32::MAX as i64,
            8 => i64::MAX,
            _ => unreachable!(),
        },
        Signedness::Unsigned => match format.width {
            1 => u8::MAX as i64,
            2 => u16::MAX as i64,
            4 => u32::MAX as i64,
            8 => i64::MAX,
            _ => unreachable!(),
        },
    };
    let min: i64 = match format.signedness {
        Signedness::Signed => match format.width {
            1 => i8::MIN as i64,
            2 => i16::MIN as i64,
            4 => i32::MIN as i64,
            8 => i64::MIN,
            _ => unreachable!(),
        },
        Signedness::Unsigned => 0,
    };

    if value < min || value > max {
        return Err(RuntimeError::new(
            span,
            format!(
                "std/integer.encode value {value} is out of range for {}",
                format_str
            ),
        ));
    }

    let raw: u64 = value as u64;
    let mut bytes = Vec::with_capacity(format.width);
    match format.endian {
        Endian::Little => {
            for i in 0..format.width {
                bytes.push(((raw >> (i * 8)) & 0xFF) as u8);
            }
        }
        Endian::Big => {
            for i in (0..format.width).rev() {
                bytes.push(((raw >> (i * 8)) & 0xFF) as u8);
            }
        }
    }

    Ok(Ok(Value::Bytes(Bytes::new(bytes))))
}

pub fn integer_decode(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "decode", span)?;
    let bytes = expect_bytes(&args[0], "decode", "data", span)?;
    let format_str = expect_string(&args[1], "decode", "format", span)?;
    let format = parse_format("decode", format_str, span)?;

    if bytes.len() != format.width {
        return Ok(Ok(Value::Nil));
    }

    let raw: u64 = match format.endian {
        Endian::Little => {
            let mut raw: u64 = 0;
            for (i, byte) in bytes.as_slice().iter().enumerate() {
                raw |= u64::from(*byte) << (i * 8);
            }
            raw
        }
        Endian::Big => {
            let mut raw: u64 = 0;
            for (i, byte) in bytes.as_slice().iter().enumerate() {
                raw |= u64::from(*byte) << ((format.width - 1 - i) * 8);
            }
            raw
        }
    };

    let value: i64 = match format.signedness {
        Signedness::Signed => match format.width {
            1 => i64::from(raw as i8),
            2 => i64::from(raw as i16),
            4 => i64::from(raw as i32),
            8 => raw as i64,
            _ => unreachable!(),
        },
        Signedness::Unsigned => {
            if raw > i64::MAX as u64 {
                return Err(RuntimeError::new(
                    span,
                    format!(
                        "std/integer.decode decoded value {raw} exceeds i64 for {}",
                        format_str
                    ),
                ));
            }
            raw as i64
        }
    };

    Ok(Ok(Value::Int(value)))
}

fn expect_arity(args: &[Value], expected: usize, name: &str, span: Span) -> RuntimeResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!(
                "std/integer.{name} expects {expected} arguments, got {}",
                args.len()
            ),
        ))
    }
}

fn expect_integer(value: &Value, name: &str, argument: &str, span: Span) -> RuntimeResult<i64> {
    match value {
        Value::Int(value) => Ok(*value),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/integer.{name} {argument} must be an integer, got {}",
                value.type_name()
            ),
        )),
    }
}

fn expect_string<'a>(
    value: &'a Value,
    name: &str,
    argument: &str,
    span: Span,
) -> RuntimeResult<&'a str> {
    match value {
        Value::String(s) => Ok(s),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/integer.{name} {argument} must be a string, got {}",
                value.type_name()
            ),
        )),
    }
}

fn expect_bytes<'a>(
    value: &'a Value,
    name: &str,
    argument: &str,
    span: Span,
) -> RuntimeResult<&'a Bytes> {
    match value {
        Value::Bytes(bytes) => Ok(bytes),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/integer.{name} {argument} must be bytes, got {}",
                value.type_name()
            ),
        )),
    }
}

#[cfg(test)]
mod tests;
