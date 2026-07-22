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

fn expect_arity(args: &[Value], expected: usize, name: &str, span: Span) -> RuntimeResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!(
                "list.{name} expects {expected} arguments, got {}",
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
            format!("list.{name} requires a list, got {}", value.type_name()),
        )),
    }
}

fn expect_index(value: &Value, name: &str, span: Span) -> RuntimeResult<(i64, usize)> {
    match value {
        Value::Int(index) if *index >= 0 => usize::try_from(*index)
            .map(|converted| (*index, converted))
            .map_err(|_| RuntimeError::new(span, format!("list.{name} index is too large"))),
        Value::Int(index) => Err(RuntimeError::new(
            span,
            format!("list.{name} index must be nonnegative, got {index}"),
        )),
        value => Err(RuntimeError::new(
            span,
            format!(
                "list.{name} index must be an integer, got {}",
                value.type_name()
            ),
        )),
    }
}

fn borrow_error(name: &str, span: Span) -> RuntimeError {
    RuntimeError::new(span, format!("list.{name} could not borrow list"))
}

#[cfg(test)]
mod tests;
