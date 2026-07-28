use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use sha2::{Digest, Sha256};

use super::lock::{LockFile, LockSource, LockedRequirement};
use super::{PackageTree, Requirement, RequirementSource, Requires, parse_requires};

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
    pub modules: Vec<(String, String)>,
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
        visiting: BTreeSet::new(),
        entries: BTreeMap::new(),
        modules: BTreeMap::new(),
    };
    if let Some(requires) = root_requires.as_ref() {
        context.resolve_requirements(requires, base)?;
    }
    let lock = LockFile {
        source: LockSource {
            path: source_path,
            digest: source_digest,
        },
        requirements: context.entries,
    };
    if let Some(locked) = locked.as_ref()
        && lock != *locked
    {
        return Err(format!(
            "lockfile `{}` does not match resolved requirements",
            lock_path.display()
        ));
    }
    Ok(ResolvedScript {
        modules: context.modules.into_iter().collect(),
        lockfile: lock.render(),
    })
}

struct ResolveContext<'a> {
    mode: ResolutionMode,
    cache: &'a Path,
    locked: Option<&'a LockFile>,
    visiting: BTreeSet<String>,
    entries: BTreeMap<String, LockedRequirement>,
    modules: BTreeMap<String, String>,
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
        if let Some(existing) = self.entries.get(&requirement.alias) {
            if existing.source != requirement.source {
                return Err(format!(
                    "requirement `{}` resolves from conflicting declared sources",
                    requirement.alias
                ));
            }
            return Ok(());
        }
        if !self.visiting.insert(requirement.alias.clone()) {
            return Err(format!(
                "cyclic package requirement involving `{}`",
                requirement.alias
            ));
        }

        let resolved = (|| {
            validate_source(&requirement.source)?;
            let (root, commit) = match &requirement.source {
                RequirementSource::Path { path } => (base.join(path), None),
                RequirementSource::Git { git, rev } => {
                    let commit = self.git_commit(&requirement.alias, git, rev)?;
                    let root = self.git_checkout(git, &commit)?;
                    (root, Some(commit))
                }
            };
            let tree = PackageTree::load(&root)
                .map_err(|error| format!("invalid package `{}`: {error}", requirement.alias))?;
            let package = tree.manifest().name().to_owned();
            let tree_digest = digest_tree(&tree);
            if let Some(locked) = self
                .locked
                .and_then(|lock| lock.requirements.get(&requirement.alias))
            {
                if locked.source != requirement.source {
                    return Err(format!(
                        "lockfile source for `{}` differs from the declaration",
                        requirement.alias
                    ));
                }
                if locked.package != package
                    || locked.commit != commit
                    || locked.tree_digest != tree_digest
                {
                    return Err(format!(
                        "lockfile entry for `{}` does not match its package tree",
                        requirement.alias
                    ));
                }
            }
            for module in tree.modules() {
                let name = module.module().name().to_owned();
                let source = module.source().to_owned();
                if let Some(existing) = self.modules.insert(name.clone(), source.clone())
                    && existing != source
                {
                    return Err(format!(
                        "public module `{name}` is supplied by more than one package"
                    ));
                }
            }
            let requirements = requirements_from_tree(&tree)?;
            self.resolve_requirements(&requirements, tree.root())?;
            Ok(LockedRequirement {
                source: requirement.source.clone(),
                package,
                commit,
                tree_digest,
            })
        })();
        self.visiting.remove(&requirement.alias);
        let entry = resolved?;
        self.entries.insert(requirement.alias.clone(), entry);
        Ok(())
    }

    fn git_commit(&self, name: &str, git_url: &str, rev: &str) -> Result<String, String> {
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
                let locked = self
                    .locked
                    .and_then(|lock| lock.requirements.get(name))
                    .ok_or_else(|| format!("lockfile is missing requirement `{name}`"))?;
                let commit = locked
                    .commit
                    .as_ref()
                    .ok_or_else(|| {
                        format!("lockfile requirement `{name}` is missing its Git commit")
                    })?
                    .clone();
                let bare = self.ensure_git_cache(git_url, false)?;
                git(&[
                    "-C", bare.to_str().ok_or("non-UTF-8 Git cache path")?,
                    "cat-file", "-e", &format!("{commit}^{{commit}}"),
                ])
                .map_err(|_| format!("cached Git repository does not contain locked commit `{commit}` for `{name}`"))?;
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

fn requirements_from_tree(tree: &PackageTree) -> Result<Requires, String> {
    let mut entries = BTreeMap::<String, Requirement>::new();
    for module in tree.modules() {
        let Some(requires) = parse_requires(module.source()).map_err(|error| {
            format!(
                "invalid requirements in `{}`: {error}",
                module.module().source_path()
            )
        })?
        else {
            continue;
        };
        for requirement in requires.entries {
            if let Some(existing) = entries.insert(requirement.alias.clone(), requirement.clone())
                && existing.source != requirement.source
            {
                return Err(format!(
                    "package `{}` declares conflicting sources for requirement `{}`",
                    tree.manifest().name(),
                    existing.alias
                ));
            }
        }
    }
    Ok(Requires {
        entries: entries.into_values().collect(),
        span: crate::span::Span::new(0, 0),
    })
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
        assert_eq!(resolved.modules.len(), 1);
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
        assert!(lock.requirements["remote"].commit.is_some());
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
            resolved.modules,
            vec![("tools".to_owned(), "\n{value = 42}".to_owned())]
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
        assert_eq!(lock.requirements["toolbox"].package, "tool-box");
        assert_eq!(
            resolved.modules,
            vec![("tool-box".to_owned(), "\n{value = 42}".to_owned())]
        );
        fs::write(lock_path(&app), &resolved.lockfile).unwrap();
        assert!(resolve_script_with_cache(&app, ResolutionMode::Locked, &cache).is_ok());
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
    fn digest_frames_paths_and_contents() {
        assert_ne!(digest_one("a", b"bc"), digest_one("ab", b"c"));
    }
}
