use simiscript::{Engine, SimiError, eval};

fn value(source: &str) -> simiscript::Value {
    eval(source)
        .expect("source should have no hard diagnostic")
        .expect("source should not raise")
}

#[test]
fn standard_string_module_is_available_through_the_public_eval_api() {
    let result = value(
        r#"
        let string = require("std/string")
        [
            string.length("aé🦀"),
            string.slice("aé🦀z", 1, 3),
            string.contains("café", "fé"),
            string.starts_with("🦀acean", "🦀"),
            string.ends_with("naïve", "ïve"),
            string.trim("  hello \n"),
            string.lower("ÄBC"),
            string.upper("Straße")
        ]
        "#,
    );

    assert_eq!(
        result.render(),
        "[3, \"é🦀\", true, true, true, \"hello\", \"äbc\", \"STRASSE\"]"
    );
}

#[test]
fn slice_bounds_and_split_semantics_are_publicly_observable() {
    let result = value(
        r#"
        let string = require("std/string")
        [
            string.slice("abc", 1, 99),
            string.slice("abc", 99, 100),
            string.slice("abc", 2, 1),
            string.split(",a,,b,", ","),
            string.split("aé🦀", ""),
            string.split("", "")
        ]
        "#,
    );

    assert_eq!(
        result.render(),
        "[\"bc\", \"\", \"\", [\"\", \"a\", \"\", \"b\", \"\"], [\"a\", \"é\", \"🦀\"], []]"
    );
}

#[test]
fn string_module_is_an_explicit_capability() {
    let missing = match Engine::new()
        .eval("require(\"std/string\")")
        .expect("missing module should be a language raise")
    {
        Err(raised) => raised,
        Ok(value) => panic!(
            "empty engine should not contain string module, got {}",
            value.render()
        ),
    };
    assert_eq!(
        missing.value.render(),
        "{error=\"module_not_found\", module=\"std/string\"}"
    );

    let direct = Engine::builder()
        .module(simiscript::stdlib::string())
        .build()
        .eval("let string = require(\"std/string\") string.upper(\"ok\")")
        .unwrap()
        .unwrap();
    assert_eq!(direct.render(), "\"OK\"");
}

#[test]
fn wrong_types_and_indices_remain_uncatchable_hard_diagnostics() {
    for (source, qualified_name) in [
        (
            "let string = require(\"std/string\") try string.length(1) catch _ do nil end end",
            "std/string.length",
        ),
        (
            "let string = require(\"std/string\") try string.slice(\"abc\", 0 - 1, 2) catch _ do nil end end",
            "std/string.slice",
        ),
        (
            "let string = require(\"std/string\") try string.slice(\"abc\", 0, 2.0) catch _ do nil end end",
            "std/string.slice",
        ),
        (
            "let string = require(\"std/string\") try string.contains(\"abc\", 1) catch _ do nil end end",
            "std/string.contains",
        ),
    ] {
        let error = match eval(source) {
            Err(error) => error,
            Ok(Ok(value)) => panic!("expected hard diagnostic, got {}", value.render()),
            Ok(Err(raised)) => panic!("expected hard diagnostic, got {raised}"),
        };
        assert!(matches!(error, SimiError::Runtime(_)));
        assert!(error.to_string().contains(qualified_name));
    }
}
