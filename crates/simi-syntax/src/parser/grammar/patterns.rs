use super::*;

pub(super) fn pattern(p: &mut Parser<'_>, bindings: &mut HashSet<String>) {
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
pub(super) fn list_pattern(p: &mut Parser<'_>, bindings: &mut HashSet<String>) {
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
pub(super) fn map_pattern(p: &mut Parser<'_>, bindings: &mut HashSet<String>) {
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
pub(super) fn rest_pattern(p: &mut Parser<'_>, bindings: &mut HashSet<String>) {
    let marker = p.start();
    p.expect(K::DOT_DOT, "`..`");
    if p.at(K::IDENT) {
        let name = p.current_text().unwrap_or_default().to_owned();
        let span = p.current_span();
        p.bump();
        if !name.starts_with('_') && !bindings.insert(name.clone()) {
            p.error_at(span, format!("duplicate binding `{name}` in pattern"));
        }
    }
    marker.complete(&mut p.events, K::REST_PATTERN);
}
