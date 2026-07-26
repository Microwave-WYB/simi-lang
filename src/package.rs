//! Restricted static metadata for reusable Simi packages.
//!
//! A package manifest is parsed as Simi syntax but never evaluated. Resolver work belongs to the
//! package-resolution layer; this module only establishes the deterministic, capability-free
//! package-tree contract that a resolver consumes.

use std::{collections::BTreeMap, error::Error, fmt};

use crate::{
    ast::{Expr, ExprKind, StmtKind},
    lexer::lex,
    parser::parse,
};

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
            .map(|item| static_string(item, "package metadata field `modules`"))
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
) -> Result<&'a [Expr], PackageManifestError> {
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
