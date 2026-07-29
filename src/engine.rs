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
    prelude_modules: Vec<(&'static str, &'static str)>,
    catalog: Option<PackageCatalog>,
    configuration_errors: Vec<String>,
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
        if self.prelude_modules.is_empty() {
            interpreter.evaluate(&program).map_err(SimiError::from)
        } else {
            interpreter
                .evaluate_with_prelude(&program, &self.prelude_modules)
                .map_err(SimiError::Runtime)
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
    prelude_modules: Vec<(&'static str, &'static str)>,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            catalog: None,
            configuration_errors: Vec::new(),
            prelude_modules: Vec::new(),
        }
    }

    /// Install the bundled minimum prelude: mutable list and map operations.
    ///
    /// The official non-prelude standard library remains unavailable until an exact official
    /// catalog is installed through [`Self::stdlib`] or [`Self::catalog`].
    pub fn prelude(mut self) -> Self {
        self.prelude_modules = vec![("list", "std/list"), ("map", "std/map")];
        for module in [stdlib::list(), stdlib::map()] {
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
                && !(stdlib::is_official_catalog(catalog)
                    && (module.name() == "std" || module.name().starts_with("std/")))
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
        let catalog = if let Some(existing) = &self.catalog {
            if stdlib::is_official_catalog(existing) && !stdlib::is_official_catalog(&catalog) {
                match existing.merged_with(&catalog) {
                    Ok(merged) => merged,
                    Err(error) => {
                        self.configuration_errors.push(format!(
                            "resolved package catalog conflicts with the official catalog: {error}"
                        ));
                        return self;
                    }
                }
            } else {
                self.configuration_errors
                    .push("an engine may receive at most one resolved package catalog".to_owned());
                return self;
            }
        } else {
            catalog
        };
        let official_stdlib = stdlib::is_official_catalog(&catalog);
        if !official_stdlib
            && catalog
                .modules()
                .iter()
                .any(|entry| entry.name() == "std" || entry.name().starts_with("std/"))
        {
            self.configuration_errors.push(
                "only the exact distribution official catalog may supply the reserved `std/` namespace"
                    .to_owned(),
            );
            return self;
        }
        for entry in catalog.modules() {
            if self.modules.contains_key(entry.name()) {
                if official_stdlib && (entry.name() == "std" || entry.name().starts_with("std/")) {
                    continue;
                }
                self.configuration_errors.push(format!(
                    "resolved package catalog module `{}` conflicts with a direct module",
                    entry.name()
                ));
                continue;
            }
            let module =
                if official_stdlib && (entry.name() == "std" || entry.name().starts_with("std/")) {
                    stdlib::official_module(entry.name())
                        .expect("official catalog has only built-in module identities")
                } else {
                    Module::source(entry.name(), entry.source()).build()
                };
            self.modules.insert(entry.name().to_owned(), module);
        }
        if official_stdlib
            && !self.prelude_modules.is_empty()
            && !self
                .prelude_modules
                .iter()
                .any(|(alias, _)| *alias == "iter")
        {
            self.prelude_modules.extend([
                ("iter", "std/iter"),
                ("number", "std/number"),
                ("string", "std/string"),
            ]);
        }
        self.catalog = Some(catalog);
        self
    }

    /// Install the bundled prelude and the exact distribution official standard-library catalog.
    pub fn stdlib(self) -> Self {
        self.prelude().catalog(stdlib::official_catalog())
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
            prelude_modules: self.prelude_modules,
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
