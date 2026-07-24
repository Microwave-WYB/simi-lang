use std::collections::HashSet;

use super::event::CompletedMarker;
use super::{Parser, token_name};
use crate::syntax::SyntaxKind as K;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flavor {
    Name,
    Field,
    Index,
    Call,
    Other,
}

#[derive(Clone, Copy)]
struct Parsed {
    marker: CompletedMarker,
    flavor: Flavor,
}

mod control_flow;
mod declarations;
mod expressions;
mod patterns;
mod types;

use control_flow::*;
use declarations::*;
use expressions::*;
use patterns::*;
use types::*;

pub(super) fn root(p: &mut Parser<'_>) {
    let root = p.start_root();
    while !p.at_end() {
        let before = p.position;
        if is_block_terminator(p.current()) {
            let name = super::token_name(p.current(), false);
            p.error(format!("unexpected `{name}` outside of a block"));
            let error = p.start();
            p.bump();
            error.complete(&mut p.events, K::ERROR);
        } else {
            statement(p);
        }
        if p.position == before {
            recover_statement(p);
        }
    }
    p.eat_trivia();
    root.complete(&mut p.events, K::ROOT);
}

fn statement(p: &mut Parser<'_>) {
    if p.at(K::FN_KW) && p.nth(1) == K::IDENT {
        function_decl(p);
    } else if p.at(K::ALIAS_KW)
        || (p.at(K::IDENT) && p.current_text() == Some("alias") && p.nth(1) == K::IDENT)
    {
        alias_decl(p);
    } else if p.at(K::LET_KW) {
        let_stmt(p);
    } else {
        let marker = p.start();
        expression(p);
        marker.complete(&mut p.events, K::EXPR_STMT);
    }
}

fn recover_statement(p: &mut Parser<'_>) {
    let marker = p.start();
    if !p.at_end() {
        p.bump();
    }
    while !(p.at_end()
        || p.at(K::FN_KW)
        || p.at(K::ALIAS_KW)
        || p.at(K::LET_KW)
        || (p.at(K::IDENT) && p.current_text() == Some("alias") && p.nth(1) == K::IDENT))
    {
        p.bump();
    }
    marker.complete(&mut p.events, K::ERROR);
}

fn can_begin_expression(kind: K) -> bool {
    matches!(
        kind,
        K::INT
            | K::FLOAT
            | K::STRING
            | K::IDENT
            | K::NIL_KW
            | K::TRUE_KW
            | K::FALSE_KW
            | K::MINUS
            | K::NOT_KW
            | K::L_PAREN
            | K::L_BRACKET
            | K::L_BRACE
            | K::DO_KW
            | K::RAISE_KW
            | K::TRY_KW
            | K::CATCH_KW
            | K::CASE_KW
            | K::IF_KW
            | K::LOOP_KW
            | K::BREAK_KW
            | K::CONTINUE_KW
    )
}

fn node_span_hint(p: &Parser<'_>, marker: CompletedMarker) -> crate::span::Span {
    let target = marker.position();
    let mut first_event = target;
    for index in 0..target {
        let mut current = index;
        while let super::event::Event::Start {
            forward_parent: Some(distance),
            ..
        } = &p.events[current]
        {
            current += *distance;
            if current == target {
                first_event = first_event.min(index);
                break;
            }
            if current > target {
                break;
            }
        }
    }
    let mut spans = p.events[first_event..]
        .iter()
        .filter_map(|event| match event {
            super::event::Event::Token(index) if !p.lexemes[*index].kind.is_trivia() => {
                Some(p.lexemes[*index].span)
            }
            _ => None,
        });
    let first = spans.next().unwrap_or_else(|| p.previous_nontrivia_span());
    spans.fold(first, crate::span::Span::merge)
}
