use super::*;

pub(super) fn block(p: &mut Parser<'_>) -> CompletedMarker {
    let marker = p.start();
    p.block_depth += 1;
    while !p.at_end() && !is_block_terminator(p.current()) {
        let before = p.position;
        statement(p);
        if p.position == before {
            recover_statement(p);
        }
    }
    p.block_depth -= 1;
    marker.complete(&mut p.events, K::BLOCK)
}
pub(super) fn is_block_terminator(kind: K) -> bool {
    matches!(
        kind,
        K::ELSEIF_KW | K::ELSE_KW | K::CATCH_KW | K::OF_KW | K::END_KW
    )
}
pub(super) fn expression(p: &mut Parser<'_>) -> Parsed {
    assignment(p)
}
pub(super) fn assignment(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn pipeline(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn pipeline_stage(p: &mut Parser<'_>) {
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
pub(super) fn trailing_argument(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn parse_or(p: &mut Parser<'_>) -> Parsed {
    binary_chain(p, parse_and, &[K::OR_KW])
}
pub(super) fn parse_and(p: &mut Parser<'_>) -> Parsed {
    binary_chain(p, equality, &[K::AND_KW])
}
pub(super) fn equality(p: &mut Parser<'_>) -> Parsed {
    binary_chain(p, comparison, &[K::EQ_EQ, K::BANG_EQ])
}
pub(super) fn comparison(p: &mut Parser<'_>) -> Parsed {
    binary_chain(
        p,
        concatenation,
        &[K::LESS, K::LESS_EQ, K::GREATER, K::GREATER_EQ],
    )
}
pub(super) fn concatenation(p: &mut Parser<'_>) -> Parsed {
    let left = additive(p);
    if !p.at(K::LESS_GREATER) {
        return left;
    }
    let marker = left.marker.precede(&mut p.events);
    p.bump();
    concatenation(p);
    Parsed {
        marker: marker.complete(&mut p.events, K::BINARY_EXPR),
        flavor: Flavor::Other,
    }
}
pub(super) fn additive(p: &mut Parser<'_>) -> Parsed {
    binary_chain(p, multiplicative, &[K::PLUS, K::MINUS])
}
pub(super) fn multiplicative(p: &mut Parser<'_>) -> Parsed {
    binary_chain(p, unary, &[K::STAR, K::SLASH, K::SLASH_SLASH, K::PERCENT])
}
pub(super) fn binary_chain(
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
pub(super) fn unary(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn postfix(p: &mut Parser<'_>) -> Parsed {
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
        } else if p.at(K::L_BRACKET) && p.current_is_lexically_adjacent() {
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
            if p.block_depth == 0 {
                p.error_at(span, "nil propagation `?` outside of a block".to_owned());
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
pub(super) fn argument_list(p: &mut Parser<'_>, open_description: &str) {
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
pub(super) fn primary(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn simple_expr(p: &mut Parser<'_>, kind: K, flavor: Flavor) -> Parsed {
    let marker = p.start();
    p.bump();
    Parsed {
        marker: marker.complete(&mut p.events, kind),
        flavor,
    }
}
pub(super) fn error_expr(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn function_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    if p.at(K::LESS) {
        callable_type_param_list(p);
    }
    function_parts(p, "`(` after `fn`");
    Parsed {
        marker: marker.complete(&mut p.events, K::FUNCTION_EXPR),
        flavor: Flavor::Other,
    }
}
pub(super) fn block_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    block(p);
    p.expect(K::END_KW, "`end` after standalone block");
    Parsed {
        marker: marker.complete(&mut p.events, K::BLOCK_EXPR),
        flavor: Flavor::Other,
    }
}
pub(super) fn paren_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    let inner = expression(p);
    p.expect(K::R_PAREN, "`)` after expression");
    Parsed {
        marker: marker.complete(&mut p.events, K::PAREN_EXPR),
        flavor: inner.flavor,
    }
}
pub(super) fn list_expr(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn map_expr(p: &mut Parser<'_>) -> Parsed {
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
