use crate::runtime::{Bytes, List, NativeResult, RuntimeError, RuntimeResult, SharedList, Value};
use crate::span::Span;

pub fn bytes_length(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "length", span)?;
    let bytes = expect_bytes(&args[0], "length", "data", span)?;
    let length = i64::try_from(bytes.len())
        .map_err(|_| RuntimeError::new(span, "std/bytes.length result exceeds i64"))?;
    Ok(Ok(Value::Int(length)))
}

pub fn bytes_get(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "get", span)?;
    let bytes = expect_bytes(&args[0], "get", "data", span)?;
    let index = expect_index(&args[1], "get", "index", span)?;
    Ok(Ok(bytes
        .get(index)
        .map_or(Value::Nil, |value| Value::Int(value.into()))))
}

pub fn bytes_slice(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 3, "slice", span)?;
    let bytes = expect_bytes(&args[0], "slice", "data", span)?;
    let start = expect_index(&args[1], "slice", "start", span)?;
    let end = expect_index(&args[2], "slice", "stop", span)?;
    let length = bytes.len();
    let start = start.min(length);
    let end = end.min(length).max(start);
    Ok(Ok(Value::Bytes(bytes.slice(start, end).expect(
        "clamped bytes slice is within its visible range",
    ))))
}

pub fn bytes_concat(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "concat", span)?;
    let left = expect_bytes(&args[0], "concat", "left", span)?;
    let right = expect_bytes(&args[1], "concat", "right", span)?;
    let capacity = left
        .len()
        .checked_add(right.len())
        .ok_or_else(|| RuntimeError::new(span, "std/bytes.concat result is too large"))?;
    let mut values = Vec::with_capacity(capacity);
    values.extend_from_slice(left.as_slice());
    values.extend_from_slice(right.as_slice());
    Ok(Ok(Value::Bytes(Bytes::new(values))))
}

pub fn bytes_from_list(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "from_list", span)?;
    let list = expect_list(&args[0], "from_list", "values", span)?;
    let values = list
        .try_borrow()
        .map_err(|_| borrow_error("from_list", span))?;
    let bytes = values.with_visible(|values| {
        for (index, value) in values.iter().enumerate() {
            expect_byte(value, index, span)?;
        }
        Ok(values
            .iter()
            .map(|value| match value {
                Value::Int(value) => *value as u8,
                _ => unreachable!("byte values were validated before construction"),
            })
            .collect())
    })?;
    Ok(Ok(Value::Bytes(Bytes::new(bytes))))
}

pub fn bytes_to_list(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "to_list", span)?;
    let bytes = expect_bytes(&args[0], "to_list", "data", span)?;
    let values = bytes
        .as_slice()
        .iter()
        .map(|value| Value::Int((*value).into()))
        .collect();
    Ok(Ok(Value::List(List::shared(values))))
}

fn expect_arity(args: &[Value], expected: usize, name: &str, span: Span) -> RuntimeResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!(
                "std/bytes.{name} expects {expected} arguments, got {}",
                args.len()
            ),
        ))
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
                "std/bytes.{name} {argument} must be bytes, got {}",
                value.type_name()
            ),
        )),
    }
}

fn expect_list(value: &Value, name: &str, argument: &str, span: Span) -> RuntimeResult<SharedList> {
    match value {
        Value::List(list) => Ok(list.clone()),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/bytes.{name} {argument} must be a list, got {}",
                value.type_name()
            ),
        )),
    }
}

fn expect_index(value: &Value, name: &str, argument: &str, span: Span) -> RuntimeResult<usize> {
    match value {
        Value::Int(index) if *index >= 0 => usize::try_from(*index).map_err(|_| {
            RuntimeError::new(
                span,
                format!("std/bytes.{name} {argument} index is too large"),
            )
        }),
        Value::Int(index) => Err(RuntimeError::new(
            span,
            format!("std/bytes.{name} {argument} index must be nonnegative, got {index}"),
        )),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/bytes.{name} {argument} index must be an integer, got {}",
                value.type_name()
            ),
        )),
    }
}

fn expect_byte(value: &Value, index: usize, span: Span) -> RuntimeResult<()> {
    match value {
        Value::Int(value) if (0..=255).contains(value) => Ok(()),
        Value::Int(value) => Err(RuntimeError::new(
            span,
            format!("std/bytes.from_list values[{index}] must be between 0 and 255, got {value}"),
        )),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/bytes.from_list values[{index}] must be an integer, got {}",
                value.type_name()
            ),
        )),
    }
}

fn borrow_error(name: &str, span: Span) -> RuntimeError {
    RuntimeError::new(span, format!("std/bytes.{name} could not borrow list"))
}

#[cfg(test)]
mod tests;
