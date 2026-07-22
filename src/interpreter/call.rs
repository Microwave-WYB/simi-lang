use super::{EvaluationError, EvaluationResult, Interpreter};
use crate::ast::{Expr, PipelineStage};
use crate::runtime::{Environment, NativeResult, Raised, RuntimeError, TraceFrame, Value};
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
                let result = match function.implementation() {
                    NativeImplementation::Callback(callback) => callback(&arguments, span),
                    NativeImplementation::Require => self.require_module(&arguments[0], span),
                };
                match result {
                    Ok(Ok(value)) => Ok(value),
                    Ok(Err(raised)) => Err(EvaluationError::Raised(raised)),
                    Err(error) => Err(EvaluationError::Runtime(error)),
                }
            }
            value => Err(EvaluationError::Runtime(RuntimeError {
                span,
                message: format!("cannot call value of type {}", value.type_name()),
            })),
        }
    }
}
