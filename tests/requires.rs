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
        )
        .unwrap()],
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
        assert!(
            error.to_string().contains("no resolved package catalog")
                || error
                    .to_string()
                    .contains("does not satisfy requirement `tools`")
        );
    }
}

#[test]
fn portable_prelude_is_runtime_owned_and_stdio_remains_opt_in() {
    let source = r#"[
        type(list),
        type(map),
        type(iter),
        type(number),
        type(string),
        type(bytes),
        type(require("std/iter")),
        type(require("std/bytes")),
    ]"#;
    for result in [
        Engine::new().eval(source),
        Engine::with_stdlib().eval(source),
        eval(source),
    ] {
        assert_eq!(
            result.unwrap().unwrap().render(),
            "[\"map\", \"map\", \"map\", \"map\", \"map\", \"map\", \"map\", \"map\"]"
        );
    }

    let no_capability = Engine::new().eval("require(\"std/io\")").unwrap();
    assert!(no_capability.is_err());

    let stdio = Engine::builder().prelude().stdio().build();
    assert_eq!(
        stdio
            .eval("type(require(\"std/io\"))")
            .unwrap()
            .unwrap()
            .render(),
        "\"map\""
    );

    let catalog_only = Engine::builder()
        .catalog(simi::stdlib::official_catalog())
        .build();
    assert_eq!(
        catalog_only
            .eval("type(require(\"std/iter\"))")
            .unwrap()
            .unwrap()
            .render(),
        "\"map\""
    );
    assert!(catalog_only.eval("iter").is_err());
}

#[test]
fn official_catalog_rejects_extra_and_overridden_std_entries() {
    let official = simi::stdlib::official_catalog();
    let extra = PackageCatalog::new(
        official.modules().iter().cloned().chain(std::iter::once(
            CatalogModule::new(
                "std/evil",
                "{}",
                "std",
                "std/evil.simi",
                CatalogModuleVisibility::Public,
            )
            .unwrap(),
        )),
        official.requirements().iter().cloned(),
    )
    .unwrap();
    let overridden = PackageCatalog::new(
        official.modules().iter().map(|module| {
            if module.name() == "std/iter" {
                CatalogModule::new(
                    "std/iter",
                    "{overridden = true}",
                    "std",
                    "std/iter.simi",
                    CatalogModuleVisibility::Public,
                )
                .unwrap()
            } else {
                module.clone()
            }
        }),
        official.requirements().iter().cloned(),
    )
    .unwrap();
    for catalog in [extra, overridden] {
        let Err(error) = Engine::builder().catalog(catalog).build().eval("42") else {
            panic!("non-exact official catalog must be rejected");
        };
        assert!(error.to_string().contains(
            "only the exact distribution official catalog may supply the reserved `std/` namespace"
        ));
    }

    for engine in [
        Engine::builder()
            .stdlib()
            .module(simi::Module::source("std/iter", "{}").build())
            .build(),
        Engine::builder()
            .module(simi::Module::source("std/iter", "{}").build())
            .stdlib()
            .build(),
    ] {
        let Err(error) = engine.eval("42") else {
            panic!("direct module must not override an official catalog module");
        };
        assert!(
            error
                .to_string()
                .contains("conflicts with the bundled prelude module")
                || error
                    .to_string()
                    .contains("conflicts with a resolved package catalog module")
        );
    }

    assert_eq!(
        Engine::builder()
            .module(simi::Module::source("custom", "42").build())
            .stdlib()
            .build()
            .eval("require(\"custom\")")
            .unwrap()
            .unwrap()
            .render(),
        "42"
    );
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
                )
                .unwrap()],
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
            )
            .unwrap(),
            CatalogModule::new(
                "tools",
                "{}",
                "tools",
                "tools.simi",
                CatalogModuleVisibility::Public,
            )
            .unwrap(),
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
        )
        .unwrap()],
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
fn catalog_module_construction_rejects_forged_provenance_and_local_identities() {
    for (name, package, source_path, visibility, expected) in [
        (
            "other",
            "tools",
            "other.simi",
            CatalogModuleVisibility::Public,
            "must equal package",
        ),
        (
            "tools",
            "tools",
            "other.simi",
            CatalogModuleVisibility::Public,
            "must use source path `tools.simi`",
        ),
        (
            "tools",
            "tools",
            "src/private.simi",
            CatalogModuleVisibility::PackageLocal,
            "must equal `__simi_package_local__/tools/src/private.simi`",
        ),
        (
            "__simi_package_local__/tools/src/../private.simi",
            "tools",
            "src/../private.simi",
            CatalogModuleVisibility::PackageLocal,
            "package-root-relative",
        ),
    ] {
        let error = CatalogModule::new(name, "{}", package, source_path, visibility).unwrap_err();
        assert!(error.to_string().contains(expected), "{name}: {error}");
    }
}

#[test]
fn catalog_requirements_need_a_proven_public_package_root() {
    let error = PackageCatalog::new(
        [CatalogModule::new(
            "tools/extra",
            "{}",
            "tools",
            "tools/extra.simi",
            CatalogModuleVisibility::Public,
        )
        .unwrap()],
        [CatalogRequirement::new(
            "tools",
            RequirementSource::Path {
                path: "deps/tools".to_owned(),
            },
        )],
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("has no proven public root module")
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

    let forged_std = PackageCatalog::new(
        [CatalogModule::new(
            "std/forged",
            "{}",
            "std",
            "std/forged.simi",
            CatalogModuleVisibility::Public,
        )
        .unwrap()],
        [],
    )
    .unwrap();
    let reserved = match Engine::builder().catalog(forged_std).build().eval("42") {
        Err(error) => error,
        Ok(_) => panic!("forged std namespace must be rejected"),
    };
    assert!(reserved.to_string().contains("reserved `std/` namespace"));

    let Err(relative) = Engine::new().eval("require(\"./private.simi\")") else {
        panic!("relative import must be a hard error");
    };
    assert!(
        relative
            .to_string()
            .contains("package-relative imports require prior package resolution")
    );
}
