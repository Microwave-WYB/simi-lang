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
pub(super) fn try_expr(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn case_expr(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn pattern_clauses(
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
pub(super) fn loop_expr(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn continue_expr(p: &mut Parser<'_>) -> Parsed {
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
pub(super) fn break_expr(p: &mut Parser<'_>) -> Parsed {
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
