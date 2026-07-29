use simi::{Engine, SimiError, eval};

fn value(source: &str) -> simi::Value {
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
            string.concat("Simi", " language"),
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
        "[\"Simi language\", 3, \"é🦀\", true, true, true, \"hello\", \"äbc\", \"STRASSE\"]"
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
fn string_prelude_and_canonical_path_share_standard_library_identity() {
    let value = Engine::with_stdlib()
        .eval(
            r#"
            string.marker = "shared"
            [string.upper("ok"), require("std/string").marker]
            "#,
        )
        .expect("portable string module should not hard fail")
        .expect("portable string module should not raise");
    assert_eq!(value.render(), "[\"OK\", \"shared\"]");
}

#[test]
fn wrong_types_and_indices_remain_uncatchable_hard_diagnostics() {
    for (source, qualified_name) in [
        (
            "let string = require(\"std/string\") do string.length(1) catch _ => nil end",
            "std/string.length",
        ),
        (
            "let string = require(\"std/string\") do string.slice(\"abc\", 0 - 1, 2) catch _ => nil end",
            "std/string.slice",
        ),
        (
            "let string = require(\"std/string\") do string.slice(\"abc\", 0, 2.0) catch _ => nil end",
            "std/string.slice",
        ),
        (
            "let string = require(\"std/string\") do string.contains(\"abc\", 1) catch _ => nil end",
            "std/string.contains",
        ),
        (
            "let string = require(\"std/string\") do string.concat(\"abc\", 1) catch _ => nil end",
            "std/string.concat",
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
