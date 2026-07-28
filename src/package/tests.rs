use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;

static NEXT_TEMP_PACKAGE: AtomicU64 = AtomicU64::new(0);

fn temporary_package_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "simi-package-{name}-{}-{}",
        std::process::id(),
        NEXT_TEMP_PACKAGE.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn write_package_manifest(root: &std::path::Path, modules: &str) {
    fs::write(
        root.join("simi.package.simi"),
        format!(r#"{{name = "polars", simi = "0.1.0-alpha.2", modules = {modules}}}"#),
    )
    .unwrap();
}

#[test]
fn parses_canonical_source_package_manifest() {
    let manifest = PackageManifest::parse(
        r#"
        ---- Polars facade package.
        {
            name = "polars",
            simi = "0.1.0-alpha.1",
            modules = ["polars", "polars/csv"],
            native = {manifest = "native/Cargo.toml"},
        }
        "#,
    )
    .unwrap();

    assert_eq!(manifest.name(), "polars");
    assert_eq!(manifest.simi(), "0.1.0-alpha.1");
    assert_eq!(
        manifest
            .modules()
            .iter()
            .map(|module| (module.name(), module.source_path()))
            .collect::<Vec<_>>(),
        [("polars", "polars.simi"), ("polars/csv", "polars/csv.simi")]
    );
    assert_eq!(
        manifest.native().map(NativePackage::manifest_path),
        Some("native/Cargo.toml")
    );
}

#[test]
fn rejects_executable_or_non_static_metadata() {
    for source in [
        "let package = {}",
        "package()",
        r#"{name = "polars", simi = "0.1", modules = source()}"#,
        r#"{name = "polars", simi = "0.1", modules = ["polars"], unexpected = true}"#,
    ] {
        let error = PackageManifest::parse(source).unwrap_err();
        assert!(
            error.to_string().contains("package metadata"),
            "{source:?}: {error}"
        );
    }
}

#[test]
fn loads_declared_public_sources_in_canonical_digest_order() {
    let root = temporary_package_root("canonical-tree");
    write_package_manifest(&root, "[\"polars/csv\", \"polars\"]");
    fs::write(root.join("polars.simi"), "{root = true}\n").unwrap();
    fs::create_dir(root.join("polars")).unwrap();
    fs::write(root.join("polars/csv.simi"), "{csv = true}\n").unwrap();
    fs::write(root.join("private.simi"), "{private = true}\n").unwrap();

    let tree = PackageTree::load(&root).unwrap();
    assert_eq!(
        tree.modules()
            .iter()
            .map(|source| source.module().source_path())
            .collect::<Vec<_>>(),
        ["polars.simi", "polars/csv.simi"],
    );
    assert_eq!(
        tree.digest_inputs()
            .iter()
            .map(|input| input.path)
            .collect::<Vec<_>>(),
        ["simi.package.simi", "polars.simi", "polars/csv.simi"],
    );
    assert!(
        tree.digest_inputs()
            .iter()
            .all(|input| input.path != "private.simi"),
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_missing_or_symlinked_declared_public_sources() {
    let root = temporary_package_root("invalid-tree");
    write_package_manifest(&root, "[\"polars\"]");
    let error = PackageTree::load(&root).unwrap_err();
    assert!(error.to_string().contains("missing declared public module"));

    #[cfg(unix)]
    {
        fs::write(root.join("outside.simi"), "{}\n").unwrap();
        std::os::unix::fs::symlink(root.join("outside.simi"), root.join("polars.simi")).unwrap();
        let error = PackageTree::load(&root).unwrap_err();
        assert!(error.to_string().contains("does not permit symlink"));
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn parses_and_rejects_nonstatic_leading_requirements() {
    let requires = parse_requires(
        r#"requires {
            remote = {git = "https://example.invalid/tools.git", rev = "v1"},
            local = {path = "dev/local"},
        }
        42"#,
    )
    .unwrap()
    .expect("requires header");
    assert_eq!(requires.entries.len(), 2);
    assert!(matches!(
        requires.entries[0].source,
        RequirementSource::Git { ref git, ref rev }
            if git == "https://example.invalid/tools.git" && rev == "v1"
    ));
    assert!(matches!(
        requires.entries[1].source,
        RequirementSource::Path { ref path } if path == "dev/local"
    ));

    for source in [
        "requires {tools = {git = url, rev = \"v1\"}}",
        "requires {tools = {git = \"\", rev = \"v1\"}}",
        "requires {tools = {path = \"../tools\"}}",
        "requires {tools = {git = \"url\", rev = \"v1\", path = \"tools\"}}",
        "let value = 1 requires {tools = {path = \"tools\"}}",
    ] {
        assert!(parse_requires(source).is_err(), "{source}");
    }
}

#[test]
fn preserves_duplicate_map_diagnostics_outside_requires_metadata() {
    for source in [
        "let value = {a = 1, a = 2}",
        "requires {text = {path = \"dev\"}} let value = {a = 1, a = 2}",
    ] {
        let error = parse_requires(source).unwrap_err();
        assert!(
            error.message.contains("duplicate map field `a`"),
            "{source:?}: {error}"
        );
    }
}

#[test]
fn rejects_noncanonical_or_unsafe_public_metadata() {
    for (source, expected) in [
        (
            r#"{name = "polars", simi = "0.1", modules = ["polars/csv"]}"#,
            "must export root module",
        ),
        (
            r#"{name = "polars", simi = "0.1", modules = ["polars", "other"]}"#,
            "must equal package",
        ),
        (
            r#"{name = "Polars", simi = "0.1", modules = ["Polars"]}"#,
            "lowercase module component",
        ),
        (
            r#"{name = "polars", simi = "0.1", modules = ["polars", "polars"]}"#,
            "more than once",
        ),
        (
            r#"{name = "polars", simi = "0.1", modules = ["polars"], native = {manifest = "../Cargo.toml"}}"#,
            "package-root-relative",
        ),
        (
            r#"{name = "polars", simi = "0.1", modules = ["polars"], native = {manifest = "native/manifest.simi"}}"#,
            "Cargo.toml",
        ),
    ] {
        let error = PackageManifest::parse(source).unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "{source:?}: expected {expected:?}, got {error}"
        );
    }
}
