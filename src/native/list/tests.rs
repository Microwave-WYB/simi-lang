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
fn active_borrows_return_errors_instead_of_panicking() {
    let values = list(vec![]);
    let Value::List(shared) = &values else {
        unreachable!()
    };
    let _borrow = shared.borrow();
    assert!(list_append(&[values.clone(), Value::Nil], Span::new(0, 1)).is_err());
}
