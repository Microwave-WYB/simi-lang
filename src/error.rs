use std::error::Error;
use std::fmt;

use crate::lexer::LexError;
use crate::parser::ParseError;
use crate::runtime::RuntimeError;
use crate::span::Span;

#[derive(Debug)]
pub enum SimiError {
    Lex(LexError),
    Parse(ParseError),
    Runtime(RuntimeError),
}

impl SimiError {
    pub const fn span(&self) -> Span {
        match self {
            Self::Lex(error) => error.span,
            Self::Parse(error) => error.span,
            Self::Runtime(error) => error.span,
        }
    }
}

impl fmt::Display for SimiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => error.fmt(formatter),
            Self::Parse(error) => error.fmt(formatter),
            Self::Runtime(error) => error.fmt(formatter),
        }
    }
}

impl Error for SimiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Lex(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::Runtime(error) => Some(error),
        }
    }
}

impl From<LexError> for SimiError {
    fn from(error: LexError) -> Self {
        Self::Lex(error)
    }
}

impl From<ParseError> for SimiError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<RuntimeError> for SimiError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}
