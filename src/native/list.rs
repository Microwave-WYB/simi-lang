use crate::interpreter::operations::values_equal;
use crate::runtime::{NativeResult, Raised, RuntimeError, RuntimeResult, SharedList, Value};
use crate::span::Span;

pub fn list_length(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "length", span)?;
    let list = expect_list(&args[0], "length", span)?;
    let values = list
        .try_borrow()
        .map_err(|_| borrow_error("length", span))?;
    let length = i64::try_from(values.len())
        .map_err(|_| RuntimeError::new(span, "list length exceeds i64"))?;
    Ok(Ok(Value::Int(length)))
}

pub fn list_copy(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "copy", span)?;
    let list = expect_list(&args[0], "copy", span)?;
    let values = list.try_borrow().map_err(|_| borrow_error("copy", span))?;
    Ok(Ok(Value::List(values.shallow_copy().into_shared())))
}

pub fn list_get(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "get", span)?;
    let list = expect_list(&args[0], "get", span)?;
    let (_, index) = expect_index(&args[1], "get", span)?;
    let values = list.try_borrow().map_err(|_| borrow_error("get", span))?;
    Ok(Ok(values.get_cloned(index).unwrap_or(Value::Nil)))
}

pub fn list_append(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "append", span)?;
    let list = expect_list(&args[0], "append", span)?;
    let mut values = list
        .try_borrow_mut()
        .map_err(|_| borrow_error("append", span))?;
    values.push(args[1].clone());
    Ok(Ok(Value::Nil))
}

pub fn list_extend(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "extend", span)?;
    let destination = expect_list(&args[0], "extend", span)?;
    let source = expect_list(&args[1], "extend", span)?;
    let snapshot = source
        .try_borrow()
        .map_err(|_| borrow_error("extend", span))?
        .to_vec();
    let mut values = destination
        .try_borrow_mut()
        .map_err(|_| borrow_error("extend", span))?;
    values.extend(snapshot);
    Ok(Ok(Value::Nil))
}

pub fn list_set(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 3, "set", span)?;
    let list = expect_list(&args[0], "set", span)?;
    let (raw_index, index) = expect_index(&args[1], "set", span)?;
    let mut values = list
        .try_borrow_mut()
        .map_err(|_| borrow_error("set", span))?;
    let length = values.len();
    if index >= length {
        return Ok(Err(Raised::index_out_of_bounds(raw_index, length, span)?));
    }
    assert!(values.set(index, args[2].clone()));
    Ok(Ok(Value::Nil))
}

pub fn list_insert(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 3, "insert", span)?;
    let list = expect_list(&args[0], "insert", span)?;
    let (raw_index, index) = expect_index(&args[1], "insert", span)?;
    let mut values = list
        .try_borrow_mut()
        .map_err(|_| borrow_error("insert", span))?;
    let length = values.len();
    if index > length {
        return Ok(Err(Raised::index_out_of_bounds(raw_index, length, span)?));
    }
    values.insert(index, args[2].clone());
    Ok(Ok(Value::Nil))
}

pub fn list_remove(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "remove", span)?;
    let list = expect_list(&args[0], "remove", span)?;
    let (raw_index, index) = expect_index(&args[1], "remove", span)?;
    let mut values = list
        .try_borrow_mut()
        .map_err(|_| borrow_error("remove", span))?;
    let length = values.len();
    if index >= length {
        return Ok(Err(Raised::index_out_of_bounds(raw_index, length, span)?));
    }
    Ok(Ok(values.remove(index)))
}

pub fn list_pop(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "pop", span)?;
    let list = expect_list(&args[0], "pop", span)?;
    let mut values = list
        .try_borrow_mut()
        .map_err(|_| borrow_error("pop", span))?;
    let length = values.len();
    if length == 0 {
        return Ok(Err(Raised::index_out_of_bounds(0, 0, span)?));
    }
    Ok(Ok(values.remove(length - 1)))
}

pub fn list_slice(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 3, "slice", span)?;
    let list = expect_list(&args[0], "slice", span)?;
    let (_, start) = expect_index(&args[1], "slice", span)?;
    let (_, end) = expect_index(&args[2], "slice", span)?;
    let values = list.try_borrow().map_err(|_| borrow_error("slice", span))?;
    let length = values.len();
    let start = start.min(length);
    let end = end.min(length).max(start);
    Ok(Ok(Value::List(values.slice(start, end).into_shared())))
}

pub fn list_contains(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 2, "contains", span)?;
    let list = expect_list(&args[0], "contains", span)?;
    let values = list
        .try_borrow()
        .map_err(|_| borrow_error("contains", span))?;
    let needle = &args[1];
    let contains = values.with_visible(|values| {
        for value in values {
            if values_equal(value, needle, span)? {
                return Ok(true);
            }
        }
        Ok(false)
    })?;
    Ok(Ok(Value::Bool(contains)))
}

pub fn list_reverse(args: &[Value], span: Span) -> NativeResult {
    expect_arity(args, 1, "reverse", span)?;
    let list = expect_list(&args[0], "reverse", span)?;
    let mut values = list
        .try_borrow_mut()
        .map_err(|_| borrow_error("reverse", span))?;
    values.reverse();
    Ok(Ok(Value::Nil))
}

fn expect_arity(args: &[Value], expected: usize, name: &str, span: Span) -> RuntimeResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!(
                "std/list.{name} expects {expected} arguments, got {}",
                args.len()
            ),
        ))
    }
}

fn expect_list(value: &Value, name: &str, span: Span) -> RuntimeResult<SharedList> {
    match value {
        Value::List(list) => Ok(list.clone()),
        value => Err(RuntimeError::new(
            span,
            format!("std/list.{name} requires a list, got {}", value.type_name()),
        )),
    }
}

fn expect_index(value: &Value, name: &str, span: Span) -> RuntimeResult<(i64, usize)> {
    match value {
        Value::Int(index) if *index >= 0 => usize::try_from(*index)
            .map(|converted| (*index, converted))
            .map_err(|_| RuntimeError::new(span, format!("std/list.{name} index is too large"))),
        Value::Int(index) => Err(RuntimeError::new(
            span,
            format!("std/list.{name} index must be nonnegative, got {index}"),
        )),
        value => Err(RuntimeError::new(
            span,
            format!(
                "std/list.{name} index must be an integer, got {}",
                value.type_name()
            ),
        )),
    }
}

fn borrow_error(name: &str, span: Span) -> RuntimeError {
    RuntimeError::new(span, format!("std/list.{name} could not borrow list"))
}

#[cfg(test)]
mod tests;
