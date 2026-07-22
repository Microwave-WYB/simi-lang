pub mod ast;
pub mod generated;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod syntax;

pub use parser::{DiagnosticKind, Parse, SyntaxDiagnostic, parse_source, parse_tokens};
pub use syntax::{SimiLanguage, SyntaxKind, SyntaxNode, SyntaxToken};
