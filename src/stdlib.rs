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
    let host = crate::host_value! {
        name: "std/list",
        functions: {
            "length" => (1, list_length),
            "iter" => (1, list_iter),
            "copy" => (1, list_copy),
            "get" => (2, list_get),
            "append" => (2, list_append),
            "extend" => (2, list_extend),
            "set" => (3, list_set),
            "insert" => (3, list_insert),
            "remove" => (2, list_remove),
            "pop" => (1, list_pop),
            "slice" => (3, list_slice),
            "contains" => (2, list_contains),
            "reverse" => (1, list_reverse),
        },
    };
    Module::source("std/list", include_str!("../stdlib/list.simi"))
        .host(host)
        .build()
}

pub fn iter() -> Module {
    let host = crate::host_value! {
        name: "std/iter",
        functions: {
            "append" => (2, list_append),
        },
    };
    Module::source("std/iter", include_str!("../stdlib/iter.simi"))
        .host(host)
        .build()
}

pub fn number() -> Module {
    let host = crate::host_value! {
        name: "std/number",
        functions: {
            "to_string" => (1, number_to_string),
        },
    };
    Module::source("std/number", include_str!("../stdlib/number.simi"))
        .host(host)
        .build()
}

pub fn string() -> Module {
    let host = crate::host_value! {
        name: "std/string",
        functions: {
            "to_number" => (1, string_to_number),
            "concat" => (2, string_concat),
            "length" => (1, string_length),
            "slice" => (3, string_slice),
            "contains" => (2, string_contains),
            "starts_with" => (2, string_starts_with),
            "ends_with" => (2, string_ends_with),
            "split" => (2, string_split),
            "trim" => (1, string_trim),
            "lower" => (1, string_lower),
            "upper" => (1, string_upper),
        },
    };
    Module::source("std/string", include_str!("../stdlib/string.simi"))
        .host(host)
        .build()
}

pub fn io() -> Module {
    let host = crate::host_value! {
        name: "std/io",
        functions: {
            "read_line" => (0, stdin_read_line),
            "print" => (1, io_print),
            "println" => (1, io_println),
            "eprint" => (1, io_eprint),
            "eprintln" => (1, io_eprintln),
        },
    };
    Module::source("std/io", include_str!("../stdlib/io.simi"))
        .host(host)
        .build()
}

pub fn map() -> Module {
    let host = crate::host_value! {
        name: "std/map",
        functions: {
            "length" => (1, map_length),
            "copy" => (1, map_copy),
            "has" => (2, map_has),
            "iter" => (1, map_iter),
            "snapshot_length" => (1, list_length),
            "clear" => (1, map_clear),
        },
    };
    Module::source("std/map", include_str!("../stdlib/map.simi"))
        .host(host)
        .build()
}
