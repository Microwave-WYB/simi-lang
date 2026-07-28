use super::*;

pub(super) fn raise_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    expression(p);
    Parsed {
        marker: marker.complete(&mut p.events, K::RAISE_EXPR),
        flavor: Flavor::Other,
    }
}
pub(super) fn terminal_expr(p: &mut Parser<'_>, kind: K) -> Parsed {
    let marker = p.start();
    let keyword = p.current_text().unwrap_or("terminal").to_owned();
    p.bump();
    if p.at(K::STRING) {
        p.bump();
    } else if p.at(K::INT) || p.at(K::FLOAT) || p.at(K::IDENT) {
        p.error(format!("`{keyword}` note must be a string"));
    }
    Parsed {
        marker: marker.complete(&mut p.events, kind),
        flavor: Flavor::Other,
    }
}
pub(super) fn legacy_try_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.error("`try` was removed; use `do … catch of … end`".to_owned());
    p.bump();
    Parsed {
        marker: marker.complete(&mut p.events, K::ERROR),
        flavor: Flavor::Other,
    }
}
pub(super) fn case_expr(p: &mut Parser<'_>) -> Parsed {
    let marker = p.start();
    p.bump();
    expression(p);
    pattern_clauses(p, K::CASE_CLAUSE, "`of` after case value");
    p.expect(K::END_KW, "`end` after case expression");
    Parsed {
        marker: marker.complete(&mut p.events, K::CASE_EXPR),
        flavor: Flavor::Other,
    }
}
pub(super) fn pattern_clauses(p: &mut Parser<'_>, clause_kind: K, first_marker: &str) {
    let mut first = true;
    while p.at(K::OF_KW) || first {
        let clause = p.start();
        if !p.expect(
            K::OF_KW,
            if first {
                first_marker
            } else {
                "`of` before next arm"
            },
        ) {
            clause.complete(&mut p.events, clause_kind);
            break;
        }
        first = false;
        if p.at(K::END_KW) {
            p.error("expected pattern after `of`, found `end`".to_owned());
            clause.complete(&mut p.events, clause_kind);
            break;
        }
        let mut bindings = HashSet::new();
        pattern(p, &mut bindings);
        if p.bump_if(K::WHEN_KW) {
            expression(p);
        }
        let body = p.start();
        p.block_depth += 1;
        if p.at(K::DO_KW) && !do_starts_protected(p) {
            p.bump();
            block(p);
            p.expect(K::END_KW, "`end` after clause block body");
        } else {
            expression(p);
        }
        p.block_depth -= 1;
        body.complete(&mut p.events, K::BODY);
        clause.complete(&mut p.events, clause_kind);
    }
}
pub(super) fn if_expr(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn if_branch_after_marker(p: &mut Parser<'_>) {
    let marker = p.start();
    expression(p);
    p.expect(K::THEN_KW, "`then` after if condition");
    block(p);
    marker.complete(&mut p.events, K::IF_BRANCH);
}
