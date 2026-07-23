use super::{EvaluationError, EvaluationResult, Interpreter};
use crate::ast::{Expr, PipelineStage};
use crate::engine::ModuleLookup;
use crate::module::HostOperation;
use crate::runtime::{
    Environment, List, MapKey, NativeResult, Raised, RuntimeError, TraceFrame, Value,
};
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
        let (source, host_operations) = match self.modules.begin_load(name) {
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
            ModuleLookup::Source {
                source,
                host_operations,
            } => (source, host_operations),
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
        let trace_module_functions = host_operations.is_empty();
        let environment = self.prelude.child();
        let host = Value::Map(gc::Gc::new(gc::GcCell::new(vec![(
            MapKey::String("call".to_owned()),
            Value::NativeFunction(crate::runtime::NativeFunction::host_call(host_operations)),
        )])));
        environment.define("host", host);
        let previous_trace_setting = self.trace_function_calls;
        let previous_module_name = self.module_name.replace(name.clone());
        self.trace_function_calls = trace_module_functions;
        let result = self.evaluate_items(&program.items, &environment);
        self.trace_function_calls = previous_trace_setting;
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
                raised.origin = span;
                Err(EvaluationError::Raised(raised))
            }
            Err(error) => {
                self.modules.fail_load(name);
                Err(error)
            }
        }
    }

    fn call_host(
        &mut self,
        operations: &std::collections::HashMap<String, HostOperation>,
        arguments: &[Value],
        span: Span,
    ) -> EvaluationResult<Value> {
        let span = self.host_call_span.unwrap_or(span);
        let Some(Value::String(id)) = arguments.first() else {
            let actual = arguments.first().map_or("no value", Value::type_name);
            return Err(RuntimeError::new(
                span,
                format!("host.call expects a string function ID, got {actual}"),
            )
            .into());
        };
        let Some(operation) = operations.get(id) else {
            return Err(EvaluationError::Raised(Raised::host_function_not_found(
                id, span,
            )));
        };
        let host_arguments = &arguments[1..];
        if host_arguments.len() != operation.arity() {
            return Err(RuntimeError::new(
                span,
                format!(
                    "host function `{id}` expects {} arguments, got {}",
                    operation.arity(),
                    host_arguments.len()
                ),
            )
            .into());
        }
        match operation {
            HostOperation::Callback { callback, .. } => {
                evaluation_from_native(callback(host_arguments, span))
            }
            HostOperation::ListMap => self.list_map(host_arguments, span),
            HostOperation::ListFilter => self.list_filter(host_arguments, span),
            HostOperation::ListFold => self.list_fold(host_arguments, span),
            HostOperation::ListFind => self.list_find(host_arguments, span),
            HostOperation::ListFindIndex => self.list_find_index(host_arguments, span),
            HostOperation::ListAny => self.list_any(host_arguments, span),
            HostOperation::ListAll => self.list_all(host_arguments, span),
            HostOperation::ListEach => self.list_each(host_arguments, span),
            HostOperation::ListCount => self.list_count(host_arguments, span),
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
                    let category = if function.trace_calls {
                        "function"
                    } else {
                        "native function"
                    };
                    return Err(EvaluationError::Runtime(RuntimeError {
                        span,
                        message: format!(
                            "{category} `{}` expects {} arguments, got {}",
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
                let previous_host_span = self.host_call_span;
                if function.trace_calls {
                    self.host_call_span = None;
                } else if self.host_call_span.is_none() {
                    self.host_call_span = Some(span);
                }
                let result = self.evaluate_block(&function.body, &call_env);
                self.host_call_span = previous_host_span;
                match result {
                    Ok(value) => Ok(value),
                    Err(EvaluationError::Raised(mut raised)) => {
                        if function.trace_calls {
                            if function.module.is_some() {
                                raised.origin = span;
                            }
                            raised.push_frame(TraceFrame {
                                function: function.name.clone(),
                                call_span: span,
                            });
                        }
                        Err(EvaluationError::Raised(raised))
                    }
                    Err(EvaluationError::Runtime(mut error)) => {
                        if function.trace_calls
                            && let Some(module) = &function.module
                        {
                            error.span = span;
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
                if !matches!(function.implementation(), NativeImplementation::HostCall(_))
                    && arguments.len() != function.arity()
                {
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
                    NativeImplementation::HostCall(operations) => {
                        self.call_host(operations, &arguments, span)
                    }
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
                "{} `{}` expects {} arguments, got {arity}",
                if function.trace_calls {
                    "function"
                } else {
                    "native function"
                },
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
