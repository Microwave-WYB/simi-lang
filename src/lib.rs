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
pub mod package;
pub mod parser;
pub mod runtime;
pub mod span;
pub mod stdlib;
mod value;

pub use engine::{Engine, EngineBuilder};
pub use error::SimiError;
pub use module::{Module, ModuleBuilder, NativeCallback, SourceModuleBuilder};
pub use package::{
    CatalogModule, CatalogModuleVisibility, CatalogRequirement, NativePackage, PackageCatalog,
    PackageCatalogError, PackageManifest, PackageManifestError, PackageModule,
    PackageRequirementsError, Requirement, RequirementSource, Requires, ResolvedScript,
    parse_requires, resolve_script,
};
pub use runtime::{NativeResult, Raised, ScriptResult, TraceFrame, Value};

pub fn eval(source: &str) -> Result<ScriptResult, SimiError> {
    Engine::with_stdlib().eval(source)
}
