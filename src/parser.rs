use std::error::Error;
use std::fmt;

use crate::ast::Program;
use crate::lexer::{Token, TokenKind};
use crate::span::Span;

mod control;
mod cursor;
mod expression;
mod pattern;
mod statement;

use cursor::SimpleToken;

#[derive(Clone, Debug)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for ParseError {}

pub struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    loop_depth: usize,
    standalone_block_depth: usize,
}

impl Parser {
    pub fn new(mut tokens: Vec<Token>) -> Self {
        let has_eof = matches!(tokens.last().map(|token| &token.kind), Some(TokenKind::Eof));
        if !has_eof {
            let offset = tokens.last().map_or(0, |token| token.span.end);
            tokens.push(Token {
                kind: TokenKind::Eof,
                span: Span::new(offset, offset),
            });
        }
        Self {
            tokens,
            cursor: 0,
            loop_depth: 0,
            standalone_block_depth: 0,
        }
    }

    pub fn parse_program(mut self) -> Result<Program, ParseError> {
        let mut items = Vec::new();
        while !self.at_eof() {
            if self.at_block_terminator() {
                return Err(self.error_current(format!(
                    "unexpected `{}` outside of a block",
                    self.current_name()
                )));
            }
            items.push(self.parse_stmt()?);
        }
        Ok(Program { items })
    }
}

pub fn parse(tokens: Vec<Token>) -> Result<Program, ParseError> {
    Parser::new(tokens).parse_program()
}

#[cfg(test)]
mod tests;
