use simi::eval;

#[test]
fn annotations_aliases_and_all_initial_type_shapes_are_runtime_erased() {
    let source = r#"
alias option('a) = 'a | nil
alias pair('a, 'b) = ['a, 'b]
let option = 40
let deliberately_wrong: integer = "still dynamic"
fn identity(value: 'a) -> 'a do value end
let add: (integer, integer) -> integer = fn(left: integer, right: integer) -> integer do left + right end
let exact: pair(integer, string) = [1, "one"]
let many: [..integer] = [2, 3]
let record: { name: string, .. } = { name = "Simi", enabled = true }
let indexed: { [string | integer]: boolean } = { ready = true, [1] = false }
fn declared_effect(left: [..integer], right: [..string]) -> nil
    after left becomes [..integer | string]
    after right becomes [..string]
do nil end
declared_effect(exact, many)
[option + exact[0], type(deliberately_wrong), identity(exact[1]), add(exact[0], many[0]), record.name, indexed[1]]
alias trailing = option(string)
"#;
    let result = eval(source)
        .expect("runtime parsing succeeds")
        .expect("no raise");
    assert_eq!(
        result.render(),
        "[41, \"string\", \"one\", 3, \"Simi\", false]"
    );
}

#[test]
fn annotations_do_not_turn_static_mismatches_into_runtime_checks() {
    let result = eval("let value: integer = \"text\" value")
        .expect("runtime accepts erased annotation")
        .expect("no raise");
    assert_eq!(result.render(), "\"text\"");
}
