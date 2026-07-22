use std::{error::Error, fmt};

use crate::ast::Program;
use crate::lexer::Token;
use crate::span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl Error for ParseError {}

pub struct Parser {
    tokens: Vec<Token>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens }
    }
    pub fn parse_program(self) -> Result<Program, ParseError> {
        parse(self.tokens)
    }
}

pub fn parse(tokens: Vec<Token>) -> Result<Program, ParseError> {
    let parsed = simi_syntax::parse_tokens(tokens);
    let syntax = parsed.ok().map_err(diagnostic_to_error)?;
    Ok(crate::lower::program(syntax))
}

pub(crate) fn parse_source(source: &str) -> Result<Program, simi_syntax::SyntaxDiagnostic> {
    simi_syntax::parse_source(source)
        .ok()
        .map(crate::lower::program)
}

fn diagnostic_to_error(diagnostic: simi_syntax::SyntaxDiagnostic) -> ParseError {
    ParseError {
        span: diagnostic.span,
        message: diagnostic.message,
    }
}

#[cfg(test)]
#[path = "parser/tests.rs"]
mod tests;
