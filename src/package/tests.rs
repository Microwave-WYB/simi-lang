use super::*;
use crate::span::Span;

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
fn parses_static_leading_requirements_without_evaluation() {
    let source = r#"
        requires {
            text = {git = "https://example.invalid/simi-text.git", rev = "v0.1.0"},
            local_tools = {path = "dev/tools"},
        }
        let executable = panic
    "#;

    let requires = parse_requires(source)
        .unwrap()
        .expect("requires declaration");
    assert_eq!(requires.span, Span::new(9, 161));
    assert_eq!(
        requires.entries,
        [
            Requirement {
                alias: "text".to_owned(),
                source: RequirementSource::Git {
                    git: "https://example.invalid/simi-text.git".to_owned(),
                    rev: "v0.1.0".to_owned(),
                },
            },
            Requirement {
                alias: "local_tools".to_owned(),
                source: RequirementSource::Path {
                    path: "dev/tools".to_owned(),
                },
            },
        ]
    );
    assert_eq!(parse_requires("let value = 1").unwrap(), None);
}

#[test]
fn rejects_non_static_or_invalid_requirements_at_the_offending_span() {
    for (source, expected) in [
        (
            "requires {text = {git = source, rev = \"v1\"}}",
            "string literal",
        ),
        ("requires {text = {git = \"url\"}}", "git` and `rev"),
        (
            "requires {text = {path = \"dev\", rev = \"v1\"}}",
            "cannot mix",
        ),
        (
            "requires {text = {git = \"url\", rev = \"v1\", extra = \"x\"}}",
            "does not permit",
        ),
        (
            "requires {Text = {path = \"dev\"}}",
            "lowercase Simi identifier",
        ),
        ("requires {text = {path = \"../dev\"}}", "non-escaping"),
        ("requires {text = {path = \"/dev\"}}", "non-escaping"),
        ("requires {text = {path = \"C:/dev\"}}", "non-escaping"),
        (
            "requires {text = {path = \"dev\\\\tools\"}}",
            "non-escaping",
        ),
    ] {
        let error = parse_requires(source).unwrap_err();
        assert!(
            error.message.contains(expected),
            "{source:?}: expected {expected:?}, got {error:?}"
        );
        assert!(error.span.start < error.span.end, "{source:?}: {error:?}");
    }
}

#[test]
fn rejects_duplicate_requirement_aliases_and_source_fields() {
    for (source, duplicate) in [
        (
            "requires {text = {path = \"dev\"}, text = {path = \"other\"}}",
            "text",
        ),
        (
            "requires {text = {path = \"dev\", path = \"other\"}}",
            "path",
        ),
    ] {
        let error = parse_requires(source).unwrap_err();
        assert!(
            error.message.contains("more than once"),
            "{source:?}: {error}"
        );
        assert_eq!(error.span.start, source.rfind(duplicate).unwrap());
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
