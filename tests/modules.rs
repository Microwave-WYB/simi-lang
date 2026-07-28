use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use simi::runtime::RuntimeError;
use simi::span::Span;
use simi::{Engine, Module, NativeResult, Value, eval};

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
            do require("missing")
                catch of {error="module_not_found", module=module, ..}
                    module
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
fn portable_prelude_values_are_direct_shadowable_and_share_require_identity() {
    for engine in [Engine::new(), Engine::with_stdlib()] {
        let value = engine
            .eval(
                r#"
                iter.marker = "cached"
                number.marker = "numeric"
                [
                    list.length([1, 2, 3]),
                    map.length({}),
                    iter.to_list(list.iter([4]))[0],
                    number.to_string(5),
                    string.upper("simi"),
                    require("std/iter").marker,
                    require("std/number").marker,
                ]
                "#,
            )
            .expect("portable prelude calls should have no hard diagnostic")
            .expect("portable prelude calls should not raise");
        assert_eq!(
            value.render(),
            "[3, 0, 4, \"5\", \"SIMI\", \"cached\", \"numeric\"]"
        );

        let value = engine
            .eval("require(\"std/iter\").marker")
            .expect("cached prelude state should have no hard diagnostic")
            .expect("cached prelude state should not raise");
        assert_eq!(value.render(), "\"cached\"");
    }

    let value = eval("list.length([1])")
        .expect("root eval should provide the portable prelude")
        .expect("portable prelude call should not raise");
    assert_eq!(value.render(), "1");

    let value = Engine::new()
        .eval("let list = 1 let map = 2 let iter = 3 let number = 4 let string = 5 [list, map, iter, number, string]")
        .expect("shadowed prelude values should have no hard diagnostic")
        .expect("shadowed prelude values should not raise");
    assert_eq!(value.render(), "[1, 2, 3, 4, 5]");

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
fn standard_paths_are_requireable_and_hosts_can_override_them() {
    let value = Engine::new()
        .eval("require(\"std/list\").length([1, 2])")
        .expect("standard list path should not hard fail")
        .expect("standard list path should not raise");
    assert_eq!(value.render(), "2");

    let engine = Engine::builder()
        .stdlib()
        .module(
            Module::builder("std/list")
                .value("host_registered", Value::Int(1))
                .build(),
        )
        .module(
            Module::builder("std/map")
                .value("host_registered", Value::Int(2))
                .build(),
        )
        .build();
    let value = engine
        .eval(
            r#"
            list.marker = "shared"
            [
                list.host_registered,
                map.host_registered,
                require("std/list").marker,
            ]
            "#,
        )
        .expect("host overrides should not have hard diagnostics")
        .expect("host overrides should not raise");
    assert_eq!(value.render(), "[1, 2, \"shared\"]");
}

#[test]
fn source_portable_override_raises_remain_language_raises() {
    let engine = Engine::builder()
        .stdlib()
        .module(Module::source("std/list", "raise \"prelude failed\"").build())
        .build();
    let raised = match engine
        .eval("nil")
        .expect("source override raise should not become a hard diagnostic")
    {
        Err(raised) => raised,
        Ok(value) => panic!("source override should raise, got {}", value.render()),
    };
    assert_eq!(raised.value.render(), "\"prelude failed\"");
    assert_eq!(raised.origin, Span::new(0, 0));
}

#[test]
fn circular_source_portable_override_remains_a_language_raise() {
    let engine = Engine::builder()
        .stdlib()
        .module(Module::source("std/list", "require(\"std/list\")").build())
        .build();
    let raised = match engine
        .eval("nil")
        .expect("circular override should not become a hard diagnostic")
    {
        Err(raised) => raised,
        Ok(value) => panic!("circular override should raise, got {}", value.render()),
    };
    assert_eq!(
        raised.value.render(),
        "{error=\"circular_module_dependency\", module=\"std/list\"}"
    );
    assert_eq!(raised.origin, Span::new(0, 0));
}

#[test]
fn source_portable_overrides_use_explicit_dependencies_before_alias_installation() {
    let implicit = Engine::builder()
        .stdlib()
        .module(Module::source("std/string", "list").build())
        .build();
    let error = match implicit.eval("nil") {
        Err(error) => error,
        Ok(_) => panic!("portable aliases must not be partially visible during installation"),
    };
    assert!(
        error.to_string().contains("undefined name `list`"),
        "unexpected implicit dependency error: {error}"
    );

    let explicit = Engine::builder()
        .stdlib()
        .module(
            Module::source(
                "std/string",
                "let dependency = require(\"std/list\") {dependency = dependency}",
            )
            .build(),
        )
        .build();
    let value = explicit
        .eval(
            r#"
            list.marker = "shared"
            string.dependency.marker
            "#,
        )
        .expect("explicit portable dependency should not hard fail")
        .expect("explicit portable dependency should not raise");
    assert_eq!(value.render(), "\"shared\"");
}

#[test]
fn bare_engine_builder_does_not_install_the_portable_prelude() {
    for name in ["list", "map", "iter", "number", "string"] {
        let error = match Engine::builder().build().eval(name) {
            Err(error) => error,
            Ok(_) => panic!("bare builder should leave `{name}` undefined"),
        };
        assert!(
            error.to_string().contains("undefined name"),
            "unexpected error for `{name}`: {error}"
        );
    }

    let missing = Engine::builder()
        .build()
        .eval("require(\"std/iter\")")
        .expect("missing module should raise rather than hard fail");
    assert!(missing.is_err());
}

#[test]
fn remaining_standard_modules_are_requireable() {
    let value = Engine::with_stdlib()
        .eval(
            r#"
            [
                type(require("std/iter")),
                type(require("std/number")),
                type(require("std/string")),
            ]
            "#,
        )
        .expect("remaining standard modules should not have hard diagnostics")
        .expect("remaining standard modules should not raise");
    assert_eq!(value.render(), "[\"map\", \"map\", \"map\"]");
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
    for engine in [Engine::new(), Engine::with_stdlib()] {
        let result = engine
            .eval("require(\"std/io\")")
            .expect("missing stdio module should raise, not hard fail");
        assert!(result.is_err());
    }

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

        list.marker = 1
        nil
        "#,
    )
    .expect("first root evaluation should have no hard diagnostic")
    .expect("first root evaluation should not raise");

    let value = eval(" list.marker")
        .expect("second root evaluation should have no hard diagnostic")
        .expect("second root evaluation should not raise");
    assert_eq!(value.render(), "nil");
}

#[test]
fn source_facades_evaluate_with_generated_functions_and_data_in_private_host() {
    let host = simi::host_value! {
        name: "generated",
        functions: {
            "add" => (2, |arguments: &[Value], _| {
                let [Value::Int(left), Value::Int(right)] = arguments else {
                    panic!("generated host function should receive two integers")
                };
                Ok(Ok(Value::Int(left + right)))
            }),
        },
        values: {
            "answer" => Value::Int(21),
        },
    };
    let module = Module::source(
        "generated",
        r#"
            let add: (integer, integer) -> integer = host.add
            let answer: integer = host.answer
            fn doubled_answer() do add(answer, answer) end
            {add = add, answer = answer, doubled_answer = doubled_answer}
        "#,
    )
    .host(host)
    .build();
    let engine = Engine::builder().module(module).build();
    let result = engine
        .eval(
            r#"
            let generated = require("generated")
            [generated.add(20, 22), generated.answer, generated.doubled_answer()]
            "#,
        )
        .unwrap()
        .unwrap();
    assert_eq!(result.render(), "[42, 21, 42]");
}

#[test]
fn requiring_a_facade_does_not_inspect_or_borrow_its_private_host() {
    let host = simi::host_value! {
        name: "borrowed-host",
        values: {
            "marker" => Value::Int(1),
        },
    };
    let Value::Map(entries) = host.clone() else {
        panic!("host_value should construct a map")
    };
    let _borrow = entries.borrow_mut();
    let engine = Engine::builder()
        .module(Module::source("borrowed-host", "42").host(host).build())
        .build();
    assert_eq!(
        engine
            .eval("require(\"borrowed-host\")")
            .unwrap()
            .unwrap()
            .render(),
        "42"
    );
}

#[test]
fn direct_native_aliases_avoid_facade_function_wrappers() {
    let engine = Engine::with_stdlib();
    for (source, expected) in [
        (
            "require(\"std/string\").length",
            "<native std/string.length>",
        ),
        ("list.append", "<native std/list.append>"),
    ] {
        assert_eq!(engine.eval(source).unwrap().unwrap().render(), expected);
    }
}

#[test]
fn shadowed_host_names_remain_ordinary_simi_functions() {
    let module = Module::source("shadowed-host", "fn invoke(host) do host.fail() end invoke")
        .host(simi::host_value! {
            name: "shadowed-host",
            values: { "label" => Value::String("private".to_owned()) },
        })
        .build();
    let engine = Engine::builder().module(module).build();
    assert_eq!(
        engine
            .eval("require(\"shadowed-host\")")
            .unwrap()
            .unwrap()
            .render(),
        "<fn shadowed-host.invoke>"
    );

    let raised = match engine
        .eval("let invoke = require(\"shadowed-host\") invoke({fail = fn() do raise \"boom\" end})")
        .unwrap()
    {
        Err(raised) => raised,
        Ok(value) => panic!("caller raise should propagate, got {}", value.render()),
    };
    assert_eq!(raised.frames.len(), 2);
    assert_eq!(raised.frames[0].function, "<anonymous>");
    assert_eq!(raised.frames[1].function, "shadowed-host.invoke");
}

#[test]
fn reassigned_and_arbitrary_host_functions_keep_public_trace_boundaries() {
    for (name, source) in [
        (
            "assigned-before",
            "let replacement = {fail = fn() do raise \"boom\" end} host = replacement fn invoke() do host.fail() end invoke",
        ),
        (
            "assigned-after",
            "fn invoke() do host.fail() end let replacement = {fail = fn() do raise \"boom\" end} host = replacement invoke",
        ),
    ] {
        let engine = Engine::builder()
            .module(
                Module::source(name, source)
                    .host(simi::host_value! {
                        name: name,
                        values: { "unused" => Value::Nil },
                    })
                    .build(),
            )
            .build();
        let call = format!("let invoke = require(\"{name}\") invoke()");
        let boundary = Span::new(call.rfind("invoke()").unwrap(), call.len());
        let raised = match engine.eval(&call).unwrap() {
            Err(raised) => raised,
            Ok(value) => panic!("caller raise should propagate, got {}", value.render()),
        };
        assert_eq!(raised.origin, boundary);
        assert_eq!(raised.frames.len(), 2);
        assert!(
            raised
                .frames
                .iter()
                .all(|frame| frame.call_span == boundary)
        );
        assert_eq!(raised.frames[1].function, format!("{name}.invoke"));
    }

    let producer = Engine::new();
    let user_function = producer
        .eval("fn fail() do raise \"host boom\" end fail")
        .unwrap()
        .unwrap();
    let direct_user_function = user_function.clone();
    let require_value = producer.eval("require").unwrap().unwrap();
    let engine = Engine::builder()
        .module(
            Module::source("direct-function", "host.fail")
                .host(simi::host_value! {
                    name: "direct-function",
                    values: { "fail" => direct_user_function },
                })
                .build(),
        )
        .module(
            Module::source(
                "arbitrary-functions",
                "fn invoke() do host.fail() end fn load() do host.load(\"absent\") end {invoke = invoke, load = load}",
            )
            .host(simi::host_value! {
                name: "arbitrary-functions",
                values: {
                    "fail" => user_function,
                    "load" => require_value,
                },
            })
            .build(),
        )
        .build();
    let direct_call = "let fail = require(\"direct-function\") fail()";
    let direct_boundary = Span::new(direct_call.rfind("fail()").unwrap(), direct_call.len());
    let direct_raised = match engine.eval(direct_call).unwrap() {
        Err(raised) => raised,
        Ok(value) => panic!("caller raise should propagate, got {}", value.render()),
    };
    assert_eq!(direct_raised.origin, direct_boundary);
    assert!(
        direct_raised
            .frames
            .iter()
            .all(|frame| frame.call_span == direct_boundary)
    );

    for function in ["invoke", "load"] {
        let call = format!("let module = require(\"arbitrary-functions\") module.{function}()");
        let start = call.rfind(&format!("module.{function}()")).unwrap();
        let boundary = Span::new(start, call.len());
        let raised = match engine.eval(&call).unwrap() {
            Err(raised) => raised,
            Ok(value) => panic!("caller raise should propagate, got {}", value.render()),
        };
        assert_eq!(raised.origin, boundary);
        assert!(
            raised
                .frames
                .iter()
                .all(|frame| frame.call_span == boundary)
        );
        assert_eq!(
            raised.frames.last().unwrap().function,
            format!("arbitrary-functions.{function}")
        );
    }
}

#[test]
fn nested_source_module_frames_collapse_to_the_public_boundary() {
    let engine = Engine::builder()
        .module(
            Module::source(
                "nested-frames",
                "fn inner() do raise \"boom\" end fn outer() do inner() end outer",
            )
            .build(),
        )
        .build();
    let source = "let outer = require(\"nested-frames\") outer()";
    let boundary = Span::new(source.rfind("outer()").unwrap(), source.len());
    let raised = match engine.eval(source).unwrap() {
        Err(raised) => raised,
        Ok(value) => panic!("caller raise should propagate, got {}", value.render()),
    };
    assert_eq!(raised.origin, boundary);
    assert_eq!(raised.frames.len(), 2);
    assert_eq!(raised.frames[0].function, "nested-frames.inner");
    assert_eq!(raised.frames[1].function, "nested-frames.outer");
    assert!(
        raised
            .frames
            .iter()
            .all(|frame| frame.call_span == boundary)
    );
}

#[test]
fn explicitly_shared_private_host_values_keep_alias_identity_across_engines() {
    let host = simi::host_value! {
        name: "shared-host",
        values: {
            "count" => Value::Int(1),
        },
    };
    let first = Engine::builder()
        .module(
            Module::source("shared-host", "host")
                .host(host.clone())
                .build(),
        )
        .build();
    let second = Engine::builder()
        .module(Module::source("shared-host", "host").host(host).build())
        .build();
    first
        .eval("let shared = require(\"shared-host\") shared.count = 2")
        .unwrap()
        .unwrap();
    assert_eq!(
        second
            .eval("require(\"shared-host\").count")
            .unwrap()
            .unwrap()
            .render(),
        "2"
    );
}

#[test]
fn source_facades_can_expose_an_arbitrary_private_host_value() {
    let engine = Engine::builder()
        .module(
            Module::source("host-value", "host")
                .host(Value::String("native data".to_owned()))
                .build(),
        )
        .build();
    assert_eq!(
        engine
            .eval("require(\"host-value\")")
            .unwrap()
            .unwrap()
            .render(),
        "\"native data\""
    );
}

#[test]
fn source_modules_cache_exports_and_capture_private_host_values() {
    let calls = Arc::new(AtomicUsize::new(0));
    let host = simi::host_value! {
        name: "source",
        functions: {
            "next" => (0, {
                let calls = calls.clone();
                move |_: &[Value], _| {
                    let value = calls.fetch_add(1, Ordering::SeqCst) + 1;
                    Ok(Ok(Value::Int(i64::try_from(value).unwrap())))
                }
            }),
        },
    };
    let module = Module::source(
        "source",
        r#"
        let next: () -> integer = host.next
        {next = next}
        "#,
    )
    .host(host)
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
    assert!(Engine::new().eval("host.next()").is_err());
}

#[test]
fn source_modules_reject_missing_host_values_and_raise_for_cycles() {
    let missing = Module::source(
        "missing-host",
        r#"
        fn call() do host.missing() end
        {call = call}
        "#,
    )
    .build();
    let engine = Engine::builder()
        .module(missing)
        .module(Module::source("left", "require(\"right\")").build())
        .module(Module::source("right", "require(\"left\")").build())
        .build();
    let missing_source = "let module = require(\"missing-host\") module.call()";
    let missing = match engine.eval(missing_source) {
        Err(error) => error,
        Ok(_) => panic!("calling a missing private host field should be a hard diagnostic"),
    };
    assert!(
        missing
            .to_string()
            .contains("cannot call value of type nil")
    );
    assert_eq!(
        missing.span().start,
        missing_source.rfind("module.call").unwrap()
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
    let nil_host = simi::host_value! {
        name: "nil-module",
        functions: {
            "load" => (0, {
                let calls = calls.clone();
                move |_: &[Value], _| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(Ok(Value::Nil))
                }
            }),
        },
    };
    let nil_module = Module::source("nil-module", "host.load() nil")
        .host(nil_host)
        .build();
    let bad_host = simi::host_value! {
        name: "bad-type",
        values: {
            "value" => Value::Int(1),
        },
    };
    let bad_type = Module::source("bad-type", "host.value()")
        .host(bad_host)
        .build();
    let wrong_host = simi::host_value! {
        name: "wrong-arity",
        functions: {
            "one" => (1, |_: &[Value], _| Ok(Ok(Value::Nil))),
        },
    };
    let wrong_arity = Module::source("wrong-arity", "host.one()")
        .host(wrong_host)
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
        Ok(_) => panic!("expected non-function host value diagnostic"),
    };
    assert!(
        bad_type
            .to_string()
            .contains("cannot call value of type integer")
    );
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
    let flaky_host = simi::host_value! {
        name: "flaky",
        functions: {
            "flaky" => (0, {
                let attempts = attempts.clone();
                move |_: &[Value], span| {
                    if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                        Err(RuntimeError::new(span, "temporary load failure"))
                    } else {
                        Ok(Ok(Value::Nil))
                    }
                }
            }),
        },
    };
    let flaky = Module::source("flaky", "host.flaky() {value = 1}")
        .host(flaky_host)
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
fn engine_lsp_catalog_includes_bundled_prelude_facades() {
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

    let mut sources = Engine::builder().stdlib().stdio().build().module_sources();
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(
        sources.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        [
            "std/io",
            "std/iter",
            "std/list",
            "std/map",
            "std/number",
            "std/string",
        ]
    );
    assert_eq!(
        sources
            .iter()
            .find(|(name, _)| name == "std/list")
            .map(|(_, source)| source.as_str()),
        Some(include_str!("../stdlib/list.simi"))
    );
    assert_eq!(
        sources
            .iter()
            .find(|(name, _)| name == "std/map")
            .map(|(_, source)| source.as_str()),
        Some(include_str!("../stdlib/map.simi"))
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
