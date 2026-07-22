mod scanner;
mod token;

pub use scanner::{Lexeme, lex_lossless};
pub use token::{LexError, Token, TokenKind};

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    scanner::lex(source)
}

/// Return whether `text` is a valid non-keyword Simi identifier.
pub fn is_identifier(text: &str) -> bool {
    let mut bytes = text.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_'))
        && !scanner::is_keyword(text)
}
