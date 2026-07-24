use super::*;

pub(super) fn type_expr(p: &mut Parser<'_>) {
    type_expr_with_post_boundary(p, false);
}
pub(super) fn type_expr_before_post(p: &mut Parser<'_>) {
    type_expr_with_post_boundary(p, true);
}
pub(super) fn type_expr_with_post_boundary(p: &mut Parser<'_>, allow_post_boundary: bool) {
    let marker = p.start();
    type_function(p, allow_post_boundary);
    marker.complete(&mut p.events, K::TYPE_EXPR);
}
pub(super) fn type_function(p: &mut Parser<'_>, allow_post_boundary: bool) {
    let marker = p.start();
    let generic = if p.at(K::LESS) {
        callable_type_param_list(p);
        true
    } else {
        false
    };
    type_union(p);
    if p.bump_if(K::ARROW) {
        type_function(p, false);
        effect_annotation(p);
    } else if generic {
        p.error("a callable generic header must be followed by `->` and a result type".to_owned());
    } else if p.at(K::FAT_ARROW) && !allow_post_boundary {
        let at = p.current_span();
        p.error_at(
            at,
            "ambiguous post-state annotation; put `before => after` inside a parenthesized function parameter list".to_owned(),
        );
        p.bump();
        type_function(p, false);
    }
    marker.complete(&mut p.events, K::TYPE_FUNCTION);
}
pub(super) fn effect_annotation(p: &mut Parser<'_>) {
    if !at_effect(p) {
        return;
    }
    let marker = p.start();
    let raises = p.current_text() == Some("raises");
    p.bump();
    if raises {
        if at_type_start(p) {
            type_expr(p);
        } else {
            p.error("expected a raised type after `raises`".to_owned());
        }
    } else if at_type_start(p) {
        p.error("`noraise` does not accept a type".to_owned());
        type_expr(p);
    }
    marker.complete(&mut p.events, K::EFFECT_ANNOTATION);
}
pub(super) fn at_effect(p: &Parser<'_>) -> bool {
    p.at(K::IDENT) && matches!(p.current_text(), Some("raises" | "noraise"))
}
pub(super) fn at_type_start(p: &Parser<'_>) -> bool {
    matches!(
        p.current(),
        K::PIPE
            | K::APOSTROPHE
            | K::STRING
            | K::NIL_KW
            | K::IDENT
            | K::L_PAREN
            | K::L_BRACKET
            | K::L_BRACE
            | K::LESS
    )
}
pub(super) fn type_union(p: &mut Parser<'_>) {
    let marker = p.start();
    p.bump_if(K::PIPE);
    type_primary(p);
    while p.bump_if(K::PIPE) {
        type_primary(p);
    }
    marker.complete(&mut p.events, K::TYPE_UNION);
}
pub(super) fn type_primary(p: &mut Parser<'_>) {
    if p.at(K::APOSTROPHE) {
        type_variable(p);
    } else if matches!(p.current(), K::STRING | K::NIL_KW) {
        let marker = p.start();
        p.bump();
        marker.complete(&mut p.events, K::TYPE_LITERAL);
    } else if p.at(K::IDENT) {
        let marker = p.start();
        p.bump();
        if p.at(K::LESS) {
            type_argument_list(p);
        }
        marker.complete(&mut p.events, K::TYPE_NAME);
    } else if p.at(K::L_PAREN) {
        let marker = p.start();
        p.bump();
        let mut post_spans = Vec::new();
        let mut label_spans = Vec::new();
        if !p.at(K::R_PAREN) {
            loop {
                let parameter = p.start();
                if p.at(K::IDENT) && p.nth(1) == K::COLON {
                    label_spans.push(p.current_span());
                    p.bump();
                    p.bump();
                }
                type_expr_before_post(p);
                if p.at(K::FAT_ARROW) {
                    post_spans.push(p.current_span());
                    post_type(p);
                }
                parameter.complete(&mut p.events, K::TYPE_FUNCTION_PARAM);
                if !p.bump_if(K::COMMA) || p.at(K::R_PAREN) {
                    break;
                }
            }
        }
        p.expect(K::R_PAREN, "`)` after type");
        if !label_spans.is_empty() && !p.at(K::ARROW) {
            for at in label_spans {
                p.error_at(
                    at,
                    "a labeled parameter list must be followed by `->` and a result type"
                        .to_owned(),
                );
            }
        }
        if !post_spans.is_empty() && !p.at(K::ARROW) {
            for at in post_spans {
                p.error_at(
                    at,
                    "a post-state parameter list must be followed by `->` and a result type"
                        .to_owned(),
                );
            }
        }
        marker.complete(&mut p.events, K::TYPE_PAREN);
    } else if p.at(K::L_BRACKET) {
        type_list(p);
    } else if p.at(K::L_BRACE) {
        type_map(p);
    } else {
        let marker = p.start();
        p.error(format!(
            "expected type, found `{}`",
            super::token_name(p.current(), p.at_end())
        ));
        if !p.at_end() {
            p.bump();
        }
        marker.complete(&mut p.events, K::ERROR);
    }
}
pub(super) fn type_variable(p: &mut Parser<'_>) {
    let marker = p.start();
    p.expect(K::APOSTROPHE, "`'` before type variable");
    p.expect(K::IDENT, "type variable name");
    marker.complete(&mut p.events, K::TYPE_VARIABLE);
}
pub(super) fn type_argument_list(p: &mut Parser<'_>) {
    let marker = p.start();
    p.bump();
    if !p.at(K::GREATER) {
        loop {
            type_expr(p);
            if !p.bump_if(K::COMMA) || p.at(K::GREATER) {
                break;
            }
        }
    }
    p.expect(K::GREATER, "`>` after type arguments");
    marker.complete(&mut p.events, K::TYPE_ARGUMENT_LIST);
}
pub(super) fn type_list(p: &mut Parser<'_>) {
    let marker = p.start();
    p.bump();
    if p.at(K::DOT_DOT) {
        let rest = p.start();
        p.bump();
        type_expr(p);
        rest.complete(&mut p.events, K::TYPE_LIST_REST);
        p.bump_if(K::COMMA);
    } else if !p.at(K::R_BRACKET) {
        loop {
            type_expr(p);
            if !p.bump_if(K::COMMA) || p.at(K::R_BRACKET) {
                break;
            }
        }
    }
    p.expect(K::R_BRACKET, "`]` after list type");
    marker.complete(&mut p.events, K::TYPE_LIST);
}
pub(super) fn type_map(p: &mut Parser<'_>) {
    let marker = p.start();
    p.bump();
    while !p.at(K::R_BRACE) && !p.at_end() {
        if p.at(K::DOT_DOT) {
            let rest = p.start();
            p.bump();
            rest.complete(&mut p.events, K::TYPE_MAP_REST);
        } else {
            let entry = p.start();
            if p.bump_if(K::L_BRACKET) {
                type_expr(p);
                p.expect(K::R_BRACKET, "`]` after map key type");
            } else {
                p.expect(K::IDENT, "map type field");
            }
            p.expect(K::COLON, "`:` in map type field");
            type_expr(p);
            entry.complete(&mut p.events, K::TYPE_MAP_ENTRY);
        }
        if !p.bump_if(K::COMMA) {
            break;
        }
    }
    p.expect(K::R_BRACE, "`}` after map type");
    marker.complete(&mut p.events, K::TYPE_MAP);
}
