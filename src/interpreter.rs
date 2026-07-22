use crate::ast::Program;
use crate::native::install_prelude;
use crate::runtime::{Environment, Raised, RuntimeError, RuntimeResult, ScriptResult, Value};
use crate::span::Span;

mod call;
mod execution;
mod operations;
mod pattern;

pub struct Interpreter {
    pub globals: Environment,
}

pub(super) enum EvaluationError {
    Runtime(RuntimeError),
    Raised(Raised),
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
        let globals = Environment::new();
        install_prelude(&globals);
        Self { globals }
    }

    pub fn with_globals(globals: Environment) -> Self {
        Self { globals }
    }

    pub fn evaluate(&mut self, program: &Program) -> RuntimeResult<ScriptResult> {
        let globals = self.globals.clone();
        match self.evaluate_items(&program.items, &globals) {
            Ok(value) => Ok(Ok(value)),
            Err(EvaluationError::Raised(raised)) => Ok(Err(raised)),
            Err(error) => Err(error.into_runtime_error()),
        }
    }
}

#[cfg(test)]
mod tests;
