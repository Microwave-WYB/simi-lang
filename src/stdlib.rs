use crate::Module;
use crate::native::{list_append, list_extend, list_get, list_length, list_set};

pub fn list() -> Module {
    Module::builder("list")
        .function("length", 1, list_length)
        .function("get", 2, list_get)
        .function("append", 2, list_append)
        .function("extend", 2, list_extend)
        .function("set", 3, list_set)
        .build()
}
