use simi::{SimiError, Value, eval};

fn value(source: &str) -> Value {
    eval(source)
        .expect("source should have no hard diagnostic")
        .expect("source should not leave an uncaught raise")
}

#[test]
fn binding_reassignment_is_a_parse_error_for_every_lexical_binding() {
    for source in [
        "let value = 1\nvalue = 2",
        "fn update(parameter) do\n    parameter = 9\nend",
        "let outer = {}\nouter.field = missing = 1",
        "case [4] of\n    [item] =>\n        item = item + 1\nend",
        "missing = 1",
    ] {
        match eval(source) {
            Err(SimiError::Parse(error)) => {
                assert_eq!(
                    error.message,
                    "bindings are immutable and cannot be reassigned; \
                     declare a new binding with let or mutate a list or map field",
                    "{source}"
                );
            }
            _ => panic!("expected reassignment parse error for {source}"),
        }
    }
}

#[test]
fn closures_evolve_state_through_mutable_container_fields() {
    let result = value(
        r#"
        fn counter() do
            let state = {count = 0}
            let next = fn()
                state.count = state.count + 1
            next
        end
        let next = counter()
        [next(), next()]
        "#,
    );
    assert_eq!(result.render(), "[1, 2]");
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
fn variable_targets_are_rejected_before_the_rhs_can_run() {
    // If evaluation reached the RHS, this would produce an inner Raised result
    // instead of the immutable-binding parse error.
    assert!(matches!(
        eval("missing = raise \"rhs ran\""),
        Err(SimiError::Parse(_))
    ));
}

#[test]
fn list_bounds_reads_return_nil_while_writes_raise_without_growth() {
    let result = value(
        r#"

        let values = [1]
        let rhs_ran = []
        let read = values[2]
        let write = do values[3] = list.append(rhs_ran, true)
            catch {error=error, index=index, length=length, ..} =>
                [error, index, length]
        end
        let get = list.get(values, 4)
        let set = do list.set(values, 5, 9)
            catch {error=error, index=index, length=length, ..} =>
                [error, index, length]
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
    let source = "\nfn write(values) do list.set(values, 2, 9) end write([1])";
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
        "do [1][0 - 1] catch _ => nil end",
        "do [1][\"0\"] = 2 catch _ => nil end",
        "\ndo list.get([1], 0 - 1) catch _ => nil end",
        "\ndo list.set([1], \"0\", 2) catch _ => nil end",
    ] {
        assert!(matches!(eval(source), Err(SimiError::Runtime(_))));
    }
}

#[test]
fn invalid_assignment_targets_are_parse_errors() {
    for source in [
        "1 = 2",
        "(1 + 2) = 3",
        "fn f()
    1 f() = 2",
    ] {
        assert!(matches!(eval(source), Err(SimiError::Parse(_))));
    }
}
