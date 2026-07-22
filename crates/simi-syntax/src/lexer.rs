mod scanner;
mod token;

pub use scanner::{Lexeme, lex_lossless};
pub use token::{LexError, Token, TokenKind};

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    scanner::lex(source)
}
