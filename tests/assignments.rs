use simiscript::{SimiError, Value, eval};

fn value(source: &str) -> Value {
    eval(source)
        .expect("source should have no hard diagnostic")
        .expect("source should not leave an uncaught raise")
}

#[test]
fn variable_assignment_is_expression_valued_right_associative_and_lexical() {
    let result = value(
        r#"
        let outer = 1
        let other = 2
        fn update(parameter) do
            outer = other = parameter = 9
            [outer, other, parameter]
        end
        let inside = update(3)
        let matched = match [4] with
            case [item] -> item = item + 1
        end
        let looped = loop state = 0 do
            if state == 3 then break state else state = state + 1 end
        end
        [inside, outer, other, matched, looped]
        "#,
    );
    assert_eq!(result.render(), "[[9, 9, 9], 9, 9, 5, 3]");
}

#[test]
fn closures_update_captured_bindings_and_undefined_assignment_is_hard() {
    let result = value(
        r#"
        fn counter() do
            let count = 0
            fn next() do count = count + 1 end
            next
        end
        let next = counter()
        [next(), next()]
        "#,
    );
    assert_eq!(result.render(), "[1, 2]");

    match eval("missing = 1") {
        Err(SimiError::Runtime(error)) => {
            assert!(
                error
                    .message
                    .contains("cannot assign to undefined name `missing`")
            );
        }
        _ => panic!("undefined assignment should be a hard runtime error"),
    }
}

#[test]
fn list_and_map_updates_mutate_aliases_and_return_the_rhs() {
    let result = value(
        r#"
        let list_value = [10, 20]
        let list_alias = list_value
        let map_value = {existing=1}
        let map_alias = map_value
        let list_result = list_alias[0] = 30
        let field_result = map_alias.field = 40
        let index_result = map_alias[true] = 50
        map_alias[7] = 60
        map_alias.existing = 2
        [list_result, field_result, index_result, list_value, map_value]
        "#,
    );
    assert_eq!(
        result.render(),
        "[30, 40, 50, [30, 20], {existing=2, field=40, [true]=50, [7]=60}]"
    );
}

#[test]
fn assignment_prepares_object_and_key_once_before_rhs() {
    let result = value(
        r#"
        let list = require("std/list")
        let events = []
        let target = {slot=0}
        fn object() do
            list.append(events, "object")
            target
        end
        fn key() do
            list.append(events, "key")
            "slot"
        end
        fn field_object() do
            list.append(events, "field_object")
            target
        end
        fn rhs() do
            list.append(events, "rhs")
            7
        end
        object()[key()] = rhs()
        field_object().slot = 8
        [events, target]
        "#,
    );
    assert_eq!(
        result.render(),
        "[[\"object\", \"key\", \"rhs\", \"field_object\"], {slot=8}]"
    );
}

#[test]
fn failed_variable_target_skips_the_rhs() {
    // If the RHS ran, this would produce an inner Raised result instead of the
    // undefined-target hard runtime error.
    assert!(matches!(
        eval("missing = raise \"rhs ran\""),
        Err(SimiError::Runtime(_))
    ));
}

#[test]
fn list_bounds_reads_return_nil_while_writes_raise_without_growth() {
    let result = value(
        r#"
        let list = require("std/list")
        let values = [1]
        let rhs_ran = []
        let read = values[2]
        let write = try values[3] = list.append(rhs_ran, true) catch
            case {error=error, index=index, length=length} -> [error, index, length]
        end
        let get = list.get(values, 4)
        let set = try list.set(values, 5, 9) catch
            case {error=error, index=index, length=length} -> [error, index, length]
        end
        [read, write, get, set, values, rhs_ran]
        "#,
    );
    assert_eq!(
        result.render(),
        "[nil, [\"index_out_of_bounds\", 3, 1], nil, [\"index_out_of_bounds\", 5, 1], [1], []]"
    );
}

#[test]
fn native_set_bounds_raises_preserve_the_call_origin_and_user_frame() {
    let source = "let list = require(\"std/list\")\nfn write(values) do list.set(values, 2, 9) end write([1])";
    let raised = match eval(source).expect("source should have no hard diagnostic") {
        Err(raised) => raised,
        Ok(value) => panic!("expected native bounds raise, got {}", value.render()),
    };
    assert_eq!(
        raised.value.render(),
        "{error=\"index_out_of_bounds\", index=2, length=1}"
    );
    let native_start = source.find("list.set").unwrap();
    assert_eq!(raised.origin.start, native_start);
    assert_eq!(raised.frames.len(), 1);
    assert_eq!(raised.frames[0].function, "write");
    assert_eq!(
        raised.frames[0].call_span.start,
        source.rfind("write([1])").unwrap()
    );
}

#[test]
fn negative_and_wrong_type_list_indices_remain_hard_errors() {
    for source in [
        "try [1][0 - 1] catch case _ -> nil end",
        "try [1][\"0\"] = 2 catch case _ -> nil end",
        "let list = require(\"std/list\")\ntry list.get([1], 0 - 1) catch case _ -> nil end",
        "let list = require(\"std/list\")\ntry list.set([1], \"0\", 2) catch case _ -> nil end",
    ] {
        assert!(matches!(eval(source), Err(SimiError::Runtime(_))));
    }
}

#[test]
fn invalid_assignment_targets_are_parse_errors() {
    for source in ["1 = 2", "(1 + 2) = 3", "fn f() do 1 end f() = 2"] {
        assert!(matches!(eval(source), Err(SimiError::Parse(_))));
    }
}
