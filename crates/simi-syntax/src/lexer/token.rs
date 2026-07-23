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
    Alias,
    Tap,
    Nil,
    True,
    False,
    And,
    Or,
    Not,
    Loop,
    Break,
    Continue,
    Case,
    Of,
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
    Colon,
    Apostrophe,
    Arrow,
    Pipe,
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
    Less,
    LessEqual,
    LessGreater,
    Greater,
    GreaterEqual,
    Question,
    QuestionGreater,
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
