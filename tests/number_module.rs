use simi::{Engine, SimiError, eval};

#[test]
fn from_string_accepts_signed_simi_numeric_forms_with_syntax_directed_types() {
    let value = eval(
        r#"
        let number = require("std/number")
        let positive = number.from_string("+42")
        let negative = number.from_string("-9223372036854775808")
        let decimal = number.from_string("-12.50")
        let exponent = number.from_string("2E+3")
        [
            positive, type(positive) == "integer",
            negative, type(negative) == "integer",
            decimal, type(decimal) == "float",
            exponent, type(exponent) == "float",
        ]
        "#,
    )
    .expect("valid conversions should have no hard diagnostic")
    .expect("valid conversions should not raise");

    assert_eq!(
        value.render(),
        "[42, true, -9223372036854775808, true, -12.5, true, 2000.0, true]"
    );
}

#[test]
fn from_string_returns_nil_for_overflow_non_finite_and_malformed_text() {
    let value = eval(
        r#"
        let number = require("std/number")
        [
            number.from_string("9223372036854775808"),
            number.from_string("-9223372036854775809"),
            number.from_string("1.7976931348623159e308"),
            number.from_string("NaN"),
            number.from_string("infinity"),
            number.from_string(""),
            number.from_string(" 1"),
            number.from_string("1 "),
            number.from_string("1_000"),
            number.from_string(".5"),
            number.from_string("1."),
            number.from_string("1e"),
            number.from_string("1e+"),
            number.from_string("1x"),
            number.from_string("0x10"),
        ]
        "#,
    )
    .expect("failed conversions should have no hard diagnostic")
    .expect("failed conversions should return normally");

    assert_eq!(
        value.render(),
        "[nil, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil]"
    );
}

#[test]
fn float_finiteness_boundary_and_integer_overflow_category_are_exact() {
    let value = eval(
        r#"
        let number = require("std/number")
        let maximum = number.from_string("1.7976931348623157e308")
        let maximum_text = number.to_string(maximum)
        let round_trip = number.from_string(maximum_text)
        let overflow_integer = number.from_string("9223372036854775808")
        [type(maximum) == "float", type(round_trip) == "float", overflow_integer]
        "#,
    )
    .expect("boundary conversions should have no hard diagnostic")
    .expect("boundary conversions should not raise");

    assert_eq!(value.render(), "[true, true, nil]");
}

#[test]
fn to_string_is_canonical_and_round_trips_numeric_categories() {
    let value = eval(
        r#"
        let number = require("std/number")
        let minimum_integer = number.from_string("-9223372036854775808")
        let integer_text = number.to_string(minimum_integer)
        let whole_float_text = number.to_string(1.0)
        let negative_zero_text = number.to_string(-0.0)
        let decimal_text = number.to_string(12.5)
        let integer = number.from_string(integer_text)
        let whole_float = number.from_string(whole_float_text)
        [
            integer_text,
            whole_float_text,
            negative_zero_text,
            decimal_text,
            integer,
            type(integer) == "integer",
            whole_float,
            type(whole_float) == "float",
        ]
        "#,
    )
    .expect("numeric round trips should have no hard diagnostic")
    .expect("numeric round trips should not raise");

    assert_eq!(
        value.render(),
        "[\"-9223372036854775808\", \"1.0\", \"-0.0\", \"12.5\", -9223372036854775808, true, 1.0, true]"
    );
}

#[test]
fn conversion_argument_errors_are_qualified_hard_diagnostics() {
    for (source, expected) in [
        (
            "let number = require(\"std/number\") number.from_string(1)",
            "std/number.from_string text must be a string, got integer",
        ),
        (
            "let number = require(\"std/number\") number.to_string(true)",
            "std/number.to_string value must be an integer or float, got boolean",
        ),
        (
            "let number = require(\"std/number\") number.from_string()",
            "native function `std/number.from_string` expects 1 arguments, got 0",
        ),
        (
            "let number = require(\"std/number\") number.to_string(1, 2)",
            "native function `std/number.to_string` expects 1 arguments, got 2",
        ),
    ] {
        let error = match eval(source) {
            Err(error) => error,
            Ok(_) => panic!("invalid conversion arguments should be hard diagnostics"),
        };
        assert!(matches!(error, SimiError::Runtime(_)));
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn number_module_is_portable_available_and_isolated_per_engine() {
    let missing = match Engine::new()
        .eval("require(\"std/number\")")
        .expect("missing number module should raise rather than hard fail")
    {
        Err(raised) => raised,
        Ok(value) => panic!(
            "empty engine should not contain std/number, got {}",
            value.render()
        ),
    };
    assert_eq!(
        missing.value.render(),
        "{error=\"module_not_found\", module=\"std/number\"}"
    );

    let first = Engine::with_stdlib();
    first
        .eval("let number = require(\"std/number\") number.marker = 1")
        .unwrap()
        .unwrap();

    let second = Engine::builder().stdlib().build();
    let exports = second
        .eval("let number = require(\"std/number\") [number.marker, number]")
        .expect("builder stdlib should provide std/number")
        .expect("std/number lookup should not raise");
    assert_eq!(
        exports.render(),
        "[nil, {from_string=<native std/number.from_string>, to_string=<native std/number.to_string>}]"
    );

    let root = eval("let number = require(\"std/number\") number.from_string(\"7\")")
        .expect("root eval should provide std/number")
        .expect("root std/number conversion should not raise");
    assert_eq!(root.render(), "7");
}
