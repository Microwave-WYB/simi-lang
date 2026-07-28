use simi::eval;

#[test]
fn legacy_loop_and_label_forms_are_rejected() {
    for source in ["loop break 1 end", "@outer loop break 1 end"] {
        assert!(eval(source).is_err(), "{source} should be rejected");
    }
}

#[test]
fn iterator_control_members_remain_callable() {
    let value = eval("let iter = require(\"std/iter\") [iter.break(7), iter.continue(nil)]")
        .unwrap()
        .unwrap();
    assert_eq!(
        value.render(),
        "[{control=\"break\", value=7}, {control=\"continue\"}]"
    );
}
