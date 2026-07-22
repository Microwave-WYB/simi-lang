use std::collections::HashSet;

use super::Parser;
use super::event::CompletedMarker;
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
    if p.at(K::FN_KW) && p.nth(1) != K::L_PAREN {
        function_decl(p);
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
    while !p.at_end() && !p.at(K::FN_KW) && !p.at(K::LET_KW) {
        p.bump();
    }
    marker.complete(&mut p.events, K::ERROR);
}

fn function_decl(p: &mut Parser<'_>) {
    let marker = p.start();
    p.expect(K::FN_KW, "`fn`");
    p.expect(K::IDENT, "function name");
    function_parts(p, "`(` after function name");
    marker.complete(&mut p.events, K::FUNCTION_DECL);
}

fn function_parts(p: &mut Parser<'_>, open: &str) {
    let params = p.start();
    p.expect(K::L_PAREN, open);
    let mut seen = HashSet::new();
    if !p.at(K::R_PAREN) && !p.at_end() {
        loop {
            let span = p.current_span();
            let name = p.current_text().unwrap_or_default().to_owned();
            if p.expect(K::IDENT, "parameter name") && !seen.insert(name.clone()) {
                p.error_at(span, format!("duplicate parameter `{name}`"));
            }
            if !p.bump_if(K::COMMA) || p.at(K::R_PAREN) {
                break;
            }
        }
    }
    p.expect(K::R_PAREN, "`)` after parameters");
    params.complete(&mut p.events, K::PARAM_LIST);
    p.expect(K::DO_KW, "`do` before function body");
    let old_loop = std::mem::replace(&mut p.loop_depth, 0);
    let old_block = std::mem::replace(&mut p.standalone_block_depth, 0);
    block(p);
    p.loop_depth = old_loop;
    p.standalone_block_depth = old_block;
    p.expect(K::END_KW, "`end` after function body");
}

fn let_stmt(p: &mut Parser<'_>) {
    let marker = p.start();
    p.bump();
    let mut bindings = HashSet::new();
    if p.at(K::IDENT) {
        let binding = p.start();
        p.bump();
        binding.complete(&mut p.events, K::BINDING_PATTERN);
    } else {
        pattern(p, &mut bindings);
    }
    p.expect(K::EQ, "`=` after let pattern");
    expression(p);
    marker.complete(&mut p.events, K::LET_STMT);
}

fn block(p: &mut Parser<'_>) -> CompletedMarker {
    let marker = p.start();
    while !p.at_end() && !is_block_terminator(p.current()) {
        let before = p.position;
        statement(p);
        if p.position == before {
            recover_statement(p);
        }
    }
    marker.complete(&mut p.events, K::BLOCK)
}

fn is_block_terminator(kind: K) -> bool {
    matches!(
        kind,
        K::ELSEIF_KW | K::ELSE_KW | K::CATCH_KW | K::OF_KW | K::END_KW
    )
}

fn expression(p: &mut Parser<'_>) -> Parsed {
    assignment(p)
}

fn assignment(p: &mut Parser<'_>) -> Parsed {
    let left = pipeline(p);
    if !p.at(K::EQ) {
        return left;
    }
    let marker = left.marker.precede(&mut p.events);
    let target_span = node_span_hint(p, left.marker);
    if !matches!(left.flavor, Flavor::Name | Flavor::Field | Flavor::Index) {
        p.error_at(target_span, "invalid assignment target".to_owned());
    }
    p.bump();
    assignment(p);
    Parsed {
        marker: marker.complete(&mut p.events, K::ASSIGN_EXPR),
        flavor: Flavor::Other,
    }
}

fn pipeline(p: &mut Parser<'_>) -> Parsed {
    let input = trailing_argument(p);
    if !p.at(K::PIPE_GREATER) && !p.at(K::QUESTION_GREATER) {
        return input;
    }
    let marker = input.marker.precede(&mut p.events);
    while p.at(K::PIPE_GREATER) || p.at(K::QUESTION_GREATER) {
        pipeline_stage(p);
    }
    Parsed {
        marker: marker.complete(&mut p.events, K::PIPELINE_EXPR),
        flavor: Flavor::Other,
    }
}

fn pipeline_stage(p: &mut Parser<'_>) {
    let marker = p.start();
    p.bump();
    p.bump_if(K::TAP_KW);
    let mut callee = if p.at(K::IDENT) {
        let name = p.start();
        p.bump();
        Parsed {
            marker: name.complete(&mut p.events, K::NAME_EXPR),
            flavor: Flavor::Name,
        }
    } else {
        p.error(format!(
            "expected pipeline stage function name, found `{}`",
            super::token_name(p.current(), p.at_end())
        ));
        error_expr(p)
    };
    while p.at(K::DOT) {
        let field = callee.marker.precede(&mut p.events);
        p.bump();
        p.expect(K::IDENT, "field name after `.`");
        callee = Parsed {
            marker: field.complete(&mut p.events, K::FIELD_EXPR),
            flavor: Flavor::Field,
        };
    }
    argument_list(p, "`(` in pipeline stage call");
    if p.bump_if(K::LESS_PIPE) {
        trailing_argument(p);
    }
    marker.complete(&mut p.events, K::PIPELINE_STAGE);
}

fn trailing_argument(p: &mut Parser<'_>) -> Parsed {
    let left = parse_or(p);
    if !p.at(K::LESS_PIPE) {
        return left;
    }
    let marker = left.marker.precede(&mut p.events);
    if left.flavor != Flavor::Call {
        p.error_at(
            node_span_hint(p, left.marker),
            "left side of `<|` must be a call".to_owned(),
        );
    }
    p.bump();
    trailing_argument(p);
    Parsed {
        marker: marker.complete(&mut p.events, K::TRAILING_ARGUMENT_EXPR),
        flavor: Flavor::Call,
    }
}

fn parse_or(p: &mut Parser<'_>) -> Parsed {
    binary_chain(p, parse_and, &[K::OR_KW])
}
fn parse_and(p: &mut Parser<'_>) -> Parsed {
    binary_chain(p, equality, &[K::AND_KW])
}
fn equality(p: &mut Parser<'_>) -> Parsed {
    binary_chain(p, comparison, &[K::EQ_EQ, K::BANG_EQ])
}
fn comparison(p: &mut Parser<'_>) -> Parsed {
    binary_chain(
        p,
        additive,
        &[K::LESS, K::LESS_EQ, K::GREATER, K::GREATER_EQ],
    )
}
fn additive(p: &mut Parser<'_>) -> Parsed {
    binary_chain(p, multiplicative, &[K::PLUS, K::MINUS])
}
fn multiplicative(p: &mut Parser<'_>) -> Parsed {
    binary_chain(p, unary, &[K::STAR, K::SLASH, K::SLASH_SLASH, K::PERCENT])
}

fn binary_chain(
    p: &mut Parser<'_>,
    operand: fn(&mut Parser<'_>) -> Parsed,
    operators: &[K],
) -> Parsed {
    let mut left = operand(p);
    while operators.contains(&p.current()) {
        let marker = left.marker.precede(&mut p.events);
        p.bump();
        operand(p);
        left = Parsed {
            marker: marker.complete(&mut p.events, K::BINARY_EXPR),
            flavor: Flavor::Other,
        };
    }
    left
}

fn unary(p: &mut Parser<'_>) -> Parsed {
    if p.at(K::MINUS) || p.at(K::NOT_KW) {
        let marker = p.start();
        p.bump();
        unary(p);
        return Parsed {
            marker: marker.complete(&mut p.events, K::UNARY_EXPR),
            flavor: Flavor::Other,
        };
    }
    postfix(p)
}

fn postfix(p: &mut Parser<'_>) -> Parsed {
    let mut value = primary(p);
    loop {
        if p.at(K::L_PAREN) {
            let marker = value.marker.precede(&mut p.events);
            argument_list(p, "`(` before arguments");
            value = Parsed {
                marker: marker.complete(&mut p.events, K::CALL_EXPR),
                flavor: Flavor::Call,
            };
        } else if p.at(K::DOT) {
            let marker = value.marker.precede(&mut p.events);
            p.bump();
            p.expect(K::IDENT, "field name after `.`");
            value = Parsed {
                marker: marker.complete(&mut p.events, K::FIELD_EXPR),
                flavor: Flavor::Field,
            };
        } else if p.at(K::L_BRACKET) && p.current_span().start == node_end_hint(p, value.marker) {
            let marker = value.marker.precede(&mut p.events);
            p.bump();
            expression(p);
            p.expect(K::R_BRACKET, "`]` after index");
            value = Parsed {
                marker: marker.complete(&mut p.events, K::INDEX_EXPR),
                flavor: Flavor::Index,
            };
        } else if p.at(K::QUESTION) {
            let marker = value.marker.precede(&mut p.events);
            let span = p.current_span();
            p.bump();
            if p.standalone_block_depth == 0 {
                p.error_at(
                    span,
                    "nil propagation `?` outside of a standalone `do ... end` block".to_owned(),
                );
            }
            value = Parsed {
                marker: marker.complete(&mut p.events, K::NIL_PROPAGATE_EXPR),
                flavor: Flavor::Other,
            };
        } else {
            break;
        }
    }
    value
}

fn argument_list(p: &mut Parser<'_>, open_description: &str) {
    let marker = p.start();
    p.expect(K::L_PAREN, open_description);
    if !p.at(K::R_PAREN) && !p.at_end() {
        loop {
            expression(p);
            if !p.bump_if(K::COMMA) || p.at(K::R_PAREN) {
                break;
            }
        }
    }
    p.expect(K::R_PAREN, "`)` after arguments");
    marker.complete(&mut p.events, K::ARG_LIST);
}

fn primary(p: &mut Parser<'_>) -> Parsed {
    match p.current() {
        K::INT | K::FLOAT | K::STRING | K::NIL_KW | K::TRUE_KW | K::FALSE_KW => {
            simple_expr(p, K::LITERAL_EXPR, Flavor::Other)
        }
        K::IDENT => simple_expr(p, K::NAME_EXPR, Flavor::Name),
        K::FN_KW => function_expr(p),
        K::DO_KW => block_expr(p),
        K::L_PAREN => paren_expr(p),
        K::L_BRACKET => list_expr(p),
        K::L_BRACE => map_expr(p),
        K::RAISE_KW => raise_expr(p),
        K::TRY_KW => try_expr(p),
        K::CASE_KW => case_expr(p),
        K::IF_KW => if_expr(p),
        K::LOOP_KW => loop_expr(p),
        K::CONTINUE_KW => continue_expr(p),
        K::BREAK_KW => break_expr(p),
        _ => {
            p.error(format!(
                "expected expression, found `{}`",
                super::token_name(p.current(), p.at_end())
            ));
            error_expr(p)
        }
    }
}

fn simple_expr(p: &mut Parser<'_>, kind: K, flavor: Flavor) -> Parsed {
    let marker = p.start();
    p.bump();
    Parsed {
        marker: marker.complete(&mut p.events, kind),
        flavor,
    }
}

fn error_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    if !p.at_end()
        && !is_block_terminator(p.current())
        && !matches!(p.current(), K::FN_KW | K::LET_KW)
    {
        p.bump();
    }
    Parsed {
        marker: marker.complete(&mut p.events, K::ERROR),
        flavor: Flavor::Other,
    }
}

fn function_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    function_parts(p, "`(` after `fn`");
    Parsed {
        marker: marker.complete(&mut p.events, K::FUNCTION_EXPR),
        flavor: Flavor::Other,
    }
}

fn block_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    p.standalone_block_depth += 1;
    block(p);
    p.standalone_block_depth -= 1;
    p.expect(K::END_KW, "`end` after standalone block");
    Parsed {
        marker: marker.complete(&mut p.events, K::BLOCK_EXPR),
        flavor: Flavor::Other,
    }
}

fn paren_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    let inner = expression(p);
    p.expect(K::R_PAREN, "`)` after expression");
    Parsed {
        marker: marker.complete(&mut p.events, K::PAREN_EXPR),
        flavor: inner.flavor,
    }
}

fn list_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    if !p.at(K::R_BRACKET) && !p.at_end() {
        loop {
            expression(p);
            if !p.bump_if(K::COMMA) || p.at(K::R_BRACKET) {
                break;
            }
        }
    }
    p.expect(K::R_BRACKET, "`]` after list elements");
    Parsed {
        marker: marker.complete(&mut p.events, K::LIST_EXPR),
        flavor: Flavor::Other,
    }
}

fn map_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    let mut fields = HashSet::new();
    if !p.at(K::R_BRACE) && !p.at_end() {
        loop {
            let entry = p.start();
            if p.bump_if(K::L_BRACKET) {
                expression(p);
                p.expect(K::R_BRACKET, "`]` after map key");
            } else {
                let name = p.current_text().unwrap_or_default().to_owned();
                let span = p.current_span();
                if p.expect(K::IDENT, "map field name or computed key")
                    && !fields.insert(name.clone())
                {
                    p.error_at(span, format!("duplicate map field `{name}`"));
                }
            }
            p.expect(K::EQ, "`=` after map key");
            expression(p);
            entry.complete(&mut p.events, K::MAP_ENTRY);
            if !p.bump_if(K::COMMA) || p.at(K::R_BRACE) {
                break;
            }
        }
    }
    p.expect(K::R_BRACE, "`}` after map entries");
    Parsed {
        marker: marker.complete(&mut p.events, K::MAP_EXPR),
        flavor: Flavor::Other,
    }
}

fn raise_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    expression(p);
    Parsed {
        marker: marker.complete(&mut p.events, K::RAISE_EXPR),
        flavor: Flavor::Other,
    }
}

fn try_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    let before = p.nontrivia_index();
    let protected = block(p);
    if p.nontrivia_index() == before {
        p.error("expected at least one protected block item".to_owned());
    }
    if p.at(K::CATCH_KW) {
        pattern_clauses(
            p,
            K::CATCH_KW,
            K::CATCH_CLAUSE,
            "catch",
            "`catch` after protected block",
        );
    } else {
        p.expect(K::CATCH_KW, "`catch` after protected block");
    }
    p.expect(K::END_KW, "`end` after try expression");
    let _ = protected;
    Parsed {
        marker: marker.complete(&mut p.events, K::TRY_EXPR),
        flavor: Flavor::Other,
    }
}

fn case_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    expression(p);
    if p.at(K::OF_KW) {
        pattern_clauses(p, K::OF_KW, K::CASE_CLAUSE, "of", "`of` after case value");
    } else {
        p.expect(K::OF_KW, "`of` after case value");
    }
    p.expect(K::END_KW, "`end` after case expression");
    Parsed {
        marker: marker.complete(&mut p.events, K::CASE_EXPR),
        flavor: Flavor::Other,
    }
}

fn pattern_clauses(
    p: &mut Parser<'_>,
    repeated_marker: K,
    clause_kind: K,
    marker_name: &str,
    first_marker_description: &str,
) {
    let mut first = true;
    loop {
        let clause = p.start();
        p.expect(
            repeated_marker,
            if first {
                first_marker_description
            } else {
                marker_name
            },
        );
        first = false;
        if p.at(K::END_KW) {
            p.error(format!(
                "expected pattern after `{marker_name}`, found `end`"
            ));
            clause.complete(&mut p.events, clause_kind);
            break;
        }
        let mut bindings = HashSet::new();
        pattern(p, &mut bindings);
        if p.bump_if(K::WHEN_KW) {
            expression(p);
        }
        p.expect(K::DO_KW, "`do` before clause body");
        block(p);
        clause.complete(&mut p.events, clause_kind);
        if !p.at(repeated_marker) {
            break;
        }
    }
}

fn if_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    if_branch_after_marker(p);
    while p.bump_if(K::ELSEIF_KW) {
        if_branch_after_marker(p);
    }
    if p.at(K::ELSE_KW) {
        let branch = p.start();
        p.bump();
        block(p);
        branch.complete(&mut p.events, K::ELSE_BRANCH);
    }
    p.expect(K::END_KW, "`end` after if expression");
    Parsed {
        marker: marker.complete(&mut p.events, K::IF_EXPR),
        flavor: Flavor::Other,
    }
}
fn if_branch_after_marker(p: &mut Parser<'_>) {
    let marker = p.start();
    expression(p);
    p.expect(K::THEN_KW, "`then` after if condition");
    block(p);
    marker.complete(&mut p.events, K::IF_BRANCH);
}

fn loop_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    if p.at(K::DO_KW) {
        p.bump();
    } else {
        p.expect(K::IDENT, "loop state name");
        p.expect(K::EQ, "`=` after loop state name");
        expression(p);
        p.expect(K::DO_KW, "`do` before loop body");
    }
    p.loop_depth += 1;
    block(p);
    p.loop_depth -= 1;
    p.expect(K::END_KW, "`end` after loop body");
    Parsed {
        marker: marker.complete(&mut p.events, K::LOOP_EXPR),
        flavor: Flavor::Other,
    }
}

fn continue_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    let span = p.current_span();
    p.bump();
    if p.loop_depth == 0 {
        p.error_at(span, "`continue` outside of a loop".to_owned());
    }
    if can_begin_expression(p.current()) {
        expression(p);
    }
    Parsed {
        marker: marker.complete(&mut p.events, K::CONTINUE_EXPR),
        flavor: Flavor::Other,
    }
}
fn break_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    let span = p.current_span();
    p.bump();
    if p.loop_depth == 0 {
        p.error_at(span, "`break` outside of a loop".to_owned());
    }
    expression(p);
    Parsed {
        marker: marker.complete(&mut p.events, K::BREAK_EXPR),
        flavor: Flavor::Other,
    }
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

fn pattern(p: &mut Parser<'_>, bindings: &mut HashSet<String>) {
    match p.current() {
        K::IDENT => {
            let marker = p.start();
            let name = p.current_text().unwrap_or_default().to_owned();
            let span = p.current_span();
            p.bump();
            let kind = if name.starts_with('_') {
                K::WILDCARD_PATTERN
            } else {
                if !bindings.insert(name.clone()) {
                    p.error_at(span, format!("duplicate binding `{name}` in pattern"));
                }
                K::BINDING_PATTERN
            };
            marker.complete(&mut p.events, kind);
        }
        K::INT | K::FLOAT | K::STRING | K::NIL_KW | K::TRUE_KW | K::FALSE_KW => {
            let marker = p.start();
            p.bump();
            marker.complete(&mut p.events, K::LITERAL_PATTERN);
        }
        K::L_BRACKET => list_pattern(p, bindings),
        K::L_BRACE => map_pattern(p, bindings),
        _ => {
            p.error(format!(
                "expected pattern, found `{}`",
                super::token_name(p.current(), p.at_end())
            ));
            let marker = p.start();
            if !p.at_end() {
                p.bump();
            }
            marker.complete(&mut p.events, K::ERROR);
        }
    }
}

fn list_pattern(p: &mut Parser<'_>, bindings: &mut HashSet<String>) {
    let marker = p.start();
    p.bump();
    if !p.at(K::R_BRACKET) && !p.at_end() {
        loop {
            if p.at(K::DOT_DOT) {
                rest_pattern(p, bindings);
                p.bump_if(K::COMMA);
                if !p.at(K::R_BRACKET) {
                    p.error(format!(
                        "expected `]` after list pattern, found `{}`",
                        super::token_name(p.current(), p.at_end())
                    ));
                }
                break;
            }
            pattern(p, bindings);
            if !p.bump_if(K::COMMA) || p.at(K::R_BRACKET) {
                break;
            }
        }
    }
    p.expect(K::R_BRACKET, "`]` after list pattern");
    marker.complete(&mut p.events, K::LIST_PATTERN);
}

fn map_pattern(p: &mut Parser<'_>, bindings: &mut HashSet<String>) {
    let marker = p.start();
    p.bump();
    let mut fields = HashSet::new();
    if !p.at(K::R_BRACE) && !p.at_end() {
        loop {
            if p.at(K::DOT_DOT) {
                rest_pattern(p, bindings);
                p.bump_if(K::COMMA);
                if !p.at(K::R_BRACE) {
                    p.error(format!(
                        "expected `}}` after map pattern, found `{}`",
                        super::token_name(p.current(), p.at_end())
                    ));
                }
                break;
            }
            let field = p.start();
            let name = p.current_text().unwrap_or_default().to_owned();
            let span = p.current_span();
            if p.expect(K::IDENT, "map pattern field name or `..`") && !fields.insert(name.clone())
            {
                p.error_at(span, format!("duplicate map pattern field `{name}`"));
            }
            p.expect(K::EQ, "`=` after map pattern field name");
            pattern(p, bindings);
            field.complete(&mut p.events, K::MAP_PATTERN_FIELD);
            if !p.bump_if(K::COMMA) || p.at(K::R_BRACE) {
                break;
            }
        }
    }
    p.expect(K::R_BRACE, "`}` after map pattern");
    marker.complete(&mut p.events, K::MAP_PATTERN);
}
fn rest_pattern(p: &mut Parser<'_>, bindings: &mut HashSet<String>) {
    let marker = p.start();
    p.expect(K::DOT_DOT, "`..`");
    let name = p.current_text().unwrap_or_default().to_owned();
    let span = p.current_span();
    if p.expect(K::IDENT, "rest binding name after `..`")
        && !name.starts_with('_')
        && !bindings.insert(name.clone())
    {
        p.error_at(span, format!("duplicate binding `{name}` in pattern"));
    }
    marker.complete(&mut p.events, K::REST_PATTERN);
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
fn node_end_hint(p: &Parser<'_>, _marker: CompletedMarker) -> usize {
    p.previous_nontrivia_span().end
}
