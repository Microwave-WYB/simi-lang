use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use simi::runtime::RuntimeError;
use simi::span::Span;
use simi::{Engine, Module, NativeResult, SimiError, Value, eval};

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
            try require("missing")
                catch {error="module_not_found", module=module, ..} do module
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
        .eval("require(\"std/list\")")
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
        "{error=\"module_not_found\", module=\"std/list\"}"
    );

    let value = eval("let list = require(\"std/list\") list.length([1, 2, 3])")
        .expect("root eval should provide standard modules")
        .expect("standard list call should not raise");
    assert_eq!(value.render(), "3");

    for legacy_name in ["list", "map", "string"] {
        let result = Engine::with_stdlib()
            .eval(&format!("require(\"{legacy_name}\")"))
            .expect("legacy module names should raise rather than hard fail");
        assert!(
            result.is_err(),
            "legacy module `{legacy_name}` must be absent"
        );
    }

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
fn global_type_reports_every_runtime_value_category() {
    let value = eval(
        r#"
        fn sample() do nil end
        [
            type(1),
            type(1.5),
            type("text"),
            type(true),
            type(nil),
            type([]),
            type({}),
            type(sample),
            type(type),
        ]
        "#,
    )
    .expect("core type calls should have no hard diagnostic")
    .expect("core type calls should not raise");

    assert_eq!(
        value.render(),
        "[\"integer\", \"float\", \"string\", \"boolean\", \"nil\", \"list\", \"map\", \"function\", \"function\"]"
    );
}

#[test]
fn global_inspect_renders_cyclic_containers_and_builtins_are_shadowable() {
    let value = eval(
        r#"
        let list = require("std/list")
        let values = []
        list.append(values, values)
        let object = {}
        object.self = object
        [inspect(values), inspect(object)]
        "#,
    )
    .expect("core inspect calls should have no hard diagnostic")
    .expect("core inspect calls should not raise");

    assert_eq!(value.render(), "[\"[<cycle>]\", \"{self=<cycle>}\"]");

    let value = Engine::new()
        .eval("[type(1), inspect(1)]")
        .unwrap()
        .unwrap();
    assert_eq!(value.render(), "[\"integer\", \"1\"]");

    let value = Engine::new()
        .eval("let type = 41 let inspect = 42 [type, inspect]")
        .unwrap()
        .unwrap();
    assert_eq!(value.render(), "[41, 42]");

    let missing = Engine::with_stdlib().eval("require(\"core\")").unwrap();
    assert!(missing.is_err());
}

#[test]
fn stdio_module_is_an_opt_in_capability() {
    let result = Engine::with_stdlib()
        .eval("require(\"std/io\")")
        .expect("missing stdio module should raise, not hard fail");
    assert!(result.is_err());

    let value = Engine::builder()
        .stdlib()
        .stdio()
        .build()
        .eval(
            r#"
            let io = require("std/io")
            [
                type(io.read_line),
                type(io.print),
                type(io.println),
                type(io.eprint),
                type(io.eprintln),
                io.read,
                io.write,
                io.flush,
            ]
            "#,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        value.render(),
        "[\"function\", \"function\", \"function\", \"function\", \"function\", nil, nil, nil]"
    );
}

#[test]
fn root_eval_uses_fresh_standard_module_instances() {
    eval(
        r#"
        let list = require("std/list")
        list.marker = 1
        nil
        "#,
    )
    .expect("first root evaluation should have no hard diagnostic")
    .expect("first root evaluation should not raise");

    let value = eval("let list = require(\"std/list\") list.marker")
        .expect("second root evaluation should have no hard diagnostic")
        .expect("second root evaluation should not raise");
    assert_eq!(value.render(), "nil");
}

#[test]
fn source_modules_cache_exports_and_capture_private_host_dispatch() {
    let calls = Arc::new(AtomicUsize::new(0));
    let module = Module::source(
        "source",
        r#"
        fn next() do host.call("com.example/next") end
        { next = next }
        "#,
    )
    .host_function("com.example/next", 0, {
        let calls = calls.clone();
        move |_, _| {
            let value = calls.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Ok(Value::Int(i64::try_from(value).unwrap())))
        }
    })
    .build();
    assert!(module.is_source_backed());
    let engine = Engine::builder().module(module).build();
    assert_eq!(
        engine
            .eval("let source = require(\"source\") [source.next(), source.next()]")
            .unwrap()
            .unwrap()
            .render(),
        "[1, 2]"
    );
    assert!(
        Engine::new()
            .eval("host.call(\"com.example/next\")")
            .is_err()
    );
}

#[test]
fn source_modules_raise_for_missing_hosts_and_cycles() {
    let missing = Module::source(
        "missing-host",
        r#"
        fn call() do host.call("com.example/missing") end
        { call = call }
        "#,
    )
    .build();
    let engine = Engine::builder()
        .module(missing)
        .module(Module::source("left", "require(\"right\")").build())
        .module(Module::source("right", "require(\"left\")").build())
        .build();
    let caught = engine
        .eval(
            r#"
            let module = require("missing-host")
            try module.call()
            catch error do error
            end
            "#,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        caught.render(),
        "{error=\"host_function_not_found\", function=\"com.example/missing\"}"
    );
    let cycle = match engine.eval("require(\"left\")").unwrap() {
        Err(raised) => raised,
        Ok(value) => panic!("expected cycle raise, got {}", value.render()),
    };
    assert_eq!(
        cycle.value.render(),
        "{error=\"circular_module_dependency\", module=\"left\"}"
    );
    assert_eq!(cycle.origin, Span::new(0, "require(\"left\")".len()));
}

#[test]
fn source_modules_cache_nil_and_validate_host_contracts() {
    let calls = Arc::new(AtomicUsize::new(0));
    let nil_module = Module::source("nil-module", "host.call(\"com.example/load\") nil")
        .host_function("com.example/load", 0, {
            let calls = calls.clone();
            move |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Ok(Value::Nil))
            }
        })
        .build();
    let bad_type = Module::source("bad-type", "host.call(1)").build();
    let wrong_arity = Module::source("wrong-arity", "host.call(\"com.example/one\")")
        .host_function("com.example/one", 1, |_, _| Ok(Ok(Value::Nil)))
        .build();
    let engine = Engine::builder()
        .module(nil_module)
        .module(bad_type)
        .module(wrong_arity)
        .build();
    engine
        .eval("[require(\"nil-module\"), require(\"nil-module\")]")
        .unwrap()
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let bad_type = match engine.eval("require(\"bad-type\")") {
        Err(error) => error,
        Ok(_) => panic!("expected host ID type diagnostic"),
    };
    assert!(bad_type.to_string().contains("string function ID"));
    let wrong_arity = match engine.eval("require(\"wrong-arity\")") {
        Err(error) => error,
        Ok(_) => panic!("expected host arity diagnostic"),
    };
    assert!(
        wrong_arity
            .to_string()
            .contains("expects 1 arguments, got 0")
    );
}

#[test]
fn source_module_nested_loads_retry_failures_and_isolate_cached_mutation() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let flaky = Module::source("flaky", "host.call(\"com.example/flaky\") { value = 1 }")
        .host_function("com.example/flaky", 0, {
            let attempts = attempts.clone();
            move |_, span| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(RuntimeError::new(span, "temporary load failure"))
                } else {
                    Ok(Ok(Value::Nil))
                }
            }
        })
        .build();
    let engine = Engine::builder()
        .module(flaky)
        .module(Module::source("outer", "require(\"flaky\")").build())
        .build();
    assert!(engine.eval("require(\"outer\")").is_err());
    let loaded = engine
        .eval("let value = require(\"outer\") value.value = 9 value")
        .unwrap()
        .unwrap();
    assert_eq!(loaded.render(), "{value=9}");
    assert_eq!(
        engine
            .eval("require(\"outer\").value")
            .unwrap()
            .unwrap()
            .render(),
        "9"
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    let separate = Engine::builder()
        .module(Module::source("flaky", "{ value = 1 }").build())
        .module(Module::source("outer", "require(\"flaky\")").build())
        .build();
    assert_eq!(
        separate
            .eval("require(\"outer\").value")
            .unwrap()
            .unwrap()
            .render(),
        "1"
    );
}

#[test]
fn every_bundled_module_is_source_backed() {
    for module in [
        simi::stdlib::list(),
        simi::stdlib::map(),
        simi::stdlib::iter(),
        simi::stdlib::number(),
        simi::stdlib::string(),
        simi::stdlib::io(),
    ] {
        assert!(
            module.is_source_backed(),
            "{} should use a Simi facade",
            module.name()
        );
    }
    let engine = Engine::builder().stdlib().stdio().build();
    let mut names = engine
        .module_sources()
        .into_iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        [
            "std/io",
            "std/iter",
            "std/list",
            "std/map",
            "std/number",
            "std/string",
        ]
    );
}

#[test]
fn source_module_failures_are_attributed_to_the_public_boundary() {
    let engine = Engine::builder()
        .module(
            Module::source(
                "failing",
                r#"
                fn raised() do raise "module raise" end
                fn hard() do nil + 1 end
                { raised = raised, hard = hard }
                "#,
            )
            .build(),
        )
        .build();

    let raised_source = "let failing = require(\"failing\")\nfailing.raised()";
    let raised = match engine.eval(raised_source).unwrap() {
        Err(raised) => raised,
        Ok(value) => panic!("expected module raise, got {}", value.render()),
    };
    assert_eq!(
        raised.origin.start,
        raised_source.find("failing.raised").unwrap()
    );
    assert_eq!(raised.frames[0].function, "failing.raised");

    let hard_source = "let failing = require(\"failing\")\nfailing.hard()";
    let hard = match engine.eval(hard_source) {
        Err(error) => error,
        Ok(_) => panic!("expected module hard diagnostic"),
    };
    assert_eq!(hard.span().start, hard_source.find("failing.hard").unwrap());
    assert!(hard.to_string().contains("module `failing`"));
}
