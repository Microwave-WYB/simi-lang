//! Restricted static metadata for reusable Simi packages.
//!
//! A package manifest is parsed as Simi syntax but never evaluated. Resolver work belongs to the
//! package-resolution layer; this module only establishes the deterministic, capability-free
//! package-tree contract that a resolver consumes.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

use crate::{
    ast::{Expr, ExprKind, ListElement, StmtKind},
    lexer::lex,
    parser::parse,
};
use simi_analysis::{AnalysisDatabase, resolve};

mod lock;
mod resolver;

pub use resolver::{ResolutionMode, ResolvedScript, lock_path, resolve_script};
pub use simi_analysis::{
    PackageRequirementsError, Requirement, RequirementSource, Requires, parse_requires,
};

/// One source module in a previously resolved package catalog.
///
/// The module name is the identity passed to `require`. Package-local modules have deterministic
/// private names and are included so normal source-module caching preserves their identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogModule {
    name: String,
    source: String,
    package: String,
    source_path: String,
    visibility: CatalogModuleVisibility,
}

impl CatalogModule {
    /// Construct a source module with validated package provenance and identity.
    pub fn new(
        name: impl Into<String>,
        source: impl Into<String>,
        package: impl Into<String>,
        source_path: impl Into<String>,
        visibility: CatalogModuleVisibility,
    ) -> Result<Self, PackageCatalogError> {
        let module = Self {
            name: name.into(),
            source: source.into(),
            package: package.into(),
            source_path: source_path.into(),
            visibility,
        };
        validate_catalog_module(&module)?;
        Ok(module)
    }

    /// Module identity used by `require`.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Rewritten Simi source evaluated lazily by the engine.
    pub fn source(&self) -> &str {
        &self.source
    }
    /// Manifest package identity that supplied this source.
    pub fn package(&self) -> &str {
        &self.package
    }
    /// Canonical package-root-relative source path.
    pub fn source_path(&self) -> &str {
        &self.source_path
    }
    /// Whether this module is manifest-public or package-local.
    pub const fn visibility(&self) -> CatalogModuleVisibility {
        self.visibility
    }
}

/// Visibility of a resolved catalog module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogModuleVisibility {
    /// A manifest-declared module whose name is part of the public package interface.
    Public,
    /// A resolver-discovered literal local source with a package-scoped private identity.
    PackageLocal,
}

/// A locked package requirement represented by a resolved catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogRequirement {
    package: String,
    source: RequirementSource,
}

impl CatalogRequirement {
    /// Construct the resolved identity for a declared requirement source.
    pub fn new(package: impl Into<String>, source: RequirementSource) -> Self {
        Self {
            package: package.into(),
            source,
        }
    }

    /// Manifest package identity selected for this requirement.
    pub fn package(&self) -> &str {
        &self.package
    }
    /// Exact static source declaration accepted by this catalog.
    pub fn source(&self) -> &RequirementSource {
        &self.source
    }
}

/// A deterministic, already-resolved collection of package source modules.
///
/// Constructing or installing a catalog does not read files, access Git, invoke Cargo, run build
/// scripts, or grant native capabilities. Hosts normally obtain one from [`resolve_script`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageCatalog {
    modules: Vec<CatalogModule>,
    requirements: Vec<CatalogRequirement>,
}

impl PackageCatalog {
    /// Build a catalog from already resolved source modules and locked requirements.
    pub fn new(
        modules: impl IntoIterator<Item = CatalogModule>,
        requirements: impl IntoIterator<Item = CatalogRequirement>,
    ) -> Result<Self, PackageCatalogError> {
        let mut modules = modules.into_iter().collect::<Vec<_>>();
        for module in &modules {
            validate_catalog_module(module)?;
        }
        modules.sort_by(|left, right| left.name.cmp(&right.name));
        for pair in modules.windows(2) {
            if pair[0].name == pair[1].name {
                return Err(PackageCatalogError::new(format!(
                    "catalog supplies module `{}` more than once",
                    pair[0].name
                )));
            }
        }
        let mut requirements = requirements.into_iter().collect::<Vec<_>>();
        requirements.sort_by(|left, right| {
            left.package
                .cmp(&right.package)
                .then_with(|| format!("{:?}", left.source).cmp(&format!("{:?}", right.source)))
        });
        requirements.dedup();
        for requirement in &requirements {
            validate_catalog_requirement(requirement)?;
            if !modules.iter().any(|module| {
                module.name == requirement.package
                    && module.package == requirement.package
                    && module.source_path == format!("{}.simi", requirement.package)
                    && module.visibility == CatalogModuleVisibility::Public
            }) {
                return Err(PackageCatalogError::new(format!(
                    "catalog requirement for package `{}` has no proven public root module",
                    requirement.package
                )));
            }
        }
        for module in &modules {
            if let Some(declared) = parse_requires(&module.source)
                .map_err(|error| PackageCatalogError::new(error.to_string()))?
            {
                for requirement in declared.entries {
                    if !requirements
                        .iter()
                        .any(|resolved| resolved.source == requirement.source)
                    {
                        return Err(PackageCatalogError::new(format!(
                            "catalog module `{}` has an unresolved requirement `{}`",
                            module.name, requirement.alias
                        )));
                    }
                }
            }
        }
        for module in &modules {
            if has_local_import(&module.source)? {
                return Err(PackageCatalogError::new(format!(
                    "catalog module `{}` still contains a package-relative import",
                    module.name
                )));
            }
        }
        Ok(Self {
            modules,
            requirements,
        })
    }

    /// Modules in deterministic identity order.
    pub fn modules(&self) -> &[CatalogModule] {
        &self.modules
    }
    /// Locked requirement identities accepted by this catalog.
    pub fn requirements(&self) -> &[CatalogRequirement] {
        &self.requirements
    }

    pub(crate) fn satisfies(&self, requirement: &Requirement) -> bool {
        self.requirements.iter().any(|catalog_requirement| {
            catalog_requirement.source == requirement.source
                && self
                    .modules
                    .iter()
                    .any(|module| module.name == catalog_requirement.package)
        })
    }
}

fn validate_catalog_module(module: &CatalogModule) -> Result<(), PackageCatalogError> {
    validate_package_name(&module.package)
        .map_err(|error| PackageCatalogError::new(error.to_string()))?;
    validate_relative_path(&module.source_path, "catalog module source path")
        .map_err(|error| PackageCatalogError::new(error.to_string()))?;

    match module.visibility {
        CatalogModuleVisibility::Public => {
            validate_module_name(&module.name, &module.package)
                .map_err(|error| PackageCatalogError::new(error.to_string()))?;
            let expected_path = format!("{}.simi", module.name);
            if module.source_path != expected_path {
                return Err(PackageCatalogError::new(format!(
                    "public catalog module `{}` must use source path `{expected_path}`",
                    module.name
                )));
            }
        }
        CatalogModuleVisibility::PackageLocal => {
            let expected_name = format!(
                "__simi_package_local__/{}/{}",
                module.package, module.source_path
            );
            if module.name != expected_name {
                return Err(PackageCatalogError::new(format!(
                    "package-local catalog module identity `{}` must equal `{expected_name}`",
                    module.name
                )));
            }
        }
    }
    Ok(())
}

fn validate_catalog_requirement(
    requirement: &CatalogRequirement,
) -> Result<(), PackageCatalogError> {
    validate_package_name(&requirement.package)
        .map_err(|error| PackageCatalogError::new(error.to_string()))
}

/// An invalid resolved catalog supplied by a host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageCatalogError {
    message: String,
}

impl PackageCatalogError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PackageCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PackageCatalogError {}

/// A parsed `simi.package.simi` manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageManifest {
    name: String,
    simi: String,
    modules: Vec<PackageModule>,
    native: Option<NativePackage>,
}

impl PackageManifest {
    /// Parse restricted static manifest data without evaluating Simi code.
    pub fn parse(source: &str) -> Result<Self, PackageManifestError> {
        let tokens = lex(source).map_err(|error| PackageManifestError::new(error.to_string()))?;
        let program =
            parse(tokens).map_err(|error| PackageManifestError::new(error.to_string()))?;
        let [statement] = program.items.as_slice() else {
            return Err(PackageManifestError::new(
                "package metadata must contain exactly one map expression",
            ));
        };
        let StmtKind::Expr(expression) = &statement.kind else {
            return Err(PackageManifestError::new(
                "package metadata must contain exactly one map expression",
            ));
        };
        let fields = map_fields(expression, "package metadata")?;
        reject_unknown_fields(
            &fields,
            &["name", "simi", "modules", "native"],
            "package metadata",
        )?;

        let name = required_string(&fields, "name", "package metadata")?;
        validate_package_name(&name)?;
        let simi = required_string(&fields, "simi", "package metadata")?;
        if simi.is_empty() {
            return Err(PackageManifestError::new(
                "package metadata field `simi` must not be empty",
            ));
        }

        let modules = required_list(&fields, "modules", "package metadata")?
            .iter()
            .map(|item| static_list_string(item, "package metadata field `modules`"))
            .collect::<Result<Vec<_>, _>>()?;
        if modules.is_empty() {
            return Err(PackageManifestError::new(
                "package metadata field `modules` must not be empty",
            ));
        }

        let mut seen = BTreeMap::new();
        let modules = modules
            .into_iter()
            .map(|module| {
                if seen.insert(module.clone(), ()).is_some() {
                    return Err(PackageManifestError::new(format!(
                        "package metadata declares module `{module}` more than once"
                    )));
                }
                PackageModule::new(module, &name)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !modules.iter().any(|module| module.name == name) {
            return Err(PackageManifestError::new(format!(
                "package metadata must export root module `{name}`"
            )));
        }

        let native = fields
            .get("native")
            .map(|native| NativePackage::parse(native))
            .transpose()?;

        Ok(Self {
            name,
            simi,
            modules,
            native,
        })
    }

    /// Stable package identity used as the root public module name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The compatible Simi runtime revision declared by the package.
    pub fn simi(&self) -> &str {
        &self.simi
    }

    /// Public source modules exported by the package, in manifest order.
    pub fn modules(&self) -> &[PackageModule] {
        &self.modules
    }

    /// Optional later-native extension metadata. Parsing it never builds or loads native code.
    pub fn native(&self) -> Option<&NativePackage> {
        self.native.as_ref()
    }
}

/// A statically validated package root, its declared public modules, and reachable local sources.
///
/// This loader reads `simi.package.simi`, manifest-declared public modules, and reachable literal
/// local sources. It neither resolves requirements nor discovers generated, native, or unreachable
/// private sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageTree {
    root: PathBuf,
    manifest: PackageManifest,
    manifest_source: String,
    modules: Vec<PackageSource>,
    local_sources: Vec<LocalPackageSource>,
}

impl PackageTree {
    /// Load a package root without evaluating any Simi source.
    ///
    /// The root, manifest, declared public sources, and reachable literal local sources must be
    /// non-symlink regular directories or files below the canonical root. Public source units are
    /// stored by canonical source path order so digest inputs are independent of filesystem
    /// iteration order.
    pub fn load(root: impl AsRef<Path>) -> Result<Self, PackageTreeError> {
        let root = canonical_package_root(root.as_ref())?;
        let manifest_source = read_package_source(&root, "simi.package.simi", "package manifest")?;
        let manifest = PackageManifest::parse(&manifest_source)
            .map_err(|error| PackageTreeError::new(format!("invalid package manifest: {error}")))?;

        let mut modules = manifest.modules().to_vec();
        modules.sort_by(|left, right| left.source_path.cmp(&right.source_path));
        let modules = modules
            .into_iter()
            .map(|module| {
                let source = read_package_source(
                    &root,
                    module.source_path(),
                    &format!("declared public module `{}`", module.name()),
                )?;
                let local_imports = local_imports(&source, module.source_path())?;
                Ok(PackageSource {
                    module,
                    source,
                    local_imports,
                })
            })
            .collect::<Result<Vec<_>, PackageTreeError>>()?;
        let mut local_sources = BTreeMap::new();
        let mut loaded_paths = modules
            .iter()
            .map(|module| module.module.source_path().to_owned())
            .collect::<BTreeSet<_>>();
        for module in &modules {
            load_local_sources(
                &root,
                module.module.source_path(),
                &module.local_imports,
                &mut loaded_paths,
                &mut local_sources,
            )?;
        }
        let local_sources = local_sources
            .into_iter()
            .map(|(source_path, source)| LocalPackageSource {
                local_imports: local_imports(&source, &source_path)
                    .expect("loaded local source was already validated"),
                source_path,
                source,
            })
            .collect();

        Ok(Self {
            root,
            manifest,
            manifest_source,
            modules,
            local_sources,
        })
    }

    /// Canonical package-root directory used for layout validation.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Parsed, restricted package metadata.
    pub fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }

    /// Declared public source units in canonical source-path order.
    pub fn modules(&self) -> &[PackageSource] {
        &self.modules
    }

    /// Reachable literal package-local source units in canonical source-path order.
    pub(crate) fn local_sources(&self) -> &[LocalPackageSource] {
        &self.local_sources
    }

    pub(crate) fn public_module_for_source_path(&self, source_path: &str) -> Option<&str> {
        self.modules
            .iter()
            .find(|module| module.module.source_path() == source_path)
            .map(|module| module.module.name())
    }

    /// Deterministic source-tree digest inputs.
    ///
    /// The manifest is first, followed by public and reachable local sources sorted by path. The
    /// exact UTF-8 source bytes are retained; callers choosing a digest algorithm must frame each
    /// path and byte sequence unambiguously. Unreachable private, generated, and native files are
    /// absent.
    pub fn digest_inputs(&self) -> Vec<PackageTreeFile<'_>> {
        std::iter::once(PackageTreeFile {
            path: "simi.package.simi",
            bytes: self.manifest_source.as_bytes(),
        })
        .chain(self.modules.iter().map(|module| PackageTreeFile {
            path: module.module.source_path(),
            bytes: module.source.as_bytes(),
        }))
        .chain(self.local_sources.iter().map(|source| PackageTreeFile {
            path: &source.source_path,
            bytes: source.source.as_bytes(),
        }))
        .collect()
    }
}

/// A declared public module and its exact UTF-8 source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageSource {
    module: PackageModule,
    source: String,
    local_imports: Vec<LocalImport>,
}

impl PackageSource {
    /// Declared public module identity and canonical source path.
    pub fn module(&self) -> &PackageModule {
        &self.module
    }

    /// Exact UTF-8 source text read from the declared public module file.
    pub fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn local_imports(&self) -> &[LocalImport] {
        &self.local_imports
    }
}

/// A reachable private source unit loaded through a literal package-local import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalPackageSource {
    source_path: String,
    source: String,
    local_imports: Vec<LocalImport>,
}

impl LocalPackageSource {
    pub(crate) fn source_path(&self) -> &str {
        &self.source_path
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn local_imports(&self) -> &[LocalImport] {
        &self.local_imports
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalImport {
    pub(crate) path: String,
    pub(crate) span: crate::span::Span,
    callee_span: crate::span::Span,
}

/// One ordered path-and-byte input to a future source-tree digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackageTreeFile<'a> {
    /// Package-root-relative slash-separated path.
    pub path: &'a str,
    /// Exact file bytes, validated as UTF-8 by [`PackageTree::load`].
    pub bytes: &'a [u8],
}

/// A rejected static package-root layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageTreeError {
    message: String,
}

impl PackageTreeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PackageTreeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PackageTreeError {}

fn canonical_package_root(root: &Path) -> Result<PathBuf, PackageTreeError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|_| PackageTreeError::new("package root is missing or unreadable"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PackageTreeError::new(
            "package root must be a non-symlink directory",
        ));
    }
    root.canonicalize()
        .map_err(|_| PackageTreeError::new("package root is missing or unreadable"))
}

fn read_package_source(
    root: &Path,
    relative_path: &str,
    description: &str,
) -> Result<String, PackageTreeError> {
    let components = relative_path.split('/').collect::<Vec<_>>();
    let mut path = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        path.push(component);
        let metadata = fs::symlink_metadata(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => PackageTreeError::new(format!(
                "package tree is missing {description} `{relative_path}`"
            )),
            _ => PackageTreeError::new(format!(
                "package tree cannot inspect {description} `{relative_path}`"
            )),
        })?;
        if metadata.file_type().is_symlink() {
            return Err(PackageTreeError::new(format!(
                "package tree does not permit symlink components in {description} `{relative_path}`"
            )));
        }
        if index + 1 != components.len() && !metadata.is_dir() {
            return Err(PackageTreeError::new(format!(
                "package tree component `{component}` for {description} `{relative_path}` must be a directory"
            )));
        }
    }

    let metadata = fs::metadata(&path).map_err(|_| {
        PackageTreeError::new(format!(
            "package tree cannot inspect {description} `{relative_path}`"
        ))
    })?;
    if !metadata.is_file() {
        return Err(PackageTreeError::new(format!(
            "package tree {description} `{relative_path}` must be a regular file"
        )));
    }
    let canonical = path.canonicalize().map_err(|_| {
        PackageTreeError::new(format!(
            "package tree cannot inspect {description} `{relative_path}`"
        ))
    })?;
    if !canonical.starts_with(root) {
        return Err(PackageTreeError::new(format!(
            "package tree {description} `{relative_path}` escapes the package root"
        )));
    }
    let bytes = fs::read(&path).map_err(|_| {
        PackageTreeError::new(format!(
            "package tree cannot read {description} `{relative_path}`"
        ))
    })?;
    String::from_utf8(bytes).map_err(|_| {
        PackageTreeError::new(format!(
            "package tree {description} `{relative_path}` must be valid UTF-8"
        ))
    })
}

fn load_local_sources(
    root: &Path,
    source_path: &str,
    imports: &[LocalImport],
    loaded_paths: &mut BTreeSet<String>,
    sources: &mut BTreeMap<String, String>,
) -> Result<(), PackageTreeError> {
    for import in imports {
        let target = local_source_path(source_path, &import.path).map_err(|message| {
            PackageTreeError::new(format!(
                "package-local import `{}` in `{source_path}` {message}",
                import.path
            ))
        })?;
        if !loaded_paths.insert(target.clone()) {
            continue;
        }
        let source = read_package_source(root, &target, "package-local source")?;
        let child_imports = local_imports(&source, &target)?;
        sources.insert(target.clone(), source);
        load_local_sources(root, &target, &child_imports, loaded_paths, sources)?;
    }
    Ok(())
}

pub(crate) fn source_has_package_relative_import(source: &str) -> Result<bool, String> {
    has_local_import(source).map_err(|error| error.to_string())
}

fn has_local_import(source: &str) -> Result<bool, PackageCatalogError> {
    // Avoid running the legacy package-source walker for ordinary source that cannot contain a
    // literal local import. The walker remains necessary to distinguish the builtin `require`
    // from a shadowed name when this syntactic prefix is present.
    if !source.contains("require") || !source.contains("\"./") {
        return Ok(false);
    }
    local_imports(source, "catalog source")
        .map(|imports| !imports.is_empty())
        .map_err(|error| PackageCatalogError::new(error.to_string()))
}

fn local_imports(source: &str, source_path: &str) -> Result<Vec<LocalImport>, PackageTreeError> {
    let tokens = lex(source).map_err(|error| {
        PackageTreeError::new(format!(
            "invalid package-local source `{source_path}`: {error}"
        ))
    })?;
    let program = parse(tokens).map_err(|error| {
        PackageTreeError::new(format!(
            "invalid package-local source `{source_path}`: {error}"
        ))
    })?;
    let mut imports = Vec::new();
    for statement in &program.items {
        collect_local_imports_stmt(statement, &mut imports);
    }
    let database = AnalysisDatabase::default();
    let file = database.add_file(source);
    let resolution = resolve(&database, file);
    imports.retain(|import| {
        resolution
            .symbol_at(import.callee_span.start)
            .and_then(|symbol| resolution.symbol_data(symbol))
            .is_some_and(|data| data.builtin && data.name == "require")
    });
    Ok(imports)
}

fn collect_local_imports_stmt(statement: &crate::ast::Stmt, imports: &mut Vec<LocalImport>) {
    match &statement.kind {
        StmtKind::Function { body, .. } => collect_local_imports_block(body, imports),
        StmtKind::Let { value, .. } | StmtKind::Expr(value) => {
            collect_local_imports_expr(value, imports)
        }
    }
}

fn collect_local_imports_block(block: &crate::ast::Block, imports: &mut Vec<LocalImport>) {
    for statement in &block.items {
        collect_local_imports_stmt(statement, imports);
    }
}

fn collect_local_imports_expr(expression: &Expr, imports: &mut Vec<LocalImport>) {
    match &expression.kind {
        ExprKind::Call { callee, args }
            if matches!(&callee.kind, ExprKind::Variable(name) if name == "require")
                && args.len() == 1 =>
        {
            if let ExprKind::String(path) = &args[0].kind
                && path.starts_with("./")
            {
                imports.push(LocalImport {
                    path: path.clone(),
                    span: args[0].span,
                    callee_span: callee.span,
                });
            }
            collect_local_imports_expr(callee, imports);
            for argument in args {
                collect_local_imports_expr(argument, imports);
            }
        }
        ExprKind::List(elements) => {
            for element in elements {
                match element {
                    ListElement::Value(value) | ListElement::Spread(value) => {
                        collect_local_imports_expr(value, imports);
                    }
                }
            }
        }
        ExprKind::Bytes(segments) => {
            for segment in segments {
                if let crate::ast::BytesSegment::Value(value) = segment {
                    collect_local_imports_expr(value, imports);
                }
            }
        }
        ExprKind::Map(entries) => {
            for (key, value) in entries {
                collect_local_imports_expr(key, imports);
                collect_local_imports_expr(value, imports);
            }
        }
        ExprKind::Function { body, .. } | ExprKind::Block(body) => {
            collect_local_imports_block(body, imports);
        }
        ExprKind::Assign { target, value } => {
            match &target.kind {
                crate::ast::AssignmentTargetKind::Variable(_) => {}
                crate::ast::AssignmentTargetKind::Field { object, .. } => {
                    collect_local_imports_expr(object, imports);
                }
                crate::ast::AssignmentTargetKind::Index { object, key } => {
                    collect_local_imports_expr(object, imports);
                    collect_local_imports_expr(key, imports);
                }
            }
            collect_local_imports_expr(value, imports);
        }
        ExprKind::Raise { value }
        | ExprKind::NilPropagate { value }
        | ExprKind::Unary { value, .. } => collect_local_imports_expr(value, imports),
        ExprKind::Try { protected, clauses } => {
            collect_local_imports_block(protected, imports);
            for clause in clauses {
                if let Some(guard) = &clause.guard {
                    collect_local_imports_expr(guard, imports);
                }
                collect_local_imports_block(&clause.body, imports);
            }
        }
        ExprKind::Case { value, clauses } => {
            collect_local_imports_expr(value, imports);
            for clause in clauses {
                if let Some(guard) = &clause.guard {
                    collect_local_imports_expr(guard, imports);
                }
                collect_local_imports_block(&clause.body, imports);
            }
        }
        ExprKind::If {
            branches,
            else_branch,
        } => {
            for (condition, body) in branches {
                collect_local_imports_expr(condition, imports);
                collect_local_imports_block(body, imports);
            }
            if let Some(body) = else_branch {
                collect_local_imports_block(body, imports);
            }
        }
        ExprKind::Field { object, .. } => collect_local_imports_expr(object, imports),
        ExprKind::Index { object, key } => {
            collect_local_imports_expr(object, imports);
            collect_local_imports_expr(key, imports);
        }
        ExprKind::Binary { left, right, .. } => {
            collect_local_imports_expr(left, imports);
            collect_local_imports_expr(right, imports);
        }
        ExprKind::Pipeline { input, stages } => {
            collect_local_imports_expr(input, imports);
            for stage in stages {
                collect_local_imports_expr(&stage.callee, imports);
                for argument in &stage.args {
                    collect_local_imports_expr(argument, imports);
                }
            }
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::String(_)
        | ExprKind::Bool(_)
        | ExprKind::Nil
        | ExprKind::Panic { .. }
        | ExprKind::Todo { .. }
        | ExprKind::Variable(_) => {}
        ExprKind::Call { callee, args } => {
            collect_local_imports_expr(callee, imports);
            for argument in args {
                collect_local_imports_expr(argument, imports);
            }
        }
    }
}

pub(crate) fn local_source_path(source_path: &str, import: &str) -> Result<String, &'static str> {
    let Some(import) = import.strip_prefix("./") else {
        return Err("must begin with `./`");
    };
    if import.is_empty()
        || import.contains('\\')
        || import
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err("must be a non-empty slash-separated path without traversal");
    }
    let parent = source_path
        .rsplit_once('/')
        .map_or("", |(parent, _)| parent);
    Ok(if parent.is_empty() {
        import.to_owned()
    } else {
        format!("{parent}/{import}")
    })
}

/// A public module with a canonical package-root-relative source path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageModule {
    name: String,
    source_path: String,
}

impl PackageModule {
    fn new(name: String, package: &str) -> Result<Self, PackageManifestError> {
        validate_module_name(&name, package)?;
        Ok(Self {
            source_path: format!("{name}.simi"),
            name,
        })
    }

    /// Canonical public module identity, such as `polars` or `polars/csv`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Package-root-relative source path, such as `polars.simi` or `polars/csv.simi`.
    pub fn source_path(&self) -> &str {
        &self.source_path
    }
}

/// Deferred metadata for a native extension crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePackage {
    manifest_path: String,
}

impl NativePackage {
    fn parse(expression: &Expr) -> Result<Self, PackageManifestError> {
        let fields = map_fields(expression, "package metadata field `native`")?;
        reject_unknown_fields(&fields, &["manifest"], "package metadata field `native`")?;
        let manifest_path =
            required_string(&fields, "manifest", "package metadata field `native`")?;
        validate_relative_path(&manifest_path, "package metadata native manifest")?;
        if !manifest_path.ends_with("Cargo.toml") {
            return Err(PackageManifestError::new(
                "package metadata native manifest must name a Cargo.toml file",
            ));
        }
        Ok(Self { manifest_path })
    }

    /// Package-root-relative Cargo manifest path. This is descriptive until native runners exist.
    pub fn manifest_path(&self) -> &str {
        &self.manifest_path
    }
}

/// A rejected restricted-metadata form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageManifestError {
    message: String,
}

impl PackageManifestError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PackageManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PackageManifestError {}

fn map_fields<'a>(
    expression: &'a Expr,
    context: &str,
) -> Result<BTreeMap<String, &'a Expr>, PackageManifestError> {
    let ExprKind::Map(entries) = &expression.kind else {
        return Err(PackageManifestError::new(format!(
            "{context} must be a map"
        )));
    };
    let mut fields = BTreeMap::new();
    for (key, value) in entries {
        let key = static_string(key, context)?;
        if fields.insert(key.clone(), value).is_some() {
            return Err(PackageManifestError::new(format!(
                "{context} declares field `{key}` more than once"
            )));
        }
    }
    Ok(fields)
}

fn reject_unknown_fields(
    fields: &BTreeMap<String, &Expr>,
    allowed: &[&str],
    context: &str,
) -> Result<(), PackageManifestError> {
    if let Some(field) = fields
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(PackageManifestError::new(format!(
            "{context} does not permit field `{field}`"
        )));
    }
    Ok(())
}

fn required_string(
    fields: &BTreeMap<String, &Expr>,
    name: &str,
    context: &str,
) -> Result<String, PackageManifestError> {
    let expression = fields
        .get(name)
        .ok_or_else(|| PackageManifestError::new(format!("{context} requires field `{name}`")))?;
    static_string(expression, &format!("{context} field `{name}`"))
}

fn required_list<'a>(
    fields: &'a BTreeMap<String, &'a Expr>,
    name: &str,
    context: &str,
) -> Result<&'a [ListElement], PackageManifestError> {
    let expression = fields
        .get(name)
        .ok_or_else(|| PackageManifestError::new(format!("{context} requires field `{name}`")))?;
    let ExprKind::List(items) = &expression.kind else {
        return Err(PackageManifestError::new(format!(
            "{context} field `{name}` must be a list"
        )));
    };
    Ok(items)
}

fn static_string(expression: &Expr, context: &str) -> Result<String, PackageManifestError> {
    let ExprKind::String(value) = &expression.kind else {
        return Err(PackageManifestError::new(format!(
            "{context} must contain only static string values"
        )));
    };
    Ok(value.clone())
}

fn static_list_string(
    element: &ListElement,
    context: &str,
) -> Result<String, PackageManifestError> {
    match element {
        ListElement::Value(expression) => static_string(expression, context),
        ListElement::Spread(_) => Err(PackageManifestError::new(format!(
            "{context} must contain only static string values"
        ))),
    }
}

fn validate_package_name(name: &str) -> Result<(), PackageManifestError> {
    if !valid_component(name) {
        return Err(PackageManifestError::new(format!(
            "package metadata name `{name}` must be a lowercase module component"
        )));
    }
    Ok(())
}

fn validate_module_name(name: &str, package: &str) -> Result<(), PackageManifestError> {
    if name != package && !name.starts_with(&format!("{package}/")) {
        return Err(PackageManifestError::new(format!(
            "public module `{name}` must equal package `{package}` or begin `{package}/`"
        )));
    }
    if !name.split('/').all(valid_component) {
        return Err(PackageManifestError::new(format!(
            "public module `{name}` must contain only lowercase module components"
        )));
    }
    Ok(())
}

fn valid_component(component: &str) -> bool {
    let mut chars = component.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn validate_relative_path(path: &str, context: &str) -> Result<(), PackageManifestError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(PackageManifestError::new(format!(
            "{context} must be a package-root-relative slash-separated path"
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "package/tests.rs"]
mod tests;
