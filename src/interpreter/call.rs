use super::{EvaluationError, EvaluationResult, Interpreter};
use crate::ast::{Expr, PipelineStage};
use crate::runtime::{Environment, List, NativeResult, Raised, RuntimeError, TraceFrame, Value};
use crate::span::Span;
use crate::value::NativeImplementation;

impl Interpreter {
    pub(super) fn evaluate_arguments(
        &mut self,
        arguments: &[Expr],
        env: &Environment,
    ) -> EvaluationResult<Vec<Value>> {
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            values.push(self.evaluate_expression(argument, env)?);
        }
        Ok(values)
    }

    pub(super) fn evaluate_pipeline(
        &mut self,
        input: &Expr,
        stages: &[PipelineStage],
        env: &Environment,
    ) -> EvaluationResult<Value> {
        let mut value = self.evaluate_expression(input, env)?;

        for stage in stages {
            if stage.nil_aware && matches!(value, Value::Nil) {
                continue;
            }
            let callee = self.evaluate_expression(&stage.callee, env)?;
            let original = value.clone();
            let mut args = Vec::with_capacity(stage.args.len() + 1);
            args.push(value);
            args.extend(self.evaluate_arguments(&stage.args, env)?);
            let result = self.call_value(callee, args, stage.span)?;
            value = if stage.tap { original } else { result };
        }

        Ok(value)
    }

    fn require_module(&self, name: &Value, span: Span) -> NativeResult {
        let Value::String(name) = name else {
            return Err(RuntimeError::new(
                span,
                format!(
                    "require expects a string module name, got {}",
                    name.type_name()
                ),
            ));
        };
        match self.modules.get(name) {
            Some(module) => Ok(Ok(module.clone())),
            None => Ok(Err(Raised::module_not_found(name, span))),
        }
    }

    fn list_map(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let values = list_snapshot(&arguments[0], "map", span)?;
        validate_callback(&arguments[1], 1, span)?;
        let mut mapped = Vec::with_capacity(values.len());
        for value in values {
            mapped.push(self.call_value(arguments[1].clone(), vec![value], span)?);
        }
        Ok(Value::List(List::shared(mapped)))
    }

    fn list_filter(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let values = list_snapshot(&arguments[0], "filter", span)?;
        validate_callback(&arguments[1], 1, span)?;
        let mut filtered = Vec::with_capacity(values.len());
        for value in values {
            let predicate = self.call_value(arguments[1].clone(), vec![value.clone()], span)?;
            match predicate {
                Value::Bool(true) => filtered.push(value),
                Value::Bool(false) => {}
                value => {
                    return Err(EvaluationError::Runtime(RuntimeError::new(
                        span,
                        format!(
                            "std/list.filter callback must return a boolean, got {}",
                            value.type_name()
                        ),
                    )));
                }
            }
        }
        Ok(Value::List(List::shared(filtered)))
    }

    fn list_fold(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let values = list_snapshot(&arguments[0], "fold", span)?;
        validate_callback(&arguments[2], 2, span)?;
        let mut accumulator = arguments[1].clone();
        for value in values {
            accumulator = self.call_value(arguments[2].clone(), vec![accumulator, value], span)?;
        }
        Ok(accumulator)
    }

    fn list_find(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let values = list_snapshot(&arguments[0], "find", span)?;
        validate_callback(&arguments[1], 1, span)?;
        for value in values {
            if self.call_list_predicate("find", &arguments[1], value.clone(), span)? {
                return Ok(value);
            }
        }
        Ok(Value::Nil)
    }

    fn list_find_index(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let values = list_snapshot(&arguments[0], "find_index", span)?;
        validate_callback(&arguments[1], 1, span)?;
        for (index, value) in values.into_iter().enumerate() {
            if self.call_list_predicate("find_index", &arguments[1], value, span)? {
                let index = i64::try_from(index).map_err(|_| {
                    EvaluationError::Runtime(RuntimeError::new(span, "list index exceeds i64"))
                })?;
                return Ok(Value::Int(index));
            }
        }
        Ok(Value::Nil)
    }

    fn list_any(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let values = list_snapshot(&arguments[0], "any", span)?;
        validate_callback(&arguments[1], 1, span)?;
        for value in values {
            if self.call_list_predicate("any", &arguments[1], value, span)? {
                return Ok(Value::Bool(true));
            }
        }
        Ok(Value::Bool(false))
    }

    fn list_all(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let values = list_snapshot(&arguments[0], "all", span)?;
        validate_callback(&arguments[1], 1, span)?;
        for value in values {
            if !self.call_list_predicate("all", &arguments[1], value, span)? {
                return Ok(Value::Bool(false));
            }
        }
        Ok(Value::Bool(true))
    }

    fn list_each(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let values = list_snapshot(&arguments[0], "each", span)?;
        validate_callback(&arguments[1], 1, span)?;
        for value in values {
            self.call_value(arguments[1].clone(), vec![value], span)?;
        }
        Ok(arguments[0].clone())
    }

    fn list_count(&mut self, arguments: &[Value], span: Span) -> EvaluationResult<Value> {
        let values = list_snapshot(&arguments[0], "count", span)?;
        validate_callback(&arguments[1], 1, span)?;
        let mut count = 0_i64;
        for value in values {
            if self.call_list_predicate("count", &arguments[1], value, span)? {
                count = count.checked_add(1).ok_or_else(|| {
                    EvaluationError::Runtime(RuntimeError::new(span, "list count exceeds i64"))
                })?;
            }
        }
        Ok(Value::Int(count))
    }

    fn call_list_predicate(
        &mut self,
        operation: &str,
        callback: &Value,
        value: Value,
        span: Span,
    ) -> EvaluationResult<bool> {
        match self.call_value(callback.clone(), vec![value], span)? {
            Value::Bool(result) => Ok(result),
            value => Err(EvaluationError::Runtime(RuntimeError::new(
                span,
                format!(
                    "std/list.{operation} callback must return a boolean, got {}",
                    value.type_name()
                ),
            ))),
        }
    }

    pub(super) fn call_value(
        &mut self,
        callee: Value,
        arguments: Vec<Value>,
        span: Span,
    ) -> EvaluationResult<Value> {
        match callee {
            Value::Function(function) => {
                if arguments.len() != function.params.len() {
                    return Err(EvaluationError::Runtime(RuntimeError {
                        span,
                        message: format!(
                            "function `{}` expects {} arguments, got {}",
                            function.name,
                            function.params.len(),
                            arguments.len()
                        ),
                    }));
                }

                let call_env = function.closure.child();
                for (parameter, argument) in function.params.iter().zip(arguments) {
                    call_env.define(parameter.clone(), argument);
                }
                match self.evaluate_block(&function.body, &call_env) {
                    Ok(value) => Ok(value),
                    Err(EvaluationError::Raised(mut raised)) => {
                        raised.push_frame(TraceFrame {
                            function: function.name.clone(),
                            call_span: span,
                        });
                        Err(EvaluationError::Raised(raised))
                    }
                    Err(error @ EvaluationError::Runtime(_)) => Err(error),
                    Err(error @ EvaluationError::NilPropagate { .. }) => {
                        Err(EvaluationError::Runtime(error.into_runtime_error()))
                    }
                    Err(
                        error @ (EvaluationError::Break { .. } | EvaluationError::Continue { .. }),
                    ) => Err(EvaluationError::Runtime(error.into_runtime_error())),
                }
            }
            Value::NativeFunction(function) => {
                if arguments.len() != function.arity() {
                    return Err(EvaluationError::Runtime(RuntimeError {
                        span,
                        message: format!(
                            "native function `{}` expects {} arguments, got {}",
                            function.name(),
                            function.arity(),
                            arguments.len()
                        ),
                    }));
                }
                match function.implementation() {
                    NativeImplementation::Callback(callback) => {
                        evaluation_from_native(callback(&arguments, span))
                    }
                    NativeImplementation::Require => {
                        evaluation_from_native(self.require_module(&arguments[0], span))
                    }
                    NativeImplementation::ListMap => self.list_map(&arguments, span),
                    NativeImplementation::ListFilter => self.list_filter(&arguments, span),
                    NativeImplementation::ListFold => self.list_fold(&arguments, span),
                    NativeImplementation::ListFind => self.list_find(&arguments, span),
                    NativeImplementation::ListFindIndex => self.list_find_index(&arguments, span),
                    NativeImplementation::ListAny => self.list_any(&arguments, span),
                    NativeImplementation::ListAll => self.list_all(&arguments, span),
                    NativeImplementation::ListEach => self.list_each(&arguments, span),
                    NativeImplementation::ListCount => self.list_count(&arguments, span),
                }
            }
            value => Err(EvaluationError::Runtime(RuntimeError {
                span,
                message: format!("cannot call value of type {}", value.type_name()),
            })),
        }
    }
}

fn evaluation_from_native(result: NativeResult) -> EvaluationResult<Value> {
    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(raised)) => Err(EvaluationError::Raised(raised)),
        Err(error) => Err(EvaluationError::Runtime(error)),
    }
}

fn list_snapshot(value: &Value, operation: &str, span: Span) -> EvaluationResult<Vec<Value>> {
    let Value::List(values) = value else {
        return Err(EvaluationError::Runtime(RuntimeError::new(
            span,
            format!(
                "std/list.{operation} requires a list, got {}",
                value.type_name()
            ),
        )));
    };
    values
        .try_borrow()
        .map(|values| values.to_vec())
        .map_err(|_| {
            EvaluationError::Runtime(RuntimeError::new(
                span,
                format!("std/list.{operation} could not borrow list"),
            ))
        })
}

fn validate_callback(callback: &Value, arity: usize, span: Span) -> EvaluationResult<()> {
    match callback {
        Value::Function(function) if function.params.len() == arity => Ok(()),
        Value::Function(function) => Err(EvaluationError::Runtime(RuntimeError::new(
            span,
            format!(
                "function `{}` expects {} arguments, got {arity}",
                function.name,
                function.params.len()
            ),
        ))),
        Value::NativeFunction(function) if function.arity() == arity => Ok(()),
        Value::NativeFunction(function) => Err(EvaluationError::Runtime(RuntimeError::new(
            span,
            format!(
                "native function `{}` expects {} arguments, got {arity}",
                function.name(),
                function.arity()
            ),
        ))),
        value => Err(EvaluationError::Runtime(RuntimeError::new(
            span,
            format!("cannot call value of type {}", value.type_name()),
        ))),
    }
}
