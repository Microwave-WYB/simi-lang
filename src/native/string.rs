use crate::runtime::{List, NativeResult, RuntimeError, RuntimeResult, Value};
use crate::span::Span;

pub fn string_length(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "length", span)?;
    let value = expect_string(&args[0], "length", "value", span)?;
    let length = i64::try_from(value.chars().count())
        .map_err(|_| RuntimeError::new(span, "std/string.length result exceeds i64"))?;
    Ok(Ok(Value::Int(length)))
}

pub fn string_slice(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 3, "slice", span)?;
    let value = expect_string(&args[0], "slice", "value", span)?;
    let start = expect_index(&args[1], "slice", "start", span)?;
    let end = expect_index(&args[2], "slice", "end", span)?;

    if end <= start {
        return Ok(Ok(Value::String(String::new())));
    }

    let result = value.chars().skip(start).take(end - start).collect();
    Ok(Ok(Value::String(result)))
}

pub fn string_contains(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "contains", span)?;
    let value = expect_string(&args[0], "contains", "value", span)?;
    let search = expect_string(&args[1], "contains", "search", span)?;
    Ok(Ok(Value::Bool(value.contains(search))))
}

pub fn string_starts_with(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "starts_with", span)?;
    let value = expect_string(&args[0], "starts_with", "value", span)?;
    let prefix = expect_string(&args[1], "starts_with", "prefix", span)?;
    Ok(Ok(Value::Bool(value.starts_with(prefix))))
}

pub fn string_ends_with(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "ends_with", span)?;
    let value = expect_string(&args[0], "ends_with", "value", span)?;
    let suffix = expect_string(&args[1], "ends_with", "suffix", span)?;
    Ok(Ok(Value::Bool(value.ends_with(suffix))))
}

pub fn string_split(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "split", span)?;
    let value = expect_string(&args[0], "split", "value", span)?;
    let separator = expect_string(&args[1], "split", "separator", span)?;
    let parts = if separator.is_empty() {
        value
            .chars()
            .map(|character| Value::String(character.to_string()))
            .collect()
    } else {
        value
            .split(separator)
            .map(|part| Value::String(part.to_owned()))
            .collect()
    };
    Ok(Ok(Value::List(List::shared(parts))))
}

pub fn string_trim(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "trim", span)?;
    let value = expect_string(&args[0], "trim", "value", span)?;
    Ok(Ok(Value::String(value.trim().to_owned())))
}

pub fn string_lower(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "lower", span)?;
    let value = expect_string(&args[0], "lower", "value", span)?;
    Ok(Ok(Value::String(value.to_lowercase())))
}

pub fn string_upper(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "upper", span)?;
    let value = expect_string(&args[0], "upper", "value", span)?;
    Ok(Ok(Value::String(value.to_uppercase())))
}

fn expect_arity(args: &[Value], expected: usize, name: &str, span: Span) -> RuntimeResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!(
                "std/string.{name} expects {expected} arguments, got {}",
                args.len()
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
        Value::String(value) => Ok(value),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/string.{name} {argument} must be a string, got {}",
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
                format!("std/string.{name} {argument} index is too large"),
            )
        }),
        Value::Int(index) => Err(RuntimeError::new(
            span,
            format!("std/string.{name} {argument} index must be nonnegative, got {index}"),
        )),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/string.{name} {argument} index must be an integer, got {}",
                value.type_name()
            ),
        )),
    }
}

#[cfg(test)]
mod tests;
