use simi::eval;

fn assert_eval(source: &str, expected: &str) {
    let value = eval(source)
        .expect("program should evaluate")
        .expect("should not raise");
    assert_eq!(value.render(), expected);
}

fn assert_parse_error(source: &str) {
    assert!(eval(source).is_err(), "{source} should be rejected");
}

#[test]
fn fallthrough_repeats_with_ordinary_mutable_lexical_state() {
    assert_eval(
        r#"
do
    let count = 0
    loop
        count = count + 1
        if count == 3 then break count end
    end
end
"#,
        "3",
    );
}

#[test]
fn bare_continue_repeats_and_discards_the_body_value() {
    assert_eval(
        r#"
do
    let count = 0
    loop
        count = count + 1
        if count < 3 then continue end
        break count
    end
end
"#,
        "3",
    );
}

#[test]
fn mutable_collections_are_ordinary_loop_state() {
    assert_eval(
        r#"
do
    let values = [1, 2, 3]
    let result = []
    let index = 0
    loop
        if index == list.length(values) then break result end
        list.append(result, values[index])
        index = index + 1
    end
end
"#,
        "[1, 2, 3]",
    );
}

#[test]
fn labels_still_target_enclosing_loops() {
    assert_eval(
        r#"
do
    let count = 0
    @outer loop
        loop
            count = count + 1
            if count == 1 then continue @outer else break @outer count end
        end
        break 0
    end
end
"#,
        "2",
    );
}

#[test]
fn loop_supports_euclids_gcd_with_reassigned_primitive_bindings() {
    assert_eval(
        r#"
fn gcd(left, right) do
    loop
        if right == 0 then break left end
        let remainder = left % right
        left = right
        right = remainder
    end
end
gcd(1071, 462)
"#,
        "21",
    );
}

#[test]
fn loop_scans_a_list_with_explicit_lexical_iterator_state() {
    assert_eval(
        r#"
do
    let values = [2, 7, 11, 15]
    let index = 0
    loop
        if index == list.length(values) then break nil end
        if values[index] == 11 then break index end
        index = index + 1
    end
end
"#,
        "2",
    );
}

#[test]
fn old_loop_forms_and_valued_continue_are_rejected() {
    for source in [
        "loop do break 1 end",
        "loop state = 0 do break state end",
        "loop continue 1 end",
        "loop continue value end",
        "loop continue (1) end",
        "loop continue list.length([]) end",
    ] {
        assert_parse_error(source);
    }
}
