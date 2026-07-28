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
fn host_bytes_have_stable_runtime_behavior_without_script_construction_syntax() {
    let value = engine()
        .eval(
            r#"
            let host = require("host/bytes")
            let data = host.data
            [
                type(data),
                inspect(data),
                data[0],
                data[2],
                data[3],
                data == host.same,
                data != host.different,
            ]
            "#,
        )
        .expect("bytes program should have no hard diagnostic")
        .expect("bytes program should not raise");

    assert_eq!(
        value.render(),
        "[\"bytes\", \"bytes[00 7f ff]\", 0, 255, nil, true, true]"
    );
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
