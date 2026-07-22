use std::cmp::Ordering;

use super::{EvaluationError, EvaluationResult, Interpreter};
use crate::ast::{AssignmentTarget, AssignmentTargetKind, BinaryOp, UnaryOp};
use crate::runtime::{
    Environment, Raised, RuntimeError, RuntimeResult, SharedList, SharedTable, TableKey, Value,
};
use crate::span::Span;

pub(super) enum PreparedTarget {
    Variable {
        name: String,
        span: Span,
    },
    List {
        values: SharedList,
        raw_index: i64,
        index: usize,
        span: Span,
    },
    Table {
        entries: SharedTable,
        key: TableKey,
        span: Span,
    },
}

impl Interpreter {
    pub(super) fn read_index(
        &self,
        object: Value,
        key: Value,
        span: Span,
    ) -> EvaluationResult<Value> {
        match object {
            Value::List(values) => {
                let (_, index) = list_index(key, span)?;
                let values = values.try_borrow().map_err(|_| {
                    EvaluationError::Runtime(RuntimeError::new(
                        span,
                        "could not borrow list for indexing",
                    ))
                })?;
                Ok(values.get_cloned(index).unwrap_or(Value::Nil))
            }
            Value::Table(entries) => {
                let key = TableKey::from_value(key, span)?;
                let entries = entries
                    .try_borrow()
                    .map_err(|_| RuntimeError::new(span, "could not borrow table for indexing"))?;
                Ok(entries
                    .iter()
                    .find(|(entry_key, _)| entry_key == &key)
                    .map_or(Value::Nil, |(_, value)| value.clone()))
            }
            value => Err(EvaluationError::Runtime(RuntimeError::new(
                span,
                format!(
                    "indexing requires a list or table, got {}",
                    value.type_name()
                ),
            ))),
        }
    }

    pub(super) fn prepare_assignment_target(
        &mut self,
        target: &AssignmentTarget,
        env: &Environment,
    ) -> EvaluationResult<PreparedTarget> {
        match &target.kind {
            AssignmentTargetKind::Variable(name) => {
                if env.get(name).is_none() {
                    return Err(EvaluationError::Runtime(RuntimeError::new(
                        target.span,
                        format!("cannot assign to undefined name `{name}`"),
                    )));
                }
                Ok(PreparedTarget::Variable {
                    name: name.clone(),
                    span: target.span,
                })
            }
            AssignmentTargetKind::Field { object, name } => {
                let object = self.evaluate_expression(object, env)?;
                self.prepare_index_target(object, Value::String(name.clone()), target.span)
            }
            AssignmentTargetKind::Index { object, key } => {
                let object = self.evaluate_expression(object, env)?;
                let key = self.evaluate_expression(key, env)?;
                self.prepare_index_target(object, key, target.span)
            }
        }
    }

    fn prepare_index_target(
        &self,
        object: Value,
        key: Value,
        span: Span,
    ) -> EvaluationResult<PreparedTarget> {
        match object {
            Value::List(values) => {
                let (raw_index, index) = list_index(key, span)?;
                let length = values
                    .try_borrow()
                    .map_err(|_| RuntimeError::new(span, "could not borrow list for assignment"))?
                    .len();
                if index >= length {
                    return Err(EvaluationError::Raised(Raised::index_out_of_bounds(
                        raw_index, length, span,
                    )?));
                }
                Ok(PreparedTarget::List {
                    values,
                    raw_index,
                    index,
                    span,
                })
            }
            Value::Table(entries) => {
                let key = TableKey::from_value(key, span)?;
                entries.try_borrow().map_err(|_| {
                    RuntimeError::new(span, "could not borrow table for assignment")
                })?;
                Ok(PreparedTarget::Table { entries, key, span })
            }
            value => Err(EvaluationError::Runtime(RuntimeError::new(
                span,
                format!(
                    "assignment target must be a list or table, got {}",
                    value.type_name()
                ),
            ))),
        }
    }

    pub(super) fn commit_assignment(
        &self,
        target: PreparedTarget,
        value: Value,
        env: &Environment,
    ) -> EvaluationResult<Value> {
        match target {
            PreparedTarget::Variable { name, span } => {
                if !env.assign(&name, value.clone()) {
                    return Err(EvaluationError::Runtime(RuntimeError::new(
                        span,
                        format!("cannot assign to undefined name `{name}`"),
                    )));
                }
            }
            PreparedTarget::List {
                values,
                raw_index,
                index,
                span,
            } => {
                let mut values = values
                    .try_borrow_mut()
                    .map_err(|_| RuntimeError::new(span, "could not borrow list for assignment"))?;
                let length = values.len();
                if index >= length {
                    return Err(EvaluationError::Raised(Raised::index_out_of_bounds(
                        raw_index, length, span,
                    )?));
                }
                assert!(values.set(index, value.clone()));
            }
            PreparedTarget::Table { entries, key, span } => {
                let mut entries = entries.try_borrow_mut().map_err(|_| {
                    RuntimeError::new(span, "could not borrow table for assignment")
                })?;
                if matches!(value, Value::Nil) {
                    if let Some(position) = entries
                        .iter()
                        .position(|(existing_key, _)| existing_key == &key)
                    {
                        entries.remove(position);
                    }
                } else if let Some((_, existing)) = entries
                    .iter_mut()
                    .find(|(existing_key, _)| existing_key == &key)
                {
                    *existing = value.clone();
                } else {
                    entries.push((key, value.clone()));
                }
            }
        }
        Ok(value)
    }

    pub(super) fn evaluate_unary(
        &self,
        operator: &UnaryOp,
        value: Value,
        span: Span,
    ) -> RuntimeResult<Value> {
        match (operator, value) {
            (UnaryOp::Negate, Value::Int(value)) => value
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| RuntimeError::new(span, "integer overflow in negation")),
            (UnaryOp::Negate, Value::Float(value)) => finite_float(-value, span),
            (UnaryOp::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
            (UnaryOp::Negate, value) => Err(RuntimeError::new(
                span,
                format!("unary `-` requires a number, got {}", value.type_name()),
            )),
            (UnaryOp::Not, value) => Err(RuntimeError::new(
                span,
                format!("`not` requires a boolean, got {}", value.type_name()),
            )),
        }
    }

    pub(super) fn evaluate_binary(
        &self,
        left: Value,
        operator: &BinaryOp,
        right: Value,
        span: Span,
    ) -> EvaluationResult<Value> {
        let result = match operator {
            BinaryOp::Add => {
                numeric_arithmetic(left, right, "+", span, i64::checked_add, |a, b| a + b)
            }
            BinaryOp::Subtract => {
                numeric_arithmetic(left, right, "-", span, i64::checked_sub, |a, b| a - b)
            }
            BinaryOp::Multiply => {
                numeric_arithmetic(left, right, "*", span, i64::checked_mul, |a, b| a * b)
            }
            BinaryOp::Divide => divide(left, right, span),
            BinaryOp::FloorDivide => floor_divide(left, right, span),
            BinaryOp::Remainder => remainder(left, right, span),
            BinaryOp::Equal => primitive_equal(&left, &right, span).map(Value::Bool),
            BinaryOp::NotEqual => {
                primitive_equal(&left, &right, span).map(|value| Value::Bool(!value))
            }
            BinaryOp::Less => compare_numbers(left, right, "<", span, Ordering::is_lt),
            BinaryOp::LessEqual => compare_numbers(left, right, "<=", span, |ordering| {
                ordering.is_lt() || ordering.is_eq()
            }),
            BinaryOp::Greater => compare_numbers(left, right, ">", span, Ordering::is_gt),
            BinaryOp::GreaterEqual => compare_numbers(left, right, ">=", span, |ordering| {
                ordering.is_gt() || ordering.is_eq()
            }),
            BinaryOp::And | BinaryOp::Or => {
                unreachable!("boolean operators short-circuit in expression evaluation")
            }
        };
        match result {
            Ok(value) => Ok(value),
            Err(NumericError::Runtime(error)) => Err(EvaluationError::Runtime(error)),
            Err(NumericError::DivisionByZero) => {
                Err(EvaluationError::Raised(Raised::division_by_zero(span)))
            }
        }
    }
}

fn list_index(key: Value, span: Span) -> EvaluationResult<(i64, usize)> {
    match key {
        Value::Int(index) if index >= 0 => usize::try_from(index)
            .map(|converted| (index, converted))
            .map_err(|_| {
                EvaluationError::Runtime(RuntimeError::new(span, "list index is too large"))
            }),
        Value::Int(index) => Err(EvaluationError::Runtime(RuntimeError::new(
            span,
            format!("list index must be nonnegative, got {index}"),
        ))),
        value => Err(EvaluationError::Runtime(RuntimeError::new(
            span,
            format!("list index must be an integer, got {}", value.type_name()),
        ))),
    }
}

enum NumericError {
    Runtime(RuntimeError),
    DivisionByZero,
}

type NumericResult<T> = Result<T, NumericError>;

impl From<RuntimeError> for NumericError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}

enum NumericPair {
    Int(i64, i64),
    Float(f64, f64),
}

fn numeric_operands(
    left: Value,
    right: Value,
    operator: &str,
    span: Span,
) -> NumericResult<NumericPair> {
    match (left, right) {
        (Value::Int(left), Value::Int(right)) => Ok(NumericPair::Int(left, right)),
        (Value::Int(left), Value::Float(right)) => Ok(NumericPair::Float(left as f64, right)),
        (Value::Float(left), Value::Int(right)) => Ok(NumericPair::Float(left, right as f64)),
        (Value::Float(left), Value::Float(right)) => Ok(NumericPair::Float(left, right)),
        (left, right) => Err(NumericError::Runtime(RuntimeError::new(
            span,
            format!(
                "operator `{operator}` requires numeric operands, got {} and {}",
                left.type_name(),
                right.type_name()
            ),
        ))),
    }
}

fn numeric_arithmetic(
    left: Value,
    right: Value,
    operator: &str,
    span: Span,
    integer: impl FnOnce(i64, i64) -> Option<i64>,
    float: impl FnOnce(f64, f64) -> f64,
) -> NumericResult<Value> {
    match numeric_operands(left, right, operator, span)? {
        NumericPair::Int(left, right) => integer(left, right)
            .map(Value::Int)
            .ok_or_else(|| NumericError::Runtime(RuntimeError::new(span, "integer overflow"))),
        NumericPair::Float(left, right) => {
            finite_float(float(left, right), span).map_err(Into::into)
        }
    }
}

fn divide(left: Value, right: Value, span: Span) -> NumericResult<Value> {
    match numeric_operands(left, right, "/", span)? {
        NumericPair::Int(left, right) => {
            if right == 0 {
                Err(NumericError::DivisionByZero)
            } else {
                finite_float(left as f64 / right as f64, span).map_err(Into::into)
            }
        }
        NumericPair::Float(left, right) => {
            if right == 0.0 {
                Err(NumericError::DivisionByZero)
            } else {
                finite_float(left / right, span).map_err(Into::into)
            }
        }
    }
}

fn floor_divide(left: Value, right: Value, span: Span) -> NumericResult<Value> {
    match numeric_operands(left, right, "//", span)? {
        NumericPair::Int(left, right) => {
            if right == 0 {
                return Err(NumericError::DivisionByZero);
            }
            let quotient = left.checked_div(right).ok_or_else(|| {
                NumericError::Runtime(RuntimeError::new(
                    span,
                    "integer overflow in floor division",
                ))
            })?;
            let remainder = left % right;
            let quotient = if remainder != 0 && (remainder < 0) != (right < 0) {
                quotient.checked_sub(1).ok_or_else(|| {
                    NumericError::Runtime(RuntimeError::new(
                        span,
                        "integer overflow in floor division",
                    ))
                })?
            } else {
                quotient
            };
            Ok(Value::Int(quotient))
        }
        NumericPair::Float(left, right) => {
            if right == 0.0 {
                Err(NumericError::DivisionByZero)
            } else {
                finite_float((left / right).floor(), span).map_err(Into::into)
            }
        }
    }
}

fn remainder(left: Value, right: Value, span: Span) -> NumericResult<Value> {
    match numeric_operands(left, right, "%", span)? {
        NumericPair::Int(left, right) => {
            if right == 0 {
                return Err(NumericError::DivisionByZero);
            }
            if right == -1 {
                return Ok(Value::Int(0));
            }
            let truncated = left % right;
            let value = if truncated != 0 && (truncated < 0) != (right < 0) {
                truncated.checked_add(right).ok_or_else(|| {
                    NumericError::Runtime(RuntimeError::new(span, "integer overflow in remainder"))
                })?
            } else {
                truncated
            };
            Ok(Value::Int(value))
        }
        NumericPair::Float(left, right) => {
            if right == 0.0 {
                Err(NumericError::DivisionByZero)
            } else {
                let truncated = left % right;
                let value = if truncated != 0.0
                    && truncated.is_sign_negative() != right.is_sign_negative()
                {
                    truncated + right
                } else {
                    truncated
                };
                finite_float(value, span).map_err(Into::into)
            }
        }
    }
}

fn compare_numbers(
    left: Value,
    right: Value,
    operator: &str,
    span: Span,
    compare: impl FnOnce(Ordering) -> bool,
) -> NumericResult<Value> {
    let ordering = numeric_ordering(&left, &right).ok_or_else(|| {
        NumericError::Runtime(RuntimeError::new(
            span,
            format!(
                "operator `{operator}` requires numeric operands, got {} and {}",
                left.type_name(),
                right.type_name()
            ),
        ))
    })?;
    Ok(Value::Bool(compare(ordering)))
}

pub(super) fn numeric_equal(left: &Value, right: &Value) -> Option<bool> {
    numeric_ordering(left, right).map(Ordering::is_eq)
}

fn numeric_ordering(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Int(left), Value::Int(right)) => Some(left.cmp(right)),
        (Value::Int(left), Value::Float(right)) => compare_int_float(*left, *right),
        (Value::Float(left), Value::Int(right)) => {
            compare_int_float(*right, *left).map(Ordering::reverse)
        }
        (Value::Float(left), Value::Float(right)) => left.partial_cmp(right),
        _ => None,
    }
}

fn compare_int_float(integer: i64, float: f64) -> Option<Ordering> {
    if !float.is_finite() {
        return None;
    }

    const I64_MIN_F64: f64 = -9_223_372_036_854_775_808.0;
    const I64_END_F64: f64 = 9_223_372_036_854_775_808.0;
    if float < I64_MIN_F64 {
        return Some(Ordering::Greater);
    }
    if float >= I64_END_F64 {
        return Some(Ordering::Less);
    }

    let floor = float.floor();
    let floor_integer = floor as i64;
    Some(match integer.cmp(&floor_integer) {
        Ordering::Equal if float == floor => Ordering::Equal,
        Ordering::Equal => Ordering::Less,
        ordering => ordering,
    })
}

fn primitive_equal(left: &Value, right: &Value, span: Span) -> NumericResult<bool> {
    if let Some(equal) = numeric_equal(left, right) {
        return Ok(equal);
    }
    match (left, right) {
        (Value::String(left), Value::String(right)) => Ok(left == right),
        (Value::Bool(left), Value::Bool(right)) => Ok(left == right),
        (Value::Nil, Value::Nil) => Ok(true),
        (left, right) if is_primitive(left) && is_primitive(right) => Ok(false),
        (left, right) => Err(NumericError::Runtime(RuntimeError::new(
            span,
            format!(
                "equality is not supported for {} and {}",
                left.type_name(),
                right.type_name()
            ),
        ))),
    }
}

fn finite_float(value: f64, span: Span) -> RuntimeResult<Value> {
    if value.is_finite() {
        Ok(Value::Float(value))
    } else {
        Err(RuntimeError::new(
            span,
            "floating-point result is not finite",
        ))
    }
}

fn is_primitive(value: &Value) -> bool {
    matches!(
        value,
        Value::Int(_) | Value::Float(_) | Value::String(_) | Value::Bool(_) | Value::Nil
    )
}
