use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gc::{Gc, GcCell};

use super::*;
use crate::ast::*;
use crate::runtime::{List, MapKey, NativeFunction, NativeResult, TraceFrame};
use crate::{lexer, parser};

static PROTECTED_EVALUATIONS: AtomicUsize = AtomicUsize::new(0);

fn count_protected_evaluation(_: &[Value], _: Span) -> NativeResult {
    let count = PROTECTED_EVALUATIONS.fetch_add(1, Ordering::SeqCst) + 1;
    Ok(Ok(Value::Int(
        i64::try_from(count).expect("test call count should fit in i64"),
    )))
}

fn evaluate_script(source: &str) -> RuntimeResult<ScriptResult> {
    let tokens = lexer::lex(source).expect("test source should lex");
    let program = parser::parse(tokens).expect("test source should parse");
    Interpreter::new().evaluate(&program)
}

fn evaluate(source: &str) -> RuntimeResult<Value> {
    match evaluate_script(source)? {
        Ok(value) => Ok(value),
        Err(raised) => panic!("unexpected uncaught {raised}"),
    }
}

fn expect_raised(source: &str) -> Raised {
    match evaluate_script(source) {
        Ok(Err(raised)) => raised,
        Ok(Ok(value)) => panic!("expected a raise, got {}", value.render()),
        Err(error) => panic!("expected a raise, got hard runtime error: {error}"),
    }
}

#[test]
fn destructuring_let_installs_bindings_atomically() {
    let globals = Environment::new();
    let tokens = lexer::lex("let existing = 1 let [fresh, existing] = [2, 3]").unwrap();
    let program = parser::parse(tokens).unwrap();
    let error = match Interpreter::with_globals(globals.clone()).evaluate(&program) {
        Err(error) => error,
        Ok(_) => panic!("binding conflict should be a hard error"),
    };
    assert_eq!(
        error.message,
        "name `existing` is already defined in this scope"
    );
    assert!(globals.get("fresh").is_none());
    assert!(matches!(globals.get("existing"), Some(Value::Int(1))));

    let globals = Environment::new();
    let tokens = lexer::lex("let [partial, 2] = [1, 3]").unwrap();
    let program = parser::parse(tokens).unwrap();
    let error = match Interpreter::with_globals(globals.clone()).evaluate(&program) {
        Err(error) => error,
        Ok(_) => panic!("pattern mismatch should be a hard error"),
    };
    assert_eq!(error.message, "let pattern did not match");
    assert!(globals.get("partial").is_none());
}

fn expect_runtime_error(source: &str) -> RuntimeError {
    match evaluate_script(source) {
        Err(error) => error,
        Ok(Ok(value)) => panic!("expected a hard runtime error, got {}", value.render()),
        Ok(Err(raised)) => panic!("expected a hard runtime error, got {raised}"),
    }
}

#[test]
fn evaluates_recursion_and_elseif() {
    let value = evaluate(
        r#"
                fn countdown(n) do
                    if n == 0 then
                        "done"
                    elseif n > 0 then
                        countdown(n - 1)
                    else
                        nil
                    end
                end
                countdown(4)
            "#,
    )
    .expect("program should evaluate");

    assert_eq!(value.render(), "\"done\"");
}

#[test]
fn anonymous_functions_capture_lexical_environments_and_recurse_through_let() {
    let value = evaluate(
        r#"
            fn make_counter(start) do
                let current = start
                fn(step) do
                    current = current + step
                end
            end
            let counter = make_counter(10)
            let factorial = fn(n) do
                if n == 0 then 1 else n * factorial(n - 1) end
            end
            [counter(2), counter(3), factorial(5)]
        "#,
    )
    .expect("anonymous closures should evaluate");

    assert_eq!(value.render(), "[12, 15, 120]");
}

#[test]
fn anonymous_function_names_are_used_for_rendering_arity_and_raise_frames() {
    assert_eq!(
        evaluate("fn() do nil end")
            .expect("anonymous function should evaluate")
            .render(),
        "<fn <anonymous>>"
    );

    let arity = expect_runtime_error("fn(value) do value end()");
    assert_eq!(
        arity.message,
        "function `<anonymous>` expects 1 arguments, got 0"
    );

    let raised = expect_raised("fn() do raise \"boom\" end()");
    assert_eq!(raised.frames.len(), 1);
    assert_eq!(raised.frames[0].function, "<anonymous>");
}

#[test]
fn missing_else_returns_nil_and_selected_branch_has_child_scope() {
    let value = evaluate(
        r#"
                let outer = 1
                let absent = if outer == 2 then 99 end
                if outer == 1 then
                    let outer = 2
                    outer
                end
                [absent, outer]
            "#,
    )
    .expect("program should evaluate");

    assert_eq!(value.render(), "[nil, 1]");
}

#[test]
fn evaluates_field_and_call_chains() {
    let value = evaluate(
        r#"
                fn identity(value) do value end
                identity({nested={answer=42}}).nested.answer
            "#,
    )
    .expect("program should evaluate");

    assert_eq!(value.render(), "42");
}

#[test]
fn pipeline_inserts_input_as_first_argument() {
    let value = evaluate(
        r#"
                fn add(left, right) do left + right end
                2 |> add(3)
            "#,
    )
    .expect("program should evaluate");

    assert_eq!(value.render(), "5");
}

#[test]
fn tap_pipeline_preserves_the_alias_to_the_mutated_list() {
    let value = crate::eval(
        r#"
                let list = require("std/list")
                []
                |> tap list.append(1)
                |> tap list.append(2)
            "#,
    )
    .expect("program should have no hard diagnostic")
    .expect("program should not raise");

    assert_eq!(value.render(), "[1, 2]");
}

#[test]
fn try_catches_values_with_match_semantics_and_unmatched_raises_are_unchanged() {
    let value = evaluate(
        r#"
                try raise {kind="missing", payload=[1, 2]} catch
                    {kind="missing", payload=[head, ..tail]} when head == 1 do tail end
                    _ do nil end
                end
            "#,
    )
    .expect("the structural catch should match");
    assert_eq!(value.render(), "[2]");

    let source = "try raise 1 catch 2 do nil end end";
    let raised = expect_raised(source);
    let raise_start = source
        .find("raise 1")
        .expect("raise expression should exist");
    assert_eq!(raised.value.render(), "1");
    assert_eq!(raised.origin, Span::new(raise_start, raise_start + 7));
    assert!(raised.frames.is_empty());
    assert!(raised.cause.is_none());
}

#[test]
fn try_evaluates_its_protected_expression_exactly_once() {
    PROTECTED_EVALUATIONS.store(0, Ordering::SeqCst);
    let globals = Environment::new();
    globals.define(
        "tick",
        Value::NativeFunction(NativeFunction::new(
            "tick",
            0,
            Arc::new(count_protected_evaluation),
        )),
    );
    let source = "try raise tick() catch count do count end end";
    let tokens = lexer::lex(source).expect("test source should lex");
    let program = parser::parse(tokens).expect("test source should parse");
    let outcome = Interpreter::with_globals(globals)
        .evaluate(&program)
        .expect("catching the value should not produce a hard error");
    let value = match outcome {
        Ok(value) => value,
        Err(raised) => panic!("counter value should be caught, got {raised}"),
    };

    assert_eq!(value.render(), "1");
    assert_eq!(PROTECTED_EVALUATIONS.load(Ordering::SeqCst), 1);
}

#[test]
fn loops_propagate_raises_from_initialization_and_iterations() {
    let initializer =
        evaluate("try loop state = raise 4 do break state end catch value do value end end")
            .expect("the enclosing try should catch an initializer raise");
    assert_eq!(initializer.render(), "4");

    let iteration = evaluate("try loop state = 0 do raise state end catch 0 do 9 end end")
        .expect("the enclosing try should catch an iteration raise");
    assert_eq!(iteration.render(), "9");
}

#[test]
fn hard_errors_and_non_boolean_catch_guards_bypass_language_catches() {
    let undefined = expect_runtime_error("try missing_name catch _ do \"must not catch\" end end");
    assert_eq!(undefined.span, Span::new(4, 16));
    assert!(undefined.message.contains("undefined name"));

    let guard_source = "try raise 1 catch _ when 2 do nil end end";
    let guard = expect_runtime_error(guard_source);
    let guard_start = guard_source.find("2").expect("guard should exist");
    assert_eq!(guard.span, Span::new(guard_start, guard_start + 1));
    assert_eq!(guard.message, "catch guard must be boolean, got integer");
}

#[test]
fn handler_raises_escape_siblings_and_append_the_caught_chain() {
    let source = r#"try try raise "old" catch
                _ do raise "middle" end
                _ do "inner sibling must not run" end
            end catch
                _ do raise "new" end
                _ do "outer sibling must not run" end
            end"#;
    let raised = expect_raised(source);

    let new_start = source
        .rfind("raise \"new\"")
        .expect("new raise should exist");
    assert_eq!(raised.value.render(), "\"new\"");
    assert_eq!(raised.origin, Span::new(new_start, new_start + 11));

    let middle = raised
        .cause
        .as_deref()
        .expect("middle raise should be kept");
    let middle_start = source
        .find("raise \"middle\"")
        .expect("middle raise should exist");
    assert_eq!(middle.value.render(), "\"middle\"");
    assert_eq!(middle.origin, Span::new(middle_start, middle_start + 14));

    let old = middle
        .cause
        .as_deref()
        .expect("original raise should be kept");
    let old_start = source
        .find("raise \"old\"")
        .expect("old raise should exist");
    assert_eq!(old.value.render(), "\"old\"");
    assert_eq!(old.origin, Span::new(old_start, old_start + 11));
    assert!(old.cause.is_none());
}

#[test]
fn handler_reraise_records_a_new_origin_and_freezes_caught_frames_in_its_cause() {
    let source = r#"fn leaf() do raise "old" end
try leaf() catch
error do raise error end
end"#;
    let raised = expect_raised(source);
    let reraised_start = source.rfind("raise error").expect("re-raise should exist");
    assert_eq!(raised.value.render(), "\"old\"");
    assert_eq!(
        raised.origin,
        Span::new(reraised_start, reraised_start + 11)
    );
    assert!(raised.frames.is_empty());

    let caught = raised
        .cause
        .as_deref()
        .expect("caught raise should be kept");
    let original_start = source.find("raise \"old\"").expect("raise should exist");
    let leaf_call_start = source.rfind("leaf()").expect("call should exist");
    assert_eq!(caught.value.render(), "\"old\"");
    assert_eq!(
        caught.origin,
        Span::new(original_start, original_start + 11)
    );
    assert_eq!(
        caught.frames,
        vec![TraceFrame {
            function: "leaf".to_owned(),
            call_span: Span::new(leaf_call_start, leaf_call_start + 6),
        }]
    );
    assert!(caught.cause.is_none());
}

#[test]
fn a_raise_from_a_catch_guard_escapes_without_trying_siblings() {
    let source = r#"try raise "caught" catch
_ when raise "guard" do "body must not run" end
_ do "sibling must not run" end
end"#;
    let raised = expect_raised(source);
    assert_eq!(raised.value.render(), "\"guard\"");
    let caught = raised
        .cause
        .as_deref()
        .expect("caught raise should be kept");
    assert_eq!(caught.value.render(), "\"caught\"");
    assert!(caught.cause.is_none());
}

#[test]
fn user_function_raises_collect_declared_names_and_call_spans() {
    let source = r#"fn leaf() do raise "boom" end
let alias = leaf
fn middle() do alias() end
middle()"#;
    let raised = expect_raised(source);
    let alias_start = source.find("alias()").expect("aliased call should exist");
    let middle_start = source.rfind("middle()").expect("outer call should exist");

    assert_eq!(raised.value.render(), "\"boom\"");
    assert_eq!(
        raised.frames,
        vec![
            TraceFrame {
                function: "leaf".to_owned(),
                call_span: Span::new(alias_start, alias_start + 7),
            },
            TraceFrame {
                function: "middle".to_owned(),
                call_span: Span::new(middle_start, middle_start + 8),
            },
        ]
    );
}

#[test]
fn recursive_and_pipeline_calls_record_each_entered_invocation() {
    let recursive = r#"fn recur(n) do
if n == 0 then raise n else recur(n - 1) end
end
recur(2)"#;
    let raised = expect_raised(recursive);
    let recursive_start = recursive
        .find("recur(n - 1)")
        .expect("recursive call should exist");
    let outer_start = recursive
        .rfind("recur(2)")
        .expect("outer call should exist");
    assert_eq!(raised.frames.len(), 3);
    assert_eq!(
        raised
            .frames
            .iter()
            .map(|frame| frame.call_span)
            .collect::<Vec<_>>(),
        vec![
            Span::new(recursive_start, recursive_start + 12),
            Span::new(recursive_start, recursive_start + 12),
            Span::new(outer_start, outer_start + 8),
        ]
    );
    assert!(raised.frames.iter().all(|frame| frame.function == "recur"));

    let pipeline = "fn fail(value) do raise value end\n1 |> fail()";
    let raised = expect_raised(pipeline);
    let pipe_start = pipeline.find("|>").expect("pipeline stage should exist");
    assert_eq!(
        raised.frames,
        vec![TraceFrame {
            function: "fail".to_owned(),
            call_span: Span::new(pipe_start, pipeline.len()),
        }]
    );
}

#[test]
fn runtime_errors_keep_the_expression_span() {
    let error = match evaluate("missing") {
        Ok(_) => panic!("undefined name should fail"),
        Err(error) => error,
    };

    assert_eq!(error.span, Span::new(0, 7));
    assert!(error.message.contains("undefined name"));
}

#[test]
fn nested_loop_initializer_control_targets_the_surrounding_loop() {
    let value = evaluate(
        r#"
                loop outer = 0 do
                    loop inner = break 7 do
                        break inner
                    end
                end
            "#,
    )
    .expect("break in the inner initializer should reach the outer loop");

    assert_eq!(value.render(), "7");
}

#[test]
fn leaked_top_level_control_becomes_a_runtime_error() {
    for (kind, expected_message) in [
        (
            ExprKind::Break {
                value: Box::new(Expr {
                    kind: ExprKind::Int(1),
                    span: Span::new(6, 7),
                }),
            },
            "`break` outside of a loop",
        ),
        (
            ExprKind::Continue {
                value: Box::new(Expr {
                    kind: ExprKind::Int(1),
                    span: Span::new(9, 10),
                }),
            },
            "`continue` outside of a loop",
        ),
    ] {
        let control_span = Span::new(2, 10);
        let program = Program {
            items: vec![Stmt {
                kind: StmtKind::Expr(Expr {
                    kind,
                    span: control_span,
                }),
                span: control_span,
            }],
        };

        let error = match Interpreter::new().evaluate(&program) {
            Ok(_) => panic!("leaked top-level control should fail defensively"),
            Err(error) => error,
        };
        assert_eq!(error.span, control_span);
        assert_eq!(error.message, expected_message);
    }
}

#[test]
fn leaked_function_control_cannot_be_caught_by_callers_loop() {
    let control_span = Span::new(20, 27);
    let function = Stmt {
        kind: StmtKind::Function {
            name: "escape".to_owned(),
            params: Vec::new(),
            body: Block {
                items: vec![Stmt {
                    kind: StmtKind::Expr(Expr {
                        kind: ExprKind::Break {
                            value: Box::new(Expr {
                                kind: ExprKind::Int(9),
                                span: Span::new(26, 27),
                            }),
                        },
                        span: control_span,
                    }),
                    span: control_span,
                }],
                span: control_span,
            },
        },
        span: Span::new(0, 31),
    };
    let call_span = Span::new(50, 58);
    let caller_loop = Stmt {
        kind: StmtKind::Expr(Expr {
            kind: ExprKind::Loop {
                state: "state".to_owned(),
                initial: Box::new(Expr {
                    kind: ExprKind::Nil,
                    span: Span::new(40, 43),
                }),
                body: Block {
                    items: vec![Stmt {
                        kind: StmtKind::Expr(Expr {
                            kind: ExprKind::Call {
                                callee: Box::new(Expr {
                                    kind: ExprKind::Variable("escape".to_owned()),
                                    span: Span::new(50, 56),
                                }),
                                args: Vec::new(),
                            },
                            span: call_span,
                        }),
                        span: call_span,
                    }],
                    span: call_span,
                },
            },
            span: Span::new(34, 62),
        }),
        span: Span::new(34, 62),
    };

    let error = match Interpreter::new().evaluate(&Program {
        items: vec![function, caller_loop],
    }) {
        Ok(_) => panic!("function control must not escape into a caller loop"),
        Err(error) => error,
    };

    assert_eq!(error.span, control_span);
    assert_eq!(error.message, "`break` outside of a loop");
}

fn expression(kind: ExprKind, span: Span) -> Expr {
    Expr { kind, span }
}

fn pattern(kind: PatternKind, span: Span) -> Pattern {
    Pattern { kind, span }
}

fn expression_block(expression: Expr) -> Block {
    let span = expression.span;
    Block {
        items: vec![Stmt {
            kind: StmtKind::Expr(expression),
            span,
        }],
        span,
    }
}

fn evaluate_ast(expression: Expr, globals: Environment) -> RuntimeResult<Value> {
    let span = expression.span;
    match Interpreter::with_globals(globals).evaluate(&Program {
        items: vec![Stmt {
            kind: StmtKind::Expr(expression),
            span,
        }],
    })? {
        Ok(value) => Ok(value),
        Err(raised) => panic!("unexpected uncaught {raised}"),
    }
}

#[test]
fn list_rest_has_a_new_container_and_retains_nested_aliases() {
    let nested = List::shared(vec![Value::Int(2)]);
    let source = List::shared(vec![Value::Int(1), Value::List(nested.clone())]);
    let globals = Environment::new();
    globals.define("source", Value::List(source.clone()));

    let match_span = Span::new(0, 40);
    let value = evaluate_ast(
        expression(
            ExprKind::Case {
                value: Box::new(expression(
                    ExprKind::Variable("source".to_owned()),
                    Span::new(6, 12),
                )),
                clauses: vec![PatternClause {
                    pattern: pattern(
                        PatternKind::List {
                            elements: vec![pattern(PatternKind::Wildcard, Span::new(24, 25))],
                            rest: Some(PatternRest::Binding("tail".to_owned())),
                        },
                        Span::new(23, 33),
                    ),
                    guard: None,
                    body: expression_block(expression(
                        ExprKind::Variable("tail".to_owned()),
                        Span::new(37, 41),
                    )),
                }],
            },
            match_span,
        ),
        globals,
    )
    .expect("list pattern should match");

    let Value::List(tail) = value else {
        panic!("list rest binding should produce a list");
    };
    assert!(!Gc::ptr_eq(&source, &tail));
    let Value::List(tail_nested) = tail.borrow().get_cloned(0).expect("tail element") else {
        panic!("tail should contain the nested list");
    };
    assert!(Gc::ptr_eq(&nested, &tail_nested));

    tail.borrow_mut().push(Value::Int(3));
    assert_eq!(source.borrow().len(), 2);
    nested.borrow_mut().push(Value::Int(4));
    assert_eq!(tail_nested.borrow().len(), 2);
}

#[test]
fn map_rest_has_a_new_ordered_container_and_retains_nested_aliases() {
    let shared = List::shared(Vec::new());
    let source = Gc::new(GcCell::new(vec![
        (MapKey::String("take".to_owned()), Value::Int(1)),
        (
            MapKey::String("first".to_owned()),
            Value::List(shared.clone()),
        ),
        (MapKey::Bool(true), Value::List(shared.clone())),
        (MapKey::String("last".to_owned()), Value::Int(3)),
    ]));
    let globals = Environment::new();
    globals.define("source", Value::Map(source.clone()));

    let value = evaluate_ast(
        expression(
            ExprKind::Case {
                value: Box::new(expression(
                    ExprKind::Variable("source".to_owned()),
                    Span::new(6, 12),
                )),
                clauses: vec![PatternClause {
                    pattern: pattern(
                        PatternKind::Map {
                            fields: vec![
                                (
                                    "take".to_owned(),
                                    pattern(PatternKind::Int(1), Span::new(25, 26)),
                                ),
                                (
                                    "last".to_owned(),
                                    pattern(
                                        PatternKind::Binding("last".to_owned()),
                                        Span::new(33, 37),
                                    ),
                                ),
                            ],
                            rest: Some(PatternRest::Binding("rest".to_owned())),
                        },
                        Span::new(18, 46),
                    ),
                    guard: None,
                    body: expression_block(expression(
                        ExprKind::Variable("rest".to_owned()),
                        Span::new(50, 54),
                    )),
                }],
            },
            Span::new(0, 58),
        ),
        globals,
    )
    .expect("map pattern should match");

    let Value::Map(rest) = value else {
        panic!("map rest binding should produce a map");
    };
    assert!(!Gc::ptr_eq(&source, &rest));
    let rest_entries = rest.borrow();
    assert_eq!(rest_entries.len(), 2);
    assert_eq!(rest_entries[0].0, MapKey::String("first".to_owned()));
    assert_eq!(rest_entries[1].0, MapKey::Bool(true));
    for (_, value) in rest_entries.iter() {
        let Value::List(alias) = value else {
            panic!("rest values should retain their nested list aliases");
        };
        assert!(Gc::ptr_eq(&shared, alias));
    }
}

#[test]
fn failed_patterns_do_not_expose_partial_bindings_or_evaluate_guards() {
    let globals = Environment::new();
    globals.define(
        "source",
        Value::List(List::shared(vec![Value::Int(1), Value::Int(2)])),
    );
    let first_pattern = pattern(
        PatternKind::List {
            elements: vec![
                pattern(PatternKind::Binding("leaked".to_owned()), Span::new(20, 26)),
                pattern(PatternKind::Int(9), Span::new(28, 29)),
            ],
            rest: None,
        },
        Span::new(19, 30),
    );

    let result = evaluate_ast(
        expression(
            ExprKind::Case {
                value: Box::new(expression(
                    ExprKind::Variable("source".to_owned()),
                    Span::new(6, 12),
                )),
                clauses: vec![
                    PatternClause {
                        pattern: first_pattern,
                        guard: Some(expression(
                            ExprKind::Variable("guard_must_not_run".to_owned()),
                            Span::new(36, 54),
                        )),
                        body: expression_block(expression(
                            ExprKind::Variable("leaked".to_owned()),
                            Span::new(58, 64),
                        )),
                    },
                    PatternClause {
                        pattern: pattern(PatternKind::Wildcard, Span::new(70, 71)),
                        guard: None,
                        body: expression_block(expression(ExprKind::Int(7), Span::new(75, 76))),
                    },
                ],
            },
            Span::new(0, 80),
        ),
        globals.clone(),
    )
    .expect("the fallback case should be selected");

    assert_eq!(result.render(), "7");
    assert!(globals.get("leaked").is_none());
}

#[test]
fn bindings_are_visible_to_guards_and_bodies_but_do_not_escape_case_scopes() {
    let globals = Environment::new();
    globals.define("n", Value::Int(99));
    let n_pattern = || pattern(PatternKind::Binding("n".to_owned()), Span::new(20, 21));

    let result = evaluate_ast(
        expression(
            ExprKind::Case {
                value: Box::new(expression(ExprKind::Int(2), Span::new(6, 7))),
                clauses: vec![
                    PatternClause {
                        pattern: n_pattern(),
                        guard: Some(expression(ExprKind::Bool(false), Span::new(27, 32))),
                        body: expression_block(expression(
                            ExprKind::Variable("body_must_not_run".to_owned()),
                            Span::new(36, 53),
                        )),
                    },
                    PatternClause {
                        pattern: n_pattern(),
                        guard: Some(expression(
                            ExprKind::Binary {
                                left: Box::new(expression(
                                    ExprKind::Variable("n".to_owned()),
                                    Span::new(65, 66),
                                )),
                                op: BinaryOp::Equal,
                                right: Box::new(expression(ExprKind::Int(2), Span::new(70, 71))),
                            },
                            Span::new(65, 71),
                        )),
                        body: expression_block(expression(
                            ExprKind::Variable("n".to_owned()),
                            Span::new(75, 76),
                        )),
                    },
                ],
            },
            Span::new(0, 80),
        ),
        globals.clone(),
    )
    .expect("the second case should be selected");

    assert_eq!(result.render(), "2");
    assert_eq!(
        globals.get("n").map(|value| value.render()),
        Some("99".to_owned())
    );
}

#[test]
fn a_false_guard_discards_its_fresh_case_scope() {
    let globals = Environment::new();
    globals.define("attempt", Value::Int(77));

    let result = evaluate_ast(
        expression(
            ExprKind::Case {
                value: Box::new(expression(ExprKind::Int(2), Span::new(6, 7))),
                clauses: vec![
                    PatternClause {
                        pattern: pattern(
                            PatternKind::Binding("attempt".to_owned()),
                            Span::new(18, 25),
                        ),
                        guard: Some(expression(ExprKind::Bool(false), Span::new(31, 36))),
                        body: expression_block(expression(ExprKind::Nil, Span::new(40, 43))),
                    },
                    PatternClause {
                        pattern: pattern(PatternKind::Wildcard, Span::new(49, 50)),
                        guard: None,
                        body: expression_block(expression(
                            ExprKind::Variable("attempt".to_owned()),
                            Span::new(54, 61),
                        )),
                    },
                ],
            },
            Span::new(0, 65),
        ),
        globals,
    )
    .expect("the fallback should see the outer binding");

    assert_eq!(result.render(), "77");
}

#[test]
fn match_errors_use_the_guard_and_complete_match_spans() {
    let guard_span = Span::new(24, 25);
    let match_span = Span::new(0, 31);
    let guard_error = match evaluate_ast(
        expression(
            ExprKind::Case {
                value: Box::new(expression(ExprKind::Int(1), Span::new(6, 7))),
                clauses: vec![PatternClause {
                    pattern: pattern(PatternKind::Wildcard, Span::new(18, 19)),
                    guard: Some(expression(ExprKind::Int(1), guard_span)),
                    body: Block {
                        items: Vec::new(),
                        span: Span::new(29, 29),
                    },
                }],
            },
            match_span,
        ),
        Environment::new(),
    ) {
        Ok(_) => panic!("an integer guard should fail"),
        Err(error) => error,
    };
    assert_eq!(guard_error.span, guard_span);
    assert_eq!(
        guard_error.message,
        "case guard must be boolean, got integer"
    );

    let no_match_span = Span::new(40, 72);
    let no_match_error = match evaluate_ast(
        expression(
            ExprKind::Case {
                value: Box::new(expression(ExprKind::Int(1), Span::new(46, 47))),
                clauses: vec![PatternClause {
                    pattern: pattern(PatternKind::Int(2), Span::new(58, 59)),
                    guard: None,
                    body: expression_block(expression(ExprKind::Nil, Span::new(63, 66))),
                }],
            },
            no_match_span,
        ),
        Environment::new(),
    ) {
        Ok(_) => panic!("a case expression with no selected clause should fail"),
        Err(error) => error,
    };
    assert_eq!(no_match_error.span, no_match_span);
    assert_eq!(no_match_error.message, "no case clause matched");
}
