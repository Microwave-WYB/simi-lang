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
             case of when raise try catch ( ) [ ] { } , . .. = => == != + - * / // % < <= > >= ? ?> |> <|"
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
            TokenKind::Case,
            TokenKind::Of,
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
            TokenKind::FatArrow,
            TokenKind::EqualEqual,
            TokenKind::BangEqual,
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::SlashSlash,
            TokenKind::Percent,
            TokenKind::Less,
            TokenKind::LessEqual,
            TokenKind::Greater,
            TokenKind::GreaterEqual,
            TokenKind::Question,
            TokenKind::QuestionGreater,
            TokenKind::PipeGreater,
            TokenKind::LessPipe,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn question_operators_use_longest_match_and_exact_spans() {
    let tokens = lex("\"é\" ? ?> > >=").unwrap();
    assert_eq!(tokens[1].kind, TokenKind::Question);
    assert_eq!(tokens[1].span, Span::new(5, 6));
    assert_eq!(tokens[2].kind, TokenKind::QuestionGreater);
    assert_eq!(tokens[2].span, Span::new(7, 9));
    assert_eq!(tokens[3].kind, TokenKind::Greater);
    assert_eq!(tokens[3].span, Span::new(10, 11));
    assert_eq!(tokens[4].kind, TokenKind::GreaterEqual);
    assert_eq!(tokens[4].span, Span::new(12, 14));
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
fn lexes_case_expression_tokens() {
    assert_eq!(
        kinds("case value of [x, ..xs] when true do x end"),
        vec![
            TokenKind::Case,
            TokenKind::Ident("value".to_owned()),
            TokenKind::Of,
            TokenKind::LBracket,
            TokenKind::Ident("x".to_owned()),
            TokenKind::Comma,
            TokenKind::DotDot,
            TokenKind::Ident("xs".to_owned()),
            TokenKind::RBracket,
            TokenKind::When,
            TokenKind::True,
            TokenKind::Do,
            TokenKind::Ident("x".to_owned()),
            TokenKind::End,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn case_keywords_are_exact_and_case_sensitive() {
    assert_eq!(
        kinds("case cases Case of often Of match with whenever When"),
        vec![
            TokenKind::Case,
            TokenKind::Ident("cases".to_owned()),
            TokenKind::Ident("Case".to_owned()),
            TokenKind::Of,
            TokenKind::Ident("often".to_owned()),
            TokenKind::Ident("Of".to_owned()),
            TokenKind::Ident("match".to_owned()),
            TokenKind::Ident("with".to_owned()),
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
fn case_tokens_use_utf8_byte_spans() {
    assert_eq!(
        lex("\"é\" case of when do ..").expect("source should lex"),
        vec![
            Token {
                kind: TokenKind::String("é".to_owned()),
                span: Span::new(0, 4),
            },
            Token {
                kind: TokenKind::Case,
                span: Span::new(5, 9),
            },
            Token {
                kind: TokenKind::Of,
                span: Span::new(10, 12),
            },
            Token {
                kind: TokenKind::When,
                span: Span::new(13, 17),
            },
            Token {
                kind: TokenKind::Do,
                span: Span::new(18, 20),
            },
            Token {
                kind: TokenKind::DotDot,
                span: Span::new(21, 23),
            },
            Token {
                kind: TokenKind::Eof,
                span: Span::new(23, 23),
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
        kinds("and And android or Or orbit not Not nothing is Is island"),
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
            TokenKind::Ident("is".to_owned()),
            TokenKind::Ident("Is".to_owned()),
            TokenKind::Ident("island".to_owned()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn is_identifier_uses_its_exact_utf8_byte_span() {
    let tokens = lex("\"é\" is \"string\"").expect("source should lex");
    assert_eq!(tokens[1].kind, TokenKind::Ident("is".to_owned()));
    assert_eq!(tokens[1].span, Span::new(5, 7));
}
