use simi::eval;

#[test]
fn leading_requires_is_runtime_metadata() {
    let result = eval(
        r#"
requires {name = "example", version = 1}
let value = 40
value + 2
"#,
    )
    .expect("runtime parsing succeeds")
    .expect("no raise");

    assert_eq!(result.render(), "42");
}
