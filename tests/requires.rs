use simi::{
    CatalogModule, CatalogModuleVisibility, CatalogRequirement, Engine, PackageCatalog,
    RequirementSource, eval,
};

const REQUIREMENT: &str = r#"requires {tools = {path = "deps/tools"}}"#;

fn catalog() -> PackageCatalog {
    PackageCatalog::new(
        [CatalogModule::new(
            "tools",
            "{value = 42, items = []}",
            "tools",
            "tools.simi",
            CatalogModuleVisibility::Public,
        )],
        [CatalogRequirement::new(
            "tools",
            RequirementSource::Path {
                path: "deps/tools".to_owned(),
            },
        )],
    )
    .unwrap()
}

#[test]
fn bare_and_portable_engines_reject_unresolved_package_metadata_before_execution() {
    let source = format!("{REQUIREMENT}\nraise \"must not run\"");
    for result in [
        Engine::new().eval(&source),
        Engine::with_stdlib().eval(&source),
        eval(&source),
    ] {
        let Err(error) = result else {
            panic!("unresolved requirements must be a hard error");
        };
        assert!(error.to_string().contains("no resolved package catalog"));
    }
}

#[test]
fn resolved_catalog_satisfies_exact_requirements_and_isolated_per_engine() {
    let source = format!(
        "{REQUIREMENT}\nlet tools = require(\"tools\")\nlist.append(tools.items, 7)\n[tools.value, list.length(tools.items)]"
    );
    let first = Engine::builder().stdlib().catalog(catalog()).build();
    let second = Engine::builder().stdlib().catalog(catalog()).build();
    assert_eq!(first.eval(&source).unwrap().unwrap().render(), "[42, 1]");
    assert_eq!(second.eval(&source).unwrap().unwrap().render(), "[42, 1]");
}

#[test]
fn missing_or_mismatched_catalogs_are_hard_errors() {
    let missing = Engine::builder()
        .stdlib()
        .catalog(PackageCatalog::new([], []).unwrap())
        .build();
    let Err(error) = missing.eval(&format!("{REQUIREMENT}\n42")) else {
        panic!("missing catalog must be a hard error");
    };
    assert!(
        error
            .to_string()
            .contains("does not satisfy requirement `tools`")
    );

    let mismatched = Engine::builder()
        .stdlib()
        .catalog(
            PackageCatalog::new(
                [CatalogModule::new(
                    "tools",
                    "{}",
                    "tools",
                    "tools.simi",
                    CatalogModuleVisibility::Public,
                )],
                [CatalogRequirement::new(
                    "tools",
                    RequirementSource::Path {
                        path: "other/tools".to_owned(),
                    },
                )],
            )
            .unwrap(),
        )
        .build();
    let Err(error) = mismatched.eval(&format!("{REQUIREMENT}\n42")) else {
        panic!("mismatched catalog must be a hard error");
    };
    assert!(
        error
            .to_string()
            .contains("does not satisfy requirement `tools`")
    );
}

#[test]
fn catalog_construction_rejects_duplicate_modules_and_unresolved_module_requirements() {
    let duplicate = PackageCatalog::new(
        [
            CatalogModule::new(
                "tools",
                "{}",
                "tools",
                "tools.simi",
                CatalogModuleVisibility::Public,
            ),
            CatalogModule::new(
                "tools",
                "{}",
                "tools",
                "other.simi",
                CatalogModuleVisibility::Public,
            ),
        ],
        [],
    )
    .unwrap_err();
    assert!(
        duplicate
            .to_string()
            .contains("supplies module `tools` more than once")
    );

    let unresolved = PackageCatalog::new(
        [CatalogModule::new(
            "tools",
            "requires {child = {path = \"deps/child\"}}\n{}",
            "tools",
            "tools.simi",
            CatalogModuleVisibility::Public,
        )],
        [],
    )
    .unwrap_err();
    assert!(
        unresolved
            .to_string()
            .contains("unresolved requirement `child`")
    );
}

#[test]
fn catalogs_and_direct_modules_remain_explicit_and_package_relative_imports_are_rejected() {
    let direct = Engine::builder()
        .stdlib()
        .module(simi::Module::source("direct", "42").build())
        .catalog(catalog())
        .build();
    assert_eq!(
        direct
            .eval("require(\"direct\")")
            .unwrap()
            .unwrap()
            .render(),
        "42"
    );

    let Err(collision) = Engine::builder()
        .module(simi::Module::source("tools", "0").build())
        .catalog(catalog())
        .build()
        .eval("42")
    else {
        panic!("module collision must be a hard error");
    };
    assert!(collision.to_string().contains("conflicts"));

    let Err(relative) = Engine::new().eval("require(\"./private.simi\")") else {
        panic!("relative import must be a hard error");
    };
    assert!(
        relative
            .to_string()
            .contains("package-relative imports require prior package resolution")
    );
}
