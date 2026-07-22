use simiscript::{SimiError, Value, eval};

fn value(source: &str) -> Value {
    eval(source)
        .expect("source should have no hard diagnostic")
        .expect("source should not leave an uncaught raise")
}

#[test]
fn float_literals_render_and_arithmetic_promotes_like_lua() {
    let result = value("[0.5, 12.0, 1e3, 1.5e-2, 2 * 3, 5 / 2, 5 // 2, 5.0 // 2, 1 + 2.5]");
    assert_eq!(
        result.render(),
        "[0.5, 12.0, 1000.0, 0.015, 6, 2.5, 2, 2.0, 3.5]"
    );
}

#[test]
fn floor_division_and_remainder_follow_the_divisor_sign() {
    let result = value("[-5 // 2, 5 // -2, -5 // -2, -5 % 2, 5 % -2, -5 % -2, 5.0 % -2]");
    assert_eq!(result.render(), "[-3, -3, 2, 1, -1, -1, -1.0]");
}

#[test]
fn precedence_unary_and_inequality_are_expression_operators() {
    let result =
        value("[1 + 2 * 3 == 7, -2 * 3 == -6, 1 != 1.0, 1 != 2, not false and true or false]");
    assert_eq!(result.render(), "[true, true, false, true, true]");
}

#[test]
fn boolean_operators_are_strict_and_short_circuit() {
    assert_eq!(
        value("[false and raise \"bad\", true or raise \"bad\"]").render(),
        "[false, true]"
    );

    for source in ["1 and true", "true and 1", "false or 1", "not 1"] {
        assert!(matches!(eval(source), Err(SimiError::Runtime(_))));
    }
}

#[test]
fn numeric_equality_comparison_and_patterns_promote_integers() {
    let result = value(
        r#"
        let first = case 1.0 of 1 do "int"  end
        let second = case 1 of 1.0 do "float"  end
        let third = case 1.5 of 1.5 do "fraction"  end
        [1 == 1.0, 1 < 1.5, 2.0 >= 2, 9007199254740992 < 9007199254740993, first, second, third]
        "#,
    );
    assert_eq!(
        result.render(),
        "[true, true, true, true, \"int\", \"float\", \"fraction\"]"
    );
}

#[test]
fn float_map_keys_normalize_integral_values_and_preserve_fractions() {
    let result = value(
        r#"
        let values = {[1]="integer", [1.0]="float replacement", [1.5]="fraction", [-0.0]="zero"}
        [values[1], values[1.0], values[1.5], values[0], values]
        "#,
    );
    assert_eq!(
        result.render(),
        "[\"float replacement\", \"float replacement\", \"fraction\", \"zero\", {[1]=\"float replacement\", [1.5]=\"fraction\", [0]=\"zero\"}]"
    );
}

#[test]
fn mixed_numeric_comparisons_remain_exact_at_float_boundaries() {
    let result = value(
        r#"
        let boundary_pattern = case 9223372036854775807
            of 9223372036854775808.0 do false
            of _ do true
        end
        [
            9007199254740993 == 9007199254740992.0,
            9223372036854775807 == 9223372036854775808.0,
            9223372036854775807 < 9223372036854775808.0,
            -9223372036854775807 - 1 == -9223372036854775808.0,
            boundary_pattern,
            -4.0 % 2.0,
        ]
        "#,
    );
    assert_eq!(result.render(), "[false, false, true, true, true, -0.0]");
}

#[test]
fn every_zero_divisor_raises_the_same_structural_value() {
    let result = value(
        r#"
        let divide = try 1 / 0 catch {error=error} do error  end
        let floor = try 1 // -0.0 catch {error=error} do error  end
        let remainder = try 1 % 0 catch {error=error} do error  end
        [divide, floor, remainder]
        "#,
    );
    assert_eq!(
        result.render(),
        "[\"division_by_zero\", \"division_by_zero\", \"division_by_zero\"]"
    );
}

#[test]
fn division_by_zero_preserves_origin_and_function_frames() {
    let source = "fn divide() do 1 / 0 end divide()";
    let raised = match eval(source).expect("source should have no hard diagnostic") {
        Err(raised) => raised,
        Ok(value) => panic!("division should raise, got {}", value.render()),
    };
    assert_eq!(raised.value.render(), "{error=\"division_by_zero\"}");
    assert_eq!(raised.origin.start, source.find("1 / 0").unwrap());
    assert_eq!(raised.frames.len(), 1);
    assert_eq!(raised.frames[0].function, "divide");
}

#[test]
fn numeric_type_overflow_and_non_finite_failures_remain_hard() {
    for source in ["\"x\" * 2", "9223372036854775807 * 2", "1e308 * 1e308"] {
        assert!(matches!(eval(source), Err(SimiError::Runtime(_))));
    }
}

#[test]
fn trailing_argument_appends_to_calls_and_composes_with_pipelines() {
    let result = value(
        r#"
        fn pair(left, right) do [left, right] end
        fn wrap(value) do [value] end
        let assigned = nil
        assigned = pair(6) <| 7
        [
            pair(1) <| 2,
            wrap() <| wrap() <| 3,
            4 |> pair() <| 5,
            assigned,
        ]
        "#,
    );
    assert_eq!(result.render(), "[[1, 2], [[3]], [4, 5], [6, 7]]");
}

#[test]
fn trailing_argument_is_right_associative_and_requires_call_left_operands() {
    for source in ["1 <| 2", "fn f(a, b) do a end f() <| 1 <| 2"] {
        let error = match eval(source) {
            Err(error) => error,
            Ok(_) => panic!("invalid trailing argument should fail to parse"),
        };
        assert!(matches!(error, SimiError::Parse(_)));
        let invalid_start = source.rfind('1').unwrap();
        assert_eq!(
            error.span(),
            simiscript::span::Span::new(invalid_start, invalid_start + 1)
        );
        assert!(
            error
                .to_string()
                .contains("left side of `<|` must be a call")
        );
    }
}

#[test]
fn trailing_argument_preserves_callee_then_argument_evaluation_order() {
    let result = value(
        r#"
        let list = require("std/list")
        let events = []
        fn mark(value) do
            list.append(events, value)
            value
        end
        fn choose() do
            mark(0)
            fn(first, second) do [first, second] end
        end
        let result = choose()(mark(1)) <| mark(2)
        [events, result]
        "#,
    );
    assert_eq!(result.render(), "[[0, 1, 2], [1, 2]]");
}
