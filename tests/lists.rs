use simiscript::{SimiError, eval};

fn assert_eval(source: &str, expected: &str) {
    let value = eval(source)
        .expect("source should have no hard diagnostic")
        .expect("source should not raise");
    assert_eq!(value.render(), expected);
}

#[test]
fn list_mutations_update_aliases_and_return_documented_values() {
    assert_eval(
        r#"
            let list = require("std/list")
            let values = [1, 3]
            let alias = values
            let first_insert = list.insert(values, 1, 2)
            let end_insert = list.insert(values, 3, 4)
            let removed = list.remove(values, 0)
            let popped = list.pop(values)
            let reversed = list.reverse(values)
            [first_insert, end_insert, removed, popped, reversed, values, alias]
        "#,
        "[nil, nil, 1, 4, nil, [3, 2], [3, 2]]",
    );
}

#[test]
fn list_mutation_bounds_raise_structural_values() {
    assert_eval(
        r#"
            let list = require("std/list")
            let values = [1]
            let insert_error = try list.insert(values, 2, 9) catch
                case error -> error
            end
            let remove_error = try list.remove(values, 1) catch
                case error -> error
            end
            let pop_error = try list.pop([]) catch
                case error -> error
            end
            [insert_error, remove_error, pop_error, values]
        "#,
        "[{error=\"index_out_of_bounds\", index=2, length=1}, {error=\"index_out_of_bounds\", index=1, length=1}, {error=\"index_out_of_bounds\", index=0, length=0}, [1]]",
    );
}

#[test]
fn list_slice_clamps_and_creates_independent_shallow_cow_views() {
    assert_eval(
        r#"
            let list = require("std/list")
            let nested = [7]
            let source = [0, nested, 2, 3]
            let view = list.slice(source, 1, 3)
            list.set(source, 2, 9)
            list.append(nested, 8)
            list.set(view, 1, 4)
            [
                source,
                view,
                list.slice(source, 2, 20),
                list.slice(source, 3, 1),
                list.slice(source, 20, 30),
            ]
        "#,
        "[[0, [7, 8], 9, 3], [[7, 8], 4], [9, 3], [], []]",
    );
}

#[test]
fn list_contains_uses_simi_primitive_equality() {
    assert_eval(
        r#"
            let list = require("std/list")
            [
                list.contains([1, "one", true, nil], 1.0),
                list.contains([1, "one", true, nil], "one"),
                list.contains([1, "one", true, nil], false),
                list.contains([1, "one", true, nil], nil),
            ]
        "#,
        "[true, true, false, true]",
    );
}

#[test]
fn list_contains_rejects_cyclic_container_comparison_without_recursing() {
    let error = match eval(
        r#"
            let list = require("std/list")
            let cyclic = []
            list.append(cyclic, cyclic)
            list.contains(cyclic, cyclic)
        "#,
    ) {
        Err(error) => error,
        Ok(_) => panic!("container equality should be a hard diagnostic"),
    };
    assert!(
        error
            .to_string()
            .contains("equality is not supported for list and list")
    );
}

#[test]
fn new_list_indices_retain_hard_type_diagnostics() {
    for source in [
        "let list = require(\"std/list\") try list.insert([], -1, nil) catch case _ -> nil end",
        "let list = require(\"std/list\") try list.remove([1], 0.0) catch case _ -> nil end",
        "let list = require(\"std/list\") try list.slice([1], \"0\", 1) catch case _ -> nil end",
        "let list = require(\"std/list\") try list.slice([1], 0, true) catch case _ -> nil end",
    ] {
        assert!(matches!(eval(source), Err(SimiError::Runtime(_))));
    }
}
