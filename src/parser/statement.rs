use std::collections::HashSet;

use super::{ParseError, Parser, SimpleToken};
use crate::ast::{Block, Stmt, StmtKind};
use crate::lexer::TokenKind;
use crate::span::Span;

impl Parser {
    pub(super) fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match &self.current().kind {
            TokenKind::Fn if !self.next_is_simple(SimpleToken::LParen) => {
                self.parse_function_declaration()
            }
            TokenKind::Let => self.parse_let(),
            _ => {
                let expression = self.parse_expression()?;
                let span = expression.span;
                Ok(Stmt {
                    kind: StmtKind::Expr(expression),
                    span,
                })
            }
        }
    }

    fn parse_function_declaration(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect_simple(SimpleToken::Fn, "`fn`")?;
        let (name, _) = self.expect_ident("function name")?;
        let (params, body, end) = self.parse_function_parts("`(` after function name")?;

        Ok(Stmt {
            kind: StmtKind::Function { name, params, body },
            span: start.merge(end),
        })
    }

    pub(super) fn parse_function_parts(
        &mut self,
        open_description: &str,
    ) -> Result<(Vec<String>, Block, Span), ParseError> {
        self.expect_simple(SimpleToken::LParen, open_description)?;

        let mut params = Vec::new();
        let mut seen = HashSet::new();
        if !self.at_simple(SimpleToken::RParen) {
            loop {
                let (parameter, span) = self.expect_ident("parameter name")?;
                if !seen.insert(parameter.clone()) {
                    return Err(ParseError {
                        span,
                        message: format!("duplicate parameter `{parameter}`"),
                    });
                }
                params.push(parameter);
                if !self.consume_simple(SimpleToken::Comma) || self.at_simple(SimpleToken::RParen) {
                    break;
                }
            }
        }
        self.expect_simple(SimpleToken::RParen, "`)` after parameters")?;
        let do_span = self.expect_simple(SimpleToken::Do, "`do` before function body")?;

        let enclosing_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let body = self.parse_block(do_span.end);
        self.loop_depth = enclosing_loop_depth;
        let body = body?;

        let end = self.expect_simple(SimpleToken::End, "`end` after function body")?;
        Ok((params, body, end))
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        let start = self.expect_simple(SimpleToken::Let, "`let`")?;
        let (name, _) = self.expect_ident("name after `let`")?;
        self.expect_simple(SimpleToken::Equal, "`=` after let binding name")?;
        let value = self.parse_expression()?;
        let span = start.merge(value.span);
        Ok(Stmt {
            kind: StmtKind::Let { name, value },
            span,
        })
    }

    pub(super) fn parse_block(&mut self, empty_offset: usize) -> Result<Block, ParseError> {
        let mut items = Vec::new();
        while !self.at_eof() && !self.at_block_terminator() {
            items.push(self.parse_stmt()?);
        }

        let span = match (items.first(), items.last()) {
            (Some(first), Some(last)) => first.span.merge(last.span),
            _ => Span::new(empty_offset, empty_offset),
        };
        Ok(Block { items, span })
    }
}
