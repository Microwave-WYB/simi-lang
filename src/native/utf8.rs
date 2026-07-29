#![allow(dead_code)]

use crate::runtime::{Bytes, NativeResult, RuntimeError, RuntimeResult, Value};
use crate::span::Span;

pub fn utf8_encode(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "encode", span)?;
    let text = expect_string(&args[0], "encode", "text", span)?;
    Ok(Ok(Value::Bytes(Bytes::new(text.as_bytes().to_vec()))))
}

pub fn utf8_decode(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "decode", span)?;
    let bytes = expect_bytes(&args[0], "decode", "data", span)?;
    match std::str::from_utf8(bytes.as_slice()) {
        Ok(text) => Ok(Ok(Value::String(text.to_owned()))),
        Err(_) => Ok(Ok(Value::Nil)),
    }
}

fn expect_arity(args: &[Value], expected: usize, name: &str, span: Span) -> RuntimeResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!(
                "std/utf8.{name} expects {expected} argument{plural}, got {got}",
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
                "std/utf8.{name} {argument} must be a string, got {}",
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
                "std/utf8.{name} {argument} must be bytes, got {}",
                value.type_name()
            ),
        )),
    }
}

#[cfg(test)]
mod tests;
