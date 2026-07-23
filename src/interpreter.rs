use std::collections::HashMap;
use std::sync::Arc;

use crate::ast::Program;
use crate::engine::ModuleRegistry;
use crate::native::{global_inspect, global_type};
use crate::runtime::{
    Environment, NativeFunction, Raised, RuntimeError, RuntimeResult, ScriptResult, Value,
};
use crate::span::Span;

mod call;
mod execution;
pub(crate) mod operations;
mod pattern;

pub struct Interpreter {
    pub globals: Environment,
    prelude: Environment,
    modules: ModuleRegistry,
    trace_function_calls: bool,
    host_call_span: Option<Span>,
    module_name: Option<String>,
}

pub(super) enum EvaluationError {
    Runtime(RuntimeError),
    Raised(Raised),
    NilPropagate { span: Span },
    Break { value: Value, span: Span },
    Continue { value: Value, span: Span },
}

pub(super) type EvaluationResult<T> = Result<T, EvaluationError>;

impl From<RuntimeError> for EvaluationError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl EvaluationError {
    pub(super) fn into_runtime_error(self) -> RuntimeError {
        match self {
            Self::Runtime(error) => error,
            Self::Raised(_) => unreachable!("raised values have a separate public result boundary"),
            Self::NilPropagate { span } => RuntimeError {
                span,
                message: "nil propagation escaped its standalone block".to_owned(),
            },
            Self::Break { span, .. } => RuntimeError {
                span,
                message: "`break` outside of a loop".to_owned(),
            },
            Self::Continue { span, .. } => RuntimeError {
                span,
                message: "`continue` outside of a loop".to_owned(),
            },
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self::with_modules(ModuleRegistry::new_for_interpreter(HashMap::new()))
    }

    pub fn with_globals(globals: Environment) -> Self {
        Self {
            prelude: globals.clone(),
            globals,
            modules: ModuleRegistry::new_for_interpreter(HashMap::new()),
            trace_function_calls: true,
            host_call_span: None,
            module_name: None,
        }
    }

    pub(crate) fn with_modules(modules: ModuleRegistry) -> Self {
        let prelude = Environment::new();
        prelude.define("require", Value::NativeFunction(NativeFunction::require()));
        prelude.define(
            "type",
            Value::NativeFunction(NativeFunction::new("type", 1, Arc::new(global_type))),
        );
        prelude.define(
            "inspect",
            Value::NativeFunction(NativeFunction::new("inspect", 1, Arc::new(global_inspect))),
        );
        let globals = prelude.child();
        Self {
            globals,
            prelude,
            modules,
            trace_function_calls: true,
            host_call_span: None,
            module_name: None,
        }
    }

    pub fn evaluate(&mut self, program: &Program) -> RuntimeResult<ScriptResult> {
        let globals = self.globals.clone();
        match self.evaluate_items_with_environment(&program.items, &globals) {
            Ok((value, globals)) => {
                self.globals = globals;
                Ok(Ok(value))
            }
            Err(EvaluationError::Raised(raised)) => Ok(Err(raised)),
            Err(error) => Err(error.into_runtime_error()),
        }
    }
}

#[cfg(test)]
mod tests;
