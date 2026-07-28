use super::*;

pub(super) fn type_expr(p: &mut Parser<'_>) {
    let marker = p.start();
    type_function(p);
    marker.complete(&mut p.events, K::TYPE_EXPR);
}
pub(super) fn type_function(p: &mut Parser<'_>) {
    let marker = p.start();
    if p.bump_if(K::FN_KW) {
        if p.at(K::LESS) {
            callable_type_param_list(p);
        }
        if p.at(K::L_PAREN) {
            type_paren(p, true);
        } else {
            p.error("expected `(` after `fn` in callable type".to_owned());
        }
        if p.bump_if(K::ARROW) {
            if at_type_start(p) {
                type_function(p);
                effect_annotation(p);
            } else {
                p.error("expected a result type after `->`".to_owned());
            }
        } else {
            p.error("expected `->` and a result type in callable type".to_owned());
        }
    } else {
        let generic = if p.at(K::LESS) {
            callable_type_param_list(p);
            true
        } else {
            false
        };
        type_union(p);
        if p.bump_if(K::ARROW) {
            p.error("legacy callable types are not supported; use `fn(...) -> result`".to_owned());
            if at_type_start(p) {
                type_function(p);
                effect_annotation(p);
            } else {
                p.error("expected a result type after `->`".to_owned());
            }
        } else if generic {
            p.error(
                "legacy callable generic headers are not supported; use `fn<'a>(...) -> result`"
                    .to_owned(),
            );
        }
    }
    marker.complete(&mut p.events, K::TYPE_FUNCTION);
}
pub(super) fn effect_annotation(p: &mut Parser<'_>) {
    if at_legacy_effect(p) {
        let takes_type = p.current_text() == Some("raises");
        p.error("legacy callable contracts are removed; use `! RaisedType`".to_owned());
        p.bump();
        if takes_type && at_type_start(p) {
            type_expr(p);
        }
        return;
    }
    if !at_effect(p) {
        return;
    }
    let marker = p.start();
    p.bump();
    if at_type_start(p) {
        type_expr(p);
    } else {
        p.error("expected a raised type after `!`".to_owned());
    }
    marker.complete(&mut p.events, K::EFFECT_ANNOTATION);
}
pub(super) fn at_effect(p: &Parser<'_>) -> bool {
    p.at(K::BANG)
}
pub(super) fn at_legacy_effect(p: &Parser<'_>) -> bool {
    p.at(K::IDENT) && matches!(p.current_text(), Some("raises" | "noraise"))
}
pub(super) fn at_type_start(p: &Parser<'_>) -> bool {
    matches!(
        p.current(),
        K::PIPE
            | K::APOSTROPHE
            | K::STRING
            | K::NIL_KW
            | K::TRUE_KW
            | K::FALSE_KW
            | K::INT
            | K::FLOAT
            | K::MINUS
            | K::IDENT
            | K::FN_KW
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
    } else if matches!(
        p.current(),
        K::STRING | K::NIL_KW | K::TRUE_KW | K::FALSE_KW | K::INT | K::FLOAT
    ) || (p.at(K::MINUS) && matches!(p.nth(1), K::INT | K::FLOAT))
    {
        let marker = p.start();
        p.bump_if(K::MINUS);
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
        type_paren(p, false);
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
fn type_paren(p: &mut Parser<'_>, callable_parameters: bool) {
    let marker = p.start();
    p.bump();
    if !p.at(K::R_PAREN) {
        loop {
            let parameter = p.start();
            if p.at(K::IDENT) && p.nth(1) == K::COLON {
                if !callable_parameters {
                    p.error_at(
                        p.current_span(),
                        "parameter labels are only valid in `fn(...)` callable types".to_owned(),
                    );
                }
                p.bump();
                p.bump();
            }
            type_expr(p);
            parameter.complete(&mut p.events, K::TYPE_FUNCTION_PARAM);
            if !p.bump_if(K::COMMA) || p.at(K::R_PAREN) {
                break;
            }
        }
    }
    p.expect(K::R_PAREN, "`)` after type");
    marker.complete(&mut p.events, K::TYPE_PAREN);
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
