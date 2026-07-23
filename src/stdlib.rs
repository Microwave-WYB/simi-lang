use crate::Module;
use crate::native::{
    io_eprint, io_eprintln, io_print, io_println, list_append, list_contains, list_copy,
    list_extend, list_get, list_insert, list_iter, list_length, list_pop, list_remove,
    list_reverse, list_set, list_slice, map_clear, map_copy, map_has, map_iter, map_length,
    number_to_string, stdin_read_line, string_concat, string_contains, string_ends_with,
    string_length, string_lower, string_slice, string_split, string_starts_with, string_to_number,
    string_trim, string_upper,
};

pub fn list() -> Module {
    Module::source("std/list", include_str!("../stdlib/list.simi"))
        .host_function("org.simi-lang/std/list/length", 1, list_length)
        .host_function("org.simi-lang/std/list/iter", 1, list_iter)
        .host_function("org.simi-lang/std/list/copy", 1, list_copy)
        .host_function("org.simi-lang/std/list/get", 2, list_get)
        .host_function("org.simi-lang/std/list/append", 2, list_append)
        .host_function("org.simi-lang/std/list/extend", 2, list_extend)
        .host_function("org.simi-lang/std/list/set", 3, list_set)
        .host_function("org.simi-lang/std/list/insert", 3, list_insert)
        .host_function("org.simi-lang/std/list/remove", 2, list_remove)
        .host_function("org.simi-lang/std/list/pop", 1, list_pop)
        .host_function("org.simi-lang/std/list/slice", 3, list_slice)
        .host_function("org.simi-lang/std/list/contains", 2, list_contains)
        .host_function("org.simi-lang/std/list/reverse", 1, list_reverse)
        .build()
}

pub fn iter() -> Module {
    Module::source("std/iter", include_str!("../stdlib/iter.simi"))
        .host_function("org.simi-lang/std/iter/append", 2, list_append)
        .host_function("org.simi-lang/std/iter/length", 1, list_length)
        .build()
}

pub fn number() -> Module {
    Module::source("std/number", include_str!("../stdlib/number.simi"))
        .host_function("org.simi-lang/std/number/to_string", 1, number_to_string)
        .build()
}

pub fn string() -> Module {
    Module::source("std/string", include_str!("../stdlib/string.simi"))
        .host_function("org.simi-lang/std/string/to_number", 1, string_to_number)
        .host_function("org.simi-lang/std/string/concat", 2, string_concat)
        .host_function("org.simi-lang/std/string/length", 1, string_length)
        .host_function("org.simi-lang/std/string/slice", 3, string_slice)
        .host_function("org.simi-lang/std/string/contains", 2, string_contains)
        .host_function(
            "org.simi-lang/std/string/starts_with",
            2,
            string_starts_with,
        )
        .host_function("org.simi-lang/std/string/ends_with", 2, string_ends_with)
        .host_function("org.simi-lang/std/string/split", 2, string_split)
        .host_function("org.simi-lang/std/string/trim", 1, string_trim)
        .host_function("org.simi-lang/std/string/lower", 1, string_lower)
        .host_function("org.simi-lang/std/string/upper", 1, string_upper)
        .build()
}

pub fn io() -> Module {
    Module::source("std/io", include_str!("../stdlib/io.simi"))
        .host_function("org.simi-lang/std/io/read_line", 0, stdin_read_line)
        .host_function("org.simi-lang/std/io/print", 1, io_print)
        .host_function("org.simi-lang/std/io/println", 1, io_println)
        .host_function("org.simi-lang/std/io/eprint", 1, io_eprint)
        .host_function("org.simi-lang/std/io/eprintln", 1, io_eprintln)
        .build()
}

pub fn map() -> Module {
    Module::source("std/map", include_str!("../stdlib/map.simi"))
        .host_function("org.simi-lang/std/map/length", 1, map_length)
        .host_function("org.simi-lang/std/map/copy", 1, map_copy)
        .host_function("org.simi-lang/std/map/has", 2, map_has)
        .host_function("org.simi-lang/std/map/iter", 1, map_iter)
        .host_function("org.simi-lang/std/iter/length", 1, list_length)
        .host_function("org.simi-lang/std/map/clear", 1, map_clear)
        .build()
}
