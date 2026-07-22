use super::*;
use crate::ast::*;
use crate::lexer::lex;

fn parse_source(source: &str) -> Result<Program, ParseError> {
    parse(lex(source).expect("source should lex"))
}

#[test]
fn respects_expression_precedence_and_associativity() {
    let program = parse_source("1 + 2 - 3 < 4 == 5").unwrap();
    let StmtKind::Expr(expression) = &program.items[0].kind else {
        panic!("expected expression statement");
    };
    let ExprKind::Binary {
        left: comparison,
        op: BinaryOp::Equal,
        ..
    } = &expression.kind
    else {
        panic!("expected equality at root");
    };
    let ExprKind::Binary {
        left: additive,
        op: BinaryOp::Less,
        ..
    } = &comparison.kind
    else {
        panic!("expected comparison below equality");
    };
    let ExprKind::Binary {
        left,
        op: BinaryOp::Subtract,
        ..
    } = &additive.kind
    else {
        panic!("expected left-associated subtraction");
    };
    assert!(matches!(
        left.kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));
}

#[test]
fn parses_type_call_comparisons_with_ordinary_precedence() {
    let program = parse_source("type(1 + 2) == \"integer\" and false").unwrap();
    let StmtKind::Expr(Expr {
        kind:
            ExprKind::Binary {
                left: equality,
                op: BinaryOp::And,
                ..
            },
        ..
    }) = &program.items[0].kind
    else {
        panic!("expected `and` at the root");
    };
    let ExprKind::Binary {
        left: type_call,
        op: BinaryOp::Equal,
        ..
    } = &equality.kind
    else {
        panic!("expected equality below boolean conjunction");
    };
    let ExprKind::Call { callee, args } = &type_call.kind else {
        panic!("expected an ordinary type call");
    };
    assert!(matches!(&callee.kind, ExprKind::Variable(name) if name == "type"));
    assert!(matches!(
        args[0].kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));
}

#[test]
fn is_is_an_identifier_and_legacy_infix_syntax_fails_in_expression_contexts() {
    let program = parse_source("let is = 1 is").unwrap();
    let StmtKind::Let {
        pattern: Pattern {
            kind: PatternKind::Binding(binding),
            ..
        },
        ..
    } = &program.items[0].kind
    else {
        panic!("expected an ordinary binding named is");
    };
    assert_eq!(binding, "is");
    assert!(matches!(
        &program.items[1].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::Variable(name),
            ..
        }) if name == "is"
    ));

    let error = parse_source("[value is \"float\"]").unwrap_err();
    assert_eq!(
        error.message,
        "expected `]` after list elements, found `identifier`"
    );
    assert_eq!(error.span, Span::new(7, 9));

    let adjacent = parse_source("value is \"float\"").unwrap();
    assert_eq!(adjacent.items.len(), 3);
}

#[test]
fn accepts_trailing_commas_in_all_comma_separated_constructs() {
    let program = parse_source(
        "fn collect(first, second,) do [first, second,] end collect({a=1, [2]=3,}, 4,) |> collect(5,)",
    )
    .unwrap();

    let StmtKind::Function { params, body, .. } = &program.items[0].kind else {
        panic!("expected function declaration");
    };
    assert_eq!(params, &["first", "second"]);
    assert!(matches!(
        body.items[0].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::List(ref elements),
            ..
        }) if elements.len() == 2
    ));
    assert!(matches!(
        program.items[1].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::Pipeline { .. },
            ..
        })
    ));
}

#[test]
fn parses_anonymous_functions_as_postfix_and_nested_expressions() {
    let source = "fn(value) do fn(inner) do value + inner end end(2)";
    let program = parse_source(source).unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Call { callee, args },
        span,
    }) = &program.items[0].kind
    else {
        panic!("expected an immediately invoked anonymous function");
    };
    let ExprKind::Function { params, body } = &callee.kind else {
        panic!("expected anonymous function call target");
    };
    assert_eq!(params, &["value"]);
    assert_eq!(args.len(), 1);
    assert_eq!(*span, Span::new(0, source.len()));
    assert!(matches!(
        body.items[0].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::Function { .. },
            ..
        })
    ));
}

#[test]
fn anonymous_functions_compose_with_indexing_and_pipelines() {
    let program = parse_source("[fn(value) do value end][0] |> apply(1)").unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Pipeline { input, .. },
        ..
    }) = &program.items[0].kind
    else {
        panic!("expected pipeline");
    };
    assert!(matches!(input.kind, ExprKind::Index { .. }));
}

#[test]
fn parses_if_followed_by_another_block_item() {
    let program = parse_source(
        "fn partition(value) do if value < 1 then nil else value end consume(value) end",
    )
    .unwrap();
    let StmtKind::Function { body, .. } = &program.items[0].kind else {
        panic!("expected function");
    };
    assert_eq!(body.items.len(), 2);
    assert!(matches!(
        body.items[0].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::If { .. },
            ..
        })
    ));
    assert!(matches!(
        body.items[1].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::Call { .. },
            ..
        })
    ));
}

#[test]
fn parses_tap_and_normal_pipeline_stages() {
    let program = parse_source("[] |> tap list.append(1) |> collect(2)").unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Pipeline { stages, .. },
        ..
    }) = &program.items[0].kind
    else {
        panic!("expected pipeline");
    };
    assert_eq!(stages.len(), 2);
    assert!(stages[0].tap);
    assert!(!stages[1].tap);
    assert!(matches!(stages[0].callee.kind, ExprKind::Field { .. }));
}

#[test]
fn parses_map_entries_and_rejects_duplicate_named_fields() {
    let program = parse_source("{word=\"simi\", [10]=3, [true]=4}").unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Map(entries),
        ..
    }) = &program.items[0].kind
    else {
        panic!("expected map");
    };
    assert!(matches!(entries[0].0.kind, ExprKind::String(ref key) if key == "word"));
    assert!(matches!(entries[1].0.kind, ExprKind::Int(10)));
    assert!(matches!(entries[2].0.kind, ExprKind::Bool(true)));

    let error = parse_source("{word=1, word=2}").unwrap_err();
    assert!(error.message.contains("duplicate map field `word`"));
    assert_eq!(error.span.start, 9);
}

#[test]
fn parses_list_and_map_indexing() {
    let program = parse_source("[{[1]=42}[1], [7, 8][0]]").unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::List(elements),
        ..
    }) = &program.items[0].kind
    else {
        panic!("expected list");
    };
    assert!(
        elements
            .iter()
            .all(|element| matches!(element.kind, ExprKind::Index { .. }))
    );
}

#[test]
fn rejects_duplicate_parameters_and_invalid_pipeline_stage() {
    let duplicate = parse_source("fn f(value, value) do nil end").unwrap_err();
    assert!(duplicate.message.contains("duplicate parameter `value`"));

    let invalid_stage = parse_source("value |> (f)(1)").unwrap_err();
    assert!(
        invalid_stage
            .message
            .contains("pipeline stage function name")
    );
    assert_eq!(invalid_stage.span.start, 9);
}

#[test]
fn reports_anonymous_function_parameter_and_delimiter_spans() {
    let duplicate_source = "let f = fn(value, value) do value end";
    let duplicate = parse_source(duplicate_source).unwrap_err();
    let duplicate_start = duplicate_source.find(", value").unwrap() + 2;
    assert_eq!(duplicate.message, "duplicate parameter `value`");
    assert_eq!(
        duplicate.span,
        Span::new(duplicate_start, duplicate_start + 5)
    );

    let missing_open_source = "let f = fn value do value end";
    let missing_open = parse_source(missing_open_source).unwrap_err();
    let value_start = missing_open_source.find("value").unwrap();
    assert_eq!(
        missing_open.message,
        "expected `(` after `fn`, found `identifier`"
    );
    assert_eq!(missing_open.span, Span::new(value_start, value_start + 5));

    let missing_end_source = "let f = fn(value) do value";
    let missing_end = parse_source(missing_end_source).unwrap_err();
    assert_eq!(
        missing_end.message,
        "expected `end` after function body, found `end of file`"
    );
    assert_eq!(
        missing_end.span,
        Span::new(missing_end_source.len(), missing_end_source.len())
    );
}

#[test]
fn rejects_malformed_or_stray_terminators() {
    let missing_end = parse_source("if 1 == 1 then nil").unwrap_err();
    assert!(missing_end.message.contains("`end` after if expression"));
    assert_eq!(missing_end.span.start, 18);

    let stray_end = parse_source("end").unwrap_err();
    assert!(stray_end.message.contains("outside of a block"));
}

#[test]
fn parses_both_loop_spellings_into_the_canonical_shape() {
    let explicit = parse_source("loop state = 0 do break state end").unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Loop {
            state,
            initial,
            body,
        },
        span,
    }) = &explicit.items[0].kind
    else {
        panic!("expected canonical loop expression");
    };
    assert_eq!(state, "state");
    assert!(matches!(initial.kind, ExprKind::Int(0)));
    assert_eq!(initial.span, Span::new(13, 14));
    assert_eq!(*span, Span::new(0, 33));
    assert_eq!(body.span, Span::new(18, 29));
    assert!(matches!(
        body.items[0].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::Break { .. },
            ..
        })
    ));

    let shorthand = parse_source("loop do break _ end").unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Loop {
            state,
            initial,
            body,
        },
        span,
    }) = &shorthand.items[0].kind
    else {
        panic!("expected canonical loop expression");
    };
    assert_eq!(state, "_");
    assert!(matches!(initial.kind, ExprKind::Nil));
    assert_eq!(initial.span, Span::new(4, 4));
    assert_eq!(*span, Span::new(0, 19));
    assert_eq!(body.span, Span::new(8, 15));
}

#[test]
fn parses_valued_and_bare_continue_with_contract_spans() {
    let valued = parse_source("loop state = 0 do continue state + 1 end").unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Loop { body, .. },
        ..
    }) = &valued.items[0].kind
    else {
        panic!("expected loop expression");
    };
    let StmtKind::Expr(Expr {
        kind: ExprKind::Continue { value },
        span,
    }) = &body.items[0].kind
    else {
        panic!("expected continue expression");
    };
    assert!(matches!(value.kind, ExprKind::Binary { .. }));
    assert_eq!(value.span, Span::new(27, 36));
    assert_eq!(*span, Span::new(18, 36));

    let bare = parse_source("loop state = 0 do continue end").unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Loop { body, .. },
        ..
    }) = &bare.items[0].kind
    else {
        panic!("expected loop expression");
    };
    let StmtKind::Expr(Expr {
        kind: ExprKind::Continue { value },
        span,
    }) = &body.items[0].kind
    else {
        panic!("expected continue expression");
    };
    assert!(matches!(value.kind, ExprKind::Nil));
    assert_eq!(value.span, Span::new(26, 26));
    assert_eq!(*span, Span::new(18, 26));
}

#[test]
fn rejects_loop_control_outside_its_lexical_loop() {
    for (source, message, span) in [
        ("break 1", "`break` outside of a loop", Span::new(0, 5)),
        (
            "continue 1",
            "`continue` outside of a loop",
            Span::new(0, 8),
        ),
    ] {
        let error = parse_source(source).unwrap_err();
        assert_eq!(error.message, message);
        assert_eq!(error.span, span);
    }

    let initializer = parse_source("loop state = break 1 do break state end").unwrap_err();
    assert_eq!(initializer.message, "`break` outside of a loop");
    assert_eq!(initializer.span, Span::new(13, 18));

    let function_boundary = parse_source("loop do fn f() do break 1 end break 2 end").unwrap_err();
    assert_eq!(function_boundary.message, "`break` outside of a loop");
    assert_eq!(function_boundary.span, Span::new(18, 23));
}

#[test]
fn handles_nested_loops_and_restores_function_loop_depth() {
    parse_source("loop do loop do break 1 end continue end").unwrap();
    parse_source("fn f() do loop do break 1 end end").unwrap();
    parse_source("loop do fn f() do loop do continue end end break 1 end").unwrap();
}

#[test]
fn reports_required_break_values_and_malformed_loop_headers() {
    let missing_value = parse_source("loop do break end").unwrap_err();
    assert_eq!(missing_value.message, "expected expression, found `end`");
    assert_eq!(missing_value.span, Span::new(14, 17));

    for (source, message, span) in [
        (
            "loop 0 do break 1 end",
            "expected loop state name, found `integer`",
            Span::new(5, 6),
        ),
        (
            "loop state do break 1 end",
            "expected `=` after loop state name, found `do`",
            Span::new(11, 13),
        ),
        (
            "loop state = 0 nil end",
            "expected `do` before loop body, found `nil`",
            Span::new(15, 18),
        ),
        (
            "loop state = 0 do break state",
            "expected `end` after loop body, found `end of file`",
            Span::new(29, 29),
        ),
    ] {
        let error = parse_source(source).unwrap_err();
        assert_eq!(error.message, message);
        assert_eq!(error.span, span);
    }
}

#[test]
fn parses_match_into_canonical_nested_patterns_and_spans() {
    let source = concat!(
        "case input ",
        "of {payload=[nil, true, 7, \"ok\", value, ..tail], ignored=_x, .._rest} ",
        "when value == 7 do value ",
        "of _ignored do ",
        "end"
    );
    let program = parse_source(source).unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Case { value, clauses },
        span,
    }) = &program.items[0].kind
    else {
        panic!("expected match expression");
    };

    assert_eq!(value.span, Span::new(5, 10));
    assert_eq!(*span, Span::new(0, source.len()));
    assert_eq!(clauses.len(), 2);
    assert!(matches!(
        clauses[0].guard,
        Some(Expr {
            kind: ExprKind::Binary { .. },
            ..
        })
    ));
    assert_eq!(clauses[0].body.items.len(), 1);

    let PatternKind::Map { fields, rest } = &clauses[0].pattern.kind else {
        panic!("expected map pattern");
    };
    assert_eq!(fields.len(), 2);
    assert!(matches!(rest, Some(PatternRest::Discard)));
    assert_eq!(clauses[0].pattern.span.start, source.find('{').unwrap());
    assert_eq!(clauses[0].pattern.span.end, source.find('}').unwrap() + 1);

    let PatternKind::List { elements, rest } = &fields[0].1.kind else {
        panic!("expected nested list pattern");
    };
    assert_eq!(elements.len(), 5);
    assert!(matches!(elements[0].kind, PatternKind::Nil));
    assert!(matches!(elements[1].kind, PatternKind::Bool(true)));
    assert!(matches!(elements[2].kind, PatternKind::Int(7)));
    assert!(matches!(elements[3].kind, PatternKind::String(ref value) if value == "ok"));
    assert!(matches!(elements[4].kind, PatternKind::Binding(ref name) if name == "value"));
    assert!(matches!(rest, Some(PatternRest::Binding(name)) if name == "tail"));
    assert!(matches!(fields[1].1.kind, PatternKind::Wildcard));
    assert!(matches!(clauses[1].pattern.kind, PatternKind::Wildcard));
    assert_eq!(clauses[1].body.items.len(), 0);
    let final_do_end = source.rfind("do").unwrap() + 2;
    assert_eq!(clauses[1].body.span, Span::new(final_do_end, final_do_end));
}

#[test]
fn case_is_a_primary_expression_and_preserves_nested_block_ownership() {
    let source = concat!(
        "loop do ",
        "case 1 ",
        "of x do if true then case x of y do y end else nil end ",
        "fn f() do case x of y do y end end ",
        "loop do break x end ",
        "of _ do break 9 ",
        "end ",
        "end"
    );
    let program = parse_source(source).unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Loop { body, .. },
        ..
    }) = &program.items[0].kind
    else {
        panic!("expected loop");
    };
    let StmtKind::Expr(Expr {
        kind: ExprKind::Case { clauses, .. },
        ..
    }) = &body.items[0].kind
    else {
        panic!("expected match");
    };
    assert_eq!(clauses.len(), 2);
    assert!(matches!(
        clauses[0].body.items[0].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::If { .. },
            ..
        })
    ));
    assert!(matches!(
        clauses[0].body.items[1].kind,
        StmtKind::Function { .. }
    ));
    assert!(matches!(
        clauses[0].body.items[2].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::Loop { .. },
            ..
        })
    ));
    assert!(matches!(
        clauses[1].body.items[0].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::Break { .. },
            ..
        })
    ));

    let postfixed = parse_source("case [1] of x do x end[0]").unwrap();
    assert!(matches!(
        postfixed.items[0].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::Index { .. },
            ..
        })
    ));
}

#[test]
fn rejects_duplicate_bindings_and_map_pattern_fields_at_second_name() {
    for (source, message, second_name) in [
        (
            "case 0 of {a=x, b=[x]} do nil end",
            "duplicate binding `x` in pattern",
            "x",
        ),
        (
            "case 0 of [x, ..x] do nil end",
            "duplicate binding `x` in pattern",
            "x",
        ),
        (
            "case 0 of {a=x, a=y} do nil end",
            "duplicate map pattern field `a`",
            "a",
        ),
    ] {
        let error = parse_source(source).unwrap_err();
        assert_eq!(error.message, message);
        let start = source.rfind(second_name).unwrap();
        assert_eq!(error.span, Span::new(start, start + second_name.len()));
    }

    parse_source("case 0 of [_x, {_x=_x}, .._x] do nil end").unwrap();
}

#[test]
fn rejects_malformed_pattern_rests_and_computed_map_keys() {
    for (source, expected_message) in [
        (
            "case [] of [..] do nil end",
            "expected rest binding name after `..`, found `]`",
        ),
        (
            "case [] of [..xs, value] do nil end",
            "expected `]` after list pattern, found `identifier`",
        ),
        (
            "case {} of {..rest, field=x} do nil end",
            "expected `}` after map pattern, found `identifier`",
        ),
        (
            "case {} of {[\"x\"]=value} do nil end",
            "expected map pattern field name or `..`, found `[`",
        ),
        (
            "case {} of {field value} do nil end",
            "expected `=` after map pattern field name, found `identifier`",
        ),
    ] {
        let error = parse_source(source).unwrap_err();
        assert_eq!(error.message, expected_message);
    }
}

#[test]
fn reports_required_case_clause_delimiters() {
    for (source, expected_message) in [
        (
            "case value _ do nil end",
            "expected `of` after case value, found `identifier`",
        ),
        (
            "case value of end",
            "expected pattern after `of`, found `end`",
        ),
        (
            "case value of _ when end",
            "expected expression, found `end`",
        ),
        (
            "case value of _ nil end",
            "expected `do` before clause body, found `nil`",
        ),
        (
            "case value of _ do nil",
            "expected `end` after case expression, found `end of file`",
        ),
        (
            "case value of _ do nil end end",
            "unexpected `end` outside of a block",
        ),
        (
            "case value of _ -> nil end",
            "expected `do` before clause body, found `-`",
        ),
    ] {
        let error = parse_source(source).unwrap_err();
        assert_eq!(error.message, expected_message);
    }
}

#[test]
fn parses_raise_with_a_full_expression_operand_and_contract_span() {
    let source = "raise 1 + 2";
    let program = parse_source(source).unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Raise { value },
        span,
    }) = &program.items[0].kind
    else {
        panic!("expected raise expression");
    };

    assert_eq!(*span, Span::new(0, source.len()));
    assert_eq!(value.span, Span::new(6, source.len()));
    assert!(matches!(
        value.kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));

    let parenthesized = parse_source("(raise 1) + 2").unwrap();
    let StmtKind::Expr(Expr {
        kind:
            ExprKind::Binary {
                left,
                op: BinaryOp::Add,
                ..
            },
        ..
    }) = &parenthesized.items[0].kind
    else {
        panic!("expected addition outside the parenthesized raise");
    };
    assert!(matches!(left.kind, ExprKind::Raise { .. }));
}

#[test]
fn parses_try_clauses_with_guards_empty_bodies_and_postfix_syntax() {
    let source = concat!(
        "try raise [1, 2] ",
        "catch [head, ..tail] when head == 1 do tail ",
        "catch _ do ",
        "end[0]"
    );
    let program = parse_source(source).unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Index { object, .. },
        span,
    }) = &program.items[0].kind
    else {
        panic!("expected postfixed try expression");
    };
    assert_eq!(*span, Span::new(0, source.len()));

    let ExprKind::Try { protected, clauses } = &object.kind else {
        panic!("expected canonical try expression");
    };
    let try_end = source.rfind("end").unwrap() + 3;
    assert_eq!(object.span, Span::new(0, try_end));
    assert!(matches!(
        protected.items[0].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::Raise { .. },
            ..
        })
    ));
    assert_eq!(clauses.len(), 2);
    assert!(matches!(
        clauses[0].pattern.kind,
        PatternKind::List {
            rest: Some(PatternRest::Binding(ref name)),
            ..
        } if name == "tail"
    ));
    assert!(matches!(
        clauses[0].guard,
        Some(Expr {
            kind: ExprKind::Binary { .. },
            ..
        })
    ));
    assert_eq!(clauses[0].body.items.len(), 1);
    assert!(clauses[1].body.items.is_empty());
    let final_do_end = source.rfind("do").unwrap() + 2;
    assert_eq!(clauses[1].body.span, Span::new(final_do_end, final_do_end));
}

#[test]
fn preserves_nested_try_match_and_if_block_ownership() {
    let source = concat!(
        "try if true then ",
        "case 1 of x do try x catch _ do nil end end ",
        "else nil end ",
        "catch error when true do if false then error end ",
        "catch _ do nil end"
    );
    let program = parse_source(source).unwrap();
    let StmtKind::Expr(Expr {
        kind: ExprKind::Try { protected, clauses },
        ..
    }) = &program.items[0].kind
    else {
        panic!("expected outer try expression");
    };
    assert!(matches!(
        protected.items[0].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::If { .. },
            ..
        })
    ));
    assert_eq!(clauses.len(), 2);
    assert!(matches!(
        clauses[0].body.items[0].kind,
        StmtKind::Expr(Expr {
            kind: ExprKind::If { .. },
            ..
        })
    ));
}

#[test]
fn reports_required_try_delimiters_and_stray_catch() {
    for (source, expected_message) in [
        ("raise", "expected expression, found `end of file`"),
        (
            "try 1 end",
            "expected `catch` after protected block, found `end`",
        ),
        (
            "try 1 catch end",
            "expected pattern after `catch`, found `end`",
        ),
        ("try 1 catch _ when end", "expected expression, found `end`"),
        (
            "try 1 catch _ nil end",
            "expected `do` before clause body, found `nil`",
        ),
        (
            "try 1 catch _ do nil",
            "expected `end` after try expression, found `end of file`",
        ),
        (
            "try 1 catch _ do nil end end",
            "unexpected `end` outside of a block",
        ),
        ("catch", "unexpected `catch` outside of a block"),
    ] {
        let error = parse_source(source).unwrap_err();
        assert_eq!(error.message, expected_message);
    }
}

#[test]
fn catch_clauses_reuse_existing_pattern_validation() {
    let source = "try 0 catch [value, ..value] do nil end";
    let error = parse_source(source).unwrap_err();
    assert_eq!(error.message, "duplicate binding `value` in pattern");
    let start = source.rfind("value").unwrap();
    assert_eq!(error.span, Span::new(start, start + "value".len()));
}

#[test]
fn parses_assignment_targets_and_right_associative_values() {
    let source = "map[key] = other.field = value";
    let program = parse_source(source).unwrap();
    let StmtKind::Expr(expression) = &program.items[0].kind else {
        panic!("expected expression statement");
    };
    assert_eq!(expression.span, Span::new(0, source.len()));
    let ExprKind::Assign { target, value } = &expression.kind else {
        panic!("expected outer assignment");
    };
    assert_eq!(target.span, Span::new(0, source.find(']').unwrap() + 1));
    assert!(matches!(target.kind, AssignmentTargetKind::Index { .. }));
    let ExprKind::Assign { target, value } = &value.kind else {
        panic!("expected right-associated assignment");
    };
    assert!(matches!(target.kind, AssignmentTargetKind::Field { .. }));
    assert!(matches!(value.kind, ExprKind::Variable(ref name) if name == "value"));
}

#[test]
fn parses_float_unary_and_operator_precedence() {
    let program = parse_source("false or true and 1 != 2 + 3 * -4").unwrap();
    let StmtKind::Expr(expression) = &program.items[0].kind else {
        panic!("expected expression statement");
    };
    let ExprKind::Binary {
        op: BinaryOp::Or,
        right,
        ..
    } = &expression.kind
    else {
        panic!("expected outer or");
    };
    let ExprKind::Binary {
        op: BinaryOp::And,
        right,
        ..
    } = &right.kind
    else {
        panic!("expected and below or");
    };
    let ExprKind::Binary {
        op: BinaryOp::NotEqual,
        right,
        ..
    } = &right.kind
    else {
        panic!("expected inequality below and");
    };
    let ExprKind::Binary {
        op: BinaryOp::Add,
        right,
        ..
    } = &right.kind
    else {
        panic!("expected addition below inequality");
    };
    let ExprKind::Binary {
        op: BinaryOp::Multiply,
        right,
        ..
    } = &right.kind
    else {
        panic!("expected multiplication below addition");
    };
    assert!(matches!(
        right.kind,
        ExprKind::Unary {
            op: UnaryOp::Negate,
            ..
        }
    ));

    let program = parse_source("case 1 of 1.0 do 2.5 end").unwrap();
    let StmtKind::Expr(expression) = &program.items[0].kind else {
        panic!("expected match expression");
    };
    let ExprKind::Case { clauses, .. } = &expression.kind else {
        panic!("expected match");
    };
    assert!(matches!(clauses[0].pattern.kind, PatternKind::Float(1.0)));
}

#[test]
fn assignment_rhs_preserves_pipeline_and_equality_precedence() {
    for (source, expected) in [("a = b |> f()", "pipeline"), ("a = b == c", "equality")] {
        let program = parse_source(source).unwrap();
        let StmtKind::Expr(expression) = &program.items[0].kind else {
            panic!("expected expression statement");
        };
        let ExprKind::Assign { value, .. } = &expression.kind else {
            panic!("expected assignment");
        };
        match expected {
            "pipeline" => assert!(matches!(value.kind, ExprKind::Pipeline { .. })),
            "equality" => assert!(matches!(
                value.kind,
                ExprKind::Binary {
                    op: BinaryOp::Equal,
                    ..
                }
            )),
            _ => unreachable!(),
        }
    }
}

#[test]
fn rejects_non_lvalue_assignment_targets() {
    for source in ["1 = 2", "call() = 2", "(a + b) = 2", "a |> f() = 2"] {
        let error = parse_source(source).unwrap_err();
        assert_eq!(error.message, "invalid assignment target");
    }
}
