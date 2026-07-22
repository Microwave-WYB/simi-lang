use gc::{Gc, GcCell};

use super::*;
use crate::runtime::List;
use crate::span::Span;

fn list(values: Vec<Value>) -> Value {
    Value::List(List::shared(values))
}

#[test]
fn list_operations_mutate_aliases() {
    let values = list(vec![Value::Int(1)]);
    let alias = values.clone();
    list_append(&[values.clone(), Value::Int(2)], Span::new(0, 1))
        .unwrap()
        .unwrap();
    list_set(
        &[values.clone(), Value::Int(0), Value::Int(3)],
        Span::new(0, 1),
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        list_length(std::slice::from_ref(&values), Span::new(0, 1))
            .unwrap()
            .unwrap()
            .render(),
        "2"
    );
    assert_eq!(
        list_get(&[values, Value::Int(1)], Span::new(0, 1))
            .unwrap()
            .unwrap()
            .render(),
        "2"
    );
    assert_eq!(alias.render(), "[3, 2]");
}

#[test]
fn copy_creates_an_independent_shallow_cow_list() {
    let nested = list(vec![Value::Int(1)]);
    let source = list(vec![nested.clone(), Value::Int(2)]);
    let copied = list_copy(std::slice::from_ref(&source), Span::new(0, 1))
        .unwrap()
        .unwrap();

    let (Value::List(source_list), Value::List(copied_list)) = (&source, &copied) else {
        panic!("copy should return a list")
    };
    assert!(!Gc::ptr_eq(source_list, copied_list));

    list_set(
        &[source.clone(), Value::Int(1), Value::Int(3)],
        Span::new(0, 1),
    )
    .unwrap()
    .unwrap();
    list_append(&[nested, Value::Int(4)], Span::new(0, 1))
        .unwrap()
        .unwrap();

    assert_eq!(source.render(), "[[1, 4], 3]");
    assert_eq!(copied.render(), "[[1, 4], 2]");
}

#[test]
fn extending_a_list_with_itself_uses_a_snapshot() {
    let values = list(vec![Value::Int(1), Value::Int(2)]);
    list_extend(&[values.clone(), values.clone()], Span::new(0, 1))
        .unwrap()
        .unwrap();
    assert_eq!(values.render(), "[1, 2, 1, 2]");
}

#[test]
fn extending_a_suffix_with_itself_snapshots_only_its_visible_range() {
    let source = List::shared(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    let suffix = Gc::new(GcCell::new(source.borrow().suffix(1)));
    let suffix_value = Value::List(suffix.clone());

    list_extend(
        &[suffix_value.clone(), suffix_value.clone()],
        Span::new(0, 1),
    )
    .unwrap()
    .unwrap();

    assert_eq!(Value::List(source).render(), "[1, 2, 3]");
    assert_eq!(suffix_value.render(), "[2, 3, 2, 3]");
}

#[test]
fn invalid_arity_type_and_indices_are_errors() {
    let values = list(vec![Value::Int(1)]);
    assert!(list_length(&[], Span::new(0, 1)).is_err());
    assert!(list_copy(&[], Span::new(0, 1)).is_err());
    assert!(list_copy(&[Value::Nil], Span::new(0, 1)).is_err());
    assert!(list_append(&[Value::Nil, Value::Nil], Span::new(0, 1)).is_err());
    assert!(
        list_get(
            &[values.clone(), Value::String("0".to_owned())],
            Span::new(0, 1)
        )
        .is_err()
    );
    assert!(list_get(&[values.clone(), Value::Int(-1)], Span::new(0, 1)).is_err());
    assert!(matches!(
        list_get(&[values.clone(), Value::Int(1)], Span::new(0, 1)),
        Ok(Ok(Value::Nil))
    ));
    let raised = match list_set(&[values, Value::Int(1), Value::Nil], Span::new(0, 1)).unwrap() {
        Err(raised) => raised,
        Ok(value) => panic!("expected bounds raise, got {}", value.render()),
    };
    assert_eq!(
        raised.value.render(),
        "{error=\"index_out_of_bounds\", index=1, length=1}"
    );
}

#[test]
fn insert_remove_pop_and_reverse_mutate_aliases() {
    let values = list(vec![Value::Int(1), Value::Int(3)]);
    let alias = values.clone();

    assert!(matches!(
        list_insert(
            &[values.clone(), Value::Int(1), Value::Int(2)],
            Span::new(0, 1)
        )
        .unwrap()
        .unwrap(),
        Value::Nil
    ));
    assert_eq!(
        list_remove(&[values.clone(), Value::Int(0)], Span::new(0, 1))
            .unwrap()
            .unwrap()
            .render(),
        "1"
    );
    assert_eq!(
        list_pop(std::slice::from_ref(&values), Span::new(0, 1))
            .unwrap()
            .unwrap()
            .render(),
        "3"
    );
    list_reverse(std::slice::from_ref(&values), Span::new(0, 1))
        .unwrap()
        .unwrap();

    assert_eq!(alias.render(), "[2]");
}

#[test]
fn insertion_removal_and_empty_pop_raise_exact_bounds_values() {
    let values = list(vec![Value::Int(1)]);
    for (result, expected) in [
        (
            list_insert(
                &[values.clone(), Value::Int(2), Value::Nil],
                Span::new(0, 1),
            ),
            "{error=\"index_out_of_bounds\", index=2, length=1}",
        ),
        (
            list_remove(&[values.clone(), Value::Int(1)], Span::new(0, 1)),
            "{error=\"index_out_of_bounds\", index=1, length=1}",
        ),
        (
            list_pop(&[list(vec![])], Span::new(0, 1)),
            "{error=\"index_out_of_bounds\", index=0, length=0}",
        ),
    ] {
        let raised = match result.unwrap() {
            Err(raised) => raised,
            Ok(value) => panic!(
                "operation should raise bounds value, got {}",
                value.render()
            ),
        };
        assert_eq!(raised.value.render(), expected);
    }

    list_insert(
        &[values.clone(), Value::Int(1), Value::Int(2)],
        Span::new(0, 1),
    )
    .unwrap()
    .unwrap();
    assert_eq!(values.render(), "[1, 2]");
}

#[test]
fn slices_are_independent_cow_views_with_shallow_aliases() {
    let nested = list(vec![Value::Int(1)]);
    let source = list(vec![Value::Int(0), nested.clone(), Value::Int(2)]);
    let slice = list_slice(
        &[source.clone(), Value::Int(1), Value::Int(10)],
        Span::new(0, 1),
    )
    .unwrap()
    .unwrap();
    assert_eq!(slice.render(), "[[1], 2]");

    list_set(
        &[source.clone(), Value::Int(1), Value::Int(9)],
        Span::new(0, 1),
    )
    .unwrap()
    .unwrap();
    list_append(&[nested, Value::Int(3)], Span::new(0, 1))
        .unwrap()
        .unwrap();
    assert_eq!(source.render(), "[0, 9, 2]");
    assert_eq!(slice.render(), "[[1, 3], 2]");

    for (start, end) in [(3, 1), (10, 20)] {
        let empty = list_slice(
            &[source.clone(), Value::Int(start), Value::Int(end)],
            Span::new(0, 1),
        )
        .unwrap()
        .unwrap();
        assert_eq!(empty.render(), "[]");
    }
}

#[test]
fn contains_uses_language_numeric_equality_and_rejects_container_equality() {
    let values = list(vec![Value::Int(1), Value::String("one".to_owned())]);
    assert!(matches!(
        list_contains(&[values.clone(), Value::Float(1.0)], Span::new(0, 1))
            .unwrap()
            .unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        list_contains(
            &[values, Value::String("missing".to_owned())],
            Span::new(0, 1)
        )
        .unwrap()
        .unwrap(),
        Value::Bool(false)
    ));

    let cyclic = List::shared(Vec::new());
    cyclic.borrow_mut().push(Value::List(cyclic.clone()));
    let cyclic = Value::List(cyclic);
    let error = match list_contains(&[cyclic.clone(), cyclic], Span::new(0, 1)) {
        Err(error) => error,
        Ok(_) => panic!("container equality should remain a hard diagnostic"),
    };
    assert!(
        error
            .message
            .contains("equality is not supported for list and list")
    );
}

#[test]
fn active_borrows_return_errors_instead_of_panicking() {
    let values = list(vec![]);
    let Value::List(shared) = &values else {
        unreachable!()
    };
    let mutable = shared.borrow_mut();
    assert!(list_copy(std::slice::from_ref(&values), Span::new(0, 1)).is_err());
    drop(mutable);

    let _borrow = shared.borrow();
    assert!(list_append(&[values.clone(), Value::Nil], Span::new(0, 1)).is_err());
    assert!(list_reverse(std::slice::from_ref(&values), Span::new(0, 1)).is_err());
}
