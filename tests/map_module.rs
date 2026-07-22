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
fn map_copy_preserves_order_and_normalized_keys_with_shallow_independence() {
    let value = eval(
        r#"
        let map = require("std/map")
        let list = require("std/list")
        let nested = [1]
        let source = {
            first=nested,
            [1.0]="one",
            [false]=3,
        }
        let copied = map.copy(source)
        source[1] = "changed"
        copied.last = 4
        list.append(nested, 2)
        [map.keys(copied), source, copied]
        "#,
    )
    .expect("std/map.copy should have no hard diagnostic")
    .expect("std/map.copy should not raise");

    assert_eq!(
        value.render(),
        "[[\"first\", 1, false, \"last\"], {first=[1, 2], [1]=\"changed\", [false]=3}, {first=[1, 2], [1]=\"one\", [false]=3, last=4}]"
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
    let wrong_copy = match eval("let map = require(\"std/map\") map.copy([])") {
        Err(error) => error,
        Ok(_) => panic!("wrong copy argument should be a hard diagnostic"),
    };
    assert!(
        wrong_copy
            .to_string()
            .contains("std/map.copy requires a map, got list")
    );

    let wrong_copy_arity = match eval("let map = require(\"std/map\") map.copy()") {
        Err(error) => error,
        Ok(_) => panic!("wrong copy arity should be a hard diagnostic"),
    };
    assert!(
        wrong_copy_arity
            .to_string()
            .contains("native function `std/map.copy` expects 1 arguments, got 0")
    );

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
        "{length=<native std/map.length>, copy=<native std/map.copy>, has=<native std/map.has>, keys=<native std/map.keys>, values=<native std/map.values>, entries=<native std/map.entries>, clear=<native std/map.clear>}"
    );
}
