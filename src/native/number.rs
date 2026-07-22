use crate::runtime::{NativeResult, RuntimeError, RuntimeResult, Value};
use crate::span::Span;

pub fn number_from_string(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "from_string", span)?;
    let text = expect_string(&args[0], "from_string", span)?;
    Ok(Ok(parse_number(text)))
}

pub fn number_to_string(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "to_string", span)?;
    match &args[0] {
        value @ (Value::Int(_) | Value::Float(_)) => Ok(Ok(Value::String(value.render()))),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/number.to_string value must be an integer or float, got {}",
                value.type_name()
            ),
        )),
    }
}

fn parse_number(text: &str) -> Value {
    let bytes = text.as_bytes();
    let mut position = 0;

    if matches!(bytes.first(), Some(b'+' | b'-')) {
        position += 1;
    }

    let integer_start = position;
    while matches!(bytes.get(position), Some(b'0'..=b'9')) {
        position += 1;
    }
    if position == integer_start {
        return Value::Nil;
    }

    let mut is_float = false;
    if bytes.get(position) == Some(&b'.') {
        is_float = true;
        position += 1;
        let fraction_start = position;
        while matches!(bytes.get(position), Some(b'0'..=b'9')) {
            position += 1;
        }
        if position == fraction_start {
            return Value::Nil;
        }
    }

    if matches!(bytes.get(position), Some(b'e' | b'E')) {
        is_float = true;
        position += 1;
        if matches!(bytes.get(position), Some(b'+' | b'-')) {
            position += 1;
        }
        let exponent_start = position;
        while matches!(bytes.get(position), Some(b'0'..=b'9')) {
            position += 1;
        }
        if position == exponent_start {
            return Value::Nil;
        }
    }

    if position != bytes.len() {
        return Value::Nil;
    }

    if is_float {
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() => Value::Float(value),
            _ => Value::Nil,
        }
    } else {
        text.parse::<i64>().map_or(Value::Nil, Value::Int)
    }
}

fn expect_arity(args: &[Value], expected: usize, name: &str, span: Span) -> RuntimeResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!(
                "std/number.{name} expects {expected} arguments, got {}",
                args.len()
            ),
        ))
    }
}

fn expect_string<'a>(value: &'a Value, name: &str, span: Span) -> RuntimeResult<&'a str> {
    match value {
        Value::String(value) => Ok(value),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/number.{name} text must be a string, got {}",
                value.type_name()
            ),
        )),
    }
}

#[cfg(test)]
mod tests;
