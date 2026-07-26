use simi::{Engine, SimiError, eval};

#[test]
fn map_inspection_preserves_mixed_key_insertion_order() {
    let value = eval(
        r#"

        let iter = require("std/iter")
        let source = {
            first=1,
            [2]="second",
            [false]=3,
            [1.5]="fourth",
        }
        [map.length(source), iter.to_list(map.iter(source))]
        "#,
    )
    .expect("map inspection should have no hard diagnostic")
    .expect("map inspection should not raise");

    assert_eq!(
        value.render(),
        "[4, [{key=\"first\", value=1}, {key=2, value=\"second\"}, {key=false, value=3}, {key=1.5, value=\"fourth\"}]]"
    );
}

#[test]
fn map_copy_preserves_order_and_normalized_keys_with_shallow_independence() {
    let value = eval(
        r#"

        let iter = require("std/iter")

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
        [iter.to_list(map.iter(copied)), source, copied]
        "#,
    )
    .expect("std/map.copy should have no hard diagnostic")
    .expect("std/map.copy should not raise");

    assert_eq!(
        value.render(),
        "[[{key=\"first\", value=[1, 2]}, {key=1, value=\"one\"}, {key=false, value=3}, {key=\"last\", value=4}], {first=[1, 2], [1]=\"changed\", [false]=3}, {first=[1, 2], [1]=\"one\", [false]=3, last=4}]"
    );
}

#[test]
fn map_has_reflects_absence_and_normalized_numeric_keys() {
    let value = eval(
        r#"

        let iter = require("std/iter")
        let source = {[1]="one", [0]="zero"}
        [
            map.has(source, 1.0),
            map.has(source, -0.0),
            map.has(source, 2),
            iter.to_list(map.iter(source)),
        ]
        "#,
    )
    .expect("std/map.has should have no hard diagnostic")
    .expect("std/map.has should not raise");

    assert_eq!(
        value.render(),
        "[true, true, false, [{key=1, value=\"one\"}, {key=0, value=\"zero\"}]]"
    );
}

#[test]
fn map_clear_mutates_aliases_and_returns_nil() {
    let value = eval(
        r#"

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
    let wrong_copy = match eval(" map.copy([])") {
        Err(error) => error,
        Ok(_) => panic!("wrong copy argument should be a hard diagnostic"),
    };
    assert!(
        wrong_copy
            .to_string()
            .contains("std/map.copy requires a map, got list")
    );

    let wrong_copy_arity = match eval(" map.copy()") {
        Err(error) => error,
        Ok(_) => panic!("wrong copy arity should be a hard diagnostic"),
    };
    assert!(
        wrong_copy_arity
            .to_string()
            .contains("native function `std/map.copy` expects 1 arguments, got 0")
    );

    let wrong_map = match eval(" map.iter([])") {
        Err(error) => error,
        Ok(_) => panic!("wrong map argument should be a hard diagnostic"),
    };
    assert!(
        wrong_map
            .to_string()
            .contains("std/map.iter requires a map, got list")
    );

    let wrong_key = match eval(" map.has({}, [])") {
        Err(error) => error,
        Ok(_) => panic!("wrong key argument should be a hard diagnostic"),
    };
    assert!(
        wrong_key
            .to_string()
            .contains("std/map.has key must be a string, integer, float, or boolean, got list")
    );

    let wrong_arity = match eval(" map.clear()") {
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
fn built_in_map_module_path_is_not_requireable() {
    let source = "require(\"std/map\")";
    let raised = match Engine::new()
        .eval(source)
        .expect("missing built-in map path should not be a hard diagnostic")
    {
        Err(raised) => raised,
        Ok(value) => panic!(
            "built-in map path should raise module_not_found, got {}",
            value.render()
        ),
    };
    assert_eq!(
        raised.value.render(),
        "{error=\"module_not_found\", module=\"std/map\"}"
    );
    assert_eq!(raised.origin.start, 0);
    assert_eq!(raised.origin.end, source.len());
}
