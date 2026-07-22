use simiscript::{Engine, SimiError, eval};

#[test]
fn map_inspection_preserves_mixed_key_insertion_order() {
    let value = eval(
        r#"
        let map = require("std/map")
        let source = {
            first=1,
            [2]="second",
            [false]=3,
            [1.5]="fourth",
        }
        [
            map.length(source),
            map.keys(source),
            map.values(source),
            map.entries(source),
        ]
        "#,
    )
    .expect("map inspection should have no hard diagnostic")
    .expect("map inspection should not raise");

    assert_eq!(
        value.render(),
        "[4, [\"first\", 2, false, 1.5], [1, \"second\", 3, \"fourth\"], [[\"first\", 1], [2, \"second\"], [false, 3], [1.5, \"fourth\"]]]"
    );
}

#[test]
fn map_has_reflects_absence_and_normalized_numeric_keys() {
    let value = eval(
        r#"
        let map = require("std/map")
        let source = {[1]="one", [0]="zero"}
        [
            map.has(source, 1.0),
            map.has(source, -0.0),
            map.has(source, 2),
            map.keys(source),
        ]
        "#,
    )
    .expect("std/map.has should have no hard diagnostic")
    .expect("std/map.has should not raise");

    assert_eq!(value.render(), "[true, true, false, [1, 0]]");
}

#[test]
fn map_clear_mutates_aliases_and_returns_nil() {
    let value = eval(
        r#"
        let map = require("std/map")
        let source = {first=1, second=2}
        let alias = source
        let result = map.clear(source)
        [result, map.length(alias), alias]
        "#,
    )
    .expect("std/map.clear should have no hard diagnostic")
    .expect("std/map.clear should not raise");

    assert_eq!(value.render(), "[nil, 0, {}]");
}

#[test]
fn map_argument_errors_are_qualified_hard_diagnostics() {
    let wrong_map = match eval("let map = require(\"std/map\") map.values([])") {
        Err(error) => error,
        Ok(_) => panic!("wrong map argument should be a hard diagnostic"),
    };
    assert!(
        wrong_map
            .to_string()
            .contains("std/map.values requires a map, got list")
    );

    let wrong_key = match eval("let map = require(\"std/map\") map.has({}, [])") {
        Err(error) => error,
        Ok(_) => panic!("wrong key argument should be a hard diagnostic"),
    };
    assert!(
        wrong_key
            .to_string()
            .contains("std/map.has key must be a string, integer, float, or boolean, got list")
    );

    let wrong_arity = match eval("let map = require(\"std/map\") map.clear()") {
        Err(error) => error,
        Ok(_) => panic!("wrong arity should be a hard diagnostic"),
    };
    assert!(matches!(wrong_arity, SimiError::Runtime(_)));
    assert!(
        wrong_arity
            .to_string()
            .contains("native function `std/map.clear` expects 1 arguments, got 0")
    );
}

#[test]
fn map_module_is_only_present_in_standard_library_engines() {
    let missing = match Engine::new()
        .eval("require(\"std/map\")")
        .expect("missing map module should be a raise")
    {
        Err(raised) => raised,
        Ok(value) => panic!(
            "empty engine should not contain the map module, got {}",
            value.render()
        ),
    };
    assert_eq!(
        missing.value.render(),
        "{error=\"module_not_found\", module=\"std/map\"}"
    );

    let exports = Engine::with_stdlib()
        .eval("require(\"std/map\")")
        .expect("standard map module should have no hard diagnostic")
        .expect("standard map module should not raise");
    assert_eq!(
        exports.render(),
        "{length=<native std/map.length>, has=<native std/map.has>, keys=<native std/map.keys>, values=<native std/map.values>, entries=<native std/map.entries>, clear=<native std/map.clear>}"
    );
}
