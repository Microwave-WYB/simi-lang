use std::collections::HashSet;

use super::{ParseError, Parser, SimpleToken};
use crate::ast::{Expr, ExprKind, PatternClause};
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
        let clauses = self.parse_pattern_clauses("catch")?;
        let end = self.expect_simple(SimpleToken::End, "`end` after try expression")?;
        Ok(Expr {
            kind: ExprKind::Try {
                protected: Box::new(protected),
                clauses,
            },
            span: start.merge(end),
        })
    }

    pub(super) fn parse_case(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_simple(SimpleToken::Case, "`case`")?;
        let value = self.parse_expression()?;
        self.expect_simple(SimpleToken::Of, "`of` after case value")?;
        let clauses = self.parse_pattern_clauses("case")?;
        let end = self.expect_simple(SimpleToken::End, "`end` after case expression")?;
        Ok(Expr {
            kind: ExprKind::Case {
                value: Box::new(value),
                clauses,
            },
            span: start.merge(end),
        })
    }

    fn parse_pattern_clauses(&mut self, construct: &str) -> Result<Vec<PatternClause>, ParseError> {
        if self.at_simple(SimpleToken::End) {
            return Err(
                self.error_current(format!("expected pattern after `{construct}`, found `end`"))
            );
        }

        let mut clauses = Vec::new();
        while !self.at_simple(SimpleToken::End) && !self.at_eof() {
            let mut bindings = HashSet::new();
            let pattern = self.parse_pattern(&mut bindings)?;
            let guard = if self.consume_simple(SimpleToken::When) {
                Some(self.parse_expression()?)
            } else {
                None
            };
            let do_span = self.expect_simple(SimpleToken::Do, "`do` before clause body")?;
            let body = self.parse_block(do_span.end)?;
            self.expect_simple(SimpleToken::End, "`end` after clause body")?;
            clauses.push(PatternClause {
                pattern,
                guard,
                body,
            });
        }
        Ok(clauses)
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
                | TokenKind::Case
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
