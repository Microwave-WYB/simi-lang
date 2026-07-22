use crate::span::Span;

use super::{FloatKey, RuntimeError, RuntimeResult, TableKey, Value};

impl TableKey {
    pub fn from_value(value: Value, span: Span) -> RuntimeResult<Self> {
        match value {
            Value::Int(value) => Ok(Self::Int(value)),
            Value::Float(value) => float_key(value, span),
            Value::String(value) => Ok(Self::String(value)),
            Value::Bool(value) => Ok(Self::Bool(value)),
            value => Err(RuntimeError::new(
                span,
                format!(
                    "table key must be a string, integer, float, or boolean, got {}",
                    value.type_name()
                ),
            )),
        }
    }
}

fn float_key(value: f64, span: Span) -> RuntimeResult<TableKey> {
    if !value.is_finite() {
        return Err(RuntimeError::new(span, "table key must be finite"));
    }
    if value == 0.0 {
        return Ok(TableKey::Int(0));
    }
    const I64_MIN_F64: f64 = -9_223_372_036_854_775_808.0;
    const I64_END_F64: f64 = 9_223_372_036_854_775_808.0;
    if (I64_MIN_F64..I64_END_F64).contains(&value) {
        let integer = value as i64;
        if integer as f64 == value {
            return Ok(TableKey::Int(integer));
        }
    }
    let key =
        FloatKey::new(value).ok_or_else(|| RuntimeError::new(span, "table key must be finite"))?;
    Ok(TableKey::Float(key))
}
