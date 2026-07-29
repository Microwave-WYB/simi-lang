use crate::{CatalogModule, CatalogModuleVisibility, Module, PackageCatalog};

/// Exact revision of this Simi distribution, used by package manifest compatibility checks.
pub const DISTRIBUTION_REVISION: &str = env!("CARGO_PKG_VERSION");

/// Source-only manifest of the runtime-owned portable catalog.
///
/// The catalog deliberately excludes the runtime-core `std/list` and `std/map` facades and
/// capability modules such as `std/io`. Engine installation pairs its source facades with fixed
/// native hosts; loading a catalog alone grants neither globals nor capabilities.
pub fn official_catalog() -> PackageCatalog {
    PackageCatalog::new(
        [
            catalog_module("std", "{}"),
            catalog_module("std/bytes", include_str!("../stdlib/bytes.simi")),
            catalog_module("std/float", include_str!("../stdlib/float.simi")),
            catalog_module("std/integer", include_str!("../stdlib/integer.simi")),
            catalog_module("std/iter", include_str!("../stdlib/iter.simi")),
            catalog_module("std/number", include_str!("../stdlib/number.simi")),
            catalog_module("std/string", include_str!("../stdlib/string.simi")),
            catalog_module("std/utf8", include_str!("../stdlib/utf8.simi")),
            catalog_module("std/utf16", include_str!("../stdlib/utf16.simi")),
        ],
        [],
    )
    .expect("the built-in official standard-library catalog is valid")
}

fn catalog_module(name: &str, source: &str) -> CatalogModule {
    CatalogModule::new(
        name,
        source,
        "std",
        format!("{name}.simi"),
        CatalogModuleVisibility::Public,
    )
    .expect("the built-in official standard-library module is valid")
}

pub(crate) fn is_official_catalog(catalog: &PackageCatalog) -> bool {
    let official = official_catalog();
    let is_std_module =
        |module: &CatalogModule| module.name() == "std" || module.name().starts_with("std/");
    // A catalog can combine the official source package with unrelated packages, but its
    // reserved `std/*` entries and `std` revision must be an exact match. `EngineBuilder`
    // installs trusted native hosts for those entries, so additions or replacements must remain
    // untrusted and be rejected before installation.
    official
        .modules()
        .iter()
        .all(|module| catalog.modules().contains(module))
        && catalog
            .modules()
            .iter()
            .filter(|module| is_std_module(module))
            .all(|module| official.modules().contains(module))
}

pub(crate) fn official_module(name: &str) -> Option<Module> {
    Some(match name {
        "std" => Module::source("std", "{}").build(),
        "std/bytes" => bytes(),
        "std/float" => float(),
        "std/integer" => integer(),
        "std/iter" => iter(),
        "std/number" => number(),
        "std/string" => string(),
        "std/utf8" => utf8(),
        "std/utf16" => utf16(),
        _ => return None,
    })
}
use crate::native::{
    bytes_concat, bytes_from_list, bytes_get, bytes_length, bytes_slice, bytes_to_list,
    float_decode, float_encode, integer_decode, integer_encode, io_eprint, io_eprintln, io_print,
    io_println, list_append, list_contains, list_copy, list_extend, list_get, list_insert,
    list_iter, list_length, list_pop, list_remove, list_reverse, list_set, list_slice, map_clear,
    map_copy, map_has, map_iter, map_length, number_to_string, stdin_read_line, string_concat,
    string_contains, string_ends_with, string_length, string_lower, string_slice, string_split,
    string_starts_with, string_to_number, string_trim, string_upper, utf8_decode, utf8_encode,
    utf16_decode_be, utf16_decode_le, utf16_encode_be, utf16_encode_le,
};
use crate::runtime::{NativeFunction, Value};
use crate::value::IteratorIntrinsic;

pub fn bytes() -> Module {
    let host = crate::host_value! {
        name: "std/bytes",
        functions: {
            "length" => (1, bytes_length),
            "get" => (2, bytes_get),
            "slice" => (3, bytes_slice),
            "concat" => (2, bytes_concat),
            "from_list" => (1, bytes_from_list),
            "to_list" => (1, bytes_to_list),
        },
    };
    Module::source("std/bytes", include_str!("../stdlib/bytes.simi"))
        .host(host)
        .build()
}

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
    let intrinsic = |name, arity, operation| {
        Value::NativeFunction(NativeFunction::iterator(
            format!("std/iter.{name}"),
            arity,
            operation,
        ))
    };
    let host = Module::builder("std/iter")
        .value(
            "typed_iterator",
            intrinsic("typed_iterator", 1, IteratorIntrinsic::TypedIterator),
        )
        .value(
            "validate_count",
            intrinsic("validate_count", 1, IteratorIntrinsic::ValidateCount),
        )
        .value(
            "validate_range",
            intrinsic("validate_range", 2, IteratorIntrinsic::ValidateRange),
        )
        .value(
            "drop_next",
            intrinsic("drop_next", 2, IteratorIntrinsic::DropNext),
        )
        .value(
            "enumerate_next",
            intrinsic("enumerate_next", 2, IteratorIntrinsic::EnumerateNext),
        )
        .value(
            "zip_next",
            intrinsic("zip_next", 2, IteratorIntrinsic::ZipNext),
        )
        .value(
            "zip_longest_next",
            intrinsic("zip_longest_next", 5, IteratorIntrinsic::ZipLongestNext),
        )
        .value(
            "filter_next",
            intrinsic("filter_next", 2, IteratorIntrinsic::FilterNext),
        )
        .value(
            "to_list",
            intrinsic("to_list", 1, IteratorIntrinsic::ToList),
        )
        .value("fold", intrinsic("fold", 3, IteratorIntrinsic::Fold))
        .value("find", intrinsic("find", 2, IteratorIntrinsic::Find))
        .value(
            "find_index",
            intrinsic("find_index", 2, IteratorIntrinsic::FindIndex),
        )
        .value(
            "contains",
            intrinsic("contains", 2, IteratorIntrinsic::Contains),
        )
        .value("any", intrinsic("any", 2, IteratorIntrinsic::Any))
        .value("all", intrinsic("all", 2, IteratorIntrinsic::All))
        .value("each", intrinsic("each", 2, IteratorIntrinsic::Each))
        .value("count", intrinsic("count", 2, IteratorIntrinsic::Count))
        .value(
            "each_while",
            intrinsic("each_while", 2, IteratorIntrinsic::EachWhile),
        )
        .value(
            "fold_while",
            intrinsic("fold_while", 3, IteratorIntrinsic::FoldWhile),
        )
        .value("loop", intrinsic("loop", 1, IteratorIntrinsic::Loop))
        .value(
            "repeat_next",
            intrinsic("repeat_next", 1, IteratorIntrinsic::RepeatNext),
        )
        .build_value();
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

pub fn integer() -> Module {
    let host = crate::host_value! {
        name: "std/integer",
        functions: {
            "encode" => (2, integer_encode),
            "decode" => (2, integer_decode),
        },
    };
    Module::source("std/integer", include_str!("../stdlib/integer.simi"))
        .host(host)
        .build()
}

pub fn float() -> Module {
    let host = crate::host_value! {
        name: "std/float",
        functions: {
            "encode" => (2, float_encode),
            "decode" => (2, float_decode),
        },
    };
    Module::source("std/float", include_str!("../stdlib/float.simi"))
        .host(host)
        .build()
}

pub fn utf8() -> Module {
    let host = crate::host_value! {
        name: "std/utf8",
        functions: {
            "encode" => (1, utf8_encode),
            "decode" => (1, utf8_decode),
        },
    };
    Module::source("std/utf8", include_str!("../stdlib/utf8.simi"))
        .host(host)
        .build()
}

pub fn utf16() -> Module {
    let host = crate::host_value! {
        name: "std/utf16",
        functions: {
            "encode_le" => (1, utf16_encode_le),
            "encode_be" => (1, utf16_encode_be),
            "decode_le" => (1, utf16_decode_le),
            "decode_be" => (1, utf16_decode_be),
        },
    };
    Module::source("std/utf16", include_str!("../stdlib/utf16.simi"))
        .host(host)
        .build()
}
