use super::*;

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
