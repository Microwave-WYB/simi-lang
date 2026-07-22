use crate::Module;
use crate::native::{
    list_append, list_contains, list_extend, list_get, list_insert, list_length, list_pop,
    list_remove, list_reverse, list_set, list_slice, string_contains, string_ends_with,
    string_length, string_lower, string_slice, string_split, string_starts_with, string_trim,
    string_upper,
};

pub fn list() -> Module {
    Module::builder("list")
        .function("length", 1, list_length)
        .function("get", 2, list_get)
        .function("append", 2, list_append)
        .function("extend", 2, list_extend)
        .function("set", 3, list_set)
        .function("insert", 3, list_insert)
        .function("remove", 2, list_remove)
        .function("pop", 1, list_pop)
        .function("slice", 3, list_slice)
        .function("contains", 2, list_contains)
        .function("reverse", 1, list_reverse)
        .build()
}

pub fn string() -> Module {
    Module::builder("string")
        .function("length", 1, string_length)
        .function("slice", 3, string_slice)
        .function("contains", 2, string_contains)
        .function("starts_with", 2, string_starts_with)
        .function("ends_with", 2, string_ends_with)
        .function("split", 2, string_split)
        .function("trim", 1, string_trim)
        .function("lower", 1, string_lower)
        .function("upper", 1, string_upper)
        .build()
}
