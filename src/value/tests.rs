use std::error::Error;

use gc::{Gc, GcCell};

use super::*;

#[test]
fn cloned_lists_share_mutations() {
    let shared = List::shared(vec![Value::Int(1)]);
    let original = Value::List(shared.clone());
    let alias = original.clone();

    shared.borrow_mut().push(Value::Int(2));

    assert_eq!(original.render(), "[1, 2]");
    assert_eq!(alias.render(), "[1, 2]");
    let Value::List(alias_values) = alias else {
        panic!("list clone changed its value kind");
    };
    assert!(Gc::ptr_eq(&shared, &alias_values));
}

#[test]
fn maps_render_in_insertion_order_with_explicit_computed_keys() {
    let value = Value::Map(Gc::new(GcCell::new(vec![
        (MapKey::String("second".to_owned()), Value::Int(2)),
        (MapKey::Int(10), Value::String("ten".to_owned())),
        (MapKey::Bool(true), Value::Int(1)),
    ])));

    assert_eq!(value.render(), "{second=2, [10]=\"ten\", [true]=1}");
}

#[test]
fn cloned_maps_share_identity() {
    let shared = Gc::new(GcCell::new(vec![(
        MapKey::String("value".to_owned()),
        Value::Int(1),
    )]));
    let original = Value::Map(shared.clone());
    let alias = original.clone();
    shared.borrow_mut()[0].1 = Value::Int(2);

    assert_eq!(original.render(), "{value=2}");
    assert_eq!(alias.render(), "{value=2}");
}

#[test]
fn rendering_marks_cycles_without_collapsing_repeated_aliases() {
    let cyclic_list = List::shared(Vec::new());
    cyclic_list
        .borrow_mut()
        .push(Value::List(cyclic_list.clone()));
    assert_eq!(Value::List(cyclic_list).render(), "[<cycle>]");

    let cyclic_map = Gc::new(GcCell::new(Vec::new()));
    cyclic_map.borrow_mut().push((
        MapKey::String("self".to_owned()),
        Value::Map(cyclic_map.clone()),
    ));
    assert_eq!(Value::Map(cyclic_map).render(), "{self=<cycle>}");

    let shared = List::shared(vec![Value::Int(1)]);
    let repeated = Value::List(List::shared(vec![
        Value::List(shared.clone()),
        Value::List(shared),
    ]));
    assert_eq!(repeated.render(), "[[1], [1]]");
}

#[test]
fn strings_render_with_simi_escapes() {
    let value = Value::String("quote: \" slash: \\ lines:\n\r\ttail".to_owned());

    assert_eq!(
        value.render(),
        "\"quote: \\\" slash: \\\\ lines:\\n\\r\\ttail\""
    );
}

#[test]
fn primitive_values_have_deterministic_rendering() {
    assert_eq!(Value::Nil.render(), "nil");
    assert_eq!(Value::Bool(true).render(), "true");
    assert_eq!(Value::Bool(false).render(), "false");
    assert_eq!(Value::Int(-12).render(), "-12");
}

#[test]
fn raised_display_and_debug_render_the_original_value() {
    let raised = Raised {
        value: Value::String("missing".to_owned()),
        origin: Span::new(3, 18),
        frames: vec![TraceFrame {
            function: "lookup".to_owned(),
            call_span: Span::new(20, 28),
        }],
        cause: None,
    };

    assert_eq!(raised.to_string(), "raised \"missing\"");
    assert_eq!(
        format!("{raised:?}"),
        "Raised { value: \"missing\", origin: Span { start: 3, end: 18 }, frames: [TraceFrame { function: \"lookup\", call_span: Span { start: 20, end: 28 } }], cause: None }"
    );
}

#[test]
fn appending_causes_preserves_order_and_error_sources() {
    let oldest = Raised::new(Value::String("oldest".to_owned()), Span::new(1, 2));
    let middle = Raised::new(Value::String("middle".to_owned()), Span::new(3, 4));
    let mut newest = Raised::new(Value::String("newest".to_owned()), Span::new(5, 6));

    newest.append_cause(middle);
    newest.append_cause(oldest);

    let middle = newest.cause.as_deref().expect("middle cause");
    assert_eq!(middle.value.render(), "\"middle\"");
    let oldest = middle.cause.as_deref().expect("oldest cause");
    assert_eq!(oldest.value.render(), "\"oldest\"");
    assert!(oldest.cause.is_none());
    assert_eq!(
        Error::source(&newest).map(ToString::to_string),
        Some("raised \"middle\"".to_owned())
    );
}

#[test]
fn raised_values_preserve_shared_identity() {
    let shared = List::shared(vec![Value::Int(1)]);
    let raised = Raised::new(Value::List(shared.clone()), Span::new(0, 7));

    let Value::List(raised_values) = &raised.value else {
        panic!("raised value changed kind");
    };
    assert!(Gc::ptr_eq(&shared, raised_values));
}

#[test]
fn floats_render_distinctly_and_float_keys_are_safe() {
    assert_eq!(Value::Float(2.0).render(), "2.0");
    assert_eq!(Value::Float(-0.0).render(), "-0.0");
    assert_eq!(Value::Float(1.25).render(), "1.25");

    let span = Span::new(0, 1);
    assert_eq!(
        MapKey::from_value(Value::Float(1.0), span).unwrap(),
        MapKey::Int(1)
    );
    assert_eq!(
        MapKey::from_value(Value::Float(-0.0), span).unwrap(),
        MapKey::Int(0)
    );
    let key = MapKey::from_value(Value::Float(1.5), span).unwrap();
    let MapKey::Float(key) = key else {
        panic!("non-integral float key should remain float");
    };
    assert_eq!(key.value(), 1.5);
    assert!(FloatKey::new(f64::NAN).is_none());
    assert!(FloatKey::new(f64::INFINITY).is_none());
}

#[test]
fn division_by_zero_raise_has_exact_value_and_origin() {
    let origin = Span::new(2, 7);
    let raised = Raised::division_by_zero(origin);
    assert_eq!(raised.origin, origin);
    assert!(raised.frames.is_empty());
    assert!(raised.cause.is_none());
    assert_eq!(raised.value.render(), "{error=\"division_by_zero\"}");
}

#[test]
fn structural_bounds_raise_has_exact_ordered_fields_and_origin() {
    let origin = Span::new(4, 9);
    let raised = Raised::index_out_of_bounds(7, 3, origin).unwrap();
    assert_eq!(raised.origin, origin);
    assert!(raised.frames.is_empty());
    assert!(raised.cause.is_none());
    let Value::Map(entries) = raised.value else {
        panic!("bounds error should be a map");
    };
    let entries = entries.borrow();
    assert_eq!(entries.len(), 3);
    assert!(
        matches!(&entries[0], (MapKey::String(key), Value::String(value)) if key == "error" && value == "index_out_of_bounds")
    );
    assert!(matches!(&entries[1], (MapKey::String(key), Value::Int(7)) if key == "index"));
    assert!(matches!(&entries[2], (MapKey::String(key), Value::Int(3)) if key == "length"));
}
