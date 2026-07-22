pub mod ast;
pub mod cli;
mod environment;
pub mod error;
pub mod interpreter;
pub mod lexer;
pub mod native;
pub mod parser;
pub mod runtime;
pub mod span;
mod value;

pub use error::SimiError;
pub use runtime::{Raised, ScriptResult, TraceFrame, Value};

use interpreter::Interpreter;

pub fn eval(source: &str) -> Result<ScriptResult, SimiError> {
    let tokens = lexer::lex(source)?;
    let program = parser::parse(tokens)?;
    let mut interpreter = Interpreter::new();
    interpreter.evaluate(&program).map_err(SimiError::from)
}
