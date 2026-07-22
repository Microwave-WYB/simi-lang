use simiscript::eval;

#[test]
fn tables_support_named_integer_string_and_boolean_keys() {
    let value = eval(
        r#"
        let dynamic = "dynamic key"
        let table = {
            name="Simi",
            [10]="ten",
            [true]="yes",
            [dynamic]=42,
        }
        [table.name, table["name"], table[10], table[true], table[dynamic]]
        "#,
    )
    .expect("table program should have no diagnostics")
    .expect("table program should not raise");

    assert_eq!(value.render(), "[\"Simi\", \"Simi\", \"ten\", \"yes\", 42]");
}

#[test]
fn missing_table_keys_return_nil_and_computed_duplicates_replace_values() {
    let value = eval(
        r#"
        let key = 7
        let table = {[key]="first", [7]="second"}
        [table[key], table.missing]
        "#,
    )
    .expect("table program should have no diagnostics")
    .expect("table program should not raise");

    assert_eq!(value.render(), "[\"second\", nil]");
}

#[test]
fn nil_table_values_omit_or_delete_keys() {
    let value = eval(
        r#"
        let dynamic = 1
        let table = {omitted=nil, kept=1, [dynamic]="temporary"}
        let field_result = table.kept = nil
        let index_result = table[dynamic] = nil
        table.after = 2
        [field_result, index_result, table.omitted, table.kept, table[dynamic], table]
        "#,
    )
    .expect("table deletion program should have no diagnostics")
    .expect("table deletion program should not raise");

    assert_eq!(value.render(), "[nil, nil, nil, nil, nil, {after=2}]");
}

#[test]
fn indexing_lists_and_tables_dispatches_by_runtime_value() {
    let value = eval(
        r#"
        let values = [10, 20]
        let table = {[0]="zero"}
        [values[1], table[0]]
        "#,
    )
    .expect("indexing program should have no diagnostics")
    .expect("indexing program should not raise");

    assert_eq!(value.render(), "[20, \"zero\"]");
}

#[test]
fn unsupported_table_keys_are_runtime_errors() {
    let error = match eval("{[nil]=1}") {
        Ok(Ok(value)) => panic!("nil table key should fail, got {}", value.render()),
        Ok(Err(raised)) => panic!("nil table key should be a hard error, got {raised}"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("table key must be"));
}
