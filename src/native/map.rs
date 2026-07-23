use gc::{Gc, GcCell};

use crate::runtime::{List, MapKey, NativeResult, RuntimeError, RuntimeResult, SharedMap, Value};
use crate::span::Span;

pub fn map_length(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "length", span)?;
    let map = expect_map(&args[0], "length", span)?;
    let entries = map.try_borrow().map_err(|_| borrow_error("length", span))?;
    let length = i64::try_from(entries.len())
        .map_err(|_| RuntimeError::new(span, "map length exceeds i64"))?;
    Ok(Ok(Value::Int(length)))
}

pub fn map_copy(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "copy", span)?;
    let map = expect_map(&args[0], "copy", span)?;
    let entries = map.try_borrow().map_err(|_| borrow_error("copy", span))?;
    Ok(Ok(Value::Map(Gc::new(GcCell::new(entries.clone())))))
}

pub fn map_has(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "has", span)?;
    let map = expect_map(&args[0], "has", span)?;
    let key = expect_key(&args[1], "has", span)?;
    let entries = map.try_borrow().map_err(|_| borrow_error("has", span))?;
    Ok(Ok(Value::Bool(entries.iter().any(|(entry_key, value)| {
        entry_key == &key && !matches!(value, Value::Nil)
    }))))
}

pub fn map_keys(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "keys", span)?;
    let map = expect_map(&args[0], "keys", span)?;
    let entries = map.try_borrow().map_err(|_| borrow_error("keys", span))?;
    let keys = entries.iter().map(|(key, _)| key_to_value(key)).collect();
    Ok(Ok(Value::List(List::shared(keys))))
}

pub fn map_values(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "values", span)?;
    let map = expect_map(&args[0], "values", span)?;
    let entries = map.try_borrow().map_err(|_| borrow_error("values", span))?;
    let values = entries.iter().map(|(_, value)| value.clone()).collect();
    Ok(Ok(Value::List(List::shared(values))))
}

pub fn map_entries(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "entries", span)?;
    let map = expect_map(&args[0], "entries", span)?;
    let entries = map
        .try_borrow()
        .map_err(|_| borrow_error("entries", span))?;
    let pairs = entries
        .iter()
        .map(|(key, value)| Value::List(List::shared(vec![key_to_value(key), value.clone()])))
        .collect();
    Ok(Ok(Value::List(List::shared(pairs))))
}

pub fn map_iter(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "iter", span)?;
    let map = expect_map(&args[0], "iter", span)?;
    let entries = map.try_borrow().map_err(|_| borrow_error("iter", span))?;
    let values = entries
        .iter()
        .map(|(key, value)| {
            let ordered = vec![
                (MapKey::String("key".into()), key_to_value(key)),
                (MapKey::String("value".into()), value.clone()),
            ];
            Value::Map(Gc::new(GcCell::new(ordered)))
        })
        .collect();
    Ok(Ok(Value::List(List::shared(values))))
}

pub fn map_clear(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "clear", span)?;
    let map = expect_map(&args[0], "clear", span)?;
    let mut entries = map
        .try_borrow_mut()
        .map_err(|_| borrow_error("clear", span))?;
    entries.clear();
    Ok(Ok(Value::Nil))
}

fn expect_arity(args: &[Value], expected: usize, name: &str, span: Span) -> RuntimeResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!(
                "std/map.{name} expects {expected} arguments, got {}",
                args.len()
            ),
        ))
    }
}

fn expect_map(value: &Value, name: &str, span: Span) -> RuntimeResult<SharedMap> {
    match value {
        Value::Map(map) => Ok(map.clone()),
        value => Err(RuntimeError::new(
            span,
            format!("std/map.{name} requires a map, got {}", value.type_name()),
        )),
    }
}

fn expect_key(value: &Value, name: &str, span: Span) -> RuntimeResult<MapKey> {
    MapKey::from_value(value.clone(), span).map_err(|error| {
        let detail = error
            .message
            .strip_prefix("map key ")
            .unwrap_or(&error.message);
        RuntimeError::new(span, format!("std/map.{name} key {detail}"))
    })
}

fn key_to_value(key: &MapKey) -> Value {
    match key {
        MapKey::Int(value) => Value::Int(*value),
        MapKey::Float(value) => Value::Float(value.value()),
        MapKey::String(value) => Value::String(value.clone()),
        MapKey::Bool(value) => Value::Bool(*value),
    }
}

fn borrow_error(name: &str, span: Span) -> RuntimeError {
    RuntimeError::new(span, format!("std/map.{name} could not borrow map"))
}

#[cfg(test)]
mod tests;
