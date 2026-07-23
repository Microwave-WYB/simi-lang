use crate::span::Span;

#[derive(Clone, Debug)]
pub struct Program {
    pub items: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub items: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Function {
        name: String,
        params: Vec<String>,
        body: Block,
    },
    Let {
        pattern: Pattern,
        value: Expr,
    },
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct PatternClause {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum PatternKind {
    Wildcard,
    Binding(String),
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Nil,
    List {
        elements: Vec<Pattern>,
        rest: Option<PatternRest>,
    },
    Map {
        fields: Vec<(String, Pattern)>,
        rest: Option<PatternRest>,
    },
}

#[derive(Clone, Debug)]
pub enum PatternRest {
    Discard,
    Binding(String),
}

#[derive(Clone, Debug)]
pub struct AssignmentTarget {
    pub kind: AssignmentTargetKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum AssignmentTargetKind {
    Variable(String),
    Field { object: Box<Expr>, name: String },
    Index { object: Box<Expr>, key: Box<Expr> },
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Nil,
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Variable(String),
    Function {
        params: Vec<String>,
        body: Block,
    },
    Assign {
        target: AssignmentTarget,
        value: Box<Expr>,
    },
    Raise {
        value: Box<Expr>,
    },
    Block(Block),
    NilPropagate {
        value: Box<Expr>,
    },
    Try {
        protected: Block,
        clauses: Vec<PatternClause>,
    },
    Case {
        value: Box<Expr>,
        clauses: Vec<PatternClause>,
    },
    If {
        branches: Vec<(Expr, Block)>,
        else_branch: Option<Block>,
    },
    Loop {
        state: String,
        initial: Box<Expr>,
        body: Block,
    },
    Continue {
        value: Box<Expr>,
    },
    Break {
        value: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Field {
        object: Box<Expr>,
        name: String,
    },
    Index {
        object: Box<Expr>,
        key: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        value: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    Pipeline {
        input: Box<Expr>,
        stages: Vec<PipelineStage>,
    },
}

#[derive(Clone, Debug)]
pub struct PipelineStage {
    pub nil_aware: bool,
    pub tap: bool,
    pub callee: Expr,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(Clone, Debug)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    FloorDivide,
    Remainder,
    Concatenate,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}
