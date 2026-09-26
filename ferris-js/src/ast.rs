#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod,
    Eq, StrictEq, NotEq, StrictNotEq,
    Lt, Gt, LtEq, GtEq,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogicalOp {
    And, Or,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Neg, Not, Typeof,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AssignOp {
    Assign, AddAssign, SubAssign, MulAssign, DivAssign,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeclKind {
    Var, Let, Const,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    StringLiteral(String),
    Boolean(bool),
    Null,
    Undefined,
    Identifier(String),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    Logical { op: LogicalOp, left: Box<Expr>, right: Box<Expr> },
    Unary { op: UnaryOp, argument: Box<Expr> },
    Assignment { op: AssignOp, target: Box<Expr>, value: Box<Expr> },
    Call { callee: Box<Expr>, arguments: Vec<Expr> },
    Member { object: Box<Expr>, property: Box<Expr>, computed: bool },
    Function { name: Option<String>, params: Vec<String>, body: Vec<Stmt> },
    Conditional { test: Box<Expr>, consequent: Box<Expr>, alternate: Box<Expr> },
    New { callee: Box<Expr>, arguments: Vec<Expr> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expression(Expr),
    VarDecl { kind: DeclKind, declarations: Vec<(String, Option<Expr>)> },
    FunctionDecl { name: String, params: Vec<String>, body: Vec<Stmt> },
    If { test: Expr, consequent: Box<Stmt>, alternate: Option<Box<Stmt>> },
    For { init: Option<Box<Stmt>>, test: Option<Expr>, update: Option<Expr>, body: Box<Stmt> },
    While { test: Expr, body: Box<Stmt> },
    Return(Option<Expr>),
    Block(Vec<Stmt>),
    Break,
    Continue,
}
