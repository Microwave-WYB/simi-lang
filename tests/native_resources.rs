use simi::span::Span;
use simi::{Engine, Module, NativeResource, NativeResult, Value};

#[derive(Debug, PartialEq, Eq)]
struct Counter(u32);

fn resource_counter(args: &[Value], _: Span) -> NativeResult {
    let Value::NativeResource(resource) = &args[0] else {
        panic!("test callback should receive a resource");
    };
    Ok(Ok(Value::Int(
        resource
            .downcast_ref::<Counter>()
            .expect("resource payload should have its original type")
            .0
            .into(),
    )))
}

#[test]
fn resources_have_opaque_identity_rendering_and_native_downcasts() {
    let resource = NativeResource::new("example.counter", Counter(7));
    let alias = resource.clone();
    let unrelated = NativeResource::new("example.counter", Counter(7));

    assert!(resource.ptr_eq(&alias));
    assert!(!resource.ptr_eq(&unrelated));
    assert_eq!(resource.type_label(), "example.counter");
    assert_eq!(resource.downcast_ref::<Counter>(), Some(&Counter(7)));
    assert_eq!(resource.downcast_ref::<String>(), None);
    assert_eq!(
        Value::NativeResource(resource).render(),
        "<resource example.counter>"
    );
}

#[test]
fn resources_flow_through_scripts_modules_closures_and_raises() {
    let resource = NativeResource::new("example.counter", Counter(7));
    let engine = Engine::builder()
        .module(
            Module::builder("resource")
                .value("value", Value::NativeResource(resource.clone()))
                .function("counter", 1, resource_counter)
                .build(),
        )
        .module(
            Module::source("facade", "host")
                .host(Value::NativeResource(resource.clone()))
                .build(),
        )
        .build();

    let value = engine
        .eval(
            r#"
            let resource = require("resource")
            let captured = resource.value
            let get = fn()
                captured
            let facade = require("facade")
            [
                get(),
                {value = captured}.value,
                facade,
                type(captured),
                inspect(captured),
                resource.counter(captured),
            ]
            "#,
        )
        .unwrap()
        .unwrap();
    let Value::List(values) = value else {
        panic!("script should return a list");
    };
    let values = values.borrow();
    for index in 0..3 {
        let Value::NativeResource(value) = values.get_cloned(index).unwrap() else {
            panic!("script should preserve an opaque resource");
        };
        assert!(value.ptr_eq(&resource));
    }
    assert_eq!(values.get_cloned(3).unwrap().render(), "\"resource\"");
    assert_eq!(
        values.get_cloned(4).unwrap().render(),
        "\"<resource example.counter>\""
    );
    assert_eq!(values.get_cloned(5).unwrap().render(), "7");

    let raised = engine
        .eval("let resource = require(\"resource\") raise resource.value")
        .unwrap();
    let raised = match raised {
        Err(raised) => raised,
        Ok(value) => panic!("expected resource raise, got {}", value.render()),
    };
    let Value::NativeResource(value) = raised.value else {
        panic!("script should raise the resource unchanged");
    };
    assert!(value.ptr_eq(&resource));
}

#[test]
fn resources_remain_engine_local_when_hosts_supply_distinct_values() {
    let first_resource = NativeResource::new("example.counter", Counter(1));
    let second_resource = NativeResource::new("example.counter", Counter(2));
    let first = Engine::builder()
        .module(
            Module::builder("resource")
                .value("value", Value::NativeResource(first_resource.clone()))
                .build(),
        )
        .build();
    let second = Engine::builder()
        .module(
            Module::builder("resource")
                .value("value", Value::NativeResource(second_resource.clone()))
                .build(),
        )
        .build();

    let load = "require(\"resource\").value";
    let Value::NativeResource(first_value) = first.eval(load).unwrap().unwrap() else {
        panic!("first engine should return its resource");
    };
    let Value::NativeResource(second_value) = second.eval(load).unwrap().unwrap() else {
        panic!("second engine should return its resource");
    };
    assert!(first_value.ptr_eq(&first_resource));
    assert!(second_value.ptr_eq(&second_resource));
    assert!(!first_value.ptr_eq(&second_value));
}

#[test]
fn resources_reject_script_operations_without_a_resource_protocol() {
    let resource = NativeResource::new("example.counter", Counter(7));
    let engine = Engine::builder()
        .module(
            Module::builder("resource")
                .value("value", Value::NativeResource(resource))
                .build(),
        )
        .build();

    for (source, expected) in [
        (
            "let value = require(\"resource\").value value()",
            "cannot call value of type resource",
        ),
        (
            "let value = require(\"resource\").value value[0]",
            "indexing requires a list, map, or bytes, got resource",
        ),
        (
            "let value = require(\"resource\").value value.field = 1",
            "assignment target must be a mutable list or map, got resource",
        ),
        (
            "let value = require(\"resource\").value value == value",
            "equality is not supported for resource and resource",
        ),
        (
            "let value = require(\"resource\").value {[value] = 1}",
            "map key must be a string, integer, float, or boolean, got resource",
        ),
    ] {
        let result = engine.eval(source);
        let error = match result {
            Err(error) => error,
            Ok(Ok(value)) => panic!("expected a hard diagnostic, got {}", value.render()),
            Ok(Err(raised)) => panic!(
                "expected a hard diagnostic, got raise {}",
                raised.value.render()
            ),
        };
        assert!(error.to_string().contains(expected), "{source}: {error}");
    }
}
