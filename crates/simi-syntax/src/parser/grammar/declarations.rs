use super::*;

pub(super) fn function_decl(p: &mut Parser<'_>) {
    let marker = p.start();
    p.expect(K::FN_KW, "`fn`");
    p.expect(K::IDENT, "function name");
    if p.at(K::LESS) {
        callable_type_param_list(p);
    }
    function_parts(p, "`(` after function name");
    marker.complete(&mut p.events, K::FUNCTION_DECL);
}
pub(super) fn function_parts(p: &mut Parser<'_>, open: &str) {
    let params = p.start();
    p.expect(K::L_PAREN, open);
    let mut seen = HashSet::new();
    if !p.at(K::R_PAREN) && !p.at_end() {
        loop {
            let param = p.start();
            let span = p.current_span();
            let name = p.current_text().unwrap_or_default().to_owned();
            if p.expect(K::IDENT, "parameter name") && !seen.insert(name.clone()) {
                p.error_at(span, format!("duplicate parameter `{name}`"));
            }
            let annotated = if p.at(K::COLON) {
                parameter_type_annotation(p);
                true
            } else {
                false
            };
            if p.at(K::FAT_ARROW) {
                if !annotated {
                    p.error("post-state annotation requires an input type".to_owned());
                }
                post_type(p);
            }
            param.complete(&mut p.events, K::PARAM);
            if !p.bump_if(K::COMMA) || p.at(K::R_PAREN) {
                break;
            }
        }
    }
    p.expect(K::R_PAREN, "`)` after parameters");
    params.complete(&mut p.events, K::PARAM_LIST);
    if p.at(K::ARROW) {
        let result = p.start();
        p.bump();
        type_expr(p);
        result.complete(&mut p.events, K::RETURN_ANNOTATION);
        effect_annotation(p);
    } else if at_effect(p) {
        p.error("a callable effect requires `->` and a result type".to_owned());
        effect_annotation(p);
    }
    p.expect(K::DO_KW, "`do` before function body");
    let old_loop = std::mem::replace(&mut p.loop_depth, 0);
    block(p);
    p.loop_depth = old_loop;
    p.expect(K::END_KW, "`end` after function body");
}
pub(super) fn alias_decl(p: &mut Parser<'_>) {
    let marker = p.start();
    p.bump();
    p.expect(K::IDENT, "type alias name");
    if p.at(K::LESS) {
        type_param_list(p, false, K::TYPE_PARAM_LIST);
    }
    p.expect(K::EQ, "`=` after type alias");
    type_expr(p);
    marker.complete(&mut p.events, K::ALIAS_DECL);
}
pub(super) fn callable_type_param_list(p: &mut Parser<'_>) {
    type_param_list(p, true, K::CALLABLE_TYPE_PARAM_LIST);
}
pub(super) fn type_param_list(p: &mut Parser<'_>, allow_constraints: bool, kind: K) {
    let parameters = p.start();
    p.expect(K::LESS, "`<` before type parameters");
    let mut seen = HashSet::new();
    if !p.at(K::GREATER) && !p.at_end() {
        loop {
            let variable = p.start();
            p.expect(K::APOSTROPHE, "`'` before type variable");
            let span = p.current_span();
            let name = p.current_text().unwrap_or_default().to_owned();
            if p.expect(K::IDENT, "type variable name") && !seen.insert(name.clone()) {
                p.error_at(span, format!("duplicate type parameter `'{}'", name));
            }
            variable.complete(&mut p.events, K::TYPE_VARIABLE);
            if allow_constraints && p.at(K::COLON) {
                let constraint = p.start();
                p.bump();
                type_expr(p);
                constraint.complete(&mut p.events, K::TYPE_CONSTRAINT);
            }
            if !p.bump_if(K::COMMA) || p.at(K::GREATER) {
                break;
            }
        }
    }
    p.expect(K::GREATER, "`>` after type parameters");
    parameters.complete(&mut p.events, kind);
}
pub(super) fn let_stmt(p: &mut Parser<'_>) {
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
    if p.at(K::COLON) {
        type_annotation(p);
    }
    p.expect(K::EQ, "`=` after let pattern");
    expression(p);
    marker.complete(&mut p.events, K::LET_STMT);
}
pub(super) fn type_annotation(p: &mut Parser<'_>) {
    let marker = p.start();
    p.bump();
    type_expr(p);
    marker.complete(&mut p.events, K::TYPE_ANNOTATION);
}
pub(super) fn parameter_type_annotation(p: &mut Parser<'_>) {
    let marker = p.start();
    p.bump();
    type_expr_before_post(p);
    marker.complete(&mut p.events, K::TYPE_ANNOTATION);
}
pub(super) fn post_type(p: &mut Parser<'_>) {
    let marker = p.start();
    p.bump();
    let ty = p.start();
    type_union(p);
    ty.complete(&mut p.events, K::TYPE_EXPR);
    marker.complete(&mut p.events, K::POST_TYPE);
}
