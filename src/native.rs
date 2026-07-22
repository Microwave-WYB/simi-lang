mod list;

pub use list::{list_append, list_extend, list_get, list_length, list_set};

use gc::{Gc, GcCell};

use crate::runtime::{Environment, NativeFunction, TableKey, Value};

pub fn install_prelude(env: &Environment) {
    let functions = vec![
        native("length", 1, list_length),
        native("get", 2, list_get),
        native("append", 2, list_append),
        native("extend", 2, list_extend),
        native("set", 3, list_set),
    ];
    env.define(
        "list",
        Value::Table(Gc::new(GcCell::new(
            functions
                .into_iter()
                .map(|(name, value)| (TableKey::String(name), value))
                .collect(),
        ))),
    );
}

fn native(name: &'static str, arity: usize, call: crate::runtime::NativeFn) -> (String, Value) {
    (
        name.to_owned(),
        Value::NativeFunction(NativeFunction { name, arity, call }),
    )
}
