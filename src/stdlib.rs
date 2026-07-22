use crate::Module;
use crate::native::{
    list_append, list_contains, list_extend, list_get, list_insert, list_length, list_pop,
    list_remove, list_reverse, list_set, list_slice, map_clear, map_entries, map_has, map_keys,
    map_length, map_values, stderr_flush, stderr_print, stderr_println, stdin_read_line,
    stdout_flush, stdout_print, stdout_println, string_contains, string_ends_with, string_length,
    string_lower, string_slice, string_split, string_starts_with, string_trim, string_upper,
};
use crate::runtime::{NativeFunction, Value};

pub fn list() -> Module {
    Module::builder("std/list")
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
        .value("map", Value::NativeFunction(NativeFunction::list_map()))
        .value(
            "filter",
            Value::NativeFunction(NativeFunction::list_filter()),
        )
        .value("fold", Value::NativeFunction(NativeFunction::list_fold()))
        .value("find", Value::NativeFunction(NativeFunction::list_find()))
        .value(
            "find_index",
            Value::NativeFunction(NativeFunction::list_find_index()),
        )
        .value("any", Value::NativeFunction(NativeFunction::list_any()))
        .value("all", Value::NativeFunction(NativeFunction::list_all()))
        .value("each", Value::NativeFunction(NativeFunction::list_each()))
        .value("count", Value::NativeFunction(NativeFunction::list_count()))
        .build()
}

pub fn string() -> Module {
    Module::builder("std/string")
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

pub fn stdin() -> Module {
    Module::builder("std/io/stdin")
        .function("read_line", 0, stdin_read_line)
        .build()
}

pub fn stdout() -> Module {
    Module::builder("std/io/stdout")
        .function("print", 1, stdout_print)
        .function("println", 1, stdout_println)
        .function("flush", 0, stdout_flush)
        .build()
}

pub fn stderr() -> Module {
    Module::builder("std/io/stderr")
        .function("print", 1, stderr_print)
        .function("println", 1, stderr_println)
        .function("flush", 0, stderr_flush)
        .build()
}

pub fn map() -> Module {
    Module::builder("std/map")
        .function("length", 1, map_length)
        .function("has", 2, map_has)
        .function("keys", 1, map_keys)
        .function("values", 1, map_values)
        .function("entries", 1, map_entries)
        .function("clear", 1, map_clear)
        .build()
}
