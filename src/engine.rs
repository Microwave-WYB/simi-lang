use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::error::SimiError;
use crate::interpreter::Interpreter;
use crate::module::{HostOperation, Module, ModuleContents, direct_value};
use crate::runtime::{ScriptResult, Value};
use crate::{parser, stdlib};

#[derive(Clone)]
pub(crate) struct ModuleRegistry {
    entries: Rc<RefCell<HashMap<String, ModuleEntry>>>,
}

pub(crate) enum ModuleEntry {
    Direct(Value),
    Source {
        source: Arc<str>,
        host_operations: Arc<HashMap<String, HostOperation>>,
        state: SourceModuleState,
    },
}

pub(crate) enum SourceModuleState {
    Unloaded,
    Loading,
    Loaded(Value),
}

pub(crate) enum ModuleLookup {
    Missing,
    Loading,
    Loaded(Value),
    Source {
        source: Arc<str>,
        host_operations: Arc<HashMap<String, HostOperation>>,
    },
}

impl ModuleRegistry {
    fn new(entries: HashMap<String, ModuleEntry>) -> Self {
        Self {
            entries: Rc::new(RefCell::new(entries)),
        }
    }

    pub(crate) fn new_for_interpreter(entries: HashMap<String, ModuleEntry>) -> Self {
        Self::new(entries)
    }

    pub(crate) fn begin_load(&self, name: &str) -> ModuleLookup {
        let mut entries = self.entries.borrow_mut();
        let Some(entry) = entries.get_mut(name) else {
            return ModuleLookup::Missing;
        };
        match entry {
            ModuleEntry::Direct(value) => ModuleLookup::Loaded(value.clone()),
            ModuleEntry::Source {
                source,
                host_operations,
                state,
            } => match state {
                SourceModuleState::Unloaded => {
                    *state = SourceModuleState::Loading;
                    ModuleLookup::Source {
                        source: source.clone(),
                        host_operations: host_operations.clone(),
                    }
                }
                SourceModuleState::Loading => ModuleLookup::Loading,
                SourceModuleState::Loaded(value) => ModuleLookup::Loaded(value.clone()),
            },
        }
    }

    pub(crate) fn finish_load(&self, name: &str, value: Value) {
        if let Some(ModuleEntry::Source { state, .. }) = self.entries.borrow_mut().get_mut(name) {
            *state = SourceModuleState::Loaded(value);
        }
    }

    pub(crate) fn fail_load(&self, name: &str) {
        if let Some(ModuleEntry::Source { state, .. }) = self.entries.borrow_mut().get_mut(name) {
            *state = SourceModuleState::Unloaded;
        }
    }

    fn sources(&self) -> Vec<(String, String)> {
        self.entries
            .borrow()
            .iter()
            .filter_map(|(name, entry)| match entry {
                ModuleEntry::Source { source, .. } => {
                    Some((name.clone(), source.as_ref().to_owned()))
                }
                ModuleEntry::Direct(_) => None,
            })
            .collect()
    }
}

pub struct Engine {
    modules: ModuleRegistry,
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

    pub fn module_sources(&self) -> Vec<(String, String)> {
        self.modules.sources()
    }

    pub fn eval(&self, source: &str) -> Result<ScriptResult, SimiError> {
        let program = parser::parse_source(source).map_err(|diagnostic| match diagnostic.kind {
            simi_syntax::DiagnosticKind::Lex => SimiError::Lex(crate::lexer::LexError {
                span: diagnostic.span,
                message: diagnostic.message,
            }),
            simi_syntax::DiagnosticKind::Parse => SimiError::Parse(crate::parser::ParseError {
                span: diagnostic.span,
                message: diagnostic.message,
            }),
        })?;
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
            .module(stdlib::iter())
            .module(stdlib::number())
            .module(stdlib::string())
    }

    pub fn stdio(self) -> Self {
        self.module(stdlib::io())
    }

    pub fn build(self) -> Engine {
        let modules = self
            .modules
            .into_values()
            .map(Module::into_parts)
            .map(|(name, contents)| {
                let entry = match contents {
                    ModuleContents::Direct(exports) => ModuleEntry::Direct(direct_value(exports)),
                    ModuleContents::Source {
                        source,
                        host_operations,
                    } => ModuleEntry::Source {
                        source,
                        host_operations: Arc::new(host_operations),
                        state: SourceModuleState::Unloaded,
                    },
                };
                (name, entry)
            })
            .collect();
        Engine {
            modules: ModuleRegistry::new(modules),
        }
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
