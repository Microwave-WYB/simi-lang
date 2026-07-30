use simi::runtime::Bytes;
use simi::{Engine, Module, SimiError, Value};

fn engine() -> Engine {
    Engine::builder()
        .module(
            Module::builder("host/bytes")
                .value("data", Value::Bytes(Bytes::new(vec![0, 127, 255])))
                .value("same", Value::Bytes(Bytes::new(vec![0, 127, 255])))
                .value("different", Value::Bytes(Bytes::new(vec![0, 127, 254])))
                .build(),
        )
        .build()
}

#[test]
fn bytes_literals_construct_flat_immutable_values_and_append_host_bytes() {
    let value = engine()
        .eval(
            r#"
            let host = require("host/bytes")
            let data = host.data
            let constructed = #[0, "A", data, 255]
            [
                type(constructed),
                inspect(constructed),
                constructed[0],
                constructed[3],
                constructed[5],
                constructed == #[0, "A", host.same, 255],
                constructed != #[0, "A", host.different, 255],
                inspect(#["猫"]),
            ]
            "#,
        )
        .expect("bytes program should have no hard diagnostic")
        .expect("bytes program should not raise");

    assert_eq!(
        value.render(),
        "[\"bytes\", \"bytes[00 41 00 7f ff ff]\", 0, 127, 255, true, true, \"bytes[e7 8c ab]\"]"
    );
}

#[test]
fn bytes_literals_evaluate_value_segments_left_to_right_once() {
    let value = engine()
        .eval(
            r#"
            let state = {observed = 0}
            fn segment(value) do
                state.observed = state.observed * 10 + value
                value
            end
            let data = #[segment(1), segment(2), segment(3)]
            [inspect(data), state.observed]
            "#,
        )
        .expect("bytes literal should have no hard diagnostic")
        .expect("bytes literal should not raise");

    assert_eq!(value.render(), "[\"bytes[01 02 03]\", 123]");
}

#[test]
fn bytes_literals_reject_dynamic_text_invalid_categories_and_out_of_range_integers() {
    for source in [
        r#"let text = "PNG" #[text]"#,
        r#"#[("PNG")]"#,
        r#"#[1.0]"#,
        r#"#[[]]"#,
        r#"#[{}]"#,
        r#"#[-1]"#,
        r#"#[256]"#,
    ] {
        assert!(
            matches!(engine().eval(source), Err(SimiError::Runtime(_))),
            "expected hard runtime diagnostic for {source}"
        );
    }
}

#[test]
fn standard_bytes_module_is_a_prelude_alias_and_converts_immutable_values() {
    let value = simi::eval(
        r#"
        let bytes = require("std/bytes")
        let source = bytes.from_list([0, 127, 255])
        let view = bytes.slice(source, 1, 20)
        [
            type(source),
            bytes.length(source),
            bytes.get(source, 3),
            inspect(view),
            inspect(bytes.concat(view, #[1])),
            bytes.to_list(source),
        ]
        "#,
    )
    .expect("std/bytes should be available from the portable engine")
    .expect("std/bytes operations should not raise");

    assert_eq!(
        value.render(),
        "[\"bytes\", 3, nil, \"bytes[7f ff]\", \"bytes[7f ff 01]\", [0, 127, 255]]"
    );

    let alias = simi::eval(
        r#"
        bytes.marker = "shared"
        require("std/bytes").marker
        "#,
    )
    .expect("bytes should be a portable prelude global")
    .expect("bytes alias access should not raise");
    assert_eq!(alias.render(), "\"shared\"");
}

#[test]
fn standard_bytes_module_rejects_invalid_arguments_as_hard_diagnostics() {
    for source in [
        r#"require("std/bytes").get(#[1], -1)"#,
        r#"require("std/bytes").slice(#[1], 0.0, 1)"#,
        r#"require("std/bytes").concat(#[1], [])"#,
        r#"require("std/bytes").from_list([0, 256])"#,
        r#"require("std/bytes").from_list([0, "x"])"#,
    ] {
        let error = match simi::eval(source) {
            Err(error) => error,
            Ok(_) => panic!("invalid bytes arguments must be hard errors"),
        };
        assert!(matches!(error, SimiError::Runtime(_)), "{error}");
        assert!(error.to_string().contains("std/bytes."), "{error}");
    }
}

#[test]
fn bytes_indices_and_writes_remain_hard_runtime_diagnostics() {
    for source in [
        r#"let data = require("host/bytes").data data[-1]"#,
        r#"let data = require("host/bytes").data data[0.0]"#,
        r#"let data = require("host/bytes").data data["0"]"#,
        r#"let data = require("host/bytes").data data[0] = 1"#,
    ] {
        assert!(matches!(engine().eval(source), Err(SimiError::Runtime(_))));
    }
}
