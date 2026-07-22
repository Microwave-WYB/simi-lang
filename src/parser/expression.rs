use std::collections::HashSet;

use super::{ParseError, Parser, SimpleToken};
use crate::ast::{
    AssignmentTarget, AssignmentTargetKind, BinaryOp, Expr, ExprKind, PipelineStage, UnaryOp,
};
use crate::lexer::TokenKind;
use crate::span::Span;

impl Parser {
    pub(super) fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, ParseError> {
        let candidate = self.parse_pipeline()?;
        if !self.consume_simple(SimpleToken::Equal) {
            return Ok(candidate);
        }

        let span = candidate.span;
        let kind = match candidate.kind {
            ExprKind::Variable(name) => AssignmentTargetKind::Variable(name),
            ExprKind::Field { object, name } => AssignmentTargetKind::Field { object, name },
            ExprKind::Index { object, key } => AssignmentTargetKind::Index { object, key },
            _ => {
                return Err(ParseError {
                    span,
                    message: "invalid assignment target".to_owned(),
                });
            }
        };
        let value = self.parse_assignment()?;
        let expression_span = span.merge(value.span);
        Ok(Expr {
            kind: ExprKind::Assign {
                target: AssignmentTarget { kind, span },
                value: Box::new(value),
            },
            span: expression_span,
        })
    }

    fn parse_pipeline(&mut self) -> Result<Expr, ParseError> {
        let input = self.parse_or()?;
        let mut stages = Vec::new();

        while self.at_simple(SimpleToken::PipeGreater) {
            let pipe_span = self.advance_span();
            let tap = self.consume_simple(SimpleToken::Tap);
            let (callee, args, close_span) = self.parse_pipeline_call()?;
            stages.push(PipelineStage {
                tap,
                callee,
                args,
                span: pipe_span.merge(close_span),
            });
        }

        if stages.is_empty() {
            Ok(input)
        } else {
            let end = stages.last().expect("pipeline has a stage").span;
            let span = input.span.merge(end);
            Ok(Expr {
                kind: ExprKind::Pipeline {
                    input: Box::new(input),
                    stages,
                },
                span,
            })
        }
    }

    fn parse_pipeline_call(&mut self) -> Result<(Expr, Vec<Expr>, Span), ParseError> {
        let (name, span) = self.expect_ident("pipeline stage function name")?;
        let mut callee = Expr {
            kind: ExprKind::Variable(name),
            span,
        };

        while self.consume_simple(SimpleToken::Dot) {
            let (field, field_span) = self.expect_ident("field name after `.`")?;
            let full_span = callee.span.merge(field_span);
            callee = Expr {
                kind: ExprKind::Field {
                    object: Box::new(callee),
                    name: field,
                },
                span: full_span,
            };
        }

        self.expect_simple(SimpleToken::LParen, "`(` in pipeline stage call")?;
        let (args, close_span) = self.parse_arguments()?;
        Ok((callee, args, close_span))
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.parse_and()?;
        while self.consume_simple(SimpleToken::Or) {
            let right = self.parse_and()?;
            expression = binary(expression, BinaryOp::Or, right);
        }
        Ok(expression)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.parse_equality()?;
        while self.consume_simple(SimpleToken::And) {
            let right = self.parse_equality()?;
            expression = binary(expression, BinaryOp::And, right);
        }
        Ok(expression)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.parse_comparison()?;
        loop {
            let operation = if self.consume_simple(SimpleToken::EqualEqual) {
                Some(BinaryOp::Equal)
            } else if self.consume_simple(SimpleToken::BangEqual) {
                Some(BinaryOp::NotEqual)
            } else {
                None
            };
            let Some(operation) = operation else { break };
            let right = self.parse_comparison()?;
            expression = binary(expression, operation, right);
        }
        Ok(expression)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.parse_additive()?;
        loop {
            let operation = if self.consume_simple(SimpleToken::Less) {
                Some(BinaryOp::Less)
            } else if self.consume_simple(SimpleToken::LessEqual) {
                Some(BinaryOp::LessEqual)
            } else if self.consume_simple(SimpleToken::Greater) {
                Some(BinaryOp::Greater)
            } else if self.consume_simple(SimpleToken::GreaterEqual) {
                Some(BinaryOp::GreaterEqual)
            } else {
                None
            };
            let Some(operation) = operation else { break };
            let right = self.parse_additive()?;
            expression = binary(expression, operation, right);
        }
        Ok(expression)
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.parse_multiplicative()?;
        loop {
            let operation = if self.consume_simple(SimpleToken::Plus) {
                Some(BinaryOp::Add)
            } else if self.consume_simple(SimpleToken::Minus) {
                Some(BinaryOp::Subtract)
            } else {
                None
            };
            let Some(operation) = operation else { break };
            let right = self.parse_multiplicative()?;
            expression = binary(expression, operation, right);
        }
        Ok(expression)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.parse_unary()?;
        loop {
            let operation = if self.consume_simple(SimpleToken::Star) {
                Some(BinaryOp::Multiply)
            } else if self.consume_simple(SimpleToken::Slash) {
                Some(BinaryOp::Divide)
            } else if self.consume_simple(SimpleToken::SlashSlash) {
                Some(BinaryOp::FloorDivide)
            } else if self.consume_simple(SimpleToken::Percent) {
                Some(BinaryOp::Remainder)
            } else {
                None
            };
            let Some(operation) = operation else { break };
            let right = self.parse_unary()?;
            expression = binary(expression, operation, right);
        }
        Ok(expression)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        let (operation, start) = if self.at_simple(SimpleToken::Minus) {
            (UnaryOp::Negate, self.advance_span())
        } else if self.at_simple(SimpleToken::Not) {
            (UnaryOp::Not, self.advance_span())
        } else {
            return self.parse_postfix();
        };
        let value = self.parse_unary()?;
        let span = start.merge(value.span);
        Ok(Expr {
            kind: ExprKind::Unary {
                op: operation,
                value: Box::new(value),
            },
            span,
        })
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.parse_primary()?;
        loop {
            if self.consume_simple(SimpleToken::LParen) {
                let (args, close_span) = self.parse_arguments()?;
                let span = expression.span.merge(close_span);
                expression = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expression),
                        args,
                    },
                    span,
                };
            } else if self.consume_simple(SimpleToken::Dot) {
                let (name, name_span) = self.expect_ident("field name after `.`")?;
                let span = expression.span.merge(name_span);
                expression = Expr {
                    kind: ExprKind::Field {
                        object: Box::new(expression),
                        name,
                    },
                    span,
                };
            } else if self.at_simple(SimpleToken::LBracket)
                && self.current().span.start == expression.span.end
            {
                self.advance_span();
                let key = self.parse_expression()?;
                let close = self.expect_simple(SimpleToken::RBracket, "`]` after index")?;
                let span = expression.span.merge(close);
                expression = Expr {
                    kind: ExprKind::Index {
                        object: Box::new(expression),
                        key: Box::new(key),
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Ok(expression)
    }

    fn parse_arguments(&mut self) -> Result<(Vec<Expr>, Span), ParseError> {
        let mut arguments = Vec::new();
        if !self.at_simple(SimpleToken::RParen) {
            loop {
                arguments.push(self.parse_expression()?);
                if !self.consume_simple(SimpleToken::Comma) || self.at_simple(SimpleToken::RParen) {
                    break;
                }
            }
        }
        let close = self.expect_simple(SimpleToken::RParen, "`)` after arguments")?;
        Ok((arguments, close))
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let span = self.current().span;
        match &self.current().kind {
            TokenKind::Int(value) => {
                let value = *value;
                self.advance_span();
                Ok(Expr {
                    kind: ExprKind::Int(value),
                    span,
                })
            }
            TokenKind::Float(value) => {
                let value = *value;
                self.advance_span();
                Ok(Expr {
                    kind: ExprKind::Float(value),
                    span,
                })
            }
            TokenKind::String(value) => {
                let value = value.clone();
                self.advance_span();
                Ok(Expr {
                    kind: ExprKind::String(value),
                    span,
                })
            }
            TokenKind::Nil => {
                self.advance_span();
                Ok(Expr {
                    kind: ExprKind::Nil,
                    span,
                })
            }
            TokenKind::True | TokenKind::False => {
                let value = matches!(self.current().kind, TokenKind::True);
                self.advance_span();
                Ok(Expr {
                    kind: ExprKind::Bool(value),
                    span,
                })
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance_span();
                Ok(Expr {
                    kind: ExprKind::Variable(name),
                    span,
                })
            }
            TokenKind::LParen => {
                self.advance_span();
                let mut expression = self.parse_expression()?;
                let close = self.expect_simple(SimpleToken::RParen, "`)` after expression")?;
                expression.span = span.merge(close);
                Ok(expression)
            }
            TokenKind::LBracket => self.parse_list(),
            TokenKind::LBrace => self.parse_table(),
            TokenKind::Raise => self.parse_raise(),
            TokenKind::Try => self.parse_try(),
            TokenKind::Match => self.parse_match(),
            TokenKind::If => self.parse_if(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::Continue => self.parse_continue(),
            TokenKind::Break => self.parse_break(),
            _ => Err(self.error_current(format!(
                "expected expression, found `{}`",
                self.current_name()
            ))),
        }
    }

    fn parse_list(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_simple(SimpleToken::LBracket, "`[`")?;
        let mut elements = Vec::new();
        if !self.at_simple(SimpleToken::RBracket) {
            loop {
                elements.push(self.parse_expression()?);
                if !self.consume_simple(SimpleToken::Comma) || self.at_simple(SimpleToken::RBracket)
                {
                    break;
                }
            }
        }
        let end = self.expect_simple(SimpleToken::RBracket, "`]` after list elements")?;
        Ok(Expr {
            kind: ExprKind::List(elements),
            span: start.merge(end),
        })
    }

    fn parse_table(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_simple(SimpleToken::LBrace, "`{`")?;
        let mut entries = Vec::new();
        let mut seen_names = HashSet::new();
        if !self.at_simple(SimpleToken::RBrace) {
            loop {
                let key = if self.consume_simple(SimpleToken::LBracket) {
                    let key = self.parse_expression()?;
                    self.expect_simple(SimpleToken::RBracket, "`]` after table key")?;
                    key
                } else {
                    let (name, name_span) =
                        self.expect_ident("table field name or computed key")?;
                    if !seen_names.insert(name.clone()) {
                        return Err(ParseError {
                            span: name_span,
                            message: format!("duplicate table field `{name}`"),
                        });
                    }
                    Expr {
                        kind: ExprKind::String(name),
                        span: name_span,
                    }
                };
                self.expect_simple(SimpleToken::Equal, "`=` after table key")?;
                let value = self.parse_expression()?;
                entries.push((key, value));
                if !self.consume_simple(SimpleToken::Comma) || self.at_simple(SimpleToken::RBrace) {
                    break;
                }
            }
        }
        let end = self.expect_simple(SimpleToken::RBrace, "`}` after table entries")?;
        Ok(Expr {
            kind: ExprKind::Table(entries),
            span: start.merge(end),
        })
    }
}

fn binary(left: Expr, op: BinaryOp, right: Expr) -> Expr {
    let span = left.span.merge(right.span);
    Expr {
        kind: ExprKind::Binary {
            left: Box::new(left),
            op,
            right: Box::new(right),
        },
        span,
    }
}
