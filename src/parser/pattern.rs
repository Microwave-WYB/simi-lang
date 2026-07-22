use std::collections::HashSet;

use super::{ParseError, Parser, SimpleToken};
use crate::ast::{Pattern, PatternKind, PatternRest};
use crate::lexer::TokenKind;

impl Parser {
    pub(super) fn parse_pattern(
        &mut self,
        bindings: &mut HashSet<String>,
    ) -> Result<Pattern, ParseError> {
        let span = self.current().span;
        let kind = match &self.current().kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance_span();
                if name.starts_with('_') {
                    PatternKind::Wildcard
                } else {
                    if !bindings.insert(name.clone()) {
                        return Err(ParseError {
                            span,
                            message: format!("duplicate binding `{name}` in pattern"),
                        });
                    }
                    PatternKind::Binding(name)
                }
            }
            TokenKind::Int(value) => {
                let value = *value;
                self.advance_span();
                PatternKind::Int(value)
            }
            TokenKind::Float(value) => {
                let value = *value;
                self.advance_span();
                PatternKind::Float(value)
            }
            TokenKind::String(value) => {
                let value = value.clone();
                self.advance_span();
                PatternKind::String(value)
            }
            TokenKind::Nil => {
                self.advance_span();
                PatternKind::Nil
            }
            TokenKind::True | TokenKind::False => {
                let value = matches!(self.current().kind, TokenKind::True);
                self.advance_span();
                PatternKind::Bool(value)
            }
            TokenKind::LBracket => return self.parse_list_pattern(bindings),
            TokenKind::LBrace => return self.parse_map_pattern(bindings),
            _ => {
                return Err(self
                    .error_current(format!("expected pattern, found `{}`", self.current_name())));
            }
        };
        Ok(Pattern { kind, span })
    }

    fn parse_list_pattern(
        &mut self,
        bindings: &mut HashSet<String>,
    ) -> Result<Pattern, ParseError> {
        let start = self.expect_simple(SimpleToken::LBracket, "`[`")?;
        let mut elements = Vec::new();
        let mut rest = None;

        if !self.at_simple(SimpleToken::RBracket) {
            loop {
                if self.consume_simple(SimpleToken::DotDot) {
                    rest = Some(self.parse_pattern_rest(bindings)?);
                    self.consume_simple(SimpleToken::Comma);
                    if !self.at_simple(SimpleToken::RBracket) {
                        return Err(self.error_current(format!(
                            "expected `]` after list pattern, found `{}`",
                            self.current_name()
                        )));
                    }
                    break;
                }

                elements.push(self.parse_pattern(bindings)?);
                if !self.consume_simple(SimpleToken::Comma) || self.at_simple(SimpleToken::RBracket)
                {
                    break;
                }
            }
        }

        let end = self.expect_simple(SimpleToken::RBracket, "`]` after list pattern")?;
        Ok(Pattern {
            kind: PatternKind::List { elements, rest },
            span: start.merge(end),
        })
    }

    fn parse_map_pattern(&mut self, bindings: &mut HashSet<String>) -> Result<Pattern, ParseError> {
        let start = self.expect_simple(SimpleToken::LBrace, "`{`")?;
        let mut fields = Vec::new();
        let mut rest = None;
        let mut seen_fields = HashSet::new();

        if !self.at_simple(SimpleToken::RBrace) {
            loop {
                if self.consume_simple(SimpleToken::DotDot) {
                    rest = Some(self.parse_pattern_rest(bindings)?);
                    self.consume_simple(SimpleToken::Comma);
                    if !self.at_simple(SimpleToken::RBrace) {
                        return Err(self.error_current(format!(
                            "expected `}}` after map pattern, found `{}`",
                            self.current_name()
                        )));
                    }
                    break;
                }

                let (name, name_span) = self.expect_ident("map pattern field name or `..`")?;
                if !seen_fields.insert(name.clone()) {
                    return Err(ParseError {
                        span: name_span,
                        message: format!("duplicate map pattern field `{name}`"),
                    });
                }
                self.expect_simple(SimpleToken::Equal, "`=` after map pattern field name")?;
                let pattern = self.parse_pattern(bindings)?;
                fields.push((name, pattern));

                if !self.consume_simple(SimpleToken::Comma) || self.at_simple(SimpleToken::RBrace) {
                    break;
                }
            }
        }

        let end = self.expect_simple(SimpleToken::RBrace, "`}` after map pattern")?;
        Ok(Pattern {
            kind: PatternKind::Map { fields, rest },
            span: start.merge(end),
        })
    }

    fn parse_pattern_rest(
        &mut self,
        bindings: &mut HashSet<String>,
    ) -> Result<PatternRest, ParseError> {
        let (name, span) = self.expect_ident("rest binding name after `..`")?;
        if name.starts_with('_') {
            return Ok(PatternRest::Discard);
        }
        if !bindings.insert(name.clone()) {
            return Err(ParseError {
                span,
                message: format!("duplicate binding `{name}` in pattern"),
            });
        }
        Ok(PatternRest::Binding(name))
    }
}
