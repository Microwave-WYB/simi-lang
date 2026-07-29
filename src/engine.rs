use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::error::SimiError;
use crate::interpreter::Interpreter;
use crate::module::{Module, ModuleContents, direct_value};
use crate::runtime::{RuntimeError, ScriptResult, Value};
use crate::{PackageCatalog, parser, stdlib};

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
    install_prelude: bool,
    catalog: Option<PackageCatalog>,
    configuration_errors: Vec<String>,
}

impl Engine {
    pub fn new() -> Self {
        Self::builder().stdlib().build()
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
        if let Some(message) = self.configuration_errors.first() {
            return Err(SimiError::Runtime(RuntimeError::new(
                crate::span::Span::new(0, 0),
                message.clone(),
            )));
        }
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
        let requirements = crate::package::parse_requires(source)
            .map_err(|error| SimiError::Runtime(RuntimeError::new(error.span, error.message)))?;
        if let Some(requirements) = requirements {
            let Some(catalog) = &self.catalog else {
                return Err(SimiError::Runtime(RuntimeError::new(
                    requirements.span,
                    "source declares package requirements but this engine has no resolved package catalog",
                )));
            };
            for requirement in &requirements.entries {
                if !catalog.satisfies(requirement) {
                    return Err(SimiError::Runtime(RuntimeError::new(
                        requirements.span,
                        format!(
                            "resolved package catalog does not satisfy requirement `{}`",
                            requirement.alias
                        ),
                    )));
                }
            }
        }
        if crate::package::source_has_package_relative_import(source).map_err(|message| {
            SimiError::Runtime(RuntimeError::new(crate::span::Span::new(0, 0), message))
        })? {
            return Err(SimiError::Runtime(RuntimeError::new(
                crate::span::Span::new(0, 0),
                "package-relative imports require prior package resolution",
            )));
        }
        let mut interpreter = Interpreter::with_modules(self.modules.clone());
        if self.install_prelude {
            interpreter
                .evaluate_with_prelude(&program)
                .map_err(SimiError::Runtime)
        } else {
            interpreter.evaluate(&program).map_err(SimiError::from)
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EngineBuilder {
    modules: HashMap<String, Module>,
    catalog: Option<PackageCatalog>,
    configuration_errors: Vec<String>,
    install_prelude: bool,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            catalog: None,
            configuration_errors: Vec::new(),
            install_prelude: false,
        }
    }

    fn prelude(mut self) -> Self {
        self.install_prelude = true;
        for module in [
            stdlib::bytes(),
            stdlib::float(),
            stdlib::integer(),
            stdlib::list(),
            stdlib::map(),
            stdlib::iter(),
            stdlib::number(),
            stdlib::string(),
            stdlib::utf8(),
            stdlib::utf16(),
        ] {
            self.modules.insert(module.name().to_owned(), module);
        }
        self
    }

    pub fn module(mut self, module: Module) -> Self {
        if self.catalog.as_ref().is_some_and(|catalog| {
            catalog
                .modules()
                .iter()
                .any(|entry| entry.name() == module.name())
        }) {
            self.configuration_errors.push(format!(
                "direct module `{}` conflicts with a resolved package catalog module",
                module.name()
            ));
        }
        self.modules.insert(module.name().to_owned(), module);
        self
    }

    /// Register an already resolved source package catalog.
    ///
    /// This operation only installs the catalog's source text. It never reads the filesystem,
    /// downloads packages, invokes Cargo, runs build scripts, or grants native capabilities.
    /// Catalog/declaration compatibility is checked as a hard error before each evaluation.
    pub fn catalog(mut self, catalog: PackageCatalog) -> Self {
        if self.catalog.is_some() {
            self.configuration_errors
                .push("an engine may receive at most one resolved package catalog".to_owned());
            return self;
        }
        for entry in catalog.modules() {
            if self.modules.contains_key(entry.name()) {
                self.configuration_errors.push(format!(
                    "resolved package catalog module `{}` conflicts with a direct module",
                    entry.name()
                ));
            }
            self.modules.insert(
                entry.name().to_owned(),
                Module::source(entry.name(), entry.source()).build(),
            );
        }
        self.catalog = Some(catalog);
        self
    }

    pub fn stdlib(self) -> Self {
        self.prelude()
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
        Engine {
            modules: ModuleRegistry::new(modules),
            install_prelude: self.install_prelude,
            catalog: self.catalog,
            configuration_errors: self.configuration_errors,
        }
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
