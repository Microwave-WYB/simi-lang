use simi::eval;

#[test]
fn repeated_let_creates_a_new_binding_while_closures_keep_the_old_one() {
    let value = eval(
        r#"
let value = 1
let before = fn() do value end
let value = value + 1
let read_after = fn() do value end
[before(), read_after(), value]
"#,
    )
    .unwrap()
    .unwrap();

    assert_eq!(value.render(), "[1, 2, 2]");
}

#[test]
fn assignments_update_the_binding_selected_by_each_lexical_view() {
    let value = eval(
        r#"
let value = 1
let get_before = fn() do value end
let set_before = fn(next) do value = next end
let value = 2
value = 3
set_before(4)
[get_before(), value]
"#,
    )
    .unwrap()
    .unwrap();

    assert_eq!(value.render(), "[4, 3]");
}

#[test]
fn forward_capture_survives_an_intervening_same_scope_shadow() {
    let value = eval(
        r#"
let read_later = fn() do later end
let value = 1
let value = 2
let later = 3
read_later()
"#,
    )
    .unwrap()
    .unwrap();

    assert_eq!(value.render(), "3");
}

#[test]
fn mixed_destructuring_shadow_backfills_fresh_siblings() {
    let value = eval(
        r#"
let read_fresh = fn() do fresh end
let old = 1
let [old, fresh] = [2, 3]
[old, read_fresh()]
"#,
    )
    .unwrap()
    .unwrap();

    assert_eq!(value.render(), "[2, 3]");
}

#[test]
fn cycle_demo_shadows_with_the_same_mutated_list_alias() {
    let value = eval(
        r#"
let list = require("std/list")
let nums = [1, 2, 3]
let nums = nums |> tap list.append(nums)
nums[3]
"#,
    )
    .unwrap()
    .unwrap();

    assert_eq!(value.render(), "[1, 2, 3, <cycle>]");
}
