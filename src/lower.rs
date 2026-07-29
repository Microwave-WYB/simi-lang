use std::str::FromStr;

use simi_syntax::ast as support;
use simi_syntax::generated::{self as syntax, AstNode};
use simi_syntax::{SyntaxKind as K, SyntaxNode, SyntaxToken};

use crate::ast;
use crate::span::Span;

mod rebase;

use rebase::*;

pub(crate) fn program(root: SyntaxNode) -> ast::Program {
    let root = syntax::Root::cast(root).expect("parser must produce a root node");
    ast::Program {
        items: root.statements().filter_map(stmt).collect(),
    }
}

pub(crate) fn program_with_origins(
    root: SyntaxNode,
    origins: &simi_syntax::TokenOrigins,
) -> ast::Program {
    let mut program = program(root);
    rebase_program(&mut program, origins);
    program
}

fn stmt(node: syntax::Stmt) -> Option<ast::Stmt> {
    let span = span(node.syntax());
    let kind = match node {
        syntax::Stmt::FunctionDecl(node) => {
            let name = direct_token(node.syntax(), K::IDENT)
                .expect("valid function has a name")
                .text()
                .to_string();
            let params = support::child::<syntax::ParamList>(node.syntax())
                .expect("valid function has params");
            let params = support::children::<syntax::Param>(params.syntax())
                .filter_map(|param| direct_token(param.syntax(), K::IDENT))
                .map(|token| token.text().to_string())
                .collect();
            let body =
                lower_body(support::child(node.syntax()).expect("valid function has a body"));
            ast::StmtKind::Function { name, params, body }
        }
        syntax::Stmt::AliasDecl(_) => return None,
        syntax::Stmt::LetStmt(node) => {
            let pattern =
                lower_pattern(support::child(node.syntax()).expect("valid let has a pattern"));
            let value = lower_expr(support::child(node.syntax()).expect("valid let has a value"));
            ast::StmtKind::Let { pattern, value }
        }
        syntax::Stmt::ExprStmt(node) => ast::StmtKind::Expr(lower_expr(
            support::child(node.syntax()).expect("valid expression statement"),
        )),
    };
    Some(ast::Stmt { kind, span })
}

fn lower_block(node: syntax::Block) -> ast::Block {
    let items = node.statements().filter_map(stmt).collect::<Vec<_>>();
    let span = match (items.first(), items.last()) {
        (Some(first), Some(last)) => first.span.merge(last.span),
        _ => {
            let offset = previous_nontrivia_token(node.syntax())
                .map_or_else(|| span(node.syntax()).start, |token| token_span(&token).end);
            Span::new(offset, offset)
        }
    };
    ast::Block { items, span }
}

fn lower_expr(node: syntax::Expr) -> ast::Expr {
    let node_span = span(node.syntax());
    let kind = match node {
        syntax::Expr::Literal(node) => literal_expr(node.syntax()),
        syntax::Expr::Name(node) => ast::ExprKind::Variable(
            direct_token(node.syntax(), K::IDENT)
                .expect("name token")
                .text()
                .to_string(),
        ),
        syntax::Expr::Function(node) => {
            let params =
                support::child::<syntax::ParamList>(node.syntax()).expect("function params");
            let params = support::children::<syntax::Param>(params.syntax())
                .filter_map(|param| direct_token(param.syntax(), K::IDENT))
                .map(|token| token.text().to_string())
                .collect();
            let body = lower_body(support::child(node.syntax()).expect("function body"));
            ast::ExprKind::Function { params, body }
        }
        syntax::Expr::Block(node) => ast::ExprKind::Block(lower_block(
            support::child(node.syntax()).expect("block body"),
        )),
        syntax::Expr::Paren(node) => {
            let mut inner = lower_expr(child_expr(node.syntax(), 0));
            inner.span = node_span;
            return inner;
        }
        syntax::Expr::List(node) => ast::ExprKind::List(
            support::children::<syntax::ListElement>(node.syntax())
                .map(|element| {
                    let value = lower_expr(child_expr(element.syntax(), 0));
                    if support::token(element.syntax(), K::DOT_DOT).is_some() {
                        ast::ListElement::Spread(value)
                    } else {
                        ast::ListElement::Value(value)
                    }
                })
                .collect(),
        ),
        syntax::Expr::Bytes(node) => ast::ExprKind::Bytes(
            expr_children(node.syntax())
                .map(|segment| {
                    if let syntax::Expr::Literal(literal) = &segment
                        && let Some(token) = direct_token(literal.syntax(), K::STRING)
                    {
                        ast::BytesSegment::String(decode_string(token.text()))
                    } else {
                        ast::BytesSegment::Value(lower_expr(segment))
                    }
                })
                .collect(),
        ),
        syntax::Expr::Map(node) => {
            let entries = support::children::<syntax::MapEntry>(node.syntax())
                .map(|entry| lower_map_entry(&entry))
                .collect();
            ast::ExprKind::Map(entries)
        }
        syntax::Expr::Call(node) => {
            let callee = lower_expr(child_expr(node.syntax(), 0));
            let args = support::child::<syntax::ArgList>(node.syntax()).expect("call args");
            let args = expr_children(args.syntax()).map(lower_expr).collect();
            ast::ExprKind::Call {
                callee: Box::new(callee),
                args,
            }
        }
        syntax::Expr::Field(node) => {
            let object = lower_expr(child_expr(node.syntax(), 0));
            let name = direct_token(node.syntax(), K::IDENT)
                .expect("field name")
                .text()
                .to_string();
            ast::ExprKind::Field {
                object: Box::new(object),
                name,
            }
        }
        syntax::Expr::Index(node) => {
            let mut expressions = expr_children(node.syntax());
            let object = lower_expr(expressions.next().expect("index object"));
            let key = lower_expr(expressions.next().expect("index key"));
            ast::ExprKind::Index {
                object: Box::new(object),
                key: Box::new(key),
            }
        }
        syntax::Expr::NilPropagate(node) => ast::ExprKind::NilPropagate {
            value: Box::new(lower_expr(child_expr(node.syntax(), 0))),
        },
        syntax::Expr::Unary(node) => {
            let op = if direct_token(node.syntax(), K::MINUS).is_some() {
                ast::UnaryOp::Negate
            } else {
                ast::UnaryOp::Not
            };
            ast::ExprKind::Unary {
                op,
                value: Box::new(lower_expr(child_expr(node.syntax(), 0))),
            }
        }
        syntax::Expr::Binary(node) => {
            let mut expressions = expr_children(node.syntax());
            let left = lower_expr(expressions.next().expect("binary left"));
            let right = lower_expr(expressions.next().expect("binary right"));
            let op = binary_operator(node.syntax());
            ast::ExprKind::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            }
        }
        syntax::Expr::Assign(node) => {
            let mut expressions = expr_children(node.syntax());
            let target_expr = lower_expr(expressions.next().expect("assignment target"));
            let value = lower_expr(expressions.next().expect("assignment value"));
            let target = assignment_target(target_expr);
            ast::ExprKind::Assign {
                target,
                value: Box::new(value),
            }
        }
        syntax::Expr::Pipeline(node) => {
            let input = lower_expr(child_expr(node.syntax(), 0));
            let stages = support::children::<syntax::PipelineStage>(node.syntax())
                .map(lower_pipeline_stage)
                .collect();
            ast::ExprKind::Pipeline {
                input: Box::new(input),
                stages,
            }
        }
        syntax::Expr::TrailingArgument(node) => {
            let mut expressions = expr_children(node.syntax());
            let mut call = lower_expr(expressions.next().expect("trailing call"));
            let trailing = lower_expr(expressions.next().expect("trailing value"));
            match &mut call.kind {
                ast::ExprKind::Call { args, .. } => args.push(trailing),
                _ => unreachable!("parser validates trailing call"),
            }
            call.span = node_span;
            return call;
        }
        syntax::Expr::Raise(node) => ast::ExprKind::Raise {
            value: Box::new(lower_expr(child_expr(node.syntax(), 0))),
        },
        syntax::Expr::Panic(node) => ast::ExprKind::Panic {
            reason: direct_token(node.syntax(), K::STRING).map(|token| decode_string(token.text())),
        },
        syntax::Expr::Todo(node) => ast::ExprKind::Todo {
            note: direct_token(node.syntax(), K::STRING).map(|token| decode_string(token.text())),
        },
        syntax::Expr::Protected(node) => {
            let protected = lower_block(support::child(node.syntax()).expect("protected block"));
            let clauses = support::children::<syntax::CatchArm>(node.syntax())
                .map(lower_clause)
                .collect();
            ast::ExprKind::Try { protected, clauses }
        }
        syntax::Expr::Case(node) => {
            let value = lower_expr(child_expr(node.syntax(), 0));
            let clauses = support::children::<syntax::CaseClause>(node.syntax())
                .map(lower_clause)
                .collect();
            ast::ExprKind::Case {
                value: Box::new(value),
                clauses,
            }
        }
        syntax::Expr::If(node) => {
            let branches = support::children::<syntax::IfBranch>(node.syntax())
                .map(|branch| {
                    let condition = lower_expr(child_expr(branch.syntax(), 0));
                    let body =
                        lower_block(support::child(branch.syntax()).expect("if branch body"));
                    (condition, body)
                })
                .collect();
            let else_branch = support::child::<syntax::ElseBranch>(node.syntax())
                .map(|branch| lower_block(support::child(branch.syntax()).expect("else body")));
            ast::ExprKind::If {
                branches,
                else_branch,
            }
        }
    };
    ast::Expr {
        kind,
        span: node_span,
    }
}

fn lower_map_entry(entry: &syntax::MapEntry) -> (ast::Expr, ast::Expr) {
    let expressions = expr_children(entry.syntax()).collect::<Vec<_>>();
    if let Some(name) = direct_token(entry.syntax(), K::IDENT) {
        let key = ast::Expr {
            kind: ast::ExprKind::String(name.text().to_string()),
            span: token_span(&name),
        };
        let value = expressions
            .first()
            .cloned()
            .map(lower_expr)
            .unwrap_or_else(|| ast::Expr {
                kind: ast::ExprKind::Variable(name.text().to_string()),
                span: token_span(&name),
            });
        (key, value)
    } else {
        (
            lower_expr(expressions[0].clone()),
            lower_expr(expressions[1].clone()),
        )
    }
}

fn lower_pipeline_stage(node: syntax::PipelineStage) -> ast::PipelineStage {
    let expressions = expr_children(node.syntax()).collect::<Vec<_>>();
    let callee = lower_expr(expressions[0].clone());
    let args_node = support::child::<syntax::ArgList>(node.syntax()).expect("pipeline args");
    let mut args = expr_children(args_node.syntax())
        .map(lower_expr)
        .collect::<Vec<_>>();
    if expressions.len() > 1 {
        args.push(lower_expr(expressions[1].clone()));
    }
    ast::PipelineStage {
        nil_aware: direct_token(node.syntax(), K::QUESTION_GREATER).is_some(),
        tap: direct_token(node.syntax(), K::TAP_KW).is_some(),
        callee,
        args,
        span: span(node.syntax()),
    }
}

fn lower_body(node: syntax::Body) -> ast::Body {
    if let Some(block) = support::child::<syntax::Block>(node.syntax()) {
        lower_block(block)
    } else {
        let expression = lower_expr(support::child(node.syntax()).expect("body expression"));
        let span = expression.span;
        ast::Block {
            items: vec![ast::Stmt {
                kind: ast::StmtKind::Expr(expression),
                span,
            }],
            span,
        }
    }
}

fn lower_clause<N: AstNode>(node: N) -> ast::PatternClause {
    let pattern = lower_pattern(support::child(node.syntax()).expect("clause pattern"));
    let expressions = support::children::<syntax::Expr>(node.syntax()).collect::<Vec<_>>();
    let guard = expressions.first().cloned().map(lower_expr);
    let body = lower_body(support::child(node.syntax()).expect("clause body"));
    ast::PatternClause {
        pattern,
        guard,
        body,
    }
}

fn lower_pattern(node: syntax::Pattern) -> ast::Pattern {
    let node_span = span(node.syntax());
    let kind = match node {
        syntax::Pattern::Binding(node) => ast::PatternKind::Binding(
            direct_token(node.syntax(), K::IDENT)
                .expect("binding")
                .text()
                .to_string(),
        ),
        syntax::Pattern::Wildcard(_) => ast::PatternKind::Wildcard,
        syntax::Pattern::Literal(node) => literal_pattern(node.syntax()),
        syntax::Pattern::List(node) => {
            let elements = pattern_children(node.syntax()).map(lower_pattern).collect();
            let rest = support::child::<syntax::RestPattern>(node.syntax()).map(lower_rest);
            ast::PatternKind::List { elements, rest }
        }
        syntax::Pattern::Bytes(node) => ast::PatternKind::Bytes(
            support::children::<syntax::BytesPatternSegment>(node.syntax())
                .map(lower_bytes_pattern_segment)
                .collect(),
        ),
        syntax::Pattern::Map(node) => {
            let fields = support::children::<syntax::MapPatternField>(node.syntax())
                .map(|field| {
                    let name = direct_token(field.syntax(), K::IDENT)
                        .expect("map pattern name")
                        .text()
                        .to_string();
                    let pattern = support::child(field.syntax()).map_or_else(
                        || ast::Pattern {
                            kind: if name.starts_with('_') {
                                ast::PatternKind::Wildcard
                            } else {
                                ast::PatternKind::Binding(name.clone())
                            },
                            span: span(field.syntax()),
                        },
                        lower_pattern,
                    );
                    (name, pattern)
                })
                .collect();
            let rest = support::child::<syntax::RestPattern>(node.syntax()).map(lower_rest);
            ast::PatternKind::Map { fields, rest }
        }
    };
    ast::Pattern {
        kind,
        span: node_span,
    }
}

fn lower_bytes_pattern_segment(node: syntax::BytesPatternSegment) -> ast::BytesPatternSegment {
    if let Some(string) = direct_token(node.syntax(), K::STRING) {
        return ast::BytesPatternSegment::String(decode_string(string.text()));
    }
    if let Some(rest) = support::child::<syntax::RestPattern>(node.syntax()) {
        return ast::BytesPatternSegment::Remainder(match lower_rest(rest) {
            ast::PatternRest::Discard => None,
            ast::PatternRest::Binding(name) => Some(name),
        });
    }

    let name = direct_token(node.syntax(), K::IDENT)
        .map(|token| token.text().to_owned())
        .filter(|name| !name.starts_with('_'));
    if direct_token(node.syntax(), K::COLON).is_none() {
        return ast::BytesPatternSegment::Byte(name);
    }
    let length =
        direct_token(node.syntax(), K::INT).expect("valid fixed byte capture has an integer width");
    ast::BytesPatternSegment::Fixed {
        name,
        length: parse_int_literal(length.text()) as usize,
    }
}

fn lower_rest(node: syntax::RestPattern) -> ast::PatternRest {
    let Some(name) = direct_token(node.syntax(), K::IDENT).map(|token| token.text().to_string())
    else {
        return ast::PatternRest::Discard;
    };
    if name.starts_with('_') {
        ast::PatternRest::Discard
    } else {
        ast::PatternRest::Binding(name)
    }
}

fn literal_expr(node: &SyntaxNode) -> ast::ExprKind {
    let token = first_direct_token(node).expect("literal token");
    match token.kind() {
        K::INT => ast::ExprKind::Int(parse_int_literal(token.text())),
        K::FLOAT => ast::ExprKind::Float(parse_float_literal(token.text())),
        K::STRING => ast::ExprKind::String(decode_string(token.text())),
        K::NIL_KW => ast::ExprKind::Nil,
        K::TRUE_KW => ast::ExprKind::Bool(true),
        K::FALSE_KW => ast::ExprKind::Bool(false),
        _ => unreachable!("literal token kind"),
    }
}
fn literal_pattern(node: &SyntaxNode) -> ast::PatternKind {
    let token = first_direct_token(node).expect("literal token");
    match token.kind() {
        K::INT => ast::PatternKind::Int(parse_int_literal(token.text())),
        K::FLOAT => ast::PatternKind::Float(parse_float_literal(token.text())),
        K::STRING => ast::PatternKind::String(decode_string(token.text())),
        K::NIL_KW => ast::PatternKind::Nil,
        K::TRUE_KW => ast::PatternKind::Bool(true),
        K::FALSE_KW => ast::PatternKind::Bool(false),
        _ => unreachable!("literal token kind"),
    }
}

fn parse_int_literal(text: &str) -> i64 {
    let normalized = text.replace('_', "");
    if let Some(digits) = normalized
        .strip_prefix("0b")
        .or_else(|| normalized.strip_prefix("0B"))
    {
        i64::from_str_radix(digits, 2).expect("validated binary integer")
    } else if let Some(digits) = normalized
        .strip_prefix("0x")
        .or_else(|| normalized.strip_prefix("0X"))
    {
        i64::from_str_radix(digits, 16).expect("validated hexadecimal integer")
    } else {
        i64::from_str(&normalized).expect("validated integer")
    }
}

fn parse_float_literal(text: &str) -> f64 {
    f64::from_str(&text.replace('_', "")).expect("validated float")
}

fn decode_string(text: &str) -> String {
    let body = text
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .expect("validated string");
    let mut result = String::new();
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        result.push(match chars.next().expect("validated escape") {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            _ => unreachable!("validated escape"),
        });
    }
    result
}

fn assignment_target(expr: ast::Expr) -> ast::AssignmentTarget {
    let span = expr.span;
    let kind = match expr.kind {
        ast::ExprKind::Variable(name) => ast::AssignmentTargetKind::Variable(name),
        ast::ExprKind::Field { object, name } => ast::AssignmentTargetKind::Field { object, name },
        ast::ExprKind::Index { object, key } => ast::AssignmentTargetKind::Index { object, key },
        _ => unreachable!("parser validates assignment targets"),
    };
    ast::AssignmentTarget { kind, span }
}
fn binary_operator(node: &SyntaxNode) -> ast::BinaryOp {
    let kinds = [
        K::PLUS,
        K::MINUS,
        K::STAR,
        K::SLASH,
        K::SLASH_SLASH,
        K::PERCENT,
        K::LESS_GREATER,
        K::EQ_EQ,
        K::BANG_EQ,
        K::LESS,
        K::LESS_EQ,
        K::GREATER,
        K::GREATER_EQ,
        K::AND_KW,
        K::OR_KW,
    ];
    let kind = node
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .map(|token| token.kind())
        .find(|kind| kinds.contains(kind))
        .expect("binary operator");
    match kind {
        K::PLUS => ast::BinaryOp::Add,
        K::MINUS => ast::BinaryOp::Subtract,
        K::STAR => ast::BinaryOp::Multiply,
        K::SLASH => ast::BinaryOp::Divide,
        K::SLASH_SLASH => ast::BinaryOp::FloorDivide,
        K::PERCENT => ast::BinaryOp::Remainder,
        K::LESS_GREATER => ast::BinaryOp::Concatenate,
        K::EQ_EQ => ast::BinaryOp::Equal,
        K::BANG_EQ => ast::BinaryOp::NotEqual,
        K::LESS => ast::BinaryOp::Less,
        K::LESS_EQ => ast::BinaryOp::LessEqual,
        K::GREATER => ast::BinaryOp::Greater,
        K::GREATER_EQ => ast::BinaryOp::GreaterEqual,
        K::AND_KW => ast::BinaryOp::And,
        K::OR_KW => ast::BinaryOp::Or,
        _ => unreachable!(),
    }
}

fn child_expr(node: &SyntaxNode, index: usize) -> syntax::Expr {
    expr_children(node).nth(index).expect("expression child")
}
fn expr_children(node: &SyntaxNode) -> impl Iterator<Item = syntax::Expr> + '_ {
    node.children().filter_map(syntax::Expr::cast)
}
fn pattern_children(node: &SyntaxNode) -> impl Iterator<Item = syntax::Pattern> + '_ {
    node.children().filter_map(syntax::Pattern::cast)
}

fn direct_token(node: &SyntaxNode, kind: K) -> Option<SyntaxToken> {
    support::token(node, kind)
}
fn first_direct_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia())
}
fn previous_nontrivia_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    let mut element = node.prev_sibling_or_token();
    while let Some(current) = element {
        match current {
            simi_syntax::syntax::SyntaxElement::Token(token) if !token.kind().is_trivia() => {
                return Some(token);
            }
            simi_syntax::syntax::SyntaxElement::Token(token) => {
                element = token.prev_sibling_or_token()
            }
            simi_syntax::syntax::SyntaxElement::Node(previous) => {
                if let Some(token) = previous
                    .last_token()
                    .filter(|token| !token.kind().is_trivia())
                {
                    return Some(token);
                }
                element = previous.prev_sibling_or_token();
            }
        }
    }
    None
}
fn span(node: &SyntaxNode) -> Span {
    let range = node.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}
fn token_span(token: &SyntaxToken) -> Span {
    let range = token.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}
