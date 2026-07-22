use std::fmt;

use crate::lexer::{LexError, Lexeme, Token, TokenKind, lex_lossless};
use crate::span::Span;
use crate::syntax::{SyntaxKind, SyntaxNode};

mod event;
mod grammar;

use event::{Event, Marker};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    Lex,
    Parse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxDiagnostic {
    pub kind: DiagnosticKind,
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct Parse {
    syntax: SyntaxNode,
    diagnostics: Vec<SyntaxDiagnostic>,
    token_origins: Option<TokenOrigins>,
}

#[derive(Clone, Debug)]
pub struct TokenOrigins {
    entries: Vec<TokenOrigin>,
    eof_span: Span,
}

#[derive(Clone, Debug)]
struct TokenOrigin {
    synthetic: Span,
    original: Span,
}

impl TokenOrigins {
    /// Translate a range in the normalized token-vector CST back to the
    /// caller-supplied token spans, merging the first and last origins.
    pub fn rebase(&self, span: Span) -> Span {
        if span.start == span.end {
            let mut empty_origins = self.entries.iter().filter(|entry| {
                entry.synthetic.start == span.start && entry.synthetic.end == span.end
            });
            if let Some(first) = empty_origins.next() {
                return empty_origins
                    .fold(first.original, |merged, entry| merged.merge(entry.original));
            }
            if let Some(entry) = self
                .entries
                .iter()
                .rev()
                .find(|entry| entry.synthetic.end == span.end)
            {
                return Span::new(entry.original.end, entry.original.end);
            }
            if let Some(entry) = self
                .entries
                .iter()
                .find(|entry| entry.synthetic.start == span.start)
            {
                return Span::new(entry.original.start, entry.original.start);
            }
            return self.eof_span;
        }

        let first = self
            .entries
            .iter()
            .find(|entry| entry.synthetic.start >= span.start && entry.synthetic.start < span.end);
        let last = self
            .entries
            .iter()
            .rev()
            .find(|entry| entry.synthetic.end > span.start && entry.synthetic.end <= span.end);
        match (first, last) {
            (Some(first), Some(last)) => first.original.merge(last.original),
            (Some(entry), None) | (None, Some(entry)) => entry.original,
            (None, None) => self.eof_span,
        }
    }
}

impl Parse {
    pub fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
    pub fn diagnostics(&self) -> &[SyntaxDiagnostic] {
        &self.diagnostics
    }
    pub fn into_syntax(self) -> SyntaxNode {
        self.syntax
    }
    pub fn ok(self) -> Result<SyntaxNode, SyntaxDiagnostic> {
        self.ok_with_origins().map(|(syntax, _)| syntax)
    }
    pub fn ok_with_origins(self) -> Result<(SyntaxNode, Option<TokenOrigins>), SyntaxDiagnostic> {
        match self.diagnostics.first().cloned() {
            Some(error) => Err(error),
            None => Ok((self.syntax, self.token_origins)),
        }
    }
}

pub fn parse_source(source: &str) -> Parse {
    let (lexemes, lex_errors) = lex_lossless(source);
    parse_lexemes(
        lexemes,
        lex_errors,
        Span::new(source.len(), source.len()),
        false,
    )
}

/// Parse a public token vector in vector order, independent of its source spans.
///
/// The Rowan tree uses normalized token spellings and therefore has contiguous
/// ranges. [`Parse::ok_with_origins`] returns the origin map used by semantic
/// lowering to restore gapped, overlapping, or unsorted caller-supplied spans.
pub fn parse_tokens(tokens: Vec<Token>) -> Parse {
    let (lexemes, eof_span) = adapt_tokens(tokens);
    parse_lexemes(lexemes, Vec::new(), eof_span, true)
}

fn parse_lexemes(
    lexemes: Vec<Lexeme>,
    lex_errors: Vec<LexError>,
    eof_span: Span,
    retain_origins: bool,
) -> Parse {
    let mut parser = Parser::new(&lexemes, eof_span);
    grammar::root(&mut parser);
    let parse_errors = parser.diagnostics;
    let syntax = event::build(parser.events, &lexemes);
    let token_origins = retain_origins.then(|| token_origins(&lexemes, eof_span));
    let mut diagnostics = lex_errors
        .into_iter()
        .map(|error| SyntaxDiagnostic {
            kind: DiagnosticKind::Lex,
            span: error.span,
            message: error.message,
        })
        .collect::<Vec<_>>();
    diagnostics.extend(parse_errors);
    Parse {
        syntax,
        diagnostics,
        token_origins,
    }
}

struct Parser<'a> {
    lexemes: &'a [Lexeme],
    eof_span: Span,
    position: usize,
    events: Vec<Event>,
    diagnostics: Vec<SyntaxDiagnostic>,
    loop_depth: usize,
    standalone_block_depth: usize,
}

impl<'a> Parser<'a> {
    fn new(lexemes: &'a [Lexeme], eof_span: Span) -> Self {
        Self {
            lexemes,
            eof_span,
            position: 0,
            events: Vec::new(),
            diagnostics: Vec::new(),
            loop_depth: 0,
            standalone_block_depth: 0,
        }
    }

    fn start(&mut self) -> Marker {
        self.eat_trivia();
        event::start(&mut self.events)
    }
    fn start_root(&mut self) -> Marker {
        event::start(&mut self.events)
    }

    fn current(&self) -> SyntaxKind {
        self.nth(0)
    }
    fn nth(&self, n: usize) -> SyntaxKind {
        let mut seen = 0;
        for lexeme in &self.lexemes[self.position..] {
            if lexeme.kind.is_trivia() {
                continue;
            }
            if seen == n {
                return lexeme.kind;
            }
            seen += 1;
        }
        SyntaxKind::ERROR_TOKEN
    }
    fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }
    fn at_end(&self) -> bool {
        self.nontrivia_index().is_none()
    }

    fn nontrivia_index(&self) -> Option<usize> {
        (self.position..self.lexemes.len()).find(|&index| !self.lexemes[index].kind.is_trivia())
    }

    fn current_is_lexically_adjacent(&self) -> bool {
        self.nontrivia_index() == Some(self.position)
    }

    fn eat_trivia(&mut self) {
        while self.position < self.lexemes.len() && self.lexemes[self.position].kind.is_trivia() {
            self.events.push(Event::Token(self.position));
            self.position += 1;
        }
    }

    fn bump(&mut self) {
        self.eat_trivia();
        if self.position < self.lexemes.len() {
            self.events.push(Event::Token(self.position));
            self.position += 1;
        }
    }

    fn bump_if(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: SyntaxKind, expected: &str) -> bool {
        if self.bump_if(kind) {
            true
        } else {
            self.error(format!(
                "expected {expected}, found `{}`",
                token_name(self.current(), self.at_end())
            ));
            false
        }
    }

    fn error(&mut self, message: String) {
        self.diagnostics.push(SyntaxDiagnostic {
            kind: DiagnosticKind::Parse,
            span: self.current_span(),
            message,
        });
    }

    fn error_at(&mut self, span: Span, message: String) {
        self.diagnostics.push(SyntaxDiagnostic {
            kind: DiagnosticKind::Parse,
            span,
            message,
        });
    }

    fn current_span(&self) -> Span {
        self.nontrivia_index()
            .map_or(self.eof_span, |index| self.lexemes[index].span)
    }

    fn current_text(&self) -> Option<&str> {
        self.nontrivia_index()
            .map(|index| self.lexemes[index].text.as_str())
    }

    fn previous_nontrivia_span(&self) -> Span {
        self.lexemes[..self.position]
            .iter()
            .rev()
            .find(|lexeme| !lexeme.kind.is_trivia())
            .map_or(Span::new(0, 0), |lexeme| lexeme.span)
    }
}

fn token_name(kind: SyntaxKind, eof: bool) -> &'static str {
    if eof {
        return "end of file";
    }
    match kind {
        SyntaxKind::INT => "integer",
        SyntaxKind::FLOAT => "float",
        SyntaxKind::STRING => "string",
        SyntaxKind::IDENT => "identifier",
        SyntaxKind::FN_KW => "fn",
        SyntaxKind::DO_KW => "do",
        SyntaxKind::END_KW => "end",
        SyntaxKind::IF_KW => "if",
        SyntaxKind::THEN_KW => "then",
        SyntaxKind::ELSEIF_KW => "elseif",
        SyntaxKind::ELSE_KW => "else",
        SyntaxKind::LET_KW => "let",
        SyntaxKind::TAP_KW => "tap",
        SyntaxKind::NIL_KW => "nil",
        SyntaxKind::TRUE_KW => "true",
        SyntaxKind::FALSE_KW => "false",
        SyntaxKind::AND_KW => "and",
        SyntaxKind::OR_KW => "or",
        SyntaxKind::NOT_KW => "not",
        SyntaxKind::LOOP_KW => "loop",
        SyntaxKind::BREAK_KW => "break",
        SyntaxKind::CONTINUE_KW => "continue",
        SyntaxKind::CASE_KW => "case",
        SyntaxKind::OF_KW => "of",
        SyntaxKind::WHEN_KW => "when",
        SyntaxKind::RAISE_KW => "raise",
        SyntaxKind::TRY_KW => "try",
        SyntaxKind::CATCH_KW => "catch",
        SyntaxKind::L_PAREN => "(",
        SyntaxKind::R_PAREN => ")",
        SyntaxKind::L_BRACKET => "[",
        SyntaxKind::R_BRACKET => "]",
        SyntaxKind::L_BRACE => "{",
        SyntaxKind::R_BRACE => "}",
        SyntaxKind::COMMA => ",",
        SyntaxKind::DOT => ".",
        SyntaxKind::DOT_DOT => "..",
        SyntaxKind::EQ => "=",
        SyntaxKind::EQ_EQ => "==",
        SyntaxKind::BANG_EQ => "!=",
        SyntaxKind::PLUS => "+",
        SyntaxKind::MINUS => "-",
        SyntaxKind::STAR => "*",
        SyntaxKind::SLASH => "/",
        SyntaxKind::SLASH_SLASH => "//",
        SyntaxKind::PERCENT => "%",
        SyntaxKind::LESS => "<",
        SyntaxKind::LESS_EQ => "<=",
        SyntaxKind::GREATER => ">",
        SyntaxKind::GREATER_EQ => ">=",
        SyntaxKind::QUESTION => "?",
        SyntaxKind::QUESTION_GREATER => "?>",
        SyntaxKind::PIPE_GREATER => "|>",
        SyntaxKind::LESS_PIPE => "<|",
        _ => "invalid token",
    }
}

fn adapt_tokens(tokens: Vec<Token>) -> (Vec<Lexeme>, Span) {
    let fallback_eof = tokens.last().map_or(Span::new(0, 0), |token| {
        Span::new(token.span.end, token.span.end)
    });
    let mut lexemes = Vec::new();
    let mut eof_span = fallback_eof;
    for token in tokens {
        if matches!(token.kind, TokenKind::Eof) {
            eof_span = token.span;
            break;
        }
        let (kind, text) = token_lexeme(token.kind);
        lexemes.push(Lexeme {
            kind,
            text,
            span: token.span,
        });
    }
    (lexemes, eof_span)
}

fn token_origins(lexemes: &[Lexeme], eof_span: Span) -> TokenOrigins {
    let mut offset = 0;
    let entries = lexemes
        .iter()
        .map(|lexeme| {
            let start = offset;
            offset += lexeme.text.len();
            TokenOrigin {
                synthetic: Span::new(start, offset),
                original: lexeme.span,
            }
        })
        .collect();
    TokenOrigins { entries, eof_span }
}

fn token_lexeme(kind: TokenKind) -> (SyntaxKind, String) {
    let (syntax, text) = match kind {
        TokenKind::Ident(value) => return (SyntaxKind::IDENT, value),
        TokenKind::Int(value) => return (SyntaxKind::INT, value.to_string()),
        TokenKind::Float(value) => return (SyntaxKind::FLOAT, value.to_string()),
        TokenKind::String(value) => return (SyntaxKind::STRING, quote_string(&value)),
        TokenKind::Fn => (SyntaxKind::FN_KW, "fn"),
        TokenKind::Do => (SyntaxKind::DO_KW, "do"),
        TokenKind::End => (SyntaxKind::END_KW, "end"),
        TokenKind::If => (SyntaxKind::IF_KW, "if"),
        TokenKind::Then => (SyntaxKind::THEN_KW, "then"),
        TokenKind::ElseIf => (SyntaxKind::ELSEIF_KW, "elseif"),
        TokenKind::Else => (SyntaxKind::ELSE_KW, "else"),
        TokenKind::Let => (SyntaxKind::LET_KW, "let"),
        TokenKind::Tap => (SyntaxKind::TAP_KW, "tap"),
        TokenKind::Nil => (SyntaxKind::NIL_KW, "nil"),
        TokenKind::True => (SyntaxKind::TRUE_KW, "true"),
        TokenKind::False => (SyntaxKind::FALSE_KW, "false"),
        TokenKind::And => (SyntaxKind::AND_KW, "and"),
        TokenKind::Or => (SyntaxKind::OR_KW, "or"),
        TokenKind::Not => (SyntaxKind::NOT_KW, "not"),
        TokenKind::Loop => (SyntaxKind::LOOP_KW, "loop"),
        TokenKind::Break => (SyntaxKind::BREAK_KW, "break"),
        TokenKind::Continue => (SyntaxKind::CONTINUE_KW, "continue"),
        TokenKind::Case => (SyntaxKind::CASE_KW, "case"),
        TokenKind::Of => (SyntaxKind::OF_KW, "of"),
        TokenKind::When => (SyntaxKind::WHEN_KW, "when"),
        TokenKind::Raise => (SyntaxKind::RAISE_KW, "raise"),
        TokenKind::Try => (SyntaxKind::TRY_KW, "try"),
        TokenKind::Catch => (SyntaxKind::CATCH_KW, "catch"),
        TokenKind::LParen => (SyntaxKind::L_PAREN, "("),
        TokenKind::RParen => (SyntaxKind::R_PAREN, ")"),
        TokenKind::LBracket => (SyntaxKind::L_BRACKET, "["),
        TokenKind::RBracket => (SyntaxKind::R_BRACKET, "]"),
        TokenKind::LBrace => (SyntaxKind::L_BRACE, "{"),
        TokenKind::RBrace => (SyntaxKind::R_BRACE, "}"),
        TokenKind::Comma => (SyntaxKind::COMMA, ","),
        TokenKind::Dot => (SyntaxKind::DOT, "."),
        TokenKind::DotDot => (SyntaxKind::DOT_DOT, ".."),
        TokenKind::Equal => (SyntaxKind::EQ, "="),
        TokenKind::EqualEqual => (SyntaxKind::EQ_EQ, "=="),
        TokenKind::BangEqual => (SyntaxKind::BANG_EQ, "!="),
        TokenKind::Plus => (SyntaxKind::PLUS, "+"),
        TokenKind::Minus => (SyntaxKind::MINUS, "-"),
        TokenKind::Star => (SyntaxKind::STAR, "*"),
        TokenKind::Slash => (SyntaxKind::SLASH, "/"),
        TokenKind::SlashSlash => (SyntaxKind::SLASH_SLASH, "//"),
        TokenKind::Percent => (SyntaxKind::PERCENT, "%"),
        TokenKind::Less => (SyntaxKind::LESS, "<"),
        TokenKind::LessEqual => (SyntaxKind::LESS_EQ, "<="),
        TokenKind::Greater => (SyntaxKind::GREATER, ">"),
        TokenKind::GreaterEqual => (SyntaxKind::GREATER_EQ, ">="),
        TokenKind::Question => (SyntaxKind::QUESTION, "?"),
        TokenKind::QuestionGreater => (SyntaxKind::QUESTION_GREATER, "?>"),
        TokenKind::PipeGreater => (SyntaxKind::PIPE_GREATER, "|>"),
        TokenKind::LessPipe => (SyntaxKind::LESS_PIPE, "<|"),
        TokenKind::Eof => unreachable!("EOF tokens are handled before adaptation"),
    };
    (syntax, text.to_owned())
}

fn quote_string(value: &str) -> String {
    let body = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{body}\"")
}

impl fmt::Display for SyntaxDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
