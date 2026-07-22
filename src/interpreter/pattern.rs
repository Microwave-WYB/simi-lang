use gc::{Gc, GcCell};

use super::{EvaluationError, EvaluationResult, Interpreter, operations::numeric_equal};
use crate::ast::{Expr, MatchCase, Pattern, PatternKind, PatternRest};
use crate::runtime::{Environment, RuntimeError, RuntimeResult, TableKey, Value};
use crate::span::Span;

impl Interpreter {
    pub(super) fn evaluate_match(
        &mut self,
        value: &Expr,
        cases: &[MatchCase],
        span: Span,
        env: &Environment,
    ) -> EvaluationResult<Value> {
        let value = self.evaluate_expression(value, env)?;

        for case in cases {
            let mut bindings = Vec::new();
            if !match_pattern(&case.pattern, &value, &mut bindings)? {
                continue;
            }

            let case_env = env.child();
            for (name, value) in bindings {
                case_env.define(name, value);
            }

            if let Some(guard) = &case.guard {
                match self.evaluate_expression(guard, &case_env)? {
                    Value::Bool(true) => {}
                    Value::Bool(false) => continue,
                    value => {
                        return Err(EvaluationError::Runtime(RuntimeError {
                            span: guard.span,
                            message: format!(
                                "match guard must be boolean, got {}",
                                value.type_name()
                            ),
                        }));
                    }
                }
            }

            return self.evaluate_block(&case.body, &case_env);
        }

        Err(EvaluationError::Runtime(RuntimeError {
            span,
            message: "no match case matched".to_owned(),
        }))
    }

    pub(super) fn evaluate_try(
        &mut self,
        protected: &Expr,
        cases: &[MatchCase],
        env: &Environment,
    ) -> EvaluationResult<Value> {
        let caught = match self.evaluate_expression(protected, env) {
            Ok(value) => return Ok(value),
            Err(EvaluationError::Raised(raised)) => raised,
            Err(error) => return Err(error),
        };

        for case in cases {
            let mut bindings = Vec::new();
            if !match_pattern(&case.pattern, &caught.value, &mut bindings)? {
                continue;
            }

            let case_env = env.child();
            for (name, value) in bindings {
                case_env.define(name, value);
            }

            if let Some(guard) = &case.guard {
                match self.evaluate_expression(guard, &case_env) {
                    Ok(Value::Bool(true)) => {}
                    Ok(Value::Bool(false)) => continue,
                    Ok(value) => {
                        return Err(EvaluationError::Runtime(RuntimeError {
                            span: guard.span,
                            message: format!(
                                "catch guard must be boolean, got {}",
                                value.type_name()
                            ),
                        }));
                    }
                    Err(EvaluationError::Raised(mut raised)) => {
                        raised.append_cause(caught);
                        return Err(EvaluationError::Raised(raised));
                    }
                    Err(error) => return Err(error),
                }
            }

            return match self.evaluate_block(&case.body, &case_env) {
                Err(EvaluationError::Raised(mut raised)) => {
                    raised.append_cause(caught);
                    Err(EvaluationError::Raised(raised))
                }
                result => result,
            };
        }

        Err(EvaluationError::Raised(caught))
    }
}

fn match_pattern(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut Vec<(String, Value)>,
) -> RuntimeResult<bool> {
    match &pattern.kind {
        PatternKind::Wildcard => Ok(true),
        PatternKind::Binding(name) => {
            bindings.push((name.clone(), value.clone()));
            Ok(true)
        }
        PatternKind::Int(expected) => {
            Ok(numeric_equal(&Value::Int(*expected), value).unwrap_or(false))
        }
        PatternKind::Float(expected) => {
            Ok(numeric_equal(&Value::Float(*expected), value).unwrap_or(false))
        }
        PatternKind::String(expected) => {
            Ok(matches!(value, Value::String(actual) if actual == expected))
        }
        PatternKind::Bool(expected) => {
            Ok(matches!(value, Value::Bool(actual) if actual == expected))
        }
        PatternKind::Nil => Ok(matches!(value, Value::Nil)),
        PatternKind::List { elements, rest } => {
            let Value::List(values) = value else {
                return Ok(false);
            };

            let values = values.try_borrow().map_err(|_| {
                RuntimeError::new(pattern.span, "could not borrow list for pattern matching")
            })?;
            if rest.is_some() {
                if values.len() < elements.len() {
                    return Ok(false);
                }
            } else if values.len() != elements.len() {
                return Ok(false);
            }

            let fixed_match = values.with_visible(|visible| {
                for (element_pattern, element_value) in elements.iter().zip(visible.iter()) {
                    if !match_pattern(element_pattern, element_value, bindings)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            })?;
            if !fixed_match {
                return Ok(false);
            }

            if let Some(PatternRest::Binding(name)) = rest {
                bindings.push((
                    name.clone(),
                    Value::List(Gc::new(GcCell::new(values.suffix(elements.len())))),
                ));
            }
            Ok(true)
        }
        PatternKind::Table { fields, rest } => {
            let Value::Table(entries) = value else {
                return Ok(false);
            };

            let (field_values, rest_value) = {
                let entries = entries.try_borrow().map_err(|_| {
                    RuntimeError::new(pattern.span, "could not borrow table for pattern matching")
                })?;
                let mut field_values = Vec::with_capacity(fields.len());
                for (name, field_pattern) in fields {
                    let key = TableKey::String(name.clone());
                    if let Some((_, value)) =
                        entries.iter().find(|(entry_key, _)| entry_key == &key)
                    {
                        field_values.push(value.clone());
                    } else if matches!(field_pattern.kind, PatternKind::Nil) {
                        field_values.push(Value::Nil);
                    } else {
                        return Ok(false);
                    }
                }

                let rest_value = match rest {
                    Some(PatternRest::Binding(_)) => {
                        let remaining = entries
                            .iter()
                            .filter(|(key, _)| {
                                !matches!(key, TableKey::String(name) if fields.iter().any(
                                    |(field_name, _)| field_name == name
                                ))
                            })
                            .cloned()
                            .collect();
                        Some(Value::Table(Gc::new(GcCell::new(remaining))))
                    }
                    Some(PatternRest::Discard) | None => None,
                };
                (field_values, rest_value)
            };

            for ((_, field_pattern), field_value) in fields.iter().zip(&field_values) {
                if !match_pattern(field_pattern, field_value, bindings)? {
                    return Ok(false);
                }
            }

            if let (Some(PatternRest::Binding(name)), Some(rest_value)) = (rest, rest_value) {
                bindings.push((name.clone(), rest_value));
            }
            Ok(true)
        }
    }
}
