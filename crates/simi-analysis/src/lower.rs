use la_arena::Arena;
use simi_syntax::ast as support;
use simi_syntax::generated::{self as syntax, AstNode};
use simi_syntax::span::Span;
use simi_syntax::{SyntaxKind as K, SyntaxNode, SyntaxToken};

use crate::model::{
    ExprData, Hir, OccurrenceKind, PatternData, ScopeData, ScopeId, SymbolData, SymbolKind,
};

pub(crate) fn lower(root: SyntaxNode) -> Hir {
    let root_node = syntax::Root::cast(root.clone()).expect("parser produces a root");
    let mut builder = Builder::new(span(&root));
    for statement in root_node.statements() {
        builder.statement(statement, builder.root_scope);
    }
    builder.finish()
}

struct Builder {
    scopes: Arena<ScopeData>,
    symbols: Arena<SymbolData>,
    expressions: Arena<ExprData>,
    patterns: Arena<PatternData>,
    occurrences: Vec<crate::model::NameOccurrence>,
    root_scope: ScopeId,
}

impl Builder {
    fn new(root_span: Span) -> Self {
        let mut scopes = Arena::new();
        let root_scope = scopes.alloc(ScopeData {
            parent: None,
            span: root_span,
            function_depth: 0,
            symbols: Vec::new(),
        });
        let mut this = Self {
            scopes,
            symbols: Arena::new(),
            expressions: Arena::new(),
            patterns: Arena::new(),
            occurrences: Vec::new(),
            root_scope,
        };
        for (name, parameter) in [
            ("require", "module"),
            ("type", "value"),
            ("inspect", "value"),
        ] {
            this.declare(
                root_scope,
                name.to_owned(),
                SymbolKind::Builtin,
                None,
                Some((vec![parameter.to_owned()], None)),
                0,
            );
        }
        this
    }

    fn finish(self) -> Hir {
        Hir {
            scopes: self.scopes,
            symbols: self.symbols,
            expressions: self.expressions,
            patterns: self.patterns,
            occurrences: self.occurrences,
            root_scope: self.root_scope,
        }
    }

    fn child_scope(&mut self, parent: ScopeId, node: &SyntaxNode, function: bool) -> ScopeId {
        self.scopes.alloc(ScopeData {
            parent: Some(parent),
            span: span(node),
            function_depth: self.scopes[parent].function_depth + u32::from(function),
            symbols: Vec::new(),
        })
    }

    fn declare(
        &mut self,
        scope: ScopeId,
        name: String,
        kind: SymbolKind,
        declaration: Option<Span>,
        function: Option<(Vec<String>, Option<String>)>,
        activation: usize,
    ) {
        let (parameters, documentation) = function
            .map(|(parameters, documentation)| (Some(parameters), documentation))
            .unwrap_or((None, None));
        let arity = parameters.as_ref().map(Vec::len);
        let symbol = self.symbols.alloc(SymbolData {
            name,
            kind,
            declaration,
            scope,
            arity,
            parameters,
            documentation,
            builtin: kind == SymbolKind::Builtin,
            activation,
        });
        self.scopes[scope].symbols.push(symbol);
    }

    fn statement(&mut self, statement: syntax::Stmt, scope: ScopeId) {
        match statement {
            syntax::Stmt::FunctionDecl(node) => {
                let Some(name) = direct_token(node.syntax(), K::IDENT) else {
                    return;
                };
                let params = support::child::<syntax::ParamList>(node.syntax());
                let parameters = params.as_ref().map_or_else(Vec::new, |params| {
                    support::children::<syntax::Param>(params.syntax())
                        .filter_map(|param| direct_token(param.syntax(), K::IDENT))
                        .map(|token| token.text().to_string())
                        .collect()
                });
                self.declare(
                    scope,
                    name.text().to_string(),
                    SymbolKind::Function,
                    Some(token_span(&name)),
                    Some((parameters, documentation(node.syntax()))),
                    token_span(&name).start,
                );
                let function_scope = self.child_scope(scope, node.syntax(), true);
                if let Some(params) = params {
                    for token in support::children::<syntax::Param>(params.syntax())
                        .filter_map(|param| direct_token(param.syntax(), K::IDENT))
                    {
                        self.declare(
                            function_scope,
                            token.text().to_string(),
                            SymbolKind::Parameter,
                            Some(token_span(&token)),
                            None,
                            token_span(&token).start,
                        );
                    }
                }
                if let Some(body) = support::child::<syntax::Block>(node.syntax()) {
                    self.block(&body, function_scope);
                }
            }
            syntax::Stmt::AliasDecl(_) => {}
            syntax::Stmt::LetStmt(node) => {
                if let Some(value) = support::child::<syntax::Expr>(node.syntax()) {
                    self.expression(value, scope);
                }
                if let Some(pattern) = support::child::<syntax::Pattern>(node.syntax()) {
                    let simple = matches!(pattern, syntax::Pattern::Binding(_));
                    let symbol_count = self.scopes[scope].symbols.len();
                    let activation = u32::from(node.syntax().text_range().end()) as usize;
                    self.pattern(pattern, scope, activation, simple);
                    if simple
                        && self.scopes[scope].symbols.len() > symbol_count
                        && let Some(symbol) = self.scopes[scope].symbols.last().copied()
                    {
                        self.symbols[symbol].documentation = documentation(node.syntax());
                    }
                }
            }
            syntax::Stmt::ExprStmt(node) => {
                if let Some(expression) = support::child::<syntax::Expr>(node.syntax()) {
                    self.expression(expression, scope);
                }
            }
        }
    }

    fn block(&mut self, block: &syntax::Block, scope: ScopeId) {
        for statement in block.statements() {
            self.statement(statement, scope);
        }
    }

    fn expression(&mut self, expression: syntax::Expr, scope: ScopeId) {
        self.expressions.alloc(ExprData {
            span: span(expression.syntax()),
            scope,
        });
        match expression {
            syntax::Expr::Name(node) => {
                if let Some(token) = direct_token(node.syntax(), K::IDENT) {
                    self.occurrence(token, scope, OccurrenceKind::Read);
                }
            }
            syntax::Expr::Function(node) => {
                let function_scope = self.child_scope(scope, node.syntax(), true);
                if let Some(params) = support::child::<syntax::ParamList>(node.syntax()) {
                    for token in support::children::<syntax::Param>(params.syntax())
                        .filter_map(|param| direct_token(param.syntax(), K::IDENT))
                    {
                        self.declare(
                            function_scope,
                            token.text().to_string(),
                            SymbolKind::Parameter,
                            Some(token_span(&token)),
                            None,
                            token_span(&token).start,
                        );
                    }
                }
                if let Some(body) = support::child::<syntax::Block>(node.syntax()) {
                    self.block(&body, function_scope);
                }
            }
            syntax::Expr::Block(node) => {
                if let Some(block) = support::child::<syntax::Block>(node.syntax()) {
                    let child = self.child_scope(scope, block.syntax(), false);
                    self.block(&block, child);
                }
            }
            syntax::Expr::Assign(node) => {
                let mut expressions = expr_children(node.syntax());
                if let Some(target) = expressions.next() {
                    self.assignment_target(target, scope);
                }
                if let Some(value) = expressions.next() {
                    self.expression(value, scope);
                }
            }
            syntax::Expr::Try(node) => {
                if let Some(protected) = support::child::<syntax::Block>(node.syntax()) {
                    let child = self.child_scope(scope, protected.syntax(), false);
                    self.block(&protected, child);
                }
                for clause in support::children::<syntax::CatchClause>(node.syntax()) {
                    self.clause(clause.syntax(), scope);
                }
            }
            syntax::Expr::Case(node) => {
                if let Some(value) = expr_children(node.syntax()).next() {
                    self.expression(value, scope);
                }
                for clause in support::children::<syntax::CaseClause>(node.syntax()) {
                    self.clause(clause.syntax(), scope);
                }
            }
            syntax::Expr::If(node) => {
                for branch in support::children::<syntax::IfBranch>(node.syntax()) {
                    let child = self.child_scope(scope, branch.syntax(), false);
                    if let Some(condition) = expr_children(branch.syntax()).next() {
                        self.expression(condition, scope);
                    }
                    if let Some(body) = support::child::<syntax::Block>(branch.syntax()) {
                        self.block(&body, child);
                    }
                }
                if let Some(branch) = support::child::<syntax::ElseBranch>(node.syntax()) {
                    let child = self.child_scope(scope, branch.syntax(), false);
                    if let Some(body) = support::child::<syntax::Block>(branch.syntax()) {
                        self.block(&body, child);
                    }
                }
            }
            syntax::Expr::Loop(node) => {
                if let Some(initial) = expr_children(node.syntax()).next() {
                    self.expression(initial, scope);
                }
                let child = self.child_scope(scope, node.syntax(), false);
                if let Some(state) = direct_token(node.syntax(), K::IDENT) {
                    self.declare(
                        child,
                        state.text().to_string(),
                        SymbolKind::LoopState,
                        Some(token_span(&state)),
                        None,
                        token_span(&state).start,
                    );
                }
                if let Some(body) = support::child::<syntax::Block>(node.syntax()) {
                    self.block(&body, child);
                }
            }
            other => self.walk_nested(other.syntax(), scope),
        }
    }

    fn assignment_target(&mut self, target: syntax::Expr, scope: ScopeId) {
        match target {
            syntax::Expr::Name(node) => {
                if let Some(token) = direct_token(node.syntax(), K::IDENT) {
                    self.occurrence(token, scope, OccurrenceKind::Assignment);
                }
            }
            other => self.walk_nested(other.syntax(), scope),
        }
    }

    fn walk_nested(&mut self, node: &SyntaxNode, scope: ScopeId) {
        for child in node.children() {
            if let Some(expression) = syntax::Expr::cast(child.clone()) {
                self.expression(expression, scope);
            } else if child.kind() != K::BLOCK && !child.kind().is_pattern() {
                self.walk_nested(&child, scope);
            }
        }
    }

    fn clause(&mut self, node: &SyntaxNode, parent: ScopeId) {
        let scope = self.child_scope(parent, node, false);
        if let Some(pattern) = support::child::<syntax::Pattern>(node) {
            self.pattern(pattern, scope, span(node).start, false);
        }
        if let Some(guard) = support::child::<syntax::Expr>(node) {
            self.expression(guard, scope);
        }
        if let Some(body) = support::child::<syntax::Block>(node) {
            self.block(&body, scope);
        }
    }

    fn pattern(
        &mut self,
        pattern: syntax::Pattern,
        scope: ScopeId,
        activation: usize,
        simple_let: bool,
    ) {
        self.patterns.alloc(PatternData {
            span: span(pattern.syntax()),
            scope,
        });
        match pattern {
            syntax::Pattern::Binding(node) => {
                if let Some(token) = direct_token(node.syntax(), K::IDENT)
                    && !token.text().starts_with('_')
                {
                    self.declare(
                        scope,
                        token.text().to_string(),
                        if simple_let {
                            SymbolKind::Let
                        } else {
                            SymbolKind::Pattern
                        },
                        Some(token_span(&token)),
                        None,
                        activation,
                    );
                }
            }
            syntax::Pattern::List(node) => {
                for child in pattern_children(node.syntax()) {
                    self.pattern(child, scope, activation, false);
                }
                self.rest_pattern(node.syntax(), scope, activation);
            }
            syntax::Pattern::Map(node) => {
                for field in support::children::<syntax::MapPatternField>(node.syntax()) {
                    if let Some(child) = support::child::<syntax::Pattern>(field.syntax()) {
                        self.pattern(child, scope, activation, false);
                    }
                }
                self.rest_pattern(node.syntax(), scope, activation);
            }
            syntax::Pattern::Wildcard(_) | syntax::Pattern::Literal(_) => {}
        }
    }

    fn rest_pattern(&mut self, node: &SyntaxNode, scope: ScopeId, activation: usize) {
        if let Some(rest) = support::child::<syntax::RestPattern>(node)
            && let Some(token) = direct_token(rest.syntax(), K::IDENT)
            && !token.text().starts_with('_')
        {
            self.declare(
                scope,
                token.text().to_string(),
                SymbolKind::Pattern,
                Some(token_span(&token)),
                None,
                activation,
            );
        }
    }

    fn occurrence(&mut self, token: SyntaxToken, scope: ScopeId, kind: OccurrenceKind) {
        self.occurrences.push(crate::model::NameOccurrence {
            name: token.text().to_string(),
            span: token_span(&token),
            scope,
            kind,
        });
    }
}

fn documentation(node: &SyntaxNode) -> Option<String> {
    let mut token = node.first_token()?.prev_token();
    let mut lines = Vec::new();
    while let Some(current) = token {
        match current.kind() {
            K::WHITESPACE => {
                let normalized = current.text().replace("\r\n", "\n");
                if normalized.split('\n').count() > 2 {
                    break;
                }
            }
            K::COMMENT => {
                let text = current.text();
                if text.starts_with("----") {
                    break;
                }
                let Some(text) = text.strip_prefix("---") else {
                    break;
                };
                lines.push(text.strip_prefix(' ').unwrap_or(text).to_owned());
            }
            _ => break,
        }
        token = current.prev_token();
    }
    if lines.is_empty() {
        None
    } else {
        lines.reverse();
        Some(lines.join("\n"))
    }
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
