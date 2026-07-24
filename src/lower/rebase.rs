use super::*;

pub(super) fn rebase_program(program: &mut ast::Program, origins: &simi_syntax::TokenOrigins) {
    for statement in &mut program.items {
        rebase_stmt(statement, origins);
    }
}
pub(super) fn rebase_stmt(statement: &mut ast::Stmt, origins: &simi_syntax::TokenOrigins) {
    match &mut statement.kind {
        ast::StmtKind::Function { body, .. } => rebase_block(body, origins),
        ast::StmtKind::Let { pattern, value } => {
            rebase_pattern(pattern, origins);
            rebase_expr(value, origins);
        }
        ast::StmtKind::Expr(expression) => rebase_expr(expression, origins),
    }
    statement.span = origins.rebase(statement.span);
}
pub(super) fn rebase_block(block: &mut ast::Block, origins: &simi_syntax::TokenOrigins) {
    for statement in &mut block.items {
        rebase_stmt(statement, origins);
    }
    block.span = origins.rebase(block.span);
}
pub(super) fn rebase_clause(clause: &mut ast::PatternClause, origins: &simi_syntax::TokenOrigins) {
    rebase_pattern(&mut clause.pattern, origins);
    if let Some(guard) = &mut clause.guard {
        rebase_expr(guard, origins);
    }
    rebase_block(&mut clause.body, origins);
}
pub(super) fn rebase_pattern(pattern: &mut ast::Pattern, origins: &simi_syntax::TokenOrigins) {
    match &mut pattern.kind {
        ast::PatternKind::List { elements, .. } => {
            for element in elements {
                rebase_pattern(element, origins);
            }
        }
        ast::PatternKind::Map { fields, .. } => {
            for (_, field) in fields {
                rebase_pattern(field, origins);
            }
        }
        ast::PatternKind::Wildcard
        | ast::PatternKind::Binding(_)
        | ast::PatternKind::Int(_)
        | ast::PatternKind::Float(_)
        | ast::PatternKind::String(_)
        | ast::PatternKind::Bool(_)
        | ast::PatternKind::Nil => {}
    }
    pattern.span = origins.rebase(pattern.span);
}
pub(super) fn rebase_target(
    target: &mut ast::AssignmentTarget,
    origins: &simi_syntax::TokenOrigins,
) {
    match &mut target.kind {
        ast::AssignmentTargetKind::Variable(_) => {}
        ast::AssignmentTargetKind::Field { object, .. } => rebase_expr(object, origins),
        ast::AssignmentTargetKind::Index { object, key } => {
            rebase_expr(object, origins);
            rebase_expr(key, origins);
        }
    }
    target.span = origins.rebase(target.span);
}
pub(super) fn rebase_expr(expression: &mut ast::Expr, origins: &simi_syntax::TokenOrigins) {
    match &mut expression.kind {
        ast::ExprKind::List(elements) => {
            for element in elements {
                rebase_expr(element, origins);
            }
        }
        ast::ExprKind::Map(entries) => {
            for (key, value) in entries {
                rebase_expr(key, origins);
                rebase_expr(value, origins);
            }
        }
        ast::ExprKind::Function { body, .. } | ast::ExprKind::Block(body) => {
            rebase_block(body, origins)
        }
        ast::ExprKind::Assign { target, value } => {
            rebase_target(target, origins);
            rebase_expr(value, origins);
        }
        ast::ExprKind::Raise { value }
        | ast::ExprKind::NilPropagate { value }
        | ast::ExprKind::Unary { value, .. }
        | ast::ExprKind::Break { value }
        | ast::ExprKind::Continue { value } => rebase_expr(value, origins),
        ast::ExprKind::Try { protected, clauses } => {
            rebase_block(protected, origins);
            for clause in clauses {
                rebase_clause(clause, origins);
            }
        }
        ast::ExprKind::Case { value, clauses } => {
            rebase_expr(value, origins);
            for clause in clauses {
                rebase_clause(clause, origins);
            }
        }
        ast::ExprKind::If {
            branches,
            else_branch,
        } => {
            for (condition, body) in branches {
                rebase_expr(condition, origins);
                rebase_block(body, origins);
            }
            if let Some(body) = else_branch {
                rebase_block(body, origins);
            }
        }
        ast::ExprKind::Loop { initial, body, .. } => {
            rebase_expr(initial, origins);
            rebase_block(body, origins);
        }
        ast::ExprKind::Call { callee, args } => {
            rebase_expr(callee, origins);
            for argument in args {
                rebase_expr(argument, origins);
            }
        }
        ast::ExprKind::Field { object, .. } => rebase_expr(object, origins),
        ast::ExprKind::Index { object, key } => {
            rebase_expr(object, origins);
            rebase_expr(key, origins);
        }
        ast::ExprKind::Binary { left, right, .. } => {
            rebase_expr(left, origins);
            rebase_expr(right, origins);
        }
        ast::ExprKind::Pipeline { input, stages } => {
            rebase_expr(input, origins);
            for stage in stages {
                rebase_expr(&mut stage.callee, origins);
                for argument in &mut stage.args {
                    rebase_expr(argument, origins);
                }
                stage.span = origins.rebase(stage.span);
            }
        }
        ast::ExprKind::Int(_)
        | ast::ExprKind::Float(_)
        | ast::ExprKind::String(_)
        | ast::ExprKind::Bool(_)
        | ast::ExprKind::Nil
        | ast::ExprKind::Variable(_) => {}
    }
    expression.span = origins.rebase(expression.span);
}
