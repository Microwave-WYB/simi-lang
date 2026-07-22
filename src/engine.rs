use std::collections::HashMap;

use crate::error::SimiError;
use crate::interpreter::Interpreter;
use crate::module::Module;
use crate::runtime::{ScriptResult, Value};
use crate::{lexer, parser, stdlib};

pub struct Engine {
    modules: HashMap<String, Value>,
}

impl Engine {
    pub fn new() -> Self {
        Self::builder().build()
    }

    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    pub fn with_stdlib() -> Self {
        Self::builder().stdlib().build()
    }

    pub fn eval(&self, source: &str) -> Result<ScriptResult, SimiError> {
        let tokens = lexer::lex(source)?;
        let program = parser::parse(tokens)?;
        let mut interpreter = Interpreter::with_modules(self.modules.clone());
        interpreter.evaluate(&program).map_err(SimiError::from)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EngineBuilder {
    modules: HashMap<String, Module>,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    pub fn module(mut self, module: Module) -> Self {
        self.modules.insert(module.name().to_owned(), module);
        self
    }

    pub fn stdlib(self) -> Self {
        self.module(stdlib::list())
            .module(stdlib::map())
            .module(stdlib::number())
            .module(stdlib::string())
    }

    pub fn stdio(self) -> Self {
        self.module(stdlib::stdin())
            .module(stdlib::stdout())
            .module(stdlib::stderr())
    }

    pub fn build(self) -> Engine {
        let modules = self.modules.into_values().map(Module::into_parts).collect();
        Engine { modules }
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
