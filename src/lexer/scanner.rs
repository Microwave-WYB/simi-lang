use crate::span::Span;

use super::{LexError, Token, TokenKind};

pub(super) fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).lex()
}

struct Lexer<'a> {
    source: &'a str,
    position: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
            tokens: Vec::new(),
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, LexError> {
        while self.position < self.source.len() {
            self.skip_ignored();
            if self.position == self.source.len() {
                break;
            }

            let start = self.position;
            let byte = self.source.as_bytes()[self.position];
            match byte {
                b'0'..=b'9' => self.lex_number(start)?,
                b'A'..=b'Z' | b'a'..=b'z' | b'_' => self.lex_identifier(start),
                b'"' => self.lex_string(start)?,
                b'(' => self.single(TokenKind::LParen),
                b')' => self.single(TokenKind::RParen),
                b'[' => self.single(TokenKind::LBracket),
                b']' => self.single(TokenKind::RBracket),
                b'{' => self.single(TokenKind::LBrace),
                b'}' => self.single(TokenKind::RBrace),
                b',' => self.single(TokenKind::Comma),
                b'.' => self.one_or_two(TokenKind::Dot, b'.', TokenKind::DotDot),
                b'+' => self.single(TokenKind::Plus),
                b'-' => self.single(TokenKind::Minus),
                b'*' => self.single(TokenKind::Star),
                b'/' => self.one_or_two(TokenKind::Slash, b'/', TokenKind::SlashSlash),
                b'%' => self.single(TokenKind::Percent),
                b'=' => self.one_or_two(TokenKind::Equal, b'=', TokenKind::EqualEqual),
                b'!' if self.peek_byte(1) == Some(b'=') => {
                    self.position += 2;
                    self.push(TokenKind::BangEqual, start);
                }
                b'<' if self.peek_byte(1) == Some(b'|') => {
                    self.position += 2;
                    self.push(TokenKind::LessPipe, start);
                }
                b'<' => self.one_or_two(TokenKind::Less, b'=', TokenKind::LessEqual),
                b'>' => self.one_or_two(TokenKind::Greater, b'=', TokenKind::GreaterEqual),
                b'|' if self.peek_byte(1) == Some(b'>') => {
                    self.position += 2;
                    self.push(TokenKind::PipeGreater, start);
                }
                _ => {
                    let character = self.source[self.position..]
                        .chars()
                        .next()
                        .expect("position is before source end");
                    return Err(LexError {
                        span: Span::new(start, start + character.len_utf8()),
                        message: format!("unexpected character {character:?}"),
                    });
                }
            }
        }

        let end = self.source.len();
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(end, end),
        });
        Ok(self.tokens)
    }

    fn skip_ignored(&mut self) {
        loop {
            while matches!(self.peek_byte(0), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                self.position += 1;
            }

            if self.peek_byte(0) == Some(b'-') && self.peek_byte(1) == Some(b'-') {
                self.position += 2;
                while !matches!(self.peek_byte(0), None | Some(b'\r' | b'\n')) {
                    self.position += 1;
                }
            } else {
                break;
            }
        }
    }

    fn lex_number(&mut self, start: usize) -> Result<(), LexError> {
        while matches!(self.peek_byte(0), Some(b'0'..=b'9')) {
            self.position += 1;
        }

        let mut is_float = false;
        if self.peek_byte(0) == Some(b'.') && matches!(self.peek_byte(1), Some(b'0'..=b'9')) {
            is_float = true;
            self.position += 1;
            while matches!(self.peek_byte(0), Some(b'0'..=b'9')) {
                self.position += 1;
            }
        }

        if matches!(self.peek_byte(0), Some(b'e' | b'E')) {
            is_float = true;
            self.position += 1;
            if matches!(self.peek_byte(0), Some(b'+' | b'-')) {
                self.position += 1;
            }
            let exponent_start = self.position;
            while matches!(self.peek_byte(0), Some(b'0'..=b'9')) {
                self.position += 1;
            }
            if self.position == exponent_start {
                return Err(LexError {
                    span: Span::new(start, self.position),
                    message: "expected digits in floating-point exponent".to_owned(),
                });
            }
        }

        let text = &self.source[start..self.position];
        if is_float {
            let value = text.parse::<f64>().map_err(|_| LexError {
                span: Span::new(start, self.position),
                message: "invalid floating-point literal".to_owned(),
            })?;
            if !value.is_finite() {
                return Err(LexError {
                    span: Span::new(start, self.position),
                    message: "floating-point literal is not finite".to_owned(),
                });
            }
            self.push(TokenKind::Float(value), start);
        } else {
            let value = text.parse::<i64>().map_err(|_| LexError {
                span: Span::new(start, self.position),
                message: "integer literal is too large for i64".to_owned(),
            })?;
            self.push(TokenKind::Int(value), start);
        }
        Ok(())
    }

    fn lex_identifier(&mut self, start: usize) {
        while matches!(
            self.peek_byte(0),
            Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_')
        ) {
            self.position += 1;
        }

        let text = &self.source[start..self.position];
        let kind = match text {
            "fn" => TokenKind::Fn,
            "do" => TokenKind::Do,
            "end" => TokenKind::End,
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "elseif" => TokenKind::ElseIf,
            "else" => TokenKind::Else,
            "let" => TokenKind::Let,
            "tap" => TokenKind::Tap,
            "nil" => TokenKind::Nil,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "is" => TokenKind::Is,
            "loop" => TokenKind::Loop,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "case" => TokenKind::Case,
            "of" => TokenKind::Of,
            "when" => TokenKind::When,
            "raise" => TokenKind::Raise,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            _ => TokenKind::Ident(text.to_owned()),
        };
        self.push(kind, start);
    }

    fn lex_string(&mut self, start: usize) -> Result<(), LexError> {
        self.position += 1;
        let mut value = String::new();

        while self.position < self.source.len() {
            let character_start = self.position;
            let character = self.source[self.position..]
                .chars()
                .next()
                .expect("position is before source end");
            self.position += character.len_utf8();

            match character {
                '"' => {
                    self.push(TokenKind::String(value), start);
                    return Ok(());
                }
                '\\' => {
                    if self.position == self.source.len() {
                        return Err(LexError {
                            span: Span::new(start, self.position),
                            message: "unterminated string literal".to_owned(),
                        });
                    }

                    let escape_start = character_start;
                    let escaped = self.source[self.position..]
                        .chars()
                        .next()
                        .expect("position is before source end");
                    self.position += escaped.len_utf8();
                    match escaped {
                        '"' => value.push('"'),
                        '\\' => value.push('\\'),
                        'n' => value.push('\n'),
                        'r' => value.push('\r'),
                        't' => value.push('\t'),
                        _ => {
                            return Err(LexError {
                                span: Span::new(escape_start, self.position),
                                message: format!("invalid string escape \\{escaped}"),
                            });
                        }
                    }
                }
                _ => value.push(character),
            }
        }

        Err(LexError {
            span: Span::new(start, self.source.len()),
            message: "unterminated string literal".to_owned(),
        })
    }

    fn single(&mut self, kind: TokenKind) {
        let start = self.position;
        self.position += 1;
        self.push(kind, start);
    }

    fn one_or_two(&mut self, one: TokenKind, second: u8, two: TokenKind) {
        let start = self.position;
        self.position += 1;
        if self.peek_byte(0) == Some(second) {
            self.position += 1;
            self.push(two, start);
        } else {
            self.push(one, start);
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.position),
        });
    }

    fn peek_byte(&self, offset: usize) -> Option<u8> {
        self.source.as_bytes().get(self.position + offset).copied()
    }
}
