use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use simiscript::span::Span;
use simiscript::{Engine, Module, NativeResult, SimiError, Value, eval};

fn constant(_: &[Value], _: Span) -> NativeResult {
    Ok(Ok(Value::Int(7)))
}

fn counter_module(name: &str, count: Arc<AtomicUsize>) -> Module {
    Module::builder(name)
        .function("next", 0, move |_, _| {
            let next = count.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Ok(Value::Int(
                i64::try_from(next).expect("test counter should fit in i64"),
            )))
        })
        .build()
}

#[test]
fn third_party_modules_support_values_functions_and_captured_state() {
    let count = Arc::new(AtomicUsize::new(0));
    let module = Module::builder("example")
        .value("version", Value::String("1.0".to_owned()))
        .function("constant", 0, constant)
        .function("next", 0, {
            let count = count.clone();
            move |_, _| {
                let next = count.fetch_add(1, Ordering::SeqCst) + 1;
                Ok(Ok(Value::Int(
                    i64::try_from(next).expect("test counter should fit in i64"),
                )))
            }
        })
        .build();
    assert_eq!(module.name(), "example");

    let engine = Engine::builder().module(module).build();
    let value = engine
        .eval(
            r#"
            let example = require("example")
            [example.version, example.constant(), example.next(), example.next()]
            "#,
        )
        .expect("module program should have no hard diagnostic")
        .expect("module program should not raise");

    assert_eq!(value.render(), "[\"1.0\", 7, 1, 2]");
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[test]
fn modules_are_cached_and_mutable_across_engine_evaluations() {
    let engine = Engine::builder()
        .module(
            Module::builder("state")
                .value("value", Value::Int(1))
                .build(),
        )
        .build();

    engine
        .eval(
            r#"
            let first = require("state")
            let second = require("state")
            first.value = 9
            second.extra = 10
            nil
            "#,
        )
        .expect("first evaluation should have no hard diagnostic")
        .expect("first evaluation should not raise");

    let value = engine
        .eval("let state = require(\"state\") [state.value, state.extra]")
        .expect("second evaluation should have no hard diagnostic")
        .expect("second evaluation should not raise");
    assert_eq!(value.render(), "[9, 10]");
}

#[test]
fn separate_engines_have_isolated_module_values_and_callback_state() {
    let first_count = Arc::new(AtomicUsize::new(0));
    let second_count = Arc::new(AtomicUsize::new(0));
    let first = Engine::builder()
        .module(counter_module("counter", first_count.clone()))
        .build();
    let second = Engine::builder()
        .module(counter_module("counter", second_count.clone()))
        .build();

    let source = "let counter = require(\"counter\") counter.next()";
    assert_eq!(
        first.eval(source).unwrap().unwrap().render(),
        "1",
        "first engine should start its own state"
    );
    assert_eq!(first.eval(source).unwrap().unwrap().render(), "2");
    assert_eq!(second.eval(source).unwrap().unwrap().render(), "1");
    assert_eq!(first_count.load(Ordering::SeqCst), 2);
    assert_eq!(second_count.load(Ordering::SeqCst), 1);
}

#[test]
fn duplicate_registrations_are_last_wins_and_preserve_export_order() {
    let replaced = Module::builder("duplicate")
        .value("first", Value::Int(1))
        .value("second", Value::Int(2))
        .value("first", Value::Int(3))
        .value("removed", Value::Int(4))
        .value("removed", Value::Nil)
        .build();
    let final_module = Module::builder("duplicate")
        .value("winner", Value::Bool(true))
        .build();

    let exports = Engine::builder()
        .module(replaced)
        .build()
        .eval("require(\"duplicate\")")
        .unwrap()
        .unwrap();
    assert_eq!(exports.render(), "{first=3, second=2}");

    let exports = Engine::builder()
        .module(
            Module::builder("duplicate")
                .value("old", Value::Int(1))
                .build(),
        )
        .module(final_module)
        .build()
        .eval("require(\"duplicate\")")
        .unwrap()
        .unwrap();
    assert_eq!(exports.render(), "{winner=true}");
}

#[test]
fn missing_modules_raise_exact_values_at_the_call_span_and_are_catchable() {
    let engine = Engine::new();
    let source = "require(\"missing\")";
    let raised = match engine
        .eval(source)
        .expect("missing module should not be a hard diagnostic")
    {
        Err(raised) => raised,
        Ok(value) => panic!("missing module should raise, got {}", value.render()),
    };
    assert_eq!(
        raised.value.render(),
        "{error=\"module_not_found\", module=\"missing\"}"
    );
    assert_eq!(raised.origin, Span::new(0, source.len()));

    let value = engine
        .eval(
            r#"
            try require("missing") catch
                case {error="module_not_found", module=module} -> module
            end
            "#,
        )
        .expect("caught module failure should have no hard diagnostic")
        .expect("module failure should be caught");
    assert_eq!(value.render(), "\"missing\"");
}

#[test]
fn require_type_errors_and_qualified_native_arity_errors_are_hard() {
    let error = match Engine::new().eval("require(1)") {
        Err(error) => error,
        Ok(_) => panic!("non-string module name should be a hard diagnostic"),
    };
    assert!(
        error
            .to_string()
            .contains("require expects a string module name")
    );

    let engine = Engine::builder()
        .module(
            Module::builder("example")
                .function("greet", 1, constant)
                .build(),
        )
        .build();
    let error = match engine.eval("let example = require(\"example\") example.greet()") {
        Err(error) => error,
        Ok(_) => panic!("wrong native arity should be a hard diagnostic"),
    };
    assert!(
        error
            .to_string()
            .contains("native function `example.greet` expects 1 arguments, got 0")
    );
}

#[test]
fn standard_modules_are_explicit_capabilities_and_require_is_shadowable() {
    let missing = match Engine::new()
        .eval("require(\"list\")")
        .expect("empty engine missing module should be a raise")
    {
        Err(raised) => raised,
        Ok(value) => panic!(
            "empty engine should not contain list, got {}",
            value.render()
        ),
    };
    assert_eq!(
        missing.value.render(),
        "{error=\"module_not_found\", module=\"list\"}"
    );

    let value = eval("let list = require(\"list\") list.length([1, 2, 3])")
        .expect("root eval should provide standard modules")
        .expect("standard list call should not raise");
    assert_eq!(value.render(), "3");

    assert!(matches!(eval("list"), Err(SimiError::Runtime(_))));

    let value = Engine::new()
        .eval("let require = 42 require")
        .expect("top-level shadowed require should have no hard diagnostic")
        .expect("top-level shadowed require should not raise");
    assert_eq!(value.render(), "42");

    let value = Engine::new()
        .eval("fn identity(require) do require end identity(43)")
        .expect("parameter-shadowed require should have no hard diagnostic")
        .expect("parameter-shadowed require should not raise");
    assert_eq!(value.render(), "43");
}

#[test]
fn root_eval_uses_fresh_standard_module_instances() {
    eval(
        r#"
        let list = require("list")
        list.marker = 1
        nil
        "#,
    )
    .expect("first root evaluation should have no hard diagnostic")
    .expect("first root evaluation should not raise");

    let value = eval("let list = require(\"list\") list.marker")
        .expect("second root evaluation should have no hard diagnostic")
        .expect("second root evaluation should not raise");
    assert_eq!(value.render(), "nil");
}
