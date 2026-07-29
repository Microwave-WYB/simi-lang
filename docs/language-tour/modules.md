# Modules

<!-- tour:contents:start -->
## Tour contents

- [Hello, world!](hello-world.md)
- [Values](values.md)
- [Types and analysis](types-and-analysis.md)
- [Expressions](expressions.md)
- [Functions and bindings](functions-and-bindings.md)
- [Control flow and patterns](control-flow-and-patterns.md)
- [Mutation and copies](mutation-and-copies.md)
- Modules
  - [The portable standard library](#the-portable-standard-library)
  - [Module identity and state](#module-identity-and-state)
  - [Conversion and string helpers](#conversion-and-string-helpers)
  - [Byte helpers](#byte-helpers)
  - [Numeric encoding](#numeric-encoding)
    - [Integer encoding](#integer-encoding)
    - [Float encoding](#float-encoding)
  - [Unicode codecs](#unicode-codecs)
    - [UTF-8](#utf-8)
    - [UTF-16](#utf-16)
  - [Prelude globals](#prelude-globals)
- [Text IO](text-io.md)
- [Iterators](iterators.md)
- [Errors and embedding](errors-and-embedding.md)
<!-- tour:contents:end -->

## The portable standard library

`Engine::new()` and `Engine::with_stdlib()` provide the same portable, shadowable value prelude:

```text
list
map
iter
number
string
```

Each prelude value also has a canonical `std/*` module path for `require`. Direct use is the ordinary form:

```simi
let values = [10, 20, 30]
list.length(values)
|> number.to_string()
```

`std/io` is deliberately not in this portable set. It is a separate host capability covered on the next page.

## Module identity and state

A module's exports are a mutable map. Prelude values and repeated `require` calls in one engine share the same map with the same alias identity, so mutations are visible through every reference:

```simi
string.tour_marker = "shared"
let second = require("std/string")

second.tour_marker
```

The cache belongs to an `Engine`. Module state persists across evaluations made by that engine, while separate engines have separate registries and caches. The root `eval` convenience function uses a fresh standard-library engine for each call.

Source-backed modules are evaluated lazily on first use and then cached. A circular lazy load raises `{error = "circular_module_dependency", module = name}`.

Hosts may override a portable module's canonical path. During portable prelude installation, all five canonical module values are resolved before any of their global aliases are installed. Source-backed overrides therefore do not see a partially installed portable value prelude; they use explicit `require("std/...")` calls for module dependencies. If every load succeeds, the five globals are installed together and retain identity with their canonical cached values. A raised or circular override load remains a language raise from `Engine::eval`.

## Conversion and string helpers

`string.to_number(text)` accepts a complete signed decimal integer or decimal/exponent float. Integer syntax produces an integer and float syntax produces a finite float. Malformed input, overflow, and non-finite results return `nil`.

```simi
[
    string.to_number("42"),
    string.to_number("42.0"),
    string.to_number("6.02e23"),
    string.to_number("not a number"),
]
```

String concatenation with `<>` is strict: both operands must be strings. `string.concat(left, right)` provides the same operation in a pipeline-friendly call form.

```simi
let name = "Ada"

name
|> string.concat("!")
|> string.upper()
```

## Byte helpers

The portable `std/bytes` module is available explicitly through `require`; it intentionally does not add a `bytes` prelude global. It provides immutable octet inspection, O(1) range views, concatenation, and conversion to or from integer lists. List input must contain only integers from `0` through `255`.

```simi
let bytes = require("std/bytes")
let header = bytes.from_list([137, 80, 78, 71])
let body = bytes.slice(header, 1, 20)

[bytes.length(body), bytes.get(body, 2), bytes.to_list(body)]
```

`bytes.get` returns `nil` beyond the end. Negative and non-integer indices, invalid octets, and wrong argument categories are hard diagnostics.

## Numeric encoding

The require-only `std/integer` and `std/float` modules encode and decode numbers into bytes with explicit endianness.

### Integer encoding

`std/integer` supports fixed-width twos-complement and unsigned formats: `i8le`, `i8be`, `u8le`, `u8be`, `i16le`, `i16be`, `u16le`, `u16be`, `i32le`, `i32be`, `u32le`, `u32be`, `i64le`, `i64be`, `u64le`, and `u64be`. A value outside the selected range — including any negative value for an unsigned format — is a hard runtime diagnostic. `decode` returns `nil` when the byte length does not match the format width. A decoded `u64` value that exceeds the `i64` range is also a hard diagnostic.

```simi
let integer = require("std/integer")

let encoded = integer.encode(0x1234, "i16be")
let value = integer.decode(#[127], "i8be")
```

### Float encoding

`std/float` supports IEEE 754 single- and double-precision in `f32le`, `f32be`, `f64le`, and `f64be`. It accepts a finite float or one of the exact special-wire strings `"inf"`, `"-inf"`, or `"nan"` for encode. Encoding a finite `f64` value that narrows to a non-finite `f32` returns `nil`. Decode returns the same special-wire string for IEEE infinities and NaN values, and `nil` when the byte-length does not match the format.

```simi
let float = require("std/float")

let encoded = float.encode("inf", "f64le")
let value = float.decode(#[0, 0, 128, 127], "f32le")
```

## Unicode codecs

The require-only `std/utf8` and `std/utf16` modules encode strings to bytes and perform strict decoding.

### UTF-8

`utf8.encode` returns the UTF-8 byte representation of a string. `utf8.decode` returns `nil` for any malformed byte sequence — invalid continuation bytes, overlong encodings, surrogate halves encoded in UTF-8, and truncated multi-byte sequences.

```simi
let utf8 = require("std/utf8")

let data = utf8.encode("aé🦀")
let text = utf8.decode(#[97, 195, 169, 240, 159, 166, 128])
```

### UTF-16

`utf16` provides explicit little-endian and big-endian functions: `encode_le`, `encode_be`, `decode_le`, and `decode_be`. Decode returns `nil` for odd-length byte sequences, unpaired surrogates, and broken surrogate pairs. The BOM codepoint `U+FEFF` is preserved as an ordinary character; this module does not strip or interpret it.

```simi
let utf16 = require("std/utf16")

let little = utf16.encode_le("AB")
let text = utf16.decode_le(little)
```

## Prelude globals

Normal `Engine` evaluations provide `require`, `type`, `inspect`, `list`, `map`, `iter`, `number`, and `string` as ordinary shadowable globals. `require("std/list")` through `require("std/string")` return the same cached values as their prelude names, while `require("std/bytes")` provides the explicit bytes module. `type` returns stable runtime category labels. `inspect` produces cycle-safe, human-readable text; it is not serialization.

```simi
let values = []
list.append(values, values)

[type(values), inspect(values)]
```

The low-level `Interpreter::with_globals` Rust constructor is different: its supplied environment is complete, so it does not add this prelude automatically.

<!-- tour:navigation:start -->
---

[Previous: Mutation and copies](mutation-and-copies.md)

[Next: Text IO](text-io.md)
<!-- tour:navigation:end -->
