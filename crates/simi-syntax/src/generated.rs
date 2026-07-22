// Generated from simi.ungram by `cargo run -p simi-xtask -- codegen`.
// Do not edit by hand.

use crate::syntax::SyntaxNode;

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    ERROR_TOKEN,
    WHITESPACE,
    COMMENT,
    INT,
    FLOAT,
    STRING,
    IDENT,
    FN_KW,
    DO_KW,
    END_KW,
    IF_KW,
    THEN_KW,
    ELSEIF_KW,
    ELSE_KW,
    LET_KW,
    TAP_KW,
    NIL_KW,
    TRUE_KW,
    FALSE_KW,
    AND_KW,
    OR_KW,
    NOT_KW,
    LOOP_KW,
    BREAK_KW,
    CONTINUE_KW,
    CASE_KW,
    OF_KW,
    WHEN_KW,
    RAISE_KW,
    TRY_KW,
    CATCH_KW,
    L_PAREN,
    R_PAREN,
    L_BRACKET,
    R_BRACKET,
    L_BRACE,
    R_BRACE,
    COMMA,
    DOT,
    DOT_DOT,
    EQ,
    EQ_EQ,
    BANG_EQ,
    PLUS,
    MINUS,
    STAR,
    SLASH,
    SLASH_SLASH,
    PERCENT,
    LESS,
    LESS_EQ,
    GREATER,
    GREATER_EQ,
    QUESTION,
    QUESTION_GREATER,
    PIPE_GREATER,
    LESS_PIPE,
    ROOT,
    FUNCTION_DECL,
    LET_STMT,
    EXPR_STMT,
    PARAM_LIST,
    BLOCK,
    LITERAL_EXPR,
    NAME_EXPR,
    FUNCTION_EXPR,
    BLOCK_EXPR,
    PAREN_EXPR,
    LIST_EXPR,
    MAP_EXPR,
    CALL_EXPR,
    FIELD_EXPR,
    INDEX_EXPR,
    NIL_PROPAGATE_EXPR,
    UNARY_EXPR,
    BINARY_EXPR,
    ASSIGN_EXPR,
    PIPELINE_EXPR,
    TRAILING_ARGUMENT_EXPR,
    RAISE_EXPR,
    TRY_EXPR,
    CASE_EXPR,
    IF_EXPR,
    LOOP_EXPR,
    CONTINUE_EXPR,
    BREAK_EXPR,
    MAP_ENTRY,
    ARG_LIST,
    PIPELINE_STAGE,
    CATCH_CLAUSE,
    CASE_CLAUSE,
    IF_BRANCH,
    ELSE_BRANCH,
    BINDING_PATTERN,
    WILDCARD_PATTERN,
    LITERAL_PATTERN,
    LIST_PATTERN,
    MAP_PATTERN,
    REST_PATTERN,
    MAP_PATTERN_FIELD,
    ERROR,
}

pub trait AstNode: Sized {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(syntax: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;
}

macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name {
            syntax: SyntaxNode,
        }
        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }
            fn cast(syntax: SyntaxNode) -> Option<Self> {
                Self::can_cast(syntax.kind()).then_some(Self { syntax })
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.syntax
            }
        }
    };
}

ast_node!(Root, ROOT);
ast_node!(FunctionDecl, FUNCTION_DECL);
ast_node!(LetStmt, LET_STMT);
ast_node!(ExprStmt, EXPR_STMT);
ast_node!(ParamList, PARAM_LIST);
ast_node!(Block, BLOCK);
ast_node!(LiteralExpr, LITERAL_EXPR);
ast_node!(NameExpr, NAME_EXPR);
ast_node!(FunctionExpr, FUNCTION_EXPR);
ast_node!(BlockExpr, BLOCK_EXPR);
ast_node!(ParenExpr, PAREN_EXPR);
ast_node!(ListExpr, LIST_EXPR);
ast_node!(MapExpr, MAP_EXPR);
ast_node!(CallExpr, CALL_EXPR);
ast_node!(FieldExpr, FIELD_EXPR);
ast_node!(IndexExpr, INDEX_EXPR);
ast_node!(NilPropagateExpr, NIL_PROPAGATE_EXPR);
ast_node!(UnaryExpr, UNARY_EXPR);
ast_node!(BinaryExpr, BINARY_EXPR);
ast_node!(AssignExpr, ASSIGN_EXPR);
ast_node!(PipelineExpr, PIPELINE_EXPR);
ast_node!(TrailingArgumentExpr, TRAILING_ARGUMENT_EXPR);
ast_node!(RaiseExpr, RAISE_EXPR);
ast_node!(TryExpr, TRY_EXPR);
ast_node!(CaseExpr, CASE_EXPR);
ast_node!(IfExpr, IF_EXPR);
ast_node!(LoopExpr, LOOP_EXPR);
ast_node!(ContinueExpr, CONTINUE_EXPR);
ast_node!(BreakExpr, BREAK_EXPR);
ast_node!(MapEntry, MAP_ENTRY);
ast_node!(ArgList, ARG_LIST);
ast_node!(PipelineStage, PIPELINE_STAGE);
ast_node!(CatchClause, CATCH_CLAUSE);
ast_node!(CaseClause, CASE_CLAUSE);
ast_node!(IfBranch, IF_BRANCH);
ast_node!(ElseBranch, ELSE_BRANCH);
ast_node!(BindingPattern, BINDING_PATTERN);
ast_node!(WildcardPattern, WILDCARD_PATTERN);
ast_node!(LiteralPattern, LITERAL_PATTERN);
ast_node!(ListPattern, LIST_PATTERN);
ast_node!(MapPattern, MAP_PATTERN);
ast_node!(RestPattern, REST_PATTERN);
ast_node!(MapPatternField, MAP_PATTERN_FIELD);
ast_node!(Error, ERROR);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stmt {
    FunctionDecl(FunctionDecl),
    LetStmt(LetStmt),
    ExprStmt(ExprStmt),
}
impl AstNode for Stmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::FUNCTION_DECL | SyntaxKind::LET_STMT | SyntaxKind::EXPR_STMT
        )
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Some(match syntax.kind() {
            SyntaxKind::FUNCTION_DECL => Self::FunctionDecl(FunctionDecl::cast(syntax)?),
            SyntaxKind::LET_STMT => Self::LetStmt(LetStmt::cast(syntax)?),
            SyntaxKind::EXPR_STMT => Self::ExprStmt(ExprStmt::cast(syntax)?),
            _ => return None,
        })
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::FunctionDecl(node) => node.syntax(),
            Self::LetStmt(node) => node.syntax(),
            Self::ExprStmt(node) => node.syntax(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Literal(LiteralExpr),
    Name(NameExpr),
    Function(FunctionExpr),
    Block(BlockExpr),
    Paren(ParenExpr),
    List(ListExpr),
    Map(MapExpr),
    Call(CallExpr),
    Field(FieldExpr),
    Index(IndexExpr),
    NilPropagate(NilPropagateExpr),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Assign(AssignExpr),
    Pipeline(PipelineExpr),
    TrailingArgument(TrailingArgumentExpr),
    Raise(RaiseExpr),
    Try(TryExpr),
    Case(CaseExpr),
    If(IfExpr),
    Loop(LoopExpr),
    Continue(ContinueExpr),
    Break(BreakExpr),
}
impl AstNode for Expr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind.is_expression()
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Some(match syntax.kind() {
            SyntaxKind::LITERAL_EXPR => Self::Literal(LiteralExpr::cast(syntax)?),
            SyntaxKind::NAME_EXPR => Self::Name(NameExpr::cast(syntax)?),
            SyntaxKind::FUNCTION_EXPR => Self::Function(FunctionExpr::cast(syntax)?),
            SyntaxKind::BLOCK_EXPR => Self::Block(BlockExpr::cast(syntax)?),
            SyntaxKind::PAREN_EXPR => Self::Paren(ParenExpr::cast(syntax)?),
            SyntaxKind::LIST_EXPR => Self::List(ListExpr::cast(syntax)?),
            SyntaxKind::MAP_EXPR => Self::Map(MapExpr::cast(syntax)?),
            SyntaxKind::CALL_EXPR => Self::Call(CallExpr::cast(syntax)?),
            SyntaxKind::FIELD_EXPR => Self::Field(FieldExpr::cast(syntax)?),
            SyntaxKind::INDEX_EXPR => Self::Index(IndexExpr::cast(syntax)?),
            SyntaxKind::NIL_PROPAGATE_EXPR => Self::NilPropagate(NilPropagateExpr::cast(syntax)?),
            SyntaxKind::UNARY_EXPR => Self::Unary(UnaryExpr::cast(syntax)?),
            SyntaxKind::BINARY_EXPR => Self::Binary(BinaryExpr::cast(syntax)?),
            SyntaxKind::ASSIGN_EXPR => Self::Assign(AssignExpr::cast(syntax)?),
            SyntaxKind::PIPELINE_EXPR => Self::Pipeline(PipelineExpr::cast(syntax)?),
            SyntaxKind::TRAILING_ARGUMENT_EXPR => {
                Self::TrailingArgument(TrailingArgumentExpr::cast(syntax)?)
            }
            SyntaxKind::RAISE_EXPR => Self::Raise(RaiseExpr::cast(syntax)?),
            SyntaxKind::TRY_EXPR => Self::Try(TryExpr::cast(syntax)?),
            SyntaxKind::CASE_EXPR => Self::Case(CaseExpr::cast(syntax)?),
            SyntaxKind::IF_EXPR => Self::If(IfExpr::cast(syntax)?),
            SyntaxKind::LOOP_EXPR => Self::Loop(LoopExpr::cast(syntax)?),
            SyntaxKind::CONTINUE_EXPR => Self::Continue(ContinueExpr::cast(syntax)?),
            SyntaxKind::BREAK_EXPR => Self::Break(BreakExpr::cast(syntax)?),
            _ => return None,
        })
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Literal(node) => node.syntax(),
            Self::Name(node) => node.syntax(),
            Self::Function(node) => node.syntax(),
            Self::Block(node) => node.syntax(),
            Self::Paren(node) => node.syntax(),
            Self::List(node) => node.syntax(),
            Self::Map(node) => node.syntax(),
            Self::Call(node) => node.syntax(),
            Self::Field(node) => node.syntax(),
            Self::Index(node) => node.syntax(),
            Self::NilPropagate(node) => node.syntax(),
            Self::Unary(node) => node.syntax(),
            Self::Binary(node) => node.syntax(),
            Self::Assign(node) => node.syntax(),
            Self::Pipeline(node) => node.syntax(),
            Self::TrailingArgument(node) => node.syntax(),
            Self::Raise(node) => node.syntax(),
            Self::Try(node) => node.syntax(),
            Self::Case(node) => node.syntax(),
            Self::If(node) => node.syntax(),
            Self::Loop(node) => node.syntax(),
            Self::Continue(node) => node.syntax(),
            Self::Break(node) => node.syntax(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pattern {
    Binding(BindingPattern),
    Wildcard(WildcardPattern),
    Literal(LiteralPattern),
    List(ListPattern),
    Map(MapPattern),
}
impl AstNode for Pattern {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind.is_pattern()
    }
    fn cast(syntax: SyntaxNode) -> Option<Self> {
        Some(match syntax.kind() {
            SyntaxKind::BINDING_PATTERN => Self::Binding(BindingPattern::cast(syntax)?),
            SyntaxKind::WILDCARD_PATTERN => Self::Wildcard(WildcardPattern::cast(syntax)?),
            SyntaxKind::LITERAL_PATTERN => Self::Literal(LiteralPattern::cast(syntax)?),
            SyntaxKind::LIST_PATTERN => Self::List(ListPattern::cast(syntax)?),
            SyntaxKind::MAP_PATTERN => Self::Map(MapPattern::cast(syntax)?),
            _ => return None,
        })
    }
    fn syntax(&self) -> &SyntaxNode {
        match self {
            Self::Binding(node) => node.syntax(),
            Self::Wildcard(node) => node.syntax(),
            Self::Literal(node) => node.syntax(),
            Self::List(node) => node.syntax(),
            Self::Map(node) => node.syntax(),
        }
    }
}

impl Root {
    pub fn statements(&self) -> impl Iterator<Item = Stmt> + '_ {
        crate::ast::children(self.syntax())
    }
}
impl Block {
    pub fn statements(&self) -> impl Iterator<Item = Stmt> + '_ {
        crate::ast::children(self.syntax())
    }
}
