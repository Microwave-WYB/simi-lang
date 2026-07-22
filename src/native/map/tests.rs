use gc::{Gc, GcCell};

use super::*;
use crate::runtime::{FloatKey, List};

fn map(entries: Vec<(MapKey, Value)>) -> Value {
    Value::Map(Gc::new(GcCell::new(entries)))
}

fn call(function: fn(&[Value], Span) -> NativeResult, args: &[Value]) -> Value {
    function(args, Span::new(0, 1)).unwrap().unwrap()
}

fn hard_error(result: NativeResult) -> RuntimeError {
    match result {
        Err(error) => error,
        Ok(Ok(value)) => panic!("expected hard error, got {}", value.render()),
        Ok(Err(raised)) => panic!("expected hard error, got raise {}", raised.value.render()),
    }
}

#[test]
fn inspection_preserves_mixed_key_insertion_order() {
    let values = map(vec![
        (MapKey::String("first".to_owned()), Value::Int(1)),
        (MapKey::Int(2), Value::String("second".to_owned())),
        (MapKey::Bool(false), Value::Int(3)),
        (
            MapKey::Float(FloatKey::new(1.5).unwrap()),
            Value::String("fourth".to_owned()),
        ),
    ]);

    assert_eq!(
        call(map_length, std::slice::from_ref(&values)).render(),
        "4"
    );
    assert_eq!(
        call(map_keys, std::slice::from_ref(&values)).render(),
        "[\"first\", 2, false, 1.5]"
    );
    assert_eq!(
        call(map_values, std::slice::from_ref(&values)).render(),
        "[1, \"second\", 3, \"fourth\"]"
    );
    assert_eq!(
        call(map_entries, std::slice::from_ref(&values)).render(),
        "[[\"first\", 1], [2, \"second\"], [false, 3], [1.5, \"fourth\"]]"
    );
}

#[test]
fn copy_preserves_order_and_nested_aliases_with_independent_outer_storage() {
    let nested = List::shared(vec![Value::Int(1)]);
    let source = map(vec![
        (
            MapKey::String("first".to_owned()),
            Value::List(nested.clone()),
        ),
        (MapKey::Int(1), Value::String("one".to_owned())),
        (MapKey::Bool(false), Value::Int(3)),
    ]);
    let copied = call(map_copy, std::slice::from_ref(&source));

    let (Value::Map(source_map), Value::Map(copied_map)) = (&source, &copied) else {
        panic!("copy should return a map")
    };
    assert!(!Gc::ptr_eq(source_map, copied_map));
    assert_eq!(
        call(map_keys, std::slice::from_ref(&copied)).render(),
        "[\"first\", 1, false]"
    );

    source_map.borrow_mut()[1].1 = Value::String("changed".to_owned());
    copied_map
        .borrow_mut()
        .push((MapKey::String("last".to_owned()), Value::Int(4)));
    nested.borrow_mut().push(Value::Int(2));

    assert_eq!(
        source.render(),
        "{first=[1, 2], [1]=\"changed\", [false]=3}"
    );
    assert_eq!(
        copied.render(),
        "{first=[1, 2], [1]=\"one\", [false]=3, last=4}"
    );
}

#[test]
fn has_normalizes_numeric_keys_and_reflects_nil_as_absence() {
    let values = map(vec![
        (MapKey::Int(1), Value::String("one".to_owned())),
        (MapKey::Int(2), Value::Nil),
    ]);

    assert_eq!(
        call(map_has, &[values.clone(), Value::Float(1.0)]).render(),
        "true"
    );
    assert_eq!(
        call(map_has, &[values.clone(), Value::Int(2)]).render(),
        "false"
    );
    assert_eq!(call(map_has, &[values, Value::Int(3)]).render(), "false");
}

#[test]
fn clear_mutates_aliases_and_returns_nil() {
    let values = map(vec![(MapKey::String("value".to_owned()), Value::Int(1))]);
    let alias = values.clone();

    assert!(matches!(
        call(map_clear, std::slice::from_ref(&values)),
        Value::Nil
    ));
    assert_eq!(alias.render(), "{}");
}

#[test]
fn invalid_arguments_are_qualified_hard_errors() {
    let values = map(Vec::new());
    let cases = [
        hard_error(map_length(&[], Span::new(0, 1))),
        hard_error(map_copy(&[Value::Nil], Span::new(0, 1))),
        hard_error(map_has(&[values.clone(), Value::Nil], Span::new(0, 1))),
        hard_error(map_keys(&[Value::Nil], Span::new(0, 1))),
        hard_error(map_values(&[Value::Nil], Span::new(0, 1))),
        hard_error(map_entries(&[Value::Nil], Span::new(0, 1))),
        hard_error(map_clear(&[Value::Nil], Span::new(0, 1))),
    ];

    for (error, name) in cases.iter().zip([
        "std/map.length",
        "std/map.copy",
        "std/map.has",
        "std/map.keys",
        "std/map.values",
        "std/map.entries",
        "std/map.clear",
    ]) {
        assert!(
            error.message.contains(name),
            "{} should contain {name}",
            error.message
        );
    }
}

#[test]
fn active_borrows_return_qualified_errors_instead_of_panicking() {
    let values = map(Vec::new());
    let Value::Map(shared) = &values else {
        unreachable!()
    };

    let mutable = shared.borrow_mut();
    let errors = [
        hard_error(map_length(std::slice::from_ref(&values), Span::new(0, 1))),
        hard_error(map_copy(std::slice::from_ref(&values), Span::new(0, 1))),
        hard_error(map_has(
            &[values.clone(), Value::String("key".to_owned())],
            Span::new(0, 1),
        )),
        hard_error(map_keys(std::slice::from_ref(&values), Span::new(0, 1))),
        hard_error(map_values(std::slice::from_ref(&values), Span::new(0, 1))),
        hard_error(map_entries(std::slice::from_ref(&values), Span::new(0, 1))),
    ];
    for (error, name) in errors.iter().zip([
        "std/map.length",
        "std/map.copy",
        "std/map.has",
        "std/map.keys",
        "std/map.values",
        "std/map.entries",
    ]) {
        assert!(
            error
                .message
                .contains(&format!("{name} could not borrow map")),
            "{} should contain qualified borrow diagnostic for {name}",
            error.message
        );
    }
    drop(mutable);

    let immutable = shared.borrow();
    let error = hard_error(map_clear(std::slice::from_ref(&values), Span::new(0, 1)));
    assert!(error.message.contains("std/map.clear could not borrow map"));
    drop(immutable);
}
