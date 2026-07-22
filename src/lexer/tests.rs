use super::*;
use crate::span::Span;

fn kinds(source: &str) -> Vec<TokenKind> {
    lex(source)
        .expect("source should lex")
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

#[test]
fn lexes_every_keyword_operator_and_delimiter() {
    assert_eq!(
        kinds(
            "fn do end if then elseif else let tap nil true false and or not loop break continue \
             match with case when raise try catch ( ) [ ] { } , . .. = == != + - * / // % -> < <= > >= |> <|"
        ),
        vec![
            TokenKind::Fn,
            TokenKind::Do,
            TokenKind::End,
            TokenKind::If,
            TokenKind::Then,
            TokenKind::ElseIf,
            TokenKind::Else,
            TokenKind::Let,
            TokenKind::Tap,
            TokenKind::Nil,
            TokenKind::True,
            TokenKind::False,
            TokenKind::And,
            TokenKind::Or,
            TokenKind::Not,
            TokenKind::Loop,
            TokenKind::Break,
            TokenKind::Continue,
            TokenKind::Match,
            TokenKind::With,
            TokenKind::Case,
            TokenKind::When,
            TokenKind::Raise,
            TokenKind::Try,
            TokenKind::Catch,
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::Comma,
            TokenKind::Dot,
            TokenKind::DotDot,
            TokenKind::Equal,
            TokenKind::EqualEqual,
            TokenKind::BangEqual,
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::SlashSlash,
            TokenKind::Percent,
            TokenKind::Arrow,
            TokenKind::Less,
            TokenKind::LessEqual,
            TokenKind::Greater,
            TokenKind::GreaterEqual,
            TokenKind::PipeGreater,
            TokenKind::LessPipe,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn trailing_argument_token_has_its_exact_two_byte_span() {
    let tokens = lex("call()<|value").unwrap();
    assert_eq!(tokens[3].kind, TokenKind::LessPipe);
    assert_eq!(tokens[3].span, Span::new(6, 8));
}

#[test]
fn lexes_anonymous_function_expression_tokens_with_exact_spans() {
    let tokens = lex("fn(value) do value end").expect("anonymous function should lex");
    assert_eq!(
        tokens.iter().map(|token| &token.kind).collect::<Vec<_>>(),
        vec![
            &TokenKind::Fn,
            &TokenKind::LParen,
            &TokenKind::Ident("value".to_owned()),
            &TokenKind::RParen,
            &TokenKind::Do,
            &TokenKind::Ident("value".to_owned()),
            &TokenKind::End,
            &TokenKind::Eof,
        ]
    );
    assert_eq!(tokens[0].span, Span::new(0, 2));
    assert_eq!(tokens[1].span, Span::new(2, 3));
    assert_eq!(tokens[6].span, Span::new(19, 22));
    assert_eq!(tokens[7].span, Span::new(22, 22));
}

#[test]
fn lexes_match_expression_tokens() {
    assert_eq!(
        kinds("match value with case [x, ..xs] when true -> x end"),
        vec![
            TokenKind::Match,
            TokenKind::Ident("value".to_owned()),
            TokenKind::With,
            TokenKind::Case,
            TokenKind::LBracket,
            TokenKind::Ident("x".to_owned()),
            TokenKind::Comma,
            TokenKind::DotDot,
            TokenKind::Ident("xs".to_owned()),
            TokenKind::RBracket,
            TokenKind::When,
            TokenKind::True,
            TokenKind::Arrow,
            TokenKind::Ident("x".to_owned()),
            TokenKind::End,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn match_keywords_are_exact_and_case_sensitive() {
    assert_eq!(
        kinds("matching Match withhold With cases Case whenever When"),
        vec![
            TokenKind::Ident("matching".to_owned()),
            TokenKind::Ident("Match".to_owned()),
            TokenKind::Ident("withhold".to_owned()),
            TokenKind::Ident("With".to_owned()),
            TokenKind::Ident("cases".to_owned()),
            TokenKind::Ident("Case".to_owned()),
            TokenKind::Ident("whenever".to_owned()),
            TokenKind::Ident("When".to_owned()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn raise_try_and_catch_keywords_are_exact_and_case_sensitive() {
    assert_eq!(
        kinds("raise Raise raiser try Try trying catch Catch catcher"),
        vec![
            TokenKind::Raise,
            TokenKind::Ident("Raise".to_owned()),
            TokenKind::Ident("raiser".to_owned()),
            TokenKind::Try,
            TokenKind::Ident("Try".to_owned()),
            TokenKind::Ident("trying".to_owned()),
            TokenKind::Catch,
            TokenKind::Ident("Catch".to_owned()),
            TokenKind::Ident("catcher".to_owned()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn raise_try_and_catch_tokens_use_utf8_byte_spans() {
    assert_eq!(
        lex("\"é\" raise try catch").expect("source should lex"),
        vec![
            Token {
                kind: TokenKind::String("é".to_owned()),
                span: Span::new(0, 4),
            },
            Token {
                kind: TokenKind::Raise,
                span: Span::new(5, 10),
            },
            Token {
                kind: TokenKind::Try,
                span: Span::new(11, 14),
            },
            Token {
                kind: TokenKind::Catch,
                span: Span::new(15, 20),
            },
            Token {
                kind: TokenKind::Eof,
                span: Span::new(20, 20),
            },
        ]
    );
}

#[test]
fn match_tokens_use_utf8_byte_spans() {
    assert_eq!(
        lex("\"é\" match with case when -> ..").expect("source should lex"),
        vec![
            Token {
                kind: TokenKind::String("é".to_owned()),
                span: Span::new(0, 4),
            },
            Token {
                kind: TokenKind::Match,
                span: Span::new(5, 10),
            },
            Token {
                kind: TokenKind::With,
                span: Span::new(11, 15),
            },
            Token {
                kind: TokenKind::Case,
                span: Span::new(16, 20),
            },
            Token {
                kind: TokenKind::When,
                span: Span::new(21, 25),
            },
            Token {
                kind: TokenKind::Arrow,
                span: Span::new(26, 28),
            },
            Token {
                kind: TokenKind::DotDot,
                span: Span::new(29, 31),
            },
            Token {
                kind: TokenKind::Eof,
                span: Span::new(31, 31),
            },
        ]
    );
}

#[test]
fn preserves_comments_minus_and_single_dot() {
    assert_eq!(
        lex("- -> . .. -- -> .. ignored\n-").expect("source should lex"),
        vec![
            Token {
                kind: TokenKind::Minus,
                span: Span::new(0, 1),
            },
            Token {
                kind: TokenKind::Arrow,
                span: Span::new(2, 4),
            },
            Token {
                kind: TokenKind::Dot,
                span: Span::new(5, 6),
            },
            Token {
                kind: TokenKind::DotDot,
                span: Span::new(7, 9),
            },
            Token {
                kind: TokenKind::Minus,
                span: Span::new(27, 28),
            },
            Token {
                kind: TokenKind::Eof,
                span: Span::new(28, 28),
            },
        ]
    );
}

#[test]
fn lexes_identifiers_integers_and_exact_keywords() {
    assert_eq!(
        kinds("fn_name Fn _x9 0 9223372036854775807"),
        vec![
            TokenKind::Ident("fn_name".to_owned()),
            TokenKind::Ident("Fn".to_owned()),
            TokenKind::Ident("_x9".to_owned()),
            TokenKind::Int(0),
            TokenKind::Int(i64::MAX),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexes_loop_control_keywords_with_exact_utf8_byte_spans() {
    assert_eq!(
        lex("\"é\" loop break continue").expect("source should lex"),
        vec![
            Token {
                kind: TokenKind::String("é".to_owned()),
                span: Span::new(0, 4),
            },
            Token {
                kind: TokenKind::Loop,
                span: Span::new(5, 9),
            },
            Token {
                kind: TokenKind::Break,
                span: Span::new(10, 15),
            },
            Token {
                kind: TokenKind::Continue,
                span: Span::new(16, 24),
            },
            Token {
                kind: TokenKind::Eof,
                span: Span::new(24, 24),
            },
        ]
    );
}

#[test]
fn loop_control_keywords_are_exact_and_case_sensitive() {
    assert_eq!(
        kinds("looping breaker continued Loop Break Continue"),
        vec![
            TokenKind::Ident("looping".to_owned()),
            TokenKind::Ident("breaker".to_owned()),
            TokenKind::Ident("continued".to_owned()),
            TokenKind::Ident("Loop".to_owned()),
            TokenKind::Ident("Break".to_owned()),
            TokenKind::Ident("Continue".to_owned()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn skips_whitespace_and_comments() {
    let tokens = lex("\tlet -- ignored |> ==\r\n  value--through eof").expect("source should lex");
    assert_eq!(
        tokens,
        vec![
            Token {
                kind: TokenKind::Let,
                span: Span::new(1, 4),
            },
            Token {
                kind: TokenKind::Ident("value".to_owned()),
                span: Span::new(25, 30),
            },
            Token {
                kind: TokenKind::Eof,
                span: Span::new(43, 43),
            },
        ]
    );
}

#[test]
fn decodes_strings_and_preserves_utf8_byte_spans() {
    let source = r#""é\"\\\n\r\t" tail"#;
    let tokens = lex(source).expect("source should lex");
    assert_eq!(
        tokens[0],
        Token {
            kind: TokenKind::String("é\"\\\n\r\t".to_owned()),
            span: Span::new(0, 14),
        }
    );
    assert_eq!(tokens[1].span, Span::new(15, 19));
    assert_eq!(tokens[2].span, Span::new(19, 19));
}

#[test]
fn rejects_integer_overflow() {
    let error = lex("9223372036854775808").expect_err("integer should overflow");
    assert_eq!(error.span, Span::new(0, 19));
    assert!(error.message.contains("too large"));
}

#[test]
fn rejects_invalid_escape_with_escape_span() {
    let error = lex(r#""bad\q""#).expect_err("escape should fail");
    assert_eq!(error.span, Span::new(4, 6));
    assert!(error.message.contains("invalid string escape"));
}

#[test]
fn rejects_unterminated_string_with_literal_span() {
    let error = lex("  \"é").expect_err("string should be unterminated");
    assert_eq!(error.span, Span::new(2, 5));
    assert!(error.message.contains("unterminated"));
}

#[test]
fn unexpected_utf8_character_has_full_byte_span() {
    let error = lex("é").expect_err("non-ASCII identifier should fail");
    assert_eq!(error.span, Span::new(0, 2));
}

#[test]
fn lexes_float_literals_and_preserves_dot_ambiguity() {
    assert_eq!(
        kinds("0 42 0.5 12.0 1e3 1E3 1e+3 1.5e-2 1.foo 1..2"),
        vec![
            TokenKind::Int(0),
            TokenKind::Int(42),
            TokenKind::Float(0.5),
            TokenKind::Float(12.0),
            TokenKind::Float(1_000.0),
            TokenKind::Float(1_000.0),
            TokenKind::Float(1_000.0),
            TokenKind::Float(0.015),
            TokenKind::Int(1),
            TokenKind::Dot,
            TokenKind::Ident("foo".to_owned()),
            TokenKind::Int(1),
            TokenKind::DotDot,
            TokenKind::Int(2),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn rejects_malformed_or_non_finite_float_literals() {
    for source in ["1e", "1e+", "1E-"] {
        assert_eq!(
            lex(source).unwrap_err().message,
            "expected digits in floating-point exponent"
        );
    }
    assert_eq!(
        lex("1e9999").unwrap_err().message,
        "floating-point literal is not finite"
    );
}

#[test]
fn boolean_operator_keywords_are_exact_and_case_sensitive() {
    assert_eq!(
        kinds("and And android or Or orbit not Not nothing"),
        vec![
            TokenKind::And,
            TokenKind::Ident("And".to_owned()),
            TokenKind::Ident("android".to_owned()),
            TokenKind::Or,
            TokenKind::Ident("Or".to_owned()),
            TokenKind::Ident("orbit".to_owned()),
            TokenKind::Not,
            TokenKind::Ident("Not".to_owned()),
            TokenKind::Ident("nothing".to_owned()),
            TokenKind::Eof,
        ]
    );
}
