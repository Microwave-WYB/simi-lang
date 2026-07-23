use crate::span::Span;
use crate::syntax::SyntaxKind;

use super::{LexError, Token, TokenKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lexeme {
    pub kind: SyntaxKind,
    pub text: String,
    pub span: Span,
}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut scanner = Scanner::new(source);
    while scanner.position < source.len() {
        if scanner.scan_one(false).is_err() {
            return Err(scanner.errors.remove(0));
        }
    }
    scanner.tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(source.len(), source.len()),
    });
    Ok(scanner.tokens)
}

pub fn lex_lossless(source: &str) -> (Vec<Lexeme>, Vec<LexError>) {
    let mut scanner = Scanner::new(source);
    while scanner.position < source.len() {
        let before = scanner.position;
        let _ = scanner.scan_one(true);
        if scanner.position == before {
            let character = source[before..].chars().next().expect("not at EOF");
            scanner.position += character.len_utf8();
        }
    }
    (scanner.lexemes, scanner.errors)
}

struct Scanner<'a> {
    source: &'a str,
    position: usize,
    tokens: Vec<Token>,
    lexemes: Vec<Lexeme>,
    errors: Vec<LexError>,
}

impl<'a> Scanner<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
            tokens: Vec::new(),
            lexemes: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn scan_one(&mut self, recovering: bool) -> Result<(), ()> {
        let start = self.position;
        let byte = self.source.as_bytes()[start];
        if matches!(byte, b' ' | b'\t' | b'\r' | b'\n') {
            self.position += 1;
            while matches!(self.peek(0), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                self.position += 1;
            }
            self.lexeme(SyntaxKind::WHITESPACE, start);
            return Ok(());
        }
        if byte == b'-' && self.peek(1) == Some(b'-') {
            self.position += 2;
            while !matches!(self.peek(0), None | Some(b'\r' | b'\n')) {
                self.position += 1;
            }
            self.lexeme(SyntaxKind::COMMENT, start);
            return Ok(());
        }

        let result = match byte {
            b'0'..=b'9' => self.number(start),
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                self.identifier(start);
                Ok(())
            }
            b'"' => self.string(start),
            b'(' => {
                self.simple(start, SyntaxKind::L_PAREN, TokenKind::LParen);
                Ok(())
            }
            b')' => {
                self.simple(start, SyntaxKind::R_PAREN, TokenKind::RParen);
                Ok(())
            }
            b'[' => {
                self.simple(start, SyntaxKind::L_BRACKET, TokenKind::LBracket);
                Ok(())
            }
            b']' => {
                self.simple(start, SyntaxKind::R_BRACKET, TokenKind::RBracket);
                Ok(())
            }
            b'{' => {
                self.simple(start, SyntaxKind::L_BRACE, TokenKind::LBrace);
                Ok(())
            }
            b'}' => {
                self.simple(start, SyntaxKind::R_BRACE, TokenKind::RBrace);
                Ok(())
            }
            b',' => {
                self.simple(start, SyntaxKind::COMMA, TokenKind::Comma);
                Ok(())
            }
            b':' => {
                self.simple(start, SyntaxKind::COLON, TokenKind::Colon);
                Ok(())
            }
            b'\'' => {
                self.simple(start, SyntaxKind::APOSTROPHE, TokenKind::Apostrophe);
                Ok(())
            }
            b'.' => {
                self.one_or_two(
                    start,
                    SyntaxKind::DOT,
                    TokenKind::Dot,
                    b'.',
                    SyntaxKind::DOT_DOT,
                    TokenKind::DotDot,
                );
                Ok(())
            }
            b'+' => {
                self.simple(start, SyntaxKind::PLUS, TokenKind::Plus);
                Ok(())
            }
            b'-' if self.peek(1) == Some(b'>') => {
                self.double(start, SyntaxKind::ARROW, TokenKind::Arrow);
                Ok(())
            }
            b'-' => {
                self.simple(start, SyntaxKind::MINUS, TokenKind::Minus);
                Ok(())
            }
            b'*' => {
                self.simple(start, SyntaxKind::STAR, TokenKind::Star);
                Ok(())
            }
            b'/' => {
                self.one_or_two(
                    start,
                    SyntaxKind::SLASH,
                    TokenKind::Slash,
                    b'/',
                    SyntaxKind::SLASH_SLASH,
                    TokenKind::SlashSlash,
                );
                Ok(())
            }
            b'%' => {
                self.simple(start, SyntaxKind::PERCENT, TokenKind::Percent);
                Ok(())
            }
            b'=' => {
                self.one_or_two(
                    start,
                    SyntaxKind::EQ,
                    TokenKind::Equal,
                    b'=',
                    SyntaxKind::EQ_EQ,
                    TokenKind::EqualEqual,
                );
                Ok(())
            }
            b'!' if self.peek(1) == Some(b'=') => {
                self.double(start, SyntaxKind::BANG_EQ, TokenKind::BangEqual);
                Ok(())
            }
            b'<' if self.peek(1) == Some(b'|') => {
                self.double(start, SyntaxKind::LESS_PIPE, TokenKind::LessPipe);
                Ok(())
            }
            b'<' if self.peek(1) == Some(b'>') => {
                self.double(start, SyntaxKind::LESS_GREATER, TokenKind::LessGreater);
                Ok(())
            }
            b'<' => {
                self.one_or_two(
                    start,
                    SyntaxKind::LESS,
                    TokenKind::Less,
                    b'=',
                    SyntaxKind::LESS_EQ,
                    TokenKind::LessEqual,
                );
                Ok(())
            }
            b'>' => {
                self.one_or_two(
                    start,
                    SyntaxKind::GREATER,
                    TokenKind::Greater,
                    b'=',
                    SyntaxKind::GREATER_EQ,
                    TokenKind::GreaterEqual,
                );
                Ok(())
            }
            b'?' => {
                self.one_or_two(
                    start,
                    SyntaxKind::QUESTION,
                    TokenKind::Question,
                    b'>',
                    SyntaxKind::QUESTION_GREATER,
                    TokenKind::QuestionGreater,
                );
                Ok(())
            }
            b'|' if self.peek(1) == Some(b'>') => {
                self.double(start, SyntaxKind::PIPE_GREATER, TokenKind::PipeGreater);
                Ok(())
            }
            b'|' => {
                self.simple(start, SyntaxKind::PIPE, TokenKind::Pipe);
                Ok(())
            }
            _ => {
                let character = self.source[start..].chars().next().expect("not EOF");
                self.position += character.len_utf8();
                self.fail(
                    Span::new(start, self.position),
                    format!("unexpected character {character:?}"),
                );
                Err(())
            }
        };
        if result.is_err() && recovering {
            let end = self.position.max(
                start
                    + self.source[start..]
                        .chars()
                        .next()
                        .expect("not EOF")
                        .len_utf8(),
            );
            self.position = end;
            self.lexemes.push(Lexeme {
                kind: SyntaxKind::ERROR_TOKEN,
                text: self.source[start..end].to_owned(),
                span: Span::new(start, end),
            });
        }
        result
    }

    fn number(&mut self, start: usize) -> Result<(), ()> {
        while matches!(self.peek(0), Some(b'0'..=b'9')) {
            self.position += 1;
        }
        let mut float = false;
        if self.peek(0) == Some(b'.') && matches!(self.peek(1), Some(b'0'..=b'9')) {
            float = true;
            self.position += 1;
            while matches!(self.peek(0), Some(b'0'..=b'9')) {
                self.position += 1;
            }
        }
        if matches!(self.peek(0), Some(b'e' | b'E')) {
            float = true;
            self.position += 1;
            if matches!(self.peek(0), Some(b'+' | b'-')) {
                self.position += 1;
            }
            let exponent = self.position;
            while matches!(self.peek(0), Some(b'0'..=b'9')) {
                self.position += 1;
            }
            if self.position == exponent {
                self.fail(
                    Span::new(start, self.position),
                    "expected digits in floating-point exponent".to_owned(),
                );
                return Err(());
            }
        }
        let text = &self.source[start..self.position];
        if float {
            match text.parse::<f64>() {
                Ok(value) if value.is_finite() => {
                    self.push(SyntaxKind::FLOAT, TokenKind::Float(value), start)
                }
                Ok(_) => {
                    self.fail(
                        Span::new(start, self.position),
                        "floating-point literal is not finite".to_owned(),
                    );
                    return Err(());
                }
                Err(_) => {
                    self.fail(
                        Span::new(start, self.position),
                        "invalid floating-point literal".to_owned(),
                    );
                    return Err(());
                }
            }
        } else {
            match text.parse::<i64>() {
                Ok(value) => self.push(SyntaxKind::INT, TokenKind::Int(value), start),
                Err(_) => {
                    self.fail(
                        Span::new(start, self.position),
                        "integer literal is too large for i64".to_owned(),
                    );
                    return Err(());
                }
            }
        }
        Ok(())
    }

    fn identifier(&mut self, start: usize) {
        while matches!(
            self.peek(0),
            Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_')
        ) {
            self.position += 1;
        }
        let text = &self.source[start..self.position];
        let (syntax, token) =
            keyword(text).unwrap_or_else(|| (SyntaxKind::IDENT, TokenKind::Ident(text.to_owned())));
        self.push(syntax, token, start);
    }

    fn string(&mut self, start: usize) -> Result<(), ()> {
        self.position += 1;
        let mut value = String::new();
        while self.position < self.source.len() {
            let character_start = self.position;
            let ch = self.source[self.position..]
                .chars()
                .next()
                .expect("not EOF");
            self.position += ch.len_utf8();
            match ch {
                '"' => {
                    self.push(SyntaxKind::STRING, TokenKind::String(value), start);
                    return Ok(());
                }
                '\\' => {
                    if self.position == self.source.len() {
                        break;
                    }
                    let escaped = self.source[self.position..]
                        .chars()
                        .next()
                        .expect("not EOF");
                    self.position += escaped.len_utf8();
                    match escaped {
                        '"' => value.push('"'),
                        '\\' => value.push('\\'),
                        'n' => value.push('\n'),
                        'r' => value.push('\r'),
                        't' => value.push('\t'),
                        _ => {
                            self.fail(
                                Span::new(character_start, self.position),
                                format!("invalid string escape \\{escaped}"),
                            );
                            return Err(());
                        }
                    }
                }
                _ => value.push(ch),
            }
        }
        self.fail(
            Span::new(start, self.source.len()),
            "unterminated string literal".to_owned(),
        );
        Err(())
    }

    fn simple(&mut self, start: usize, syntax: SyntaxKind, token: TokenKind) {
        self.position += 1;
        self.push(syntax, token, start);
    }
    fn double(&mut self, start: usize, syntax: SyntaxKind, token: TokenKind) {
        self.position += 2;
        self.push(syntax, token, start);
    }
    fn one_or_two(
        &mut self,
        start: usize,
        one_s: SyntaxKind,
        one_t: TokenKind,
        second: u8,
        two_s: SyntaxKind,
        two_t: TokenKind,
    ) {
        self.position += 1;
        if self.peek(0) == Some(second) {
            self.position += 1;
            self.push(two_s, two_t, start);
        } else {
            self.push(one_s, one_t, start);
        }
    }
    fn push(&mut self, syntax: SyntaxKind, token: TokenKind, start: usize) {
        let span = Span::new(start, self.position);
        self.lexemes.push(Lexeme {
            kind: syntax,
            text: self.source[start..self.position].to_owned(),
            span,
        });
        self.tokens.push(Token { kind: token, span });
    }
    fn lexeme(&mut self, kind: SyntaxKind, start: usize) {
        self.lexemes.push(Lexeme {
            kind,
            text: self.source[start..self.position].to_owned(),
            span: Span::new(start, self.position),
        });
    }
    fn fail(&mut self, span: Span, message: String) {
        self.errors.push(LexError { span, message });
    }
    fn peek(&self, offset: usize) -> Option<u8> {
        self.source.as_bytes().get(self.position + offset).copied()
    }
}

pub(super) fn is_keyword(text: &str) -> bool {
    keyword(text).is_some()
}

fn keyword(text: &str) -> Option<(SyntaxKind, TokenKind)> {
    Some(match text {
        "fn" => (SyntaxKind::FN_KW, TokenKind::Fn),
        "do" => (SyntaxKind::DO_KW, TokenKind::Do),
        "end" => (SyntaxKind::END_KW, TokenKind::End),
        "if" => (SyntaxKind::IF_KW, TokenKind::If),
        "then" => (SyntaxKind::THEN_KW, TokenKind::Then),
        "elseif" => (SyntaxKind::ELSEIF_KW, TokenKind::ElseIf),
        "else" => (SyntaxKind::ELSE_KW, TokenKind::Else),
        "let" => (SyntaxKind::LET_KW, TokenKind::Let),
        "tap" => (SyntaxKind::TAP_KW, TokenKind::Tap),
        "nil" => (SyntaxKind::NIL_KW, TokenKind::Nil),
        "true" => (SyntaxKind::TRUE_KW, TokenKind::True),
        "false" => (SyntaxKind::FALSE_KW, TokenKind::False),
        "and" => (SyntaxKind::AND_KW, TokenKind::And),
        "or" => (SyntaxKind::OR_KW, TokenKind::Or),
        "not" => (SyntaxKind::NOT_KW, TokenKind::Not),
        "loop" => (SyntaxKind::LOOP_KW, TokenKind::Loop),
        "break" => (SyntaxKind::BREAK_KW, TokenKind::Break),
        "continue" => (SyntaxKind::CONTINUE_KW, TokenKind::Continue),
        "case" => (SyntaxKind::CASE_KW, TokenKind::Case),
        "of" => (SyntaxKind::OF_KW, TokenKind::Of),
        "when" => (SyntaxKind::WHEN_KW, TokenKind::When),
        "raise" => (SyntaxKind::RAISE_KW, TokenKind::Raise),
        "try" => (SyntaxKind::TRY_KW, TokenKind::Try),
        "catch" => (SyntaxKind::CATCH_KW, TokenKind::Catch),
        _ => return None,
    })
}
