use super::{EvaluationError, EvaluationResult, Interpreter};
use crate::ast::{Expr, PipelineStage};
use crate::engine::ModuleLookup;
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

    fn require_module(&mut self, name: &Value, span: Span) -> EvaluationResult<Value> {
        let Value::String(name) = name else {
            return Err(RuntimeError::new(
                span,
                format!(
                    "require expects a string module name, got {}",
                    name.type_name()
                ),
            )
            .into());
        };
        let (source, host) = match self.modules.begin_load(name) {
            ModuleLookup::Missing => {
                return Err(EvaluationError::Raised(Raised::module_not_found(
                    name, span,
                )));
            }
            ModuleLookup::Loading => {
                return Err(EvaluationError::Raised(Raised::circular_module_dependency(
                    name, span,
                )));
            }
            ModuleLookup::Loaded(value) => return Ok(value),
            ModuleLookup::Source { source, host } => (source, host),
        };

        let program = match crate::parser::parse_source(&source) {
            Ok(program) => program,
            Err(diagnostic) => {
                self.modules.fail_load(name);
                return Err(EvaluationError::Runtime(RuntimeError::new(
                    span,
                    format!("module `{name}` has invalid source: {}", diagnostic.message),
                )));
            }
        };
        let environment = self.prelude.child();
        environment.define("host", host);
        let previous_module_name = self.module_name.replace(name.clone());
        let previous_source_domain =
            std::mem::replace(&mut self.source_domain, super::next_source_domain());
        let result = self.evaluate_items(&program.items, &environment);
        self.source_domain = previous_source_domain;
        self.module_name = previous_module_name;
        match result {
            Ok(value) => {
                self.modules.finish_load(name, value.clone());
                Ok(value)
            }
            Err(EvaluationError::Runtime(mut error)) => {
                self.modules.fail_load(name);
                error.span = span;
                error.message = format!("module `{name}`: {}", error.message);
                Err(EvaluationError::Runtime(error))
            }
            Err(EvaluationError::Raised(mut raised)) => {
                self.modules.fail_load(name);
                raised.remap_to_boundary(span);
                Err(EvaluationError::Raised(raised))
            }
            Err(error) => {
                self.modules.fail_load(name);
                Err(error)
            }
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
                let caller_source_domain = self.source_domain;
                let crosses_source = function.source_domain != caller_source_domain;
                self.source_domain = function.source_domain;
                let result = self.evaluate_block(&function.body, &call_env);
                self.source_domain = caller_source_domain;
                match result {
                    Ok(value) => Ok(value),
                    Err(EvaluationError::Raised(mut raised)) => {
                        if crosses_source {
                            raised.remap_to_boundary(span);
                        }
                        raised.push_frame(TraceFrame {
                            function: function.name.clone(),
                            call_span: span,
                        });
                        Err(EvaluationError::Raised(raised))
                    }
                    Err(EvaluationError::Runtime(mut error)) => {
                        if crosses_source {
                            error.span = span;
                        }
                        if let Some(module) = &function.module {
                            error.message = format!("module `{module}`: {}", error.message);
                        }
                        Err(EvaluationError::Runtime(error))
                    }
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
                    NativeImplementation::Require => self.require_module(&arguments[0], span),
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
