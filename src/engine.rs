use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::error::SimiError;
use crate::interpreter::Interpreter;
use crate::module::{Module, ModuleContents, direct_value};
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
        host: Value,
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
    Source { source: Arc<str>, host: Value },
}

#[derive(Clone)]
pub(crate) struct PreludeRegistry {
    entries: Rc<RefCell<HashMap<String, PreludeEntry>>>,
}

pub(crate) enum PreludeEntry {
    Source {
        name: String,
        source: Arc<str>,
        host: Value,
        state: SourceModuleState,
    },
}

pub(crate) enum PreludeLookup {
    Loading,
    Loaded(Value),
    Source {
        name: String,
        source: Arc<str>,
        host: Value,
    },
}

impl PreludeRegistry {
    fn new(entries: HashMap<String, PreludeEntry>) -> Self {
        Self {
            entries: Rc::new(RefCell::new(entries)),
        }
    }

    pub(crate) fn begin_load(&self, alias: &str) -> PreludeLookup {
        let mut entries = self.entries.borrow_mut();
        let PreludeEntry::Source {
            name,
            source,
            host,
            state,
        } = entries
            .get_mut(alias)
            .expect("configured prelude global should exist");
        match state {
            SourceModuleState::Unloaded => {
                *state = SourceModuleState::Loading;
                PreludeLookup::Source {
                    name: name.clone(),
                    source: source.clone(),
                    host: host.clone(),
                }
            }
            SourceModuleState::Loading => PreludeLookup::Loading,
            SourceModuleState::Loaded(value) => PreludeLookup::Loaded(value.clone()),
        }
    }

    pub(crate) fn finish_load(&self, alias: &str, value: Value) {
        let mut entries = self.entries.borrow_mut();
        let PreludeEntry::Source { state, .. } = entries
            .get_mut(alias)
            .expect("configured prelude global should exist");
        *state = SourceModuleState::Loaded(value);
    }

    pub(crate) fn fail_load(&self, alias: &str) {
        let mut entries = self.entries.borrow_mut();
        let PreludeEntry::Source { state, .. } = entries
            .get_mut(alias)
            .expect("configured prelude global should exist");
        *state = SourceModuleState::Unloaded;
    }
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
                host,
                state,
            } => match state {
                SourceModuleState::Unloaded => {
                    *state = SourceModuleState::Loading;
                    ModuleLookup::Source {
                        source: source.clone(),
                        host: host.clone(),
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
    prelude: PreludeRegistry,
    install_prelude: bool,
}

impl Engine {
    pub fn new() -> Self {
        Self::builder().prelude().build()
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
        if self.install_prelude {
            interpreter
                .install_prelude_modules(&self.prelude)
                .map_err(SimiError::Runtime)?;
        }
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
    prelude_modules: Vec<(String, Module)>,
    install_prelude: bool,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            prelude_modules: Vec::new(),
            install_prelude: false,
        }
    }

    fn prelude(mut self) -> Self {
        self.install_prelude = true;
        self.prelude_modules = vec![
            ("list".to_owned(), stdlib::list()),
            ("map".to_owned(), stdlib::map()),
        ];
        self
    }

    pub fn module(mut self, module: Module) -> Self {
        self.modules.insert(module.name().to_owned(), module);
        self
    }

    pub fn stdlib(self) -> Self {
        self.prelude()
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
                    ModuleContents::Source { source, host } => ModuleEntry::Source {
                        source,
                        host,
                        state: SourceModuleState::Unloaded,
                    },
                };
                (name, entry)
            })
            .collect();
        let prelude = self
            .prelude_modules
            .into_iter()
            .map(|(alias, module)| {
                let (name, contents) = module.into_parts();
                let ModuleContents::Source { source, host } = contents else {
                    unreachable!("built-in prelude modules are source-backed");
                };
                (
                    alias,
                    PreludeEntry::Source {
                        name,
                        source,
                        host,
                        state: SourceModuleState::Unloaded,
                    },
                )
            })
            .collect();
        Engine {
            modules: ModuleRegistry::new(modules),
            prelude: PreludeRegistry::new(prelude),
            install_prelude: self.install_prelude,
        }
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
