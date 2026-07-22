pub mod ast;
pub mod cli;
mod engine;
mod environment;
pub mod error;
pub mod interpreter;
pub mod lexer;
mod lower;
mod module;
pub mod native;
pub mod parser;
pub mod runtime;
pub mod span;
pub mod stdlib;
mod value;

pub use engine::{Engine, EngineBuilder};
pub use error::SimiError;
pub use module::{Module, ModuleBuilder, NativeCallback};
pub use runtime::{NativeResult, Raised, ScriptResult, TraceFrame, Value};

pub fn eval(source: &str) -> Result<ScriptResult, SimiError> {
    Engine::with_stdlib().eval(source)
}
