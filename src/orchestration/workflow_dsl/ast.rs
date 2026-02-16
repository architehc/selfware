//! AST -- Abstract Syntax Tree node definitions for the workflow DSL.

/// AST Node types
#[derive(Debug, Clone)]
pub enum AstNode {
    /// Workflow definition
    Workflow {
        name: String,
        body: Vec<AstNode>,
    },

    /// Step definition
    Step {
        name: String,
        command: Box<AstNode>,
    },

    /// Parallel execution block
    Parallel {
        body: Vec<AstNode>,
    },

    /// Sequence block
    Sequence {
        body: Vec<AstNode>,
    },

    /// If statement
    If {
        condition: Box<AstNode>,
        then_branch: Vec<AstNode>,
        else_branch: Option<Vec<AstNode>>,
    },

    /// For loop
    For {
        variable: String,
        iterable: Box<AstNode>,
        body: Vec<AstNode>,
    },

    /// While loop
    While {
        condition: Box<AstNode>,
        body: Vec<AstNode>,
    },

    /// Variable assignment
    Let {
        name: String,
        value: Box<AstNode>,
    },

    /// Function definition
    FnDef {
        name: String,
        params: Vec<String>,
        body: Vec<AstNode>,
    },

    /// Function call
    Call {
        name: String,
        args: Vec<AstNode>,
    },

    /// Binary expression
    Binary {
        left: Box<AstNode>,
        operator: String,
        right: Box<AstNode>,
    },

    /// Unary expression
    Unary {
        operator: String,
        operand: Box<AstNode>,
    },

    /// Property access
    Property {
        object: Box<AstNode>,
        property: String,
    },

    /// Pipeline
    Pipeline {
        stages: Vec<AstNode>,
    },

    /// Return statement
    Return {
        value: Option<Box<AstNode>>,
    },

    /// Error handler
    OnError {
        handler: Box<AstNode>,
    },

    /// Literals
    Identifier(String),
    StringLit(String),
    IntegerLit(i64),
    FloatLit(f64),
    BooleanLit(bool),
    ArrayLit(Vec<AstNode>),

    /// Command (for shell execution)
    Command(String),
}
