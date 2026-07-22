pub use simi_syntax::lexer::{LexError, Token, TokenKind, lex};

#[cfg(test)]
#[path = "lexer/tests.rs"]
mod tests;
