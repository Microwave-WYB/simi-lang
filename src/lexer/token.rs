use std::fmt;

use crate::span::Span;

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Int(i64),
    Float(f64),
    String(String),
    Ident(String),
    Fn,
    Do,
    End,
    If,
    Then,
    ElseIf,
    Else,
    Let,
    Tap,
    Nil,
    True,
    False,
    And,
    Or,
    Not,
    Is,
    Loop,
    Break,
    Continue,
    Match,
    With,
    Case,
    When,
    Raise,
    Try,
    Catch,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
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
    Arrow,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    PipeGreater,
    LessPipe,
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    pub span: Span,
    pub message: String,
}

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for LexError {}
