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
        match self.diagnostics.first().cloned() {
            Some(error) => Err(error),
            None => Ok(self.syntax),
        }
    }
}

pub fn parse_source(source: &str) -> Parse {
    let (lexemes, lex_errors) = lex_lossless(source);
    parse_lexemes(lexemes, lex_errors, source.len())
}

pub fn parse_tokens(tokens: Vec<Token>) -> Parse {
    let source = reconstruct_source(&tokens);
    parse_source(&source)
}

fn parse_lexemes(lexemes: Vec<Lexeme>, lex_errors: Vec<LexError>, source_len: usize) -> Parse {
    let mut parser = Parser::new(&lexemes, source_len);
    grammar::root(&mut parser);
    let parse_errors = parser.diagnostics;
    let syntax = event::build(parser.events, &lexemes);
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
    }
}

struct Parser<'a> {
    lexemes: &'a [Lexeme],
    source_len: usize,
    position: usize,
    events: Vec<Event>,
    diagnostics: Vec<SyntaxDiagnostic>,
    loop_depth: usize,
    standalone_block_depth: usize,
}

impl<'a> Parser<'a> {
    fn new(lexemes: &'a [Lexeme], source_len: usize) -> Self {
        Self {
            lexemes,
            source_len,
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
            .map_or(Span::new(self.source_len, self.source_len), |index| {
                self.lexemes[index].span
            })
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

fn reconstruct_source(tokens: &[Token]) -> String {
    let length = tokens.iter().map(|token| token.span.end).max().unwrap_or(0);
    let mut bytes = vec![b' '; length];
    for token in tokens {
        if matches!(token.kind, TokenKind::Eof) {
            continue;
        }
        let width = token.span.end.saturating_sub(token.span.start);
        let text = render_token(&token.kind, width);
        if text.len() == width && token.span.end <= bytes.len() {
            bytes[token.span.start..token.span.end].copy_from_slice(text.as_bytes());
        }
    }
    String::from_utf8(bytes).expect("reconstructed token source is ASCII except escaped strings")
}

fn render_token(kind: &TokenKind, width: usize) -> String {
    let fixed = match kind {
        TokenKind::Ident(value) => return value.clone(),
        TokenKind::Int(value) => {
            let text = value.to_string();
            return format!("{}{}", "0".repeat(width.saturating_sub(text.len())), text);
        }
        TokenKind::Float(value) => return fit_float(*value, width),
        TokenKind::String(value) => return fit_string(value, width),
        TokenKind::Fn => "fn",
        TokenKind::Do => "do",
        TokenKind::End => "end",
        TokenKind::If => "if",
        TokenKind::Then => "then",
        TokenKind::ElseIf => "elseif",
        TokenKind::Else => "else",
        TokenKind::Let => "let",
        TokenKind::Tap => "tap",
        TokenKind::Nil => "nil",
        TokenKind::True => "true",
        TokenKind::False => "false",
        TokenKind::And => "and",
        TokenKind::Or => "or",
        TokenKind::Not => "not",
        TokenKind::Loop => "loop",
        TokenKind::Break => "break",
        TokenKind::Continue => "continue",
        TokenKind::Case => "case",
        TokenKind::Of => "of",
        TokenKind::When => "when",
        TokenKind::Raise => "raise",
        TokenKind::Try => "try",
        TokenKind::Catch => "catch",
        TokenKind::LParen => "(",
        TokenKind::RParen => ")",
        TokenKind::LBracket => "[",
        TokenKind::RBracket => "]",
        TokenKind::LBrace => "{",
        TokenKind::RBrace => "}",
        TokenKind::Comma => ",",
        TokenKind::Dot => ".",
        TokenKind::DotDot => "..",
        TokenKind::Equal => "=",
        TokenKind::EqualEqual => "==",
        TokenKind::BangEqual => "!=",
        TokenKind::Plus => "+",
        TokenKind::Minus => "-",
        TokenKind::Star => "*",
        TokenKind::Slash => "/",
        TokenKind::SlashSlash => "//",
        TokenKind::Percent => "%",
        TokenKind::Less => "<",
        TokenKind::LessEqual => "<=",
        TokenKind::Greater => ">",
        TokenKind::GreaterEqual => ">=",
        TokenKind::Question => "?",
        TokenKind::QuestionGreater => "?>",
        TokenKind::PipeGreater => "|>",
        TokenKind::LessPipe => "<|",
        TokenKind::Eof => "",
    };
    fixed.to_owned()
}

fn fit_float(value: f64, width: usize) -> String {
    let mut plain = value.to_string();
    if !plain.contains(['.', 'e', 'E']) {
        plain.push_str(".0");
    }
    let scientific = format!("{value:e}");
    let mut text = [plain, scientific]
        .into_iter()
        .filter(|candidate| candidate.len() <= width)
        .min_by_key(String::len)
        .unwrap_or_else(|| value.to_string());
    if text.len() < width {
        text.insert_str(0, &"0".repeat(width - text.len()));
    }
    text
}
fn fit_string(value: &str, width: usize) -> String {
    let target = width.saturating_sub(2);
    let mut parts = vec![String::new()];
    for ch in value.chars() {
        let choices: &[&str] = match ch {
            '"' => &["\\\""],
            '\\' => &["\\\\"],
            '\n' => &["\n", "\\n"],
            '\r' => &["\r", "\\r"],
            '\t' => &["\t", "\\t"],
            _ => {
                parts.iter_mut().for_each(|part| part.push(ch));
                continue;
            }
        };
        let previous = std::mem::take(&mut parts);
        for prefix in previous {
            for choice in choices {
                let mut next = prefix.clone();
                next.push_str(choice);
                if next.len() <= target {
                    parts.push(next);
                }
            }
        }
    }
    let body = parts
        .into_iter()
        .find(|part| part.len() == target)
        .unwrap_or_else(|| value.replace('\\', "\\\\").replace('"', "\\\""));
    format!("\"{body}\"")
}

impl fmt::Display for SyntaxDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
