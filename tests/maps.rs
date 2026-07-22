use simiscript::eval;

#[test]
fn maps_support_named_integer_string_and_boolean_keys() {
    let value = eval(
        r#"
        let dynamic = "dynamic key"
        let map = {
            name="Simi",
            [10]="ten",
            [true]="yes",
            [dynamic]=42,
        }
        [map.name, map["name"], map[10], map[true], map[dynamic]]
        "#,
    )
    .expect("map program should have no diagnostics")
    .expect("map program should not raise");

    assert_eq!(value.render(), "[\"Simi\", \"Simi\", \"ten\", \"yes\", 42]");
}

#[test]
fn missing_map_keys_return_nil_and_computed_duplicates_replace_values() {
    let value = eval(
        r#"
        let key = 7
        let map = {[key]="first", [7]="second"}
        [map[key], map.missing]
        "#,
    )
    .expect("map program should have no diagnostics")
    .expect("map program should not raise");

    assert_eq!(value.render(), "[\"second\", nil]");
}

#[test]
fn nil_map_values_omit_or_delete_keys() {
    let value = eval(
        r#"
        let dynamic = 1
        let map = {omitted=nil, kept=1, [dynamic]="temporary"}
        let field_result = map.kept = nil
        let index_result = map[dynamic] = nil
        map.after = 2
        [field_result, index_result, map.omitted, map.kept, map[dynamic], map]
        "#,
    )
    .expect("map deletion program should have no diagnostics")
    .expect("map deletion program should not raise");

    assert_eq!(value.render(), "[nil, nil, nil, nil, nil, {after=2}]");
}

#[test]
fn indexing_lists_and_maps_dispatches_by_runtime_value() {
    let value = eval(
        r#"
        let values = [10, 20]
        let map = {[0]="zero"}
        [values[1], map[0]]
        "#,
    )
    .expect("indexing program should have no diagnostics")
    .expect("indexing program should not raise");

    assert_eq!(value.render(), "[20, \"zero\"]");
}

#[test]
fn unsupported_map_keys_are_runtime_errors() {
    let error = match eval("{[nil]=1}") {
        Ok(Ok(value)) => panic!("nil map key should fail, got {}", value.render()),
        Ok(Err(raised)) => panic!("nil map key should be a hard error, got {raised}"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("map key must be"));
}
