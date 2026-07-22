use crate::runtime::{NativeResult, Value};
use crate::span::Span;

pub(crate) fn global_type(args: &[Value], _: Span) -> NativeResult {
    Ok(Ok(Value::String(args[0].type_name().to_owned())))
}

pub(crate) fn global_inspect(args: &[Value], _: Span) -> NativeResult {
    Ok(Ok(Value::String(args[0].render())))
}
