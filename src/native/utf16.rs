#![allow(dead_code)]

use crate::runtime::{Bytes, NativeResult, RuntimeError, RuntimeResult, Value};
use crate::span::Span;

pub fn utf16_encode_le(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "encode_le", span)?;
    let text = expect_string(&args[0], "encode_le", "text", span)?;
    let units: Vec<u16> = text.encode_utf16().collect();
    let bytes = u16_slice_to_bytes_le(&units);
    Ok(Ok(Value::Bytes(Bytes::new(bytes))))
}

pub fn utf16_encode_be(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "encode_be", span)?;
    let text = expect_string(&args[0], "encode_be", "text", span)?;
    let units: Vec<u16> = text.encode_utf16().collect();
    let bytes = u16_slice_to_bytes_be(&units);
    Ok(Ok(Value::Bytes(Bytes::new(bytes))))
}

pub fn utf16_decode_le(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "decode_le", span)?;
    let bytes = expect_bytes(&args[0], "decode_le", "data", span)?;
    decode_utf16_from_bytes(bytes.as_slice(), u16_from_bytes_le)
}

pub fn utf16_decode_be(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "decode_be", span)?;
    let bytes = expect_bytes(&args[0], "decode_be", "data", span)?;
    decode_utf16_from_bytes(bytes.as_slice(), u16_from_bytes_be)
}

fn decode_utf16_from_bytes<F>(raw: &[u8], to_units: F) -> NativeResult
where
    F: Fn(&[u8]) -> Option<Vec<u16>>,
{
    let units = match to_units(raw) {
        Some(units) => units,
        None => return Ok(Ok(Value::Nil)),
    };
    match String::from_utf16(&units) {
        Ok(text) => Ok(Ok(Value::String(text))),
        Err(_) => Ok(Ok(Value::Nil)),
    }
}

fn u16_from_bytes_le(raw: &[u8]) -> Option<Vec<u16>> {
    if !raw.len().is_multiple_of(2) {
        return None;
    }
    let mut units = Vec::with_capacity(raw.len() / 2);
    for chunk in raw.chunks_exact(2) {
        let value = u16::from_le_bytes([chunk[0], chunk[1]]);
        units.push(value);
    }
    Some(units)
}

fn u16_from_bytes_be(raw: &[u8]) -> Option<Vec<u16>> {
    if !raw.len().is_multiple_of(2) {
        return None;
    }
    let mut units = Vec::with_capacity(raw.len() / 2);
    for chunk in raw.chunks_exact(2) {
        let value = u16::from_be_bytes([chunk[0], chunk[1]]);
        units.push(value);
    }
    Some(units)
}

fn u16_slice_to_bytes_le(units: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(units.len() * 2);
    for unit in units {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn u16_slice_to_bytes_be(units: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(units.len() * 2);
    for unit in units {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

fn expect_arity(args: &[Value], expected: usize, name: &str, span: Span) -> RuntimeResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!(
                "std/utf16.{name} expects {expected} argument{plural}, got {got}",
                plural = if expected == 1 { "" } else { "s" },
                got = args.len()
            ),
        ))
    }
}

fn expect_string<'a>(
    value: &'a Value,
    name: &str,
    argument: &str,
    span: Span,
) -> RuntimeResult<&'a str> {
    match value {
        Value::String(text) => Ok(text),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/utf16.{name} {argument} must be a string, got {}",
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
                "std/utf16.{name} {argument} must be bytes, got {}",
                value.type_name()
            ),
        )),
    }
}

#[cfg(test)]
mod tests;
