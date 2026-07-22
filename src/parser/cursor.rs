use super::{ParseError, Parser};
use crate::lexer::{Token, TokenKind};
use crate::span::Span;

impl Parser {
    pub(super) fn expect_ident(&mut self, expected: &str) -> Result<(String, Span), ParseError> {
        let span = self.current().span;
        if let TokenKind::Ident(name) = &self.current().kind {
            let name = name.clone();
            self.cursor += 1;
            Ok((name, span))
        } else {
            Err(self.error_current(format!(
                "expected {expected}, found `{}`",
                self.current_name()
            )))
        }
    }

    pub(super) fn expect_simple(
        &mut self,
        expected: SimpleToken,
        description: &str,
    ) -> Result<Span, ParseError> {
        if self.at_simple(expected) {
            Ok(self.advance_span())
        } else {
            Err(self.error_current(format!(
                "expected {description}, found `{}`",
                self.current_name()
            )))
        }
    }

    pub(super) fn consume_simple(&mut self, expected: SimpleToken) -> bool {
        if self.at_simple(expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn at_simple(&self, expected: SimpleToken) -> bool {
        expected.matches(&self.current().kind)
    }

    pub(super) fn next_is_simple(&self, expected: SimpleToken) -> bool {
        let next = (self.cursor + 1).min(self.tokens.len() - 1);
        expected.matches(&self.tokens[next].kind)
    }

    pub(super) fn at_block_terminator(&self) -> bool {
        matches!(
            &self.current().kind,
            TokenKind::ElseIf | TokenKind::Else | TokenKind::Catch | TokenKind::End
        )
    }

    pub(super) fn at_eof(&self) -> bool {
        matches!(&self.current().kind, TokenKind::Eof)
    }

    pub(super) fn advance_span(&mut self) -> Span {
        let span = self.current().span;
        self.cursor += 1;
        span
    }

    pub(super) fn previous_span(&self) -> Span {
        self.tokens[self.cursor.saturating_sub(1)].span
    }

    pub(super) fn current(&self) -> &Token {
        &self.tokens[self.cursor.min(self.tokens.len() - 1)]
    }

    pub(super) fn current_name(&self) -> &'static str {
        token_name(&self.current().kind)
    }

    pub(super) fn error_current(&self, message: String) -> ParseError {
        ParseError {
            span: self.current().span,
            message,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum SimpleToken {
    Fn,
    Loop,
    Do,
    End,
    Raise,
    Try,
    Catch,
    Case,
    Of,
    When,
    If,
    Then,
    ElseIf,
    Else,
    Let,
    Tap,
    LParen,
    RParen,
    RBracket,
    LBracket,
    RBrace,
    LBrace,
    Comma,
    Dot,
    DotDot,
    Equal,
    EqualEqual,
    BangEqual,
    Plus,
    Minus,
    Star,
    Slash,
    SlashSlash,
    Percent,
    And,
    Or,
    Not,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    PipeGreater,
    LessPipe,
}

impl SimpleToken {
    fn matches(self, kind: &TokenKind) -> bool {
        matches!(
            (self, kind),
            (Self::Fn, TokenKind::Fn)
                | (Self::Loop, TokenKind::Loop)
                | (Self::Do, TokenKind::Do)
                | (Self::End, TokenKind::End)
                | (Self::Raise, TokenKind::Raise)
                | (Self::Try, TokenKind::Try)
                | (Self::Catch, TokenKind::Catch)
                | (Self::Case, TokenKind::Case)
                | (Self::Of, TokenKind::Of)
                | (Self::When, TokenKind::When)
                | (Self::If, TokenKind::If)
                | (Self::Then, TokenKind::Then)
                | (Self::ElseIf, TokenKind::ElseIf)
                | (Self::Else, TokenKind::Else)
                | (Self::Let, TokenKind::Let)
                | (Self::Tap, TokenKind::Tap)
                | (Self::LParen, TokenKind::LParen)
                | (Self::RParen, TokenKind::RParen)
                | (Self::LBracket, TokenKind::LBracket)
                | (Self::RBracket, TokenKind::RBracket)
                | (Self::LBrace, TokenKind::LBrace)
                | (Self::RBrace, TokenKind::RBrace)
                | (Self::Comma, TokenKind::Comma)
                | (Self::Dot, TokenKind::Dot)
                | (Self::DotDot, TokenKind::DotDot)
                | (Self::Equal, TokenKind::Equal)
                | (Self::EqualEqual, TokenKind::EqualEqual)
                | (Self::BangEqual, TokenKind::BangEqual)
                | (Self::Plus, TokenKind::Plus)
                | (Self::Minus, TokenKind::Minus)
                | (Self::Star, TokenKind::Star)
                | (Self::Slash, TokenKind::Slash)
                | (Self::SlashSlash, TokenKind::SlashSlash)
                | (Self::Percent, TokenKind::Percent)
                | (Self::And, TokenKind::And)
                | (Self::Or, TokenKind::Or)
                | (Self::Not, TokenKind::Not)
                | (Self::Less, TokenKind::Less)
                | (Self::LessEqual, TokenKind::LessEqual)
                | (Self::Greater, TokenKind::Greater)
                | (Self::GreaterEqual, TokenKind::GreaterEqual)
                | (Self::PipeGreater, TokenKind::PipeGreater)
                | (Self::LessPipe, TokenKind::LessPipe)
        )
    }
}

fn token_name(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Int(_) => "integer",
        TokenKind::Float(_) => "float",
        TokenKind::String(_) => "string",
        TokenKind::Ident(_) => "identifier",
        TokenKind::Fn => "fn",
        TokenKind::Loop => "loop",
        TokenKind::Break => "break",
        TokenKind::Continue => "continue",
        TokenKind::Raise => "raise",
        TokenKind::Try => "try",
        TokenKind::Catch => "catch",
        TokenKind::Do => "do",
        TokenKind::End => "end",
        TokenKind::Case => "case",
        TokenKind::Of => "of",
        TokenKind::When => "when",
        TokenKind::If => "if",
        TokenKind::Then => "then",
        TokenKind::ElseIf => "elseif",
        TokenKind::Else => "else",
        TokenKind::Let => "let",
        TokenKind::Tap => "tap",
        TokenKind::Nil => "nil",
        TokenKind::True => "true",
        TokenKind::False => "false",
        TokenKind::And => "and",
        TokenKind::Or => "or",
        TokenKind::Not => "not",
        TokenKind::LParen => "(",
        TokenKind::RParen => ")",
        TokenKind::LBracket => "[",
        TokenKind::RBracket => "]",
        TokenKind::LBrace => "{",
        TokenKind::RBrace => "}",
        TokenKind::Comma => ",",
        TokenKind::Dot => ".",
        TokenKind::DotDot => "..",
        TokenKind::Equal => "=",
        TokenKind::EqualEqual => "==",
        TokenKind::BangEqual => "!=",
        TokenKind::Plus => "+",
        TokenKind::Minus => "-",
        TokenKind::Star => "*",
        TokenKind::Slash => "/",
        TokenKind::SlashSlash => "//",
        TokenKind::Percent => "%",
        TokenKind::Less => "<",
        TokenKind::LessEqual => "<=",
        TokenKind::Greater => ">",
        TokenKind::GreaterEqual => ">=",
        TokenKind::PipeGreater => "|>",
        TokenKind::LessPipe => "<|",
        TokenKind::Eof => "end of file",
    }
}
