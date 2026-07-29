use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use sha2::{Digest, Sha256};

use super::lock::{LockFile, LockSource, LockedRequirement};
use super::{
    CatalogModule, CatalogModuleVisibility, CatalogRequirement, LocalImport, PackageCatalog,
    PackageTree, Requirement, RequirementSource, Requires, local_source_path, parse_requires,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionMode {
    Update,
    Locked,
    Offline,
    /// Generate a new lockfile from local path sources and an already validated Git cache.
    OfflineUpdate,
}

/// Fully resolved source modules and the canonical lockfile that describes them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedScript {
    /// The deterministic, rewritten source catalog ready for explicit engine installation.
    pub catalog: PackageCatalog,
    /// Canonical lockfile content describing the resolved graph.
    pub lockfile: String,
}

/// Resolve a script's static package requirements before it is evaluated.
///
/// This function owns filesystem and Git access. It deliberately returns source modules rather
/// than evaluating them so `Engine::eval` remains capability-free with respect to packages.
pub fn resolve_script(path: &Path, mode: ResolutionMode) -> Result<ResolvedScript, String> {
    resolve_script_with_cache(path, mode, &default_git_cache()?)
}

pub fn lock_path(path: &Path) -> PathBuf {
    let mut lock = path.to_path_buf();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("app.simi");
    lock.set_file_name(format!(
        "{}.lock.simi",
        name.strip_suffix(".simi").unwrap_or(name)
    ));
    lock
}

fn resolve_script_with_cache(
    path: &Path,
    mode: ResolutionMode,
    cache: &Path,
) -> Result<ResolvedScript, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("cannot read source `{}`: {error}", path.display()))?;
    let source_path = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "source path `{}` must have a UTF-8 filename",
                path.display()
            )
        })?
        .to_owned();
    let source_digest = digest_one(&source_path, source.as_bytes());
    let root_requires = parse_requires(&source).map_err(|error| error.to_string())?;
    let lock_path = lock_path(path);
    let locked = match mode {
        ResolutionMode::Update | ResolutionMode::OfflineUpdate => None,
        ResolutionMode::Locked | ResolutionMode::Offline => {
            let contents = fs::read_to_string(&lock_path).map_err(|error| {
                format!(
                    "cannot read required lockfile `{}`: {error}",
                    lock_path.display()
                )
            })?;
            let lock = LockFile::parse(&contents)?;
            if lock.render() != contents {
                return Err(format!(
                    "lockfile `{}` is not canonical",
                    lock_path.display()
                ));
            }
            if lock.source
                != (LockSource {
                    path: source_path.clone(),
                    digest: source_digest.clone(),
                })
            {
                return Err(format!(
                    "lockfile `{}` does not match the root source",
                    lock_path.display()
                ));
            }
            Some(lock)
        }
    };
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut context = ResolveContext {
        mode,
        cache,
        locked: locked.as_ref(),
        visiting: BTreeMap::new(),
        entries: BTreeMap::new(),
        modules: BTreeMap::new(),
    };
    if let Some(requires) = root_requires.as_ref() {
        context.resolve_requirements(requires, base)?;
    }
    let catalog_requirements = context
        .entries
        .values()
        .map(|node| {
            CatalogRequirement::new(
                node.requirement.package.clone(),
                node.requirement.source.clone(),
            )
        })
        .collect::<Vec<_>>();
    let lock = LockFile {
        source: LockSource {
            path: source_path,
            digest: source_digest,
        },
        requirements: context
            .entries
            .into_iter()
            .map(|(package, node)| (package, node.requirement))
            .collect(),
    };
    if let Some(locked) = locked.as_ref()
        && lock != *locked
    {
        return Err(format!(
            "lockfile `{}` does not match resolved requirements",
            lock_path.display()
        ));
    }
    let catalog = PackageCatalog::new(context.modules.into_values(), catalog_requirements)
        .map_err(|error| format!("invalid resolved package catalog: {error}"))?;
    Ok(ResolvedScript {
        catalog,
        lockfile: lock.render(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ResolvedSource {
    Path(PathBuf),
    Git { url: String, commit: String },
}

#[derive(Clone, Debug)]
struct ResolvedNode {
    requirement: LockedRequirement,
    source: ResolvedSource,
}

struct ResolveContext<'a> {
    mode: ResolutionMode,
    cache: &'a Path,
    locked: Option<&'a LockFile>,
    /// Package identities currently being expanded. Requirement aliases are deliberately absent:
    /// they are lexical to the source that declares them, not graph identities.
    visiting: BTreeMap<String, ResolvedNode>,
    /// Fully resolved graph nodes keyed by manifest package identity.
    entries: BTreeMap<String, ResolvedNode>,
    modules: BTreeMap<String, CatalogModule>,
}

impl ResolveContext<'_> {
    fn resolve_requirements(&mut self, requires: &Requires, base: &Path) -> Result<(), String> {
        for requirement in &requires.entries {
            self.resolve_requirement(requirement, base)?;
        }
        Ok(())
    }

    fn resolve_requirement(
        &mut self,
        requirement: &Requirement,
        base: &Path,
    ) -> Result<(), String> {
        validate_source(&requirement.source)?;
        let (root, commit, source) = match &requirement.source {
            RequirementSource::Path { path } => (resolve_path_requirement(base, path)?, None, None),
            RequirementSource::Git { git, rev } => {
                let commit = self.git_commit(git, rev)?;
                let root = self.git_checkout(git, &commit)?;
                let source = ResolvedSource::Git {
                    url: canonical_git_url(git)?,
                    commit: commit.clone(),
                };
                (root, Some(commit), Some(source))
            }
        };
        let tree = PackageTree::load(&root)
            .map_err(|error| format!("invalid package `{}`: {error}", requirement.alias))?;
        let package = tree.manifest().name().to_owned();
        let source = source.unwrap_or_else(|| ResolvedSource::Path(tree.root().to_owned()));
        let node = ResolvedNode {
            requirement: LockedRequirement {
                source: requirement.source.clone(),
                package: package.clone(),
                commit,
                tree_digest: digest_tree(&tree),
            },
            source,
        };
        self.validate_locked_node(&node)?;

        if let Some(existing) = self.entries.get(&package) {
            return self.reuse_or_conflict(&package, existing, &node);
        }
        if let Some(existing) = self.visiting.get(&package) {
            self.reuse_or_conflict(&package, existing, &node)?;
            return Err(format!("cyclic package requirement involving `{package}`"));
        }

        self.visiting.insert(package.clone(), node.clone());
        let resolved = (|| {
            for module in tree.modules() {
                let name = module.module().name().to_owned();
                let source = rewrite_local_imports(
                    &tree,
                    &package,
                    module.module().source_path(),
                    module.source(),
                    module.local_imports(),
                )?;
                let module = CatalogModule::new(
                    name.clone(),
                    source,
                    package.clone(),
                    module.module().source_path(),
                    CatalogModuleVisibility::Public,
                );
                if let Some(existing) = self.modules.get(&name)
                    && existing.source() != module.source()
                {
                    return Err(format!(
                        "public module `{name}` is supplied by more than one package"
                    ));
                }
                self.modules.insert(name, module);
            }
            for source in tree.local_sources() {
                let source_path = source.source_path().to_owned();
                let name = local_module_name(&package, &source_path);
                let source = rewrite_local_imports(
                    &tree,
                    &package,
                    &source_path,
                    source.source(),
                    source.local_imports(),
                )?;
                let module = CatalogModule::new(
                    name.clone(),
                    source,
                    package.clone(),
                    source_path,
                    CatalogModuleVisibility::PackageLocal,
                );
                if self.modules.insert(name.clone(), module).is_some() {
                    return Err(format!(
                        "package-local module identity `{name}` is supplied more than once"
                    ));
                }
            }
            self.resolve_tree_requirements(&tree)
        })();
        self.visiting.remove(&package);
        resolved?;
        self.entries.insert(package, node);
        Ok(())
    }

    fn reuse_or_conflict(
        &self,
        package: &str,
        existing: &ResolvedNode,
        node: &ResolvedNode,
    ) -> Result<(), String> {
        if existing.requirement.source != node.requirement.source || existing.source != node.source
        {
            return Err(format!(
                "package `{package}` resolves from conflicting declared sources"
            ));
        }
        Ok(())
    }

    fn validate_locked_node(&self, node: &ResolvedNode) -> Result<(), String> {
        let Some(lock) = self.locked else {
            return Ok(());
        };
        let Some(locked) = lock.requirements.get(&node.requirement.package) else {
            return Err(format!(
                "lockfile is missing package `{}`",
                node.requirement.package
            ));
        };
        if locked.source != node.requirement.source {
            return Err(format!(
                "lockfile source for package `{}` differs from the declaration",
                node.requirement.package
            ));
        }
        if locked.commit != node.requirement.commit
            || locked.tree_digest != node.requirement.tree_digest
        {
            return Err(format!(
                "lockfile entry for package `{}` does not match its package tree",
                node.requirement.package
            ));
        }
        Ok(())
    }

    fn resolve_tree_requirements(&mut self, tree: &PackageTree) -> Result<(), String> {
        for (source_path, source) in tree
            .modules()
            .iter()
            .map(|module| (module.module().source_path(), module.source()))
            .chain(
                tree.local_sources()
                    .iter()
                    .map(|source| (source.source_path(), source.source())),
            )
        {
            let Some(requires) = parse_requires(source)
                .map_err(|error| format!("invalid requirements in `{source_path}`: {error}"))?
            else {
                continue;
            };
            let source_path = tree.root().join(source_path);
            let base = source_path
                .parent()
                .expect("package module source has a parent");
            self.resolve_requirements(&requires, base)?;
        }
        Ok(())
    }

    fn locked_requirement_for_source(
        &self,
        source: &RequirementSource,
    ) -> Option<&LockedRequirement> {
        self.locked?
            .requirements
            .values()
            .find(|locked| locked.source == *source)
    }

    fn git_commit(&self, git_url: &str, rev: &str) -> Result<String, String> {
        match self.mode {
            ResolutionMode::Update => {
                let bare = self.ensure_git_cache(git_url, true)?;
                git(&[
                    "-C",
                    bare.to_str().ok_or("non-UTF-8 Git cache path")?,
                    "fetch",
                    "--no-tags",
                    "--force",
                    "origin",
                    "--",
                    rev,
                ])?;
                let commit = git(&[
                    "-C",
                    bare.to_str().ok_or("non-UTF-8 Git cache path")?,
                    "rev-parse",
                    "FETCH_HEAD^{commit}",
                ])?;
                Ok(commit.trim().to_owned())
            }
            ResolutionMode::Locked | ResolutionMode::Offline => {
                let source = RequirementSource::Git {
                    git: git_url.to_owned(),
                    rev: rev.to_owned(),
                };
                let locked = self.locked_requirement_for_source(&source).ok_or_else(|| {
                    format!("lockfile has no Git requirement for `{git_url}` at `{rev}`")
                })?;
                let commit = locked
                    .commit
                    .as_ref()
                    .ok_or_else(|| "lockfile Git requirement is missing its commit".to_owned())?
                    .clone();
                let bare = self.ensure_git_cache(git_url, false)?;
                git(&[
                    "-C",
                    bare.to_str().ok_or("non-UTF-8 Git cache path")?,
                    "cat-file",
                    "-e",
                    &format!("{commit}^{{commit}}"),
                ])
                .map_err(|_| {
                    format!("cached Git repository does not contain locked commit `{commit}`")
                })?;
                Ok(commit)
            }
            ResolutionMode::OfflineUpdate => {
                let bare = self.ensure_git_cache(git_url, false)?;
                let commit = git(&[
                    "-C",
                    bare.to_str().ok_or("non-UTF-8 Git cache path")?,
                    "rev-parse",
                    "--verify",
                    &format!("{rev}^{{commit}}"),
                ])?;
                Ok(commit.trim().to_owned())
            }
        }
    }

    fn git_checkout(&self, git_url: &str, commit: &str) -> Result<PathBuf, String> {
        let canonical_url = canonical_git_url(git_url)?;
        let url_key = sha256(canonical_url.as_bytes());
        let checkout = self.cache.join("checkouts").join(url_key).join(commit);
        let checkout_metadata = match fs::symlink_metadata(&checkout) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(format!(
                    "cannot inspect Git checkout cache `{}`: {error}",
                    checkout.display()
                ));
            }
        };
        if self.mode == ResolutionMode::Offline && checkout_metadata.is_none() {
            return Err(format!(
                "offline mode requires cached checkout for Git commit `{commit}`"
            ));
        }
        if let Some(metadata) = checkout_metadata {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(format!(
                    "Git checkout cache `{}` is not a safe directory",
                    checkout.display()
                ));
            }
            fs::remove_dir_all(&checkout).map_err(|error| {
                format!(
                    "cannot reset Git checkout cache `{}`: {error}",
                    checkout.display()
                )
            })?;
        }

        let bare = self.ensure_git_cache(git_url, false)?;
        let parent = checkout.parent().expect("checkout has a parent");
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create Git checkout cache: {error}"))?;
        git(&[
            "clone",
            "--no-checkout",
            "--no-local",
            bare.to_str().ok_or("non-UTF-8 Git cache path")?,
            checkout.to_str().ok_or("non-UTF-8 Git checkout path")?,
        ])?;
        git(&[
            "-C",
            checkout.to_str().ok_or("non-UTF-8 Git checkout path")?,
            "checkout",
            "--detach",
            "--force",
            commit,
        ])?;
        let head = git(&[
            "-C",
            checkout.to_str().ok_or("non-UTF-8 Git checkout path")?,
            "rev-parse",
            "HEAD^{commit}",
        ])?;
        if head.trim() != commit {
            return Err(format!(
                "cached checkout does not match locked Git commit `{commit}`"
            ));
        }
        Ok(checkout)
    }

    fn ensure_git_cache(&self, git_url: &str, allow_network: bool) -> Result<PathBuf, String> {
        let canonical_url = canonical_git_url(git_url)?;
        let bare = self
            .cache
            .join("git")
            .join(sha256(canonical_url.as_bytes()));
        if !bare.is_dir() {
            if !allow_network {
                return Err(format!("Git cache is missing `{canonical_url}`"));
            }
            fs::create_dir_all(bare.parent().expect("git cache has parent"))
                .map_err(|error| format!("cannot create Git cache: {error}"))?;
            git(&[
                "clone",
                "--bare",
                "--no-local",
                &canonical_url,
                bare.to_str().ok_or("non-UTF-8 Git cache path")?,
            ])?;
        }
        let remote = git(&[
            "-C",
            bare.to_str().ok_or("non-UTF-8 Git cache path")?,
            "remote",
            "get-url",
            "origin",
        ])
        .map_err(|_| format!("Git cache `{}` has no origin remote", bare.display()))?;
        if canonical_git_url(remote.trim())? != canonical_url {
            return Err(format!("Git cache URL mismatch for `{canonical_url}`"));
        }
        Ok(bare)
    }
}

fn validate_source(source: &RequirementSource) -> Result<(), String> {
    match source {
        RequirementSource::Git { git, rev } if git.is_empty() || rev.is_empty() => {
            Err("Git requirement `git` and `rev` must not be empty".to_owned())
        }
        RequirementSource::Path { path } if !valid_relative_path(path) => {
            Err("path requirement must be package-root-relative".to_owned())
        }
        _ => Ok(()),
    }
}

/// Resolve a path source without allowing any component to redirect outside its declaration.
fn resolve_path_requirement(base: &Path, path: &str) -> Result<PathBuf, String> {
    if !valid_relative_path(path) || Path::new(path).is_absolute() {
        return Err("path requirement must be package-root-relative".to_owned());
    }
    let base = base.canonicalize().map_err(|error| {
        format!(
            "cannot canonicalize declaring source directory `{}`: {error}",
            base.display()
        )
    })?;
    let mut candidate = base.clone();
    for component in path.split('/') {
        candidate.push(component);
        let metadata = fs::symlink_metadata(&candidate).map_err(|error| {
            format!(
                "cannot inspect path requirement `{path}` from `{}`: {error}",
                base.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "path requirement `{path}` contains a symlink component"
            ));
        }
    }
    let canonical = candidate.canonicalize().map_err(|error| {
        format!(
            "cannot canonicalize path requirement `{path}` from `{}`: {error}",
            base.display()
        )
    })?;
    if !canonical.starts_with(&base) {
        return Err(format!(
            "path requirement `{path}` escapes declaring source directory `{}`",
            base.display()
        ));
    }
    Ok(canonical)
}

fn default_git_cache() -> Result<PathBuf, String> {
    if let Some(cache) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(cache).join("simi"));
    }
    let home = env::var_os("HOME").ok_or("cannot determine cache directory: HOME is unset")?;
    Ok(PathBuf::from(home).join(".cache").join("simi"))
}

fn canonical_git_url(url: &str) -> Result<String, String> {
    let result = url.trim().trim_end_matches('/');
    if result.is_empty() {
        return Err("Git requirement URL must not be empty".to_owned());
    }
    Ok(result.to_owned())
}

fn git(arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("cannot invoke git: {error}"))?;
    if output.status.success() {
        return String::from_utf8(output.stdout)
            .map_err(|_| "git produced non-UTF-8 output".to_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(if stderr.is_empty() {
        format!("git command failed with status {}", output.status)
    } else {
        format!("git command failed: {stderr}")
    })
}

pub(super) fn digest_one(path: &str, contents: &[u8]) -> String {
    digest_entries([(path, contents)])
}

fn rewrite_local_imports(
    tree: &PackageTree,
    package: &str,
    source_path: &str,
    source: &str,
    imports: &[LocalImport],
) -> Result<String, String> {
    let mut rewritten = source.to_owned();
    for import in imports.iter().rev() {
        let target = local_source_path(source_path, &import.path).map_err(|message| {
            format!(
                "package-local import `{}` in `{source_path}` {message}",
                import.path
            )
        })?;
        let module = tree
            .public_module_for_source_path(&target)
            .map(str::to_owned)
            .unwrap_or_else(|| local_module_name(package, &target));
        let replacement = simi_string_literal(&module);
        rewritten.replace_range(import.span.start..import.span.end, &replacement);
    }
    Ok(rewritten)
}

fn local_module_name(package: &str, source_path: &str) -> String {
    format!("__simi_package_local__/{package}/{source_path}")
}

fn simi_string_literal(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}

fn digest_tree(tree: &PackageTree) -> String {
    digest_entries(
        tree.digest_inputs()
            .into_iter()
            .map(|entry| (entry.path, entry.bytes)),
    )
}

fn digest_entries<'a>(entries: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> String {
    let mut entries = entries.into_iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    let mut hasher = Sha256::new();
    for (path, contents) in entries {
        hasher.update(path.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(path.as_bytes());
        hasher.update(contents.len().to_string().as_bytes());
        hasher.update(b":");
        hasher.update(contents);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::Engine;
    use simi_analysis::{AnalysisDatabase, Type, infer_types, module_shape};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temporary(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "simi-resolver-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn package(root: &Path, name: &str, requirements: &str) {
        fs::write(
            root.join("simi.package.simi"),
            format!("{{name = \"{name}\", simi = \"0.1\", modules = [\"{name}\"]}}"),
        )
        .unwrap();
        fs::write(
            root.join(format!("{name}.simi")),
            format!("{requirements}\n{{value = 42}}"),
        )
        .unwrap();
    }

    #[test]
    fn path_resolution_writes_a_canonical_transitive_graph() {
        let root = temporary("path");
        let dependency = root.join("deps").join("tools");
        fs::create_dir_all(&dependency).unwrap();
        package(&dependency, "tools", "");
        let app = root.join("app.simi");
        fs::write(&app, "requires {tools = {path = \"deps/tools\"}}\nlet tools = require(\"tools\")\ntools.value").unwrap();
        let resolved =
            resolve_script_with_cache(&app, ResolutionMode::Update, &root.join("cache")).unwrap();
        assert_eq!(resolved.catalog.modules().len(), 1);
        assert!(
            resolved
                .lockfile
                .contains("tools = {source = {path = \"deps/tools\"}")
        );
        fs::write(lock_path(&app), &resolved.lockfile).unwrap();
        assert!(
            resolve_script_with_cache(&app, ResolutionMode::Locked, &root.join("cache")).is_ok()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn locked_path_resolution_rejects_source_changes() {
        let root = temporary("changed");
        let dependency = root.join("deps").join("tools");
        fs::create_dir_all(&dependency).unwrap();
        package(&dependency, "tools", "");
        let app = root.join("app.simi");
        fs::write(&app, "requires {tools = {path = \"deps/tools\"}}\n42").unwrap();
        let cache = root.join("cache");
        let resolved = resolve_script_with_cache(&app, ResolutionMode::Update, &cache).unwrap();
        fs::write(lock_path(&app), resolved.lockfile).unwrap();
        fs::write(dependency.join("tools.simi"), "{value = 9}").unwrap();
        assert!(resolve_script_with_cache(&app, ResolutionMode::Locked, &cache).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn path_resolution_rejects_symlinked_intermediate_components() {
        use std::os::unix::fs::symlink;

        let root = temporary("path-symlink");
        let outside = temporary("path-symlink-outside");
        let dependency = outside.join("tools");
        fs::create_dir_all(&dependency).unwrap();
        package(&dependency, "tools", "");
        symlink(&outside, root.join("deps")).unwrap();
        let app = root.join("app.simi");
        fs::write(&app, "requires {tools = {path = \"deps/tools\"}}\n42").unwrap();

        let error = resolve_script_with_cache(&app, ResolutionMode::Update, &root.join("cache"))
            .unwrap_err();
        assert!(error.contains("contains a symlink component"), "{error}");
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn transitive_path_resolution_rejects_symlinked_intermediate_components() {
        use std::os::unix::fs::symlink;

        let root = temporary("transitive-path-symlink");
        let outside = temporary("transitive-path-symlink-outside");
        let alpha = root.join("deps/alpha");
        let dependency = outside.join("tools");
        fs::create_dir_all(&alpha).unwrap();
        fs::create_dir_all(&dependency).unwrap();
        package(&dependency, "tools", "");
        package(
            &alpha,
            "alpha",
            "requires {tools = {path = \"deps/tools\"}}",
        );
        symlink(&outside, alpha.join("deps")).unwrap();
        let app = root.join("app.simi");
        fs::write(&app, "requires {alpha = {path = \"deps/alpha\"}}\n42").unwrap();

        let error = resolve_script_with_cache(&app, ResolutionMode::Update, &root.join("cache"))
            .unwrap_err();
        assert!(error.contains("contains a symlink component"), "{error}");
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    fn git_at(root: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(arguments)
            .status()
            .unwrap();
        assert!(status.success(), "git {arguments:?}");
    }

    #[test]
    fn git_resolution_locks_exact_commits_and_supports_offline_replay() {
        let root = temporary("git");
        let package_root = root.join("tools");
        fs::create_dir_all(&package_root).unwrap();
        git_at(&root, &["init", "tools"]);
        git_at(
            &package_root,
            &["config", "user.email", "test@example.invalid"],
        );
        git_at(&package_root, &["config", "user.name", "Simi test"]);
        package(&package_root, "tools", "");
        git_at(&package_root, &["add", "."]);
        git_at(&package_root, &["commit", "-m", "initial"]);

        let app = root.join("app.simi");
        fs::write(
            &app,
            format!(
                "requires {{tools = {{git = \"{}\", rev = \"HEAD\"}}}}\nlet tools = require(\"tools\")\ntools.value",
                package_root.display()
            ),
        )
        .unwrap();
        let cache = root.join("cache");
        let resolved = resolve_script_with_cache(&app, ResolutionMode::Update, &cache).unwrap();
        assert!(resolved.lockfile.contains("commit = "));
        fs::write(lock_path(&app), resolved.lockfile).unwrap();
        assert!(resolve_script_with_cache(&app, ResolutionMode::Offline, &cache).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn offline_lock_generation_uses_only_a_validated_bare_cache_and_local_paths() {
        let root = temporary("offline-lock");
        let local = root.join("deps/local-tools");
        fs::create_dir_all(&local).unwrap();
        package(&local, "local-tools", "");

        let package_root = root.join("remote-tools");
        fs::create_dir_all(&package_root).unwrap();
        git_at(&root, &["init", "remote-tools"]);
        git_at(
            &package_root,
            &["config", "user.email", "test@example.invalid"],
        );
        git_at(&package_root, &["config", "user.name", "Simi test"]);
        package(&package_root, "remote-tools", "");
        git_at(&package_root, &["add", "."]);
        git_at(&package_root, &["commit", "-m", "initial"]);

        let cache = root.join("cache");
        let bare = cache
            .join("git")
            .join(sha256(package_root.to_str().unwrap().as_bytes()));
        fs::create_dir_all(bare.parent().unwrap()).unwrap();
        git(&[
            "clone",
            "--bare",
            "--no-local",
            package_root.to_str().unwrap(),
            bare.to_str().unwrap(),
        ])
        .unwrap();
        fs::remove_dir_all(&package_root).unwrap();

        let app = root.join("app.simi");
        fs::write(
            &app,
            format!(
                "requires {{local = {{path = \"deps/local-tools\"}}, remote = {{git = \"{}\", rev = \"HEAD\"}}}}\n42",
                package_root.display()
            ),
        )
        .unwrap();
        let resolved =
            resolve_script_with_cache(&app, ResolutionMode::OfflineUpdate, &cache).unwrap();
        let lock = LockFile::parse(&resolved.lockfile).unwrap();
        assert_eq!(lock.requirements.len(), 2);
        assert!(lock.requirements["remote-tools"].commit.is_some());
        assert!(cache.join("checkouts").is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn locked_resolution_recreates_tampered_git_checkouts() {
        let root = temporary("tampered-checkout");
        let package_root = root.join("tools");
        fs::create_dir_all(&package_root).unwrap();
        git_at(&root, &["init", "tools"]);
        git_at(
            &package_root,
            &["config", "user.email", "test@example.invalid"],
        );
        git_at(&package_root, &["config", "user.name", "Simi test"]);
        package(&package_root, "tools", "");
        git_at(&package_root, &["add", "."]);
        git_at(&package_root, &["commit", "-m", "initial"]);

        let app = root.join("app.simi");
        fs::write(
            &app,
            format!(
                "requires {{tools = {{git = \"{}\", rev = \"HEAD\"}}}}\n42",
                package_root.display()
            ),
        )
        .unwrap();
        let cache = root.join("cache");
        let resolved = resolve_script_with_cache(&app, ResolutionMode::Update, &cache).unwrap();
        fs::write(lock_path(&app), &resolved.lockfile).unwrap();
        let lock = LockFile::parse(&resolved.lockfile).unwrap();
        let commit = lock.requirements["tools"].commit.as_ref().unwrap();
        let checkout = cache
            .join("checkouts")
            .join(sha256(package_root.to_str().unwrap().as_bytes()))
            .join(commit);
        fs::write(checkout.join("tools.simi"), "{value = 9}").unwrap();

        let resolved = resolve_script_with_cache(&app, ResolutionMode::Locked, &cache).unwrap();
        assert_eq!(
            resolved
                .catalog
                .modules()
                .iter()
                .map(|module| (module.name(), module.source()))
                .collect::<Vec<_>>(),
            vec![("tools", "\n{value = 42}")]
        );
        assert!(resolved.lockfile.contains("tree_digest"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn aliases_can_differ_from_hyphenated_manifest_package_identities() {
        let root = temporary("hyphenated-package");
        let dependency = root.join("deps/tool-box");
        fs::create_dir_all(&dependency).unwrap();
        package(&dependency, "tool-box", "");
        let app = root.join("app.simi");
        fs::write(
            &app,
            "requires {toolbox = {path = \"deps/tool-box\"}}\nlet tools = require(\"tool-box\")\ntools.value",
        )
        .unwrap();
        let cache = root.join("cache");
        let resolved = resolve_script_with_cache(&app, ResolutionMode::Update, &cache).unwrap();
        let lock = LockFile::parse(&resolved.lockfile).unwrap();
        assert_eq!(lock.requirements["tool-box"].package, "tool-box");
        assert_eq!(
            resolved
                .catalog
                .modules()
                .iter()
                .map(|module| (module.name(), module.source()))
                .collect::<Vec<_>>(),
            vec![("tool-box", "\n{value = 42}")]
        );
        fs::write(lock_path(&app), &resolved.lockfile).unwrap();
        assert!(resolve_script_with_cache(&app, ResolutionMode::Locked, &cache).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn independent_packages_can_reuse_requirement_aliases() {
        let root = temporary("scoped-aliases");
        let alpha = root.join("deps/alpha");
        let beta = root.join("deps/beta");
        let alpha_tools = alpha.join("deps/alpha-tools");
        let beta_tools = beta.join("deps/beta-tools");
        for package_root in [&alpha, &beta, &alpha_tools, &beta_tools] {
            fs::create_dir_all(package_root).unwrap();
        }
        package(&alpha_tools, "alpha-tools", "");
        package(&beta_tools, "beta-tools", "");
        package(
            &alpha,
            "alpha",
            "requires {tools = {path = \"deps/alpha-tools\"}}",
        );
        package(
            &beta,
            "beta",
            "requires {tools = {path = \"deps/beta-tools\"}}",
        );
        let app = root.join("app.simi");
        fs::write(
            &app,
            "requires {alpha = {path = \"deps/alpha\"}, beta = {path = \"deps/beta\"}}\n42",
        )
        .unwrap();

        let cache = root.join("cache");
        let resolved = resolve_script_with_cache(&app, ResolutionMode::Update, &cache).unwrap();
        let lock = LockFile::parse(&resolved.lockfile).unwrap();
        assert_eq!(
            lock.requirements.keys().collect::<Vec<_>>(),
            ["alpha", "alpha-tools", "beta", "beta-tools"]
        );
        fs::write(lock_path(&app), resolved.lockfile).unwrap();
        assert!(resolve_script_with_cache(&app, ResolutionMode::Locked, &cache).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn conflicting_sources_for_one_package_identity_are_rejected() {
        let root = temporary("identity-conflict");
        let alpha = root.join("deps/alpha");
        let beta = root.join("deps/beta");
        let alpha_tools = alpha.join("deps/tools");
        let beta_tools = beta.join("deps/tools");
        for package_root in [&alpha, &beta, &alpha_tools, &beta_tools] {
            fs::create_dir_all(package_root).unwrap();
        }
        package(&alpha_tools, "tools", "");
        package(&beta_tools, "tools", "");
        package(
            &alpha,
            "alpha",
            "requires {tools = {path = \"deps/tools\"}}",
        );
        package(&beta, "beta", "requires {tools = {path = \"deps/tools\"}}");
        let app = root.join("app.simi");
        fs::write(
            &app,
            "requires {alpha = {path = \"deps/alpha\"}, beta = {path = \"deps/beta\"}}\n42",
        )
        .unwrap();

        let error = resolve_script_with_cache(&app, ResolutionMode::Update, &root.join("cache"))
            .unwrap_err();
        assert!(
            error.contains("package `tools` resolves from conflicting declared sources"),
            "{error}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_cycles_before_execution() {
        let root = temporary("cycle");
        let package_root = root.join("loop");
        fs::create_dir_all(&package_root).unwrap();
        git_at(&root, &["init", "loop"]);
        git_at(
            &package_root,
            &["config", "user.email", "test@example.invalid"],
        );
        git_at(&package_root, &["config", "user.name", "Simi test"]);
        package(&package_root, "loop", "");
        git_at(&package_root, &["add", "."]);
        git_at(&package_root, &["commit", "-m", "initial"]);
        fs::write(
            package_root.join("loop.simi"),
            format!(
                "requires {{loop = {{git = \"{}\", rev = \"HEAD\"}}}}\n{{value = 42}}",
                package_root.display()
            ),
        )
        .unwrap();
        git_at(&package_root, &["add", "."]);
        git_at(&package_root, &["commit", "-m", "cycle"]);

        let app = root.join("app.simi");
        fs::write(
            &app,
            format!(
                "requires {{loop = {{git = \"{}\", rev = \"HEAD\"}}}}\n42",
                package_root.display()
            ),
        )
        .unwrap();
        let error = resolve_script_with_cache(&app, ResolutionMode::Update, &root.join("cache"))
            .unwrap_err();
        assert!(error.contains("cyclic package requirement"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_local_imports_are_nested_cached_and_keep_catalog_imports() {
        let root = temporary("local-imports");
        let package_root = root.join("deps/tools");
        fs::create_dir_all(package_root.join("src")).unwrap();
        fs::write(
            package_root.join("simi.package.simi"),
            r#"{name = "tools", simi = "0.1", modules = ["tools"]}"#,
        )
        .unwrap();
        fs::write(
            package_root.join("tools.simi"),
            r#"
let left = require("./src/left.simi")
let right = require("./src/right.simi")
let string = require("std/string")
{left = left, right = right, string = string}
"#,
        )
        .unwrap();
        fs::write(
            package_root.join("src/left.simi"),
            r#"let shared = require("./shared.simi") {state = shared.state}"#,
        )
        .unwrap();
        fs::write(
            package_root.join("src/right.simi"),
            r#"let shared = require("./shared.simi") {state = shared.state}"#,
        )
        .unwrap();
        fs::write(
            package_root.join("src/shared.simi"),
            "let state = [] {state = state}",
        )
        .unwrap();
        let app = root.join("app.simi");
        fs::write(
            &app,
            "requires {tools = {path = \"deps/tools\"}}\nlet tools = require(\"tools\")\nlist.append(tools.left.state, 7)\ntools.right.state[0]",
        )
        .unwrap();

        let resolved =
            resolve_script_with_cache(&app, ResolutionMode::Update, &root.join("cache")).unwrap();
        assert!(
            resolved
                .catalog
                .modules()
                .iter()
                .any(|module| module.name() == "__simi_package_local__/tools/src/shared.simi")
        );
        let public_source = resolved
            .catalog
            .modules()
            .iter()
            .find_map(|module| (module.name() == "tools").then_some(module.source()))
            .unwrap();
        assert!(public_source.contains("require(\"std/string\")"));
        assert!(!public_source.contains("require(\"./src/left.simi\")"));

        let builder = Engine::builder().stdlib().catalog(resolved.catalog);
        assert_eq!(
            builder
                .build()
                .eval(&fs::read_to_string(&app).unwrap())
                .unwrap()
                .unwrap()
                .render(),
            "7"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_local_imports_retain_literal_module_types() {
        let root = temporary("local-types");
        let package_root = root.join("deps/tools");
        fs::create_dir_all(package_root.join("src")).unwrap();
        fs::write(
            package_root.join("simi.package.simi"),
            r#"{name = "tools", simi = "0.1", modules = ["tools"]}"#,
        )
        .unwrap();
        fs::write(
            package_root.join("tools.simi"),
            "let value = require(\"./src/value.simi\")\nvalue.answer",
        )
        .unwrap();
        fs::write(package_root.join("src/value.simi"), "{answer = 42}").unwrap();
        let app = root.join("app.simi");
        fs::write(&app, "requires {tools = {path = \"deps/tools\"}}\n42").unwrap();

        let resolved =
            resolve_script_with_cache(&app, ResolutionMode::Update, &root.join("cache")).unwrap();
        let db = AnalysisDatabase::default();
        let files = resolved
            .catalog
            .modules()
            .iter()
            .map(|module| (module.name().to_owned(), db.add_file(module.source())))
            .collect::<BTreeMap<_, _>>();
        let modules = files
            .iter()
            .map(|(name, file)| (name.clone(), module_shape(&db, *file)))
            .collect::<std::collections::HashMap<_, _>>();
        let inference = infer_types(&db, files["tools"], &modules);
        assert_eq!(inference.result_type, Some(Type::Int));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_local_import_cycles_use_source_module_cycle_diagnostics() {
        let root = temporary("local-cycle");
        let package_root = root.join("deps/tools");
        fs::create_dir_all(package_root.join("src")).unwrap();
        fs::write(
            package_root.join("simi.package.simi"),
            r#"{name = "tools", simi = "0.1", modules = ["tools"]}"#,
        )
        .unwrap();
        fs::write(package_root.join("tools.simi"), "require(\"./src/a.simi\")").unwrap();
        fs::write(package_root.join("src/a.simi"), "require(\"./b.simi\")").unwrap();
        fs::write(package_root.join("src/b.simi"), "require(\"./a.simi\")").unwrap();
        let app = root.join("app.simi");
        fs::write(
            &app,
            "requires {tools = {path = \"deps/tools\"}}\nrequire(\"tools\")",
        )
        .unwrap();

        let resolved =
            resolve_script_with_cache(&app, ResolutionMode::Update, &root.join("cache")).unwrap();
        let builder = Engine::builder().stdlib().catalog(resolved.catalog);
        let result = builder
            .build()
            .eval(&fs::read_to_string(&app).unwrap())
            .unwrap();
        let Err(raised) = result else {
            panic!("expected package-local import cycle to raise");
        };
        assert!(raised.value.render().contains("circular_module_dependency"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_local_imports_reject_traversal_and_include_reachable_sources_in_locks() {
        let root = temporary("local-traversal");
        let package_root = root.join("deps/tools");
        fs::create_dir_all(package_root.join("src")).unwrap();
        fs::write(
            package_root.join("simi.package.simi"),
            r#"{name = "tools", simi = "0.1", modules = ["tools"]}"#,
        )
        .unwrap();
        fs::write(
            package_root.join("tools.simi"),
            "require(\"./src/value.simi\")",
        )
        .unwrap();
        fs::write(package_root.join("src/value.simi"), "42").unwrap();
        let app = root.join("app.simi");
        fs::write(&app, "requires {tools = {path = \"deps/tools\"}}\n42").unwrap();
        let cache = root.join("cache");
        let resolved = resolve_script_with_cache(&app, ResolutionMode::Update, &cache).unwrap();
        fs::write(lock_path(&app), &resolved.lockfile).unwrap();
        fs::write(package_root.join("src/value.simi"), "43").unwrap();
        assert!(resolve_script_with_cache(&app, ResolutionMode::Locked, &cache).is_err());

        fs::write(
            package_root.join("tools.simi"),
            "require(\"./../outside.simi\")",
        )
        .unwrap();
        let error = resolve_script_with_cache(&app, ResolutionMode::Update, &cache).unwrap_err();
        assert!(error.contains("without traversal"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shadowed_require_does_not_trigger_package_local_resolution() {
        let root = temporary("shadowed-local-require");
        let package_root = root.join("deps/tools");
        fs::create_dir_all(&package_root).unwrap();
        fs::write(
            package_root.join("simi.package.simi"),
            r#"{name = "tools", simi = "0.1", modules = ["tools"]}"#,
        )
        .unwrap();
        fs::write(
            package_root.join("tools.simi"),
            "let require = fn(path) do 42 end\nrequire(\"./missing.simi\")",
        )
        .unwrap();
        let app = root.join("app.simi");
        fs::write(
            &app,
            "requires {tools = {path = \"deps/tools\"}}\nrequire(\"tools\")",
        )
        .unwrap();

        let resolved =
            resolve_script_with_cache(&app, ResolutionMode::Update, &root.join("cache")).unwrap();
        assert_eq!(resolved.catalog.modules().len(), 1);
        let builder = Engine::builder().stdlib().catalog(resolved.catalog);
        assert_eq!(
            builder
                .build()
                .eval(&fs::read_to_string(&app).unwrap())
                .unwrap()
                .unwrap()
                .render(),
            "42"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_source_parse_errors_name_the_source_unit() {
        let root = temporary("local-diagnostic");
        let package_root = root.join("deps/tools");
        fs::create_dir_all(package_root.join("src")).unwrap();
        fs::write(
            package_root.join("simi.package.simi"),
            r#"{name = "tools", simi = "0.1", modules = ["tools"]}"#,
        )
        .unwrap();
        fs::write(
            package_root.join("tools.simi"),
            "require(\"./src/bad.simi\")",
        )
        .unwrap();
        fs::write(package_root.join("src/bad.simi"), "let =").unwrap();
        let app = root.join("app.simi");
        fs::write(&app, "requires {tools = {path = \"deps/tools\"}}\n42").unwrap();

        let error = resolve_script_with_cache(&app, ResolutionMode::Update, &root.join("cache"))
            .unwrap_err();
        assert!(error.contains("src/bad.simi"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn digest_frames_paths_and_contents() {
        assert_ne!(digest_one("a", b"bc"), digest_one("ab", b"c"));
    }
}
