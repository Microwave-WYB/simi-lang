use std::collections::HashSet;

use super::{ParseError, Parser, SimpleToken};
use crate::ast::{Expr, ExprKind, MatchCase};
use crate::lexer::TokenKind;
use crate::span::Span;

impl Parser {
    pub(super) fn parse_raise(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_simple(SimpleToken::Raise, "`raise`")?;
        let value = self.parse_expression()?;
        let span = start.merge(value.span);
        Ok(Expr {
            kind: ExprKind::Raise {
                value: Box::new(value),
            },
            span,
        })
    }

    pub(super) fn parse_try(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_simple(SimpleToken::Try, "`try`")?;
        let protected = self.parse_expression()?;
        self.expect_simple(SimpleToken::Catch, "`catch` after protected expression")?;

        if !self.at_simple(SimpleToken::Case) {
            return Err(self.error_current(format!(
                "expected `case` after `catch`, found `{}`",
                self.current_name()
            )));
        }

        let cases = self.parse_cases("`->` after catch case")?;
        let end = self.expect_simple(SimpleToken::End, "`end` after try expression")?;
        Ok(Expr {
            kind: ExprKind::Try {
                protected: Box::new(protected),
                cases,
            },
            span: start.merge(end),
        })
    }

    pub(super) fn parse_match(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_simple(SimpleToken::Match, "`match`")?;
        let value = self.parse_expression()?;
        self.expect_simple(SimpleToken::With, "`with` after match value")?;

        if !self.at_simple(SimpleToken::Case) {
            return Err(self.error_current(format!(
                "expected `case` after `with`, found `{}`",
                self.current_name()
            )));
        }

        let cases = self.parse_cases("`->` after match case")?;
        let end = self.expect_simple(SimpleToken::End, "`end` after match expression")?;
        Ok(Expr {
            kind: ExprKind::Match {
                value: Box::new(value),
                cases,
            },
            span: start.merge(end),
        })
    }

    fn parse_cases(&mut self, arrow_description: &str) -> Result<Vec<MatchCase>, ParseError> {
        let mut cases = Vec::new();
        while self.consume_simple(SimpleToken::Case) {
            let mut bindings = HashSet::new();
            let pattern = self.parse_pattern(&mut bindings)?;
            let guard = if self.consume_simple(SimpleToken::When) {
                Some(self.parse_expression()?)
            } else {
                None
            };
            let arrow = self.expect_simple(SimpleToken::Arrow, arrow_description)?;
            let body = self.parse_block(arrow.end)?;
            cases.push(MatchCase {
                pattern,
                guard,
                body,
            });
        }
        Ok(cases)
    }

    pub(super) fn parse_loop(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_simple(SimpleToken::Loop, "`loop`")?;
        let (state, initial, do_span) = if self.at_simple(SimpleToken::Do) {
            let initial = Expr {
                kind: ExprKind::Nil,
                span: Span::new(start.end, start.end),
            };
            let do_span = self.advance_span();
            ("_".to_owned(), initial, do_span)
        } else {
            let (state, _) = self.expect_ident("loop state name")?;
            self.expect_simple(SimpleToken::Equal, "`=` after loop state name")?;
            let initial = self.parse_expression()?;
            let do_span = self.expect_simple(SimpleToken::Do, "`do` before loop body")?;
            (state, initial, do_span)
        };

        self.loop_depth += 1;
        let body = self.parse_block(do_span.end);
        self.loop_depth -= 1;
        let body = body?;

        let end = self.expect_simple(SimpleToken::End, "`end` after loop body")?;
        Ok(Expr {
            kind: ExprKind::Loop {
                state,
                initial: Box::new(initial),
                body,
            },
            span: start.merge(end),
        })
    }

    pub(super) fn parse_continue(&mut self) -> Result<Expr, ParseError> {
        let keyword = self.current().span;
        if self.loop_depth == 0 {
            return Err(ParseError {
                span: keyword,
                message: "`continue` outside of a loop".to_owned(),
            });
        }
        self.advance_span();

        let (value, span) = if self.can_begin_expression() {
            let value = self.parse_expression()?;
            let span = keyword.merge(value.span);
            (value, span)
        } else {
            (
                Expr {
                    kind: ExprKind::Nil,
                    span: Span::new(keyword.end, keyword.end),
                },
                keyword,
            )
        };
        Ok(Expr {
            kind: ExprKind::Continue {
                value: Box::new(value),
            },
            span,
        })
    }

    pub(super) fn parse_break(&mut self) -> Result<Expr, ParseError> {
        let keyword = self.current().span;
        if self.loop_depth == 0 {
            return Err(ParseError {
                span: keyword,
                message: "`break` outside of a loop".to_owned(),
            });
        }
        self.advance_span();

        let value = self.parse_expression()?;
        let span = keyword.merge(value.span);
        Ok(Expr {
            kind: ExprKind::Break {
                value: Box::new(value),
            },
            span,
        })
    }

    fn can_begin_expression(&self) -> bool {
        matches!(
            &self.current().kind,
            TokenKind::Int(_)
                | TokenKind::Float(_)
                | TokenKind::String(_)
                | TokenKind::Ident(_)
                | TokenKind::Nil
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Minus
                | TokenKind::Not
                | TokenKind::LParen
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::Raise
                | TokenKind::Try
                | TokenKind::Catch
                | TokenKind::Match
                | TokenKind::If
                | TokenKind::Loop
                | TokenKind::Break
                | TokenKind::Continue
        )
    }

    pub(super) fn parse_if(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_simple(SimpleToken::If, "`if`")?;
        let condition = self.parse_expression()?;
        let then = self.expect_simple(SimpleToken::Then, "`then` after if condition")?;
        let body = self.parse_block(then.end)?;
        let mut branches = vec![(condition, body)];

        while self.consume_simple(SimpleToken::ElseIf) {
            let condition = self.parse_expression()?;
            let then = self.expect_simple(SimpleToken::Then, "`then` after elseif condition")?;
            let body = self.parse_block(then.end)?;
            branches.push((condition, body));
        }

        let else_branch = if self.consume_simple(SimpleToken::Else) {
            let offset = self.previous_span().end;
            Some(self.parse_block(offset)?)
        } else {
            None
        };
        let end = self.expect_simple(SimpleToken::End, "`end` after if expression")?;
        Ok(Expr {
            kind: ExprKind::If {
                branches,
                else_branch,
            },
            span: start.merge(end),
        })
    }
}
