mod scanner;
mod token;

pub use token::{LexError, Token, TokenKind};

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    scanner::lex(source)
}

#[cfg(test)]
mod tests;
