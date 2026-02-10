//! Workflow DSL - Domain-Specific Language for Agent Workflows
//!
//! Custom workflow language: declarative pipelines, conditional logic,
//! loop constructs, and composition.
//!
//! # Syntax Examples
//!
//! ```text
//! workflow build_project {
//!     step check = cargo check;
//!     if check.success {
//!         step test = cargo test;
//!     }
//!     parallel {
//!         step lint = cargo clippy;
//!         step fmt = cargo fmt --check;
//!     }
//! }
//! ```

// Feature-gated module - dead_code lint disabled at crate level

use std::collections::HashMap;

/// Type alias for command executor callback
/// Returns (success, stdout, stderr)
pub type CommandExecutor = Box<dyn Fn(&str) -> (bool, String, String) + Send + Sync>;

/// Token types for the DSL
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    Workflow,
    Step,
    If,
    Else,
    For,
    While,
    Parallel,
    Sequence,
    On,
    Return,
    Let,
    Fn,

    // Operators
    Equals,       // =
    DoubleEquals, // ==
    NotEquals,    // !=
    LessThan,     // <
    GreaterThan,  // >
    LessEqual,    // <=
    GreaterEqual, // >=
    And,          // &&
    Or,           // ||
    Not,          // !
    Plus,         // +
    Minus,        // -
    Star,         // *
    Slash,        // /
    Pipe,         // |
    Arrow,        // ->
    DoubleArrow,  // =>
    Dot,          // .
    Comma,        // ,
    Colon,        // :
    Semicolon,    // ;

    // Delimiters
    OpenBrace,    // {
    CloseBrace,   // }
    OpenParen,    // (
    CloseParen,   // )
    OpenBracket,  // [
    CloseBracket, // ]

    // Literals
    Identifier(String),
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),

    // Special
    Eof,
    Error(String),
}

impl Token {
    /// Check if token is a keyword
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Token::Workflow
                | Token::Step
                | Token::If
                | Token::Else
                | Token::For
                | Token::While
                | Token::Parallel
                | Token::Sequence
                | Token::On
                | Token::Return
                | Token::Let
                | Token::Fn
        )
    }
}

/// Lexer for tokenizing DSL source
#[derive(Debug)]
pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    /// Create a new lexer
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    /// Get current character
    fn current(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    /// Peek ahead
    fn peek(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    /// Advance position
    fn advance(&mut self) -> Option<char> {
        let ch = self.current();
        self.pos += 1;
        if ch == Some('\n') {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        ch
    }

    /// Skip whitespace
    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.current() {
            if ch.is_whitespace() {
                self.advance();
            } else if ch == '/' && self.peek() == Some('/') {
                // Skip line comment
                while self.current() != Some('\n') && self.current().is_some() {
                    self.advance();
                }
            } else if ch == '/' && self.peek() == Some('*') {
                // Skip block comment
                self.advance(); // /
                self.advance(); // *
                while !(self.current() == Some('*') && self.peek() == Some('/')) {
                    if self.current().is_none() {
                        break;
                    }
                    self.advance();
                }
                self.advance(); // *
                self.advance(); // /
            } else {
                break;
            }
        }
    }

    /// Read an identifier
    fn read_identifier(&mut self) -> String {
        let mut id = String::new();
        while let Some(ch) = self.current() {
            if ch.is_alphanumeric() || ch == '_' {
                id.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        id
    }

    /// Read a number
    fn read_number(&mut self) -> Token {
        let mut num = String::new();
        let mut is_float = false;

        while let Some(ch) = self.current() {
            if ch.is_ascii_digit() {
                num.push(ch);
                self.advance();
            } else if ch == '.' && !is_float {
                if let Some(next) = self.peek() {
                    if next.is_ascii_digit() {
                        is_float = true;
                        num.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if is_float {
            Token::Float(num.parse().unwrap_or(0.0))
        } else {
            Token::Integer(num.parse().unwrap_or(0))
        }
    }

    /// Read a string literal
    fn read_string(&mut self) -> Token {
        let quote = self.advance().unwrap(); // " or '
        let mut s = String::new();

        while let Some(ch) = self.current() {
            if ch == quote {
                self.advance();
                return Token::String(s);
            } else if ch == '\\' {
                self.advance();
                if let Some(escaped) = self.current() {
                    match escaped {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        _ => s.push(escaped),
                    }
                    self.advance();
                }
            } else {
                s.push(ch);
                self.advance();
            }
        }

        Token::Error("Unterminated string".to_string())
    }

    /// Get next token
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let ch = match self.current() {
            Some(c) => c,
            None => return Token::Eof,
        };

        // Identifiers and keywords
        if ch.is_alphabetic() || ch == '_' {
            let id = self.read_identifier();
            return match id.as_str() {
                "workflow" => Token::Workflow,
                "step" => Token::Step,
                "if" => Token::If,
                "else" => Token::Else,
                "for" => Token::For,
                "while" => Token::While,
                "parallel" => Token::Parallel,
                "sequence" => Token::Sequence,
                "on" => Token::On,
                "return" => Token::Return,
                "let" => Token::Let,
                "fn" => Token::Fn,
                "true" => Token::Boolean(true),
                "false" => Token::Boolean(false),
                _ => Token::Identifier(id),
            };
        }

        // Numbers
        if ch.is_ascii_digit() {
            return self.read_number();
        }

        // Strings
        if ch == '"' || ch == '\'' {
            return self.read_string();
        }

        // Operators and delimiters
        self.advance();
        match ch {
            '{' => Token::OpenBrace,
            '}' => Token::CloseBrace,
            '(' => Token::OpenParen,
            ')' => Token::CloseParen,
            '[' => Token::OpenBracket,
            ']' => Token::CloseBracket,
            ',' => Token::Comma,
            ':' => Token::Colon,
            ';' => Token::Semicolon,
            '.' => Token::Dot,
            '+' => Token::Plus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '|' => {
                if self.current() == Some('|') {
                    self.advance();
                    Token::Or
                } else {
                    Token::Pipe
                }
            }
            '&' => {
                if self.current() == Some('&') {
                    self.advance();
                    Token::And
                } else {
                    Token::Error("Expected &&".to_string())
                }
            }
            '=' => {
                if self.current() == Some('=') {
                    self.advance();
                    Token::DoubleEquals
                } else if self.current() == Some('>') {
                    self.advance();
                    Token::DoubleArrow
                } else {
                    Token::Equals
                }
            }
            '!' => {
                if self.current() == Some('=') {
                    self.advance();
                    Token::NotEquals
                } else {
                    Token::Not
                }
            }
            '<' => {
                if self.current() == Some('=') {
                    self.advance();
                    Token::LessEqual
                } else {
                    Token::LessThan
                }
            }
            '>' => {
                if self.current() == Some('=') {
                    self.advance();
                    Token::GreaterEqual
                } else {
                    Token::GreaterThan
                }
            }
            '-' => {
                if self.current() == Some('>') {
                    self.advance();
                    Token::Arrow
                } else {
                    Token::Minus
                }
            }
            _ => Token::Error(format!("Unexpected character: {}", ch)),
        }
    }

    /// Tokenize entire source
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            if token == Token::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        tokens
    }
}

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

/// Parser for the DSL
#[derive(Debug)]
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    /// Create a new parser
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Get current token
    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    /// Peek ahead
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos + 1).unwrap_or(&Token::Eof)
    }

    /// Advance position
    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        self.pos += 1;
        token
    }

    /// Check if current token matches
    fn check(&self, expected: &Token) -> bool {
        std::mem::discriminant(self.current()) == std::mem::discriminant(expected)
    }

    /// Consume expected token or error
    fn expect(&mut self, expected: Token) -> Result<Token, String> {
        if self.check(&expected) {
            Ok(self.advance())
        } else {
            Err(format!("Expected {:?}, got {:?}", expected, self.current()))
        }
    }

    /// Parse source into AST
    pub fn parse(&mut self) -> Result<Vec<AstNode>, String> {
        let mut nodes = Vec::new();

        while !self.check(&Token::Eof) {
            let node = self.parse_top_level()?;
            nodes.push(node);
        }

        Ok(nodes)
    }

    /// Parse top-level declaration
    fn parse_top_level(&mut self) -> Result<AstNode, String> {
        match self.current() {
            Token::Workflow => self.parse_workflow(),
            Token::Fn => self.parse_function(),
            _ => self.parse_statement(),
        }
    }

    /// Parse workflow definition
    fn parse_workflow(&mut self) -> Result<AstNode, String> {
        self.expect(Token::Workflow)?;

        let name = match self.advance() {
            Token::Identifier(n) => n,
            _ => return Err("Expected workflow name".to_string()),
        };

        self.expect(Token::OpenBrace)?;
        let body = self.parse_block()?;
        self.expect(Token::CloseBrace)?;

        Ok(AstNode::Workflow { name, body })
    }

    /// Parse function definition
    fn parse_function(&mut self) -> Result<AstNode, String> {
        self.expect(Token::Fn)?;

        let name = match self.advance() {
            Token::Identifier(n) => n,
            _ => return Err("Expected function name".to_string()),
        };

        self.expect(Token::OpenParen)?;
        let mut params = Vec::new();
        while !self.check(&Token::CloseParen) {
            if let Token::Identifier(p) = self.advance() {
                params.push(p);
            }
            if self.check(&Token::Comma) {
                self.advance();
            }
        }
        self.expect(Token::CloseParen)?;

        self.expect(Token::OpenBrace)?;
        let body = self.parse_block()?;
        self.expect(Token::CloseBrace)?;

        Ok(AstNode::FnDef { name, params, body })
    }

    /// Parse block of statements
    fn parse_block(&mut self) -> Result<Vec<AstNode>, String> {
        let mut statements = Vec::new();

        while !self.check(&Token::CloseBrace) && !self.check(&Token::Eof) {
            let stmt = self.parse_statement()?;
            statements.push(stmt);
        }

        Ok(statements)
    }

    /// Parse a statement
    fn parse_statement(&mut self) -> Result<AstNode, String> {
        match self.current() {
            Token::Step => self.parse_step(),
            Token::If => self.parse_if(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::Parallel => self.parse_parallel(),
            Token::Sequence => self.parse_sequence(),
            Token::Let => self.parse_let(),
            Token::Return => self.parse_return(),
            Token::On => self.parse_on_error(),
            _ => self.parse_expression_statement(),
        }
    }

    /// Parse step
    fn parse_step(&mut self) -> Result<AstNode, String> {
        self.expect(Token::Step)?;

        let name = match self.advance() {
            Token::Identifier(n) => n,
            _ => return Err("Expected step name".to_string()),
        };

        self.expect(Token::Equals)?;

        // Parse command (everything until semicolon or newline)
        let command = self.parse_expression()?;

        // Optional semicolon
        if self.check(&Token::Semicolon) {
            self.advance();
        }

        Ok(AstNode::Step {
            name,
            command: Box::new(command),
        })
    }

    /// Parse if statement
    fn parse_if(&mut self) -> Result<AstNode, String> {
        self.expect(Token::If)?;
        let condition = self.parse_expression()?;

        self.expect(Token::OpenBrace)?;
        let then_branch = self.parse_block()?;
        self.expect(Token::CloseBrace)?;

        let else_branch = if self.check(&Token::Else) {
            self.advance();
            if self.check(&Token::If) {
                Some(vec![self.parse_if()?])
            } else {
                self.expect(Token::OpenBrace)?;
                let branch = self.parse_block()?;
                self.expect(Token::CloseBrace)?;
                Some(branch)
            }
        } else {
            None
        };

        Ok(AstNode::If {
            condition: Box::new(condition),
            then_branch,
            else_branch,
        })
    }

    /// Parse for loop
    fn parse_for(&mut self) -> Result<AstNode, String> {
        self.expect(Token::For)?;

        let variable = match self.advance() {
            Token::Identifier(n) => n,
            _ => return Err("Expected variable name".to_string()),
        };

        // Expect 'in' (as identifier for now)
        match self.current() {
            Token::Identifier(s) if s == "in" => {
                self.advance();
            }
            _ => return Err("Expected 'in'".to_string()),
        }

        let iterable = self.parse_expression()?;

        self.expect(Token::OpenBrace)?;
        let body = self.parse_block()?;
        self.expect(Token::CloseBrace)?;

        Ok(AstNode::For {
            variable,
            iterable: Box::new(iterable),
            body,
        })
    }

    /// Parse while loop
    fn parse_while(&mut self) -> Result<AstNode, String> {
        self.expect(Token::While)?;
        let condition = self.parse_expression()?;

        self.expect(Token::OpenBrace)?;
        let body = self.parse_block()?;
        self.expect(Token::CloseBrace)?;

        Ok(AstNode::While {
            condition: Box::new(condition),
            body,
        })
    }

    /// Parse parallel block
    fn parse_parallel(&mut self) -> Result<AstNode, String> {
        self.expect(Token::Parallel)?;
        self.expect(Token::OpenBrace)?;
        let body = self.parse_block()?;
        self.expect(Token::CloseBrace)?;

        Ok(AstNode::Parallel { body })
    }

    /// Parse sequence block
    fn parse_sequence(&mut self) -> Result<AstNode, String> {
        self.expect(Token::Sequence)?;
        self.expect(Token::OpenBrace)?;
        let body = self.parse_block()?;
        self.expect(Token::CloseBrace)?;

        Ok(AstNode::Sequence { body })
    }

    /// Parse let statement
    fn parse_let(&mut self) -> Result<AstNode, String> {
        self.expect(Token::Let)?;

        let name = match self.advance() {
            Token::Identifier(n) => n,
            _ => return Err("Expected variable name".to_string()),
        };

        self.expect(Token::Equals)?;
        let value = self.parse_expression()?;

        if self.check(&Token::Semicolon) {
            self.advance();
        }

        Ok(AstNode::Let {
            name,
            value: Box::new(value),
        })
    }

    /// Parse return statement
    fn parse_return(&mut self) -> Result<AstNode, String> {
        self.expect(Token::Return)?;

        let value = if !self.check(&Token::Semicolon) && !self.check(&Token::CloseBrace) {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        if self.check(&Token::Semicolon) {
            self.advance();
        }

        Ok(AstNode::Return { value })
    }

    /// Parse on error handler
    fn parse_on_error(&mut self) -> Result<AstNode, String> {
        self.expect(Token::On)?;

        // Expect 'error' identifier
        match self.current() {
            Token::Identifier(s) if s == "error" => {
                self.advance();
            }
            _ => return Err("Expected 'error'".to_string()),
        }

        let handler = self.parse_statement()?;

        Ok(AstNode::OnError {
            handler: Box::new(handler),
        })
    }

    /// Parse expression statement
    fn parse_expression_statement(&mut self) -> Result<AstNode, String> {
        let expr = self.parse_expression()?;

        if self.check(&Token::Semicolon) {
            self.advance();
        }

        Ok(expr)
    }

    /// Parse expression (handles precedence)
    fn parse_expression(&mut self) -> Result<AstNode, String> {
        self.parse_pipeline()
    }

    /// Parse pipeline expression
    fn parse_pipeline(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_or()?;

        while self.check(&Token::Pipe) {
            self.advance();
            let right = self.parse_or()?;
            left = AstNode::Pipeline {
                stages: match left {
                    AstNode::Pipeline { mut stages } => {
                        stages.push(right);
                        stages
                    }
                    _ => vec![left, right],
                },
            };
        }

        Ok(left)
    }

    /// Parse or expression
    fn parse_or(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_and()?;

        while self.check(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = AstNode::Binary {
                left: Box::new(left),
                operator: "||".to_string(),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse and expression
    fn parse_and(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_equality()?;

        while self.check(&Token::And) {
            self.advance();
            let right = self.parse_equality()?;
            left = AstNode::Binary {
                left: Box::new(left),
                operator: "&&".to_string(),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse equality
    fn parse_equality(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_comparison()?;

        loop {
            let op = match self.current() {
                Token::DoubleEquals => "==",
                Token::NotEquals => "!=",
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            left = AstNode::Binary {
                left: Box::new(left),
                operator: op.to_string(),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse comparison
    fn parse_comparison(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_additive()?;

        loop {
            let op = match self.current() {
                Token::LessThan => "<",
                Token::GreaterThan => ">",
                Token::LessEqual => "<=",
                Token::GreaterEqual => ">=",
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            left = AstNode::Binary {
                left: Box::new(left),
                operator: op.to_string(),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse additive
    fn parse_additive(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_multiplicative()?;

        loop {
            let op = match self.current() {
                Token::Plus => "+",
                Token::Minus => "-",
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = AstNode::Binary {
                left: Box::new(left),
                operator: op.to_string(),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse multiplicative
    fn parse_multiplicative(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_unary()?;

        loop {
            let op = match self.current() {
                Token::Star => "*",
                Token::Slash => "/",
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = AstNode::Binary {
                left: Box::new(left),
                operator: op.to_string(),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse unary
    fn parse_unary(&mut self) -> Result<AstNode, String> {
        let op = match self.current() {
            Token::Not => "!",
            Token::Minus => "-",
            _ => return self.parse_postfix(),
        };
        self.advance();
        let operand = self.parse_unary()?;

        Ok(AstNode::Unary {
            operator: op.to_string(),
            operand: Box::new(operand),
        })
    }

    /// Parse postfix (property access, calls)
    fn parse_postfix(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_primary()?;

        loop {
            match self.current() {
                Token::Dot => {
                    self.advance();
                    let property = match self.advance() {
                        Token::Identifier(n) => n,
                        _ => return Err("Expected property name".to_string()),
                    };
                    left = AstNode::Property {
                        object: Box::new(left),
                        property,
                    };
                }
                Token::OpenParen => {
                    if let AstNode::Identifier(name) = left {
                        self.advance();
                        let args = self.parse_args()?;
                        self.expect(Token::CloseParen)?;
                        left = AstNode::Call { name, args };
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        Ok(left)
    }

    /// Parse function arguments
    fn parse_args(&mut self) -> Result<Vec<AstNode>, String> {
        let mut args = Vec::new();

        while !self.check(&Token::CloseParen) && !self.check(&Token::Eof) {
            args.push(self.parse_expression()?);
            if self.check(&Token::Comma) {
                self.advance();
            }
        }

        Ok(args)
    }

    /// Parse primary expression
    fn parse_primary(&mut self) -> Result<AstNode, String> {
        match self.current().clone() {
            Token::Identifier(name) => {
                self.advance();
                Ok(AstNode::Identifier(name))
            }
            Token::String(s) => {
                self.advance();
                Ok(AstNode::StringLit(s))
            }
            Token::Integer(n) => {
                self.advance();
                Ok(AstNode::IntegerLit(n))
            }
            Token::Float(n) => {
                self.advance();
                Ok(AstNode::FloatLit(n))
            }
            Token::Boolean(b) => {
                self.advance();
                Ok(AstNode::BooleanLit(b))
            }
            Token::OpenParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(Token::CloseParen)?;
                Ok(expr)
            }
            Token::OpenBracket => {
                self.advance();
                let mut elements = Vec::new();
                while !self.check(&Token::CloseBracket) && !self.check(&Token::Eof) {
                    elements.push(self.parse_expression()?);
                    if self.check(&Token::Comma) {
                        self.advance();
                    }
                }
                self.expect(Token::CloseBracket)?;
                Ok(AstNode::ArrayLit(elements))
            }
            _ => Err(format!("Unexpected token: {:?}", self.current())),
        }
    }
}

/// Runtime value
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
    Function {
        params: Vec<String>,
        body: Vec<AstNode>,
    },
    StepResult {
        name: String,
        success: bool,
        output: String,
        error: Option<String>,
    },
}

impl Value {
    /// Convert to boolean
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Boolean(b) => *b,
            Value::Integer(n) => *n != 0,
            Value::Float(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Object(o) => !o.is_empty(),
            Value::Function { .. } => true,
            Value::StepResult { success, .. } => *success,
        }
    }

    /// Convert to string
    pub fn as_string(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Integer(n) => n.to_string(),
            Value::Float(n) => n.to_string(),
            Value::String(s) => s.clone(),
            Value::Array(a) => format!(
                "[{}]",
                a.iter()
                    .map(|v| v.as_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Object(_) => "[object]".to_string(),
            Value::Function { .. } => "[function]".to_string(),
            Value::StepResult { output, .. } => output.clone(),
        }
    }
}

/// Workflow runtime/executor
pub struct Runtime {
    /// Global scope
    pub globals: HashMap<String, Value>,
    /// Function definitions
    pub functions: HashMap<String, AstNode>,
    /// Execution history
    pub history: Vec<ExecutionEvent>,
    /// Built-in command executor
    command_executor: Option<CommandExecutor>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("globals", &self.globals)
            .field("functions", &self.functions)
            .field("history", &self.history)
            .field("command_executor", &self.command_executor.is_some())
            .finish()
    }
}

/// Execution event for tracing
#[derive(Debug, Clone)]
pub struct ExecutionEvent {
    /// Event type
    pub event_type: String,
    /// Step/workflow name
    pub name: String,
    /// Result
    pub result: Option<Value>,
    /// Timestamp
    pub timestamp: u64,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// Create a new runtime
    pub fn new() -> Self {
        Self {
            globals: HashMap::new(),
            functions: HashMap::new(),
            history: Vec::new(),
            command_executor: None,
        }
    }

    /// Set command executor
    pub fn with_executor<F>(mut self, executor: F) -> Self
    where
        F: Fn(&str) -> (bool, String, String) + Send + Sync + 'static,
    {
        self.command_executor = Some(Box::new(executor));
        self
    }

    /// Execute AST nodes
    pub fn execute(&mut self, nodes: &[AstNode]) -> Result<Value, String> {
        let mut result = Value::Null;

        for node in nodes {
            result = self.eval(node)?;
        }

        Ok(result)
    }

    /// Evaluate a single node
    fn eval(&mut self, node: &AstNode) -> Result<Value, String> {
        match node {
            AstNode::Workflow { name, body } => {
                self.log_event("workflow_start", name);
                let result = self.execute(body)?;
                self.log_event("workflow_end", name);
                Ok(result)
            }

            AstNode::Step { name, command } => {
                self.log_event("step_start", name);
                let cmd_value = self.eval(command)?;
                let cmd_str = cmd_value.as_string();

                let (success, output, error) = if let Some(ref executor) = self.command_executor {
                    executor(&cmd_str)
                } else {
                    // Simulated execution
                    (true, format!("[Executed: {}]", cmd_str), String::new())
                };

                let result = Value::StepResult {
                    name: name.clone(),
                    success,
                    output,
                    error: if error.is_empty() { None } else { Some(error) },
                };

                self.globals.insert(name.clone(), result.clone());
                self.log_event("step_end", name);
                Ok(result)
            }

            AstNode::Parallel { body } => {
                // Execute all in "parallel" (sequential for now, but could be async)
                let mut results = Vec::new();
                for node in body {
                    results.push(self.eval(node)?);
                }
                Ok(Value::Array(results))
            }

            AstNode::Sequence { body } => self.execute(body),

            AstNode::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.eval(condition)?;
                if cond.as_bool() {
                    self.execute(then_branch)
                } else if let Some(else_body) = else_branch {
                    self.execute(else_body)
                } else {
                    Ok(Value::Null)
                }
            }

            AstNode::For {
                variable,
                iterable,
                body,
            } => {
                let iter_value = self.eval(iterable)?;
                let items = match iter_value {
                    Value::Array(arr) => arr,
                    _ => return Err("For loop requires iterable".to_string()),
                };

                let mut result = Value::Null;
                for item in items {
                    self.globals.insert(variable.clone(), item);
                    result = self.execute(body)?;
                }

                Ok(result)
            }

            AstNode::While { condition, body } => {
                let mut result = Value::Null;
                let mut iterations = 0;
                const MAX_ITERATIONS: usize = 10000;

                while self.eval(condition)?.as_bool() {
                    result = self.execute(body)?;
                    iterations += 1;
                    if iterations >= MAX_ITERATIONS {
                        return Err("Maximum loop iterations exceeded".to_string());
                    }
                }

                Ok(result)
            }

            AstNode::Let { name, value } => {
                let val = self.eval(value)?;
                self.globals.insert(name.clone(), val.clone());
                Ok(val)
            }

            AstNode::FnDef { name, params, body } => {
                self.functions.insert(name.clone(), node.clone());
                Ok(Value::Function {
                    params: params.clone(),
                    body: body.clone(),
                })
            }

            AstNode::Call { name, args } => {
                // Check built-in functions first
                if let Some(result) = self.call_builtin(name, args)? {
                    return Ok(result);
                }

                // Check user-defined functions
                if let Some(AstNode::FnDef { params, body, .. }) = self.functions.get(name).cloned()
                {
                    if args.len() != params.len() {
                        return Err(format!(
                            "Function {} expects {} arguments",
                            name,
                            params.len()
                        ));
                    }

                    // Bind arguments
                    for (param, arg) in params.iter().zip(args.iter()) {
                        let value = self.eval(arg)?;
                        self.globals.insert(param.clone(), value);
                    }

                    return self.execute(&body);
                }

                Err(format!("Unknown function: {}", name))
            }

            AstNode::Binary {
                left,
                operator,
                right,
            } => {
                let lval = self.eval(left)?;
                let rval = self.eval(right)?;
                self.eval_binary(&lval, operator, &rval)
            }

            AstNode::Unary { operator, operand } => {
                let val = self.eval(operand)?;
                self.eval_unary(operator, &val)
            }

            AstNode::Property { object, property } => {
                let obj = self.eval(object)?;
                match obj {
                    Value::StepResult {
                        success,
                        output,
                        error,
                        ..
                    } => match property.as_str() {
                        "success" => Ok(Value::Boolean(success)),
                        "output" => Ok(Value::String(output)),
                        "error" => Ok(error.map(Value::String).unwrap_or(Value::Null)),
                        _ => Err(format!("Unknown property: {}", property)),
                    },
                    Value::Object(map) => Ok(map.get(property).cloned().unwrap_or(Value::Null)),
                    _ => Err(format!("Cannot access property on {:?}", obj)),
                }
            }

            AstNode::Pipeline { stages } => {
                let mut input = Value::Null;
                for stage in stages {
                    // Pass previous output as implicit input
                    self.globals.insert("_input".to_string(), input);
                    input = self.eval(stage)?;
                }
                Ok(input)
            }

            AstNode::Return { value } => {
                if let Some(v) = value {
                    self.eval(v)
                } else {
                    Ok(Value::Null)
                }
            }

            AstNode::OnError { handler } => {
                // Register error handler (simplified)
                self.eval(handler)
            }

            AstNode::Identifier(name) => Ok(self.globals.get(name).cloned().unwrap_or(Value::Null)),

            AstNode::StringLit(s) => Ok(Value::String(s.clone())),
            AstNode::IntegerLit(n) => Ok(Value::Integer(*n)),
            AstNode::FloatLit(n) => Ok(Value::Float(*n)),
            AstNode::BooleanLit(b) => Ok(Value::Boolean(*b)),
            AstNode::ArrayLit(elements) => {
                let values: Result<Vec<_>, _> = elements.iter().map(|e| self.eval(e)).collect();
                Ok(Value::Array(values?))
            }

            AstNode::Command(cmd) => Ok(Value::String(cmd.clone())),
        }
    }

    /// Evaluate binary operation
    fn eval_binary(&self, left: &Value, op: &str, right: &Value) -> Result<Value, String> {
        match op {
            "+" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
                _ => Ok(Value::String(format!(
                    "{}{}",
                    left.as_string(),
                    right.as_string()
                ))),
            },
            "-" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
                _ => Err("Cannot subtract non-numeric values".to_string()),
            },
            "*" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
                _ => Err("Cannot multiply non-numeric values".to_string()),
            },
            "/" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) if *b != 0 => Ok(Value::Integer(a / b)),
                (Value::Float(a), Value::Float(b)) if *b != 0.0 => Ok(Value::Float(a / b)),
                _ => Err("Division by zero or invalid types".to_string()),
            },
            "==" => Ok(Value::Boolean(left.as_string() == right.as_string())),
            "!=" => Ok(Value::Boolean(left.as_string() != right.as_string())),
            "<" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a < b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a < b)),
                _ => Err("Cannot compare non-numeric values".to_string()),
            },
            ">" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a > b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a > b)),
                _ => Err("Cannot compare non-numeric values".to_string()),
            },
            "<=" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a <= b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a <= b)),
                _ => Err("Cannot compare non-numeric values".to_string()),
            },
            ">=" => match (left, right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(a >= b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Boolean(a >= b)),
                _ => Err("Cannot compare non-numeric values".to_string()),
            },
            "&&" => Ok(Value::Boolean(left.as_bool() && right.as_bool())),
            "||" => Ok(Value::Boolean(left.as_bool() || right.as_bool())),
            _ => Err(format!("Unknown operator: {}", op)),
        }
    }

    /// Evaluate unary operation
    fn eval_unary(&self, op: &str, val: &Value) -> Result<Value, String> {
        match op {
            "!" => Ok(Value::Boolean(!val.as_bool())),
            "-" => match val {
                Value::Integer(n) => Ok(Value::Integer(-n)),
                Value::Float(n) => Ok(Value::Float(-n)),
                _ => Err("Cannot negate non-numeric value".to_string()),
            },
            _ => Err(format!("Unknown unary operator: {}", op)),
        }
    }

    /// Call built-in function
    fn call_builtin(&mut self, name: &str, args: &[AstNode]) -> Result<Option<Value>, String> {
        match name {
            "print" => {
                let values: Result<Vec<_>, _> = args.iter().map(|a| self.eval(a)).collect();
                let strings: Vec<String> = values?.iter().map(|v| v.as_string()).collect();
                println!("{}", strings.join(" "));
                Ok(Some(Value::Null))
            }
            "len" => {
                if args.len() != 1 {
                    return Err("len expects 1 argument".to_string());
                }
                let val = self.eval(&args[0])?;
                match val {
                    Value::String(s) => Ok(Some(Value::Integer(s.len() as i64))),
                    Value::Array(a) => Ok(Some(Value::Integer(a.len() as i64))),
                    _ => Err("len requires string or array".to_string()),
                }
            }
            "range" => {
                let (start, end) = match args.len() {
                    1 => {
                        let end = match self.eval(&args[0])? {
                            Value::Integer(n) => n,
                            _ => return Err("range expects integer".to_string()),
                        };
                        (0, end)
                    }
                    2 => {
                        let start = match self.eval(&args[0])? {
                            Value::Integer(n) => n,
                            _ => return Err("range expects integers".to_string()),
                        };
                        let end = match self.eval(&args[1])? {
                            Value::Integer(n) => n,
                            _ => return Err("range expects integers".to_string()),
                        };
                        (start, end)
                    }
                    _ => return Err("range expects 1 or 2 arguments".to_string()),
                };
                let values: Vec<Value> = (start..end).map(Value::Integer).collect();
                Ok(Some(Value::Array(values)))
            }
            "env" => {
                if args.len() != 1 {
                    return Err("env expects 1 argument".to_string());
                }
                let key = self.eval(&args[0])?.as_string();
                let value = std::env::var(&key).unwrap_or_default();
                Ok(Some(Value::String(value)))
            }
            _ => Ok(None),
        }
    }

    /// Log execution event
    fn log_event(&mut self, event_type: &str, name: &str) {
        self.history.push(ExecutionEvent {
            event_type: event_type.to_string(),
            name: name.to_string(),
            result: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });
    }

    /// Get variable value
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.globals.get(name)
    }

    /// Set variable value
    pub fn set(&mut self, name: &str, value: Value) {
        self.globals.insert(name.to_string(), value);
    }
}

/// Compile and run DSL source
pub fn run(source: &str) -> Result<Value, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();

    let mut parser = Parser::new(tokens);
    let ast = parser.parse()?;

    let mut runtime = Runtime::new();
    runtime.execute(&ast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_keywords() {
        let mut lexer = Lexer::new("workflow step if else");
        assert_eq!(lexer.next_token(), Token::Workflow);
        assert_eq!(lexer.next_token(), Token::Step);
        assert_eq!(lexer.next_token(), Token::If);
        assert_eq!(lexer.next_token(), Token::Else);
    }

    #[test]
    fn test_lexer_operators() {
        let mut lexer = Lexer::new("= == != < > <= >= && ||");
        assert_eq!(lexer.next_token(), Token::Equals);
        assert_eq!(lexer.next_token(), Token::DoubleEquals);
        assert_eq!(lexer.next_token(), Token::NotEquals);
        assert_eq!(lexer.next_token(), Token::LessThan);
        assert_eq!(lexer.next_token(), Token::GreaterThan);
        assert_eq!(lexer.next_token(), Token::LessEqual);
        assert_eq!(lexer.next_token(), Token::GreaterEqual);
        assert_eq!(lexer.next_token(), Token::And);
        assert_eq!(lexer.next_token(), Token::Or);
    }

    #[test]
    fn test_lexer_literals() {
        let mut lexer = Lexer::new("42 3.15 \"hello\" true false");
        assert_eq!(lexer.next_token(), Token::Integer(42));
        assert_eq!(lexer.next_token(), Token::Float(3.15));
        assert_eq!(lexer.next_token(), Token::String("hello".to_string()));
        assert_eq!(lexer.next_token(), Token::Boolean(true));
        assert_eq!(lexer.next_token(), Token::Boolean(false));
    }

    #[test]
    fn test_lexer_identifiers() {
        let mut lexer = Lexer::new("foo bar_baz _test");
        assert_eq!(lexer.next_token(), Token::Identifier("foo".to_string()));
        assert_eq!(lexer.next_token(), Token::Identifier("bar_baz".to_string()));
        assert_eq!(lexer.next_token(), Token::Identifier("_test".to_string()));
    }

    #[test]
    fn test_lexer_comments() {
        let mut lexer = Lexer::new("foo // comment\nbar /* block */ baz");
        assert_eq!(lexer.next_token(), Token::Identifier("foo".to_string()));
        assert_eq!(lexer.next_token(), Token::Identifier("bar".to_string()));
        assert_eq!(lexer.next_token(), Token::Identifier("baz".to_string()));
    }

    #[test]
    fn test_parser_workflow() {
        let tokens = Lexer::new("workflow test {}").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        assert_eq!(ast.len(), 1);
        if let AstNode::Workflow { name, body } = &ast[0] {
            assert_eq!(name, "test");
            assert!(body.is_empty());
        } else {
            panic!("Expected workflow");
        }
    }

    #[test]
    fn test_parser_step() {
        let tokens = Lexer::new("step build = \"cargo build\"").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        assert_eq!(ast.len(), 1);
        if let AstNode::Step { name, .. } = &ast[0] {
            assert_eq!(name, "build");
        } else {
            panic!("Expected step");
        }
    }

    #[test]
    fn test_parser_if() {
        let tokens = Lexer::new("if true { let x = 1 }").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        assert_eq!(ast.len(), 1);
        assert!(matches!(ast[0], AstNode::If { .. }));
    }

    #[test]
    fn test_parser_if_else() {
        let tokens = Lexer::new("if false { let x = 1 } else { let y = 2 }").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        if let AstNode::If { else_branch, .. } = &ast[0] {
            assert!(else_branch.is_some());
        } else {
            panic!("Expected if");
        }
    }

    #[test]
    fn test_parser_for() {
        let tokens = Lexer::new("for i in [1, 2, 3] { print(i) }").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        if let AstNode::For { variable, .. } = &ast[0] {
            assert_eq!(variable, "i");
        } else {
            panic!("Expected for");
        }
    }

    #[test]
    fn test_parser_while() {
        let tokens = Lexer::new("while x < 10 { let x = x + 1 }").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        assert!(matches!(ast[0], AstNode::While { .. }));
    }

    #[test]
    fn test_parser_parallel() {
        let tokens = Lexer::new("parallel { step a = test; step b = lint }").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        if let AstNode::Parallel { body } = &ast[0] {
            assert_eq!(body.len(), 2);
        } else {
            panic!("Expected parallel");
        }
    }

    #[test]
    fn test_parser_function() {
        let tokens = Lexer::new("fn greet(name) { print(name) }").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        if let AstNode::FnDef { name, params, .. } = &ast[0] {
            assert_eq!(name, "greet");
            assert_eq!(params.len(), 1);
        } else {
            panic!("Expected function");
        }
    }

    #[test]
    fn test_parser_binary_expr() {
        let tokens = Lexer::new("1 + 2 * 3").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        // Should parse as 1 + (2 * 3) due to precedence
        if let AstNode::Binary { operator, .. } = &ast[0] {
            assert_eq!(operator, "+");
        } else {
            panic!("Expected binary");
        }
    }

    #[test]
    fn test_parser_pipeline() {
        let tokens = Lexer::new("a | b | c").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        if let AstNode::Pipeline { stages } = &ast[0] {
            assert_eq!(stages.len(), 3);
        } else {
            panic!("Expected pipeline");
        }
    }

    #[test]
    fn test_runtime_literals() {
        let result = run("42").unwrap();
        assert!(matches!(result, Value::Integer(42)));

        let result = run("\"hello\"").unwrap();
        assert!(matches!(result, Value::String(s) if s == "hello"));

        let result = run("true").unwrap();
        assert!(matches!(result, Value::Boolean(true)));
    }

    #[test]
    fn test_runtime_arithmetic() {
        let result = run("1 + 2").unwrap();
        assert!(matches!(result, Value::Integer(3)));

        let result = run("10 - 3").unwrap();
        assert!(matches!(result, Value::Integer(7)));

        let result = run("4 * 5").unwrap();
        assert!(matches!(result, Value::Integer(20)));

        let result = run("15 / 3").unwrap();
        assert!(matches!(result, Value::Integer(5)));
    }

    #[test]
    fn test_runtime_comparison() {
        let result = run("5 > 3").unwrap();
        assert!(matches!(result, Value::Boolean(true)));

        let result = run("5 < 3").unwrap();
        assert!(matches!(result, Value::Boolean(false)));

        let result = run("5 == 5").unwrap();
        assert!(matches!(result, Value::Boolean(true)));
    }

    #[test]
    fn test_runtime_logic() {
        let result = run("true && false").unwrap();
        assert!(matches!(result, Value::Boolean(false)));

        let result = run("true || false").unwrap();
        assert!(matches!(result, Value::Boolean(true)));

        let result = run("!false").unwrap();
        assert!(matches!(result, Value::Boolean(true)));
    }

    #[test]
    fn test_runtime_let() {
        let result = run("let x = 10; x").unwrap();
        assert!(matches!(result, Value::Integer(10)));
    }

    #[test]
    fn test_runtime_if() {
        let result = run("if true { 1 } else { 2 }").unwrap();
        assert!(matches!(result, Value::Integer(1)));

        let result = run("if false { 1 } else { 2 }").unwrap();
        assert!(matches!(result, Value::Integer(2)));
    }

    #[test]
    fn test_runtime_for() {
        let result = run("let sum = 0\n for i in [1, 2, 3] { let sum = sum + i }\n sum").unwrap();
        assert!(matches!(result, Value::Integer(6)));
    }

    #[test]
    fn test_runtime_while() {
        let result = run("let x = 0\n while x < 5 { let x = x + 1 }\n x").unwrap();
        assert!(matches!(result, Value::Integer(5)));
    }

    #[test]
    fn test_runtime_function() {
        let result = run("fn add(a, b) { a + b }\n add(2, 3)").unwrap();
        assert!(matches!(result, Value::Integer(5)));
    }

    #[test]
    fn test_runtime_step() {
        let result = run("step test = \"cargo test\"\n test.success").unwrap();
        assert!(matches!(result, Value::Boolean(true)));
    }

    #[test]
    fn test_runtime_workflow() {
        let result = run("workflow build { let x = 1; let y = 2; x + y }").unwrap();
        assert!(matches!(result, Value::Integer(3)));
    }

    #[test]
    fn test_runtime_array() {
        let result = run("[1, 2, 3]").unwrap();
        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 3);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_runtime_builtin_len() {
        let result = run("len(\"hello\")").unwrap();
        assert!(matches!(result, Value::Integer(5)));

        let result = run("len([1, 2, 3])").unwrap();
        assert!(matches!(result, Value::Integer(3)));
    }

    #[test]
    fn test_runtime_builtin_range() {
        let result = run("range(5)").unwrap();
        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 5);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_value_as_bool() {
        assert!(!Value::Null.as_bool());
        assert!(Value::Boolean(true).as_bool());
        assert!(!Value::Boolean(false).as_bool());
        assert!(Value::Integer(1).as_bool());
        assert!(!Value::Integer(0).as_bool());
        assert!(Value::String("hello".to_string()).as_bool());
        assert!(!Value::String(String::new()).as_bool());
    }

    #[test]
    fn test_value_as_string() {
        assert_eq!(Value::Integer(42).as_string(), "42");
        assert_eq!(Value::Boolean(true).as_string(), "true");
        assert_eq!(Value::String("test".to_string()).as_string(), "test");
    }

    #[test]
    fn test_token_is_keyword() {
        assert!(Token::Workflow.is_keyword());
        assert!(Token::If.is_keyword());
        assert!(!Token::Identifier("test".to_string()).is_keyword());
    }

    #[test]
    fn test_lexer_string_escape() {
        let mut lexer = Lexer::new("\"hello\\nworld\"");
        if let Token::String(s) = lexer.next_token() {
            assert!(s.contains('\n'));
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_lexer_arrow() {
        let mut lexer = Lexer::new("-> =>");
        assert_eq!(lexer.next_token(), Token::Arrow);
        assert_eq!(lexer.next_token(), Token::DoubleArrow);
    }

    #[test]
    fn test_complex_workflow() {
        let source = r#"
            workflow build {
                step check = cargo check;
                if check.success {
                    parallel {
                        step test = cargo test;
                        step lint = cargo clippy;
                    }
                }
            }
        "#;

        let result = run(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_runtime_parallel() {
        let result = run("parallel { let a = 1; let b = 2 }").unwrap();
        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 2);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_runtime_with_executor() {
        let mut runtime = Runtime::new().with_executor(|cmd| {
            if cmd.contains("fail") {
                (false, String::new(), "Command failed".to_string())
            } else {
                (true, format!("Output of: {}", cmd), String::new())
            }
        });

        let tokens = Lexer::new("step test = \"echo hello\"").tokenize();
        let mut parser = Parser::new(tokens);
        let ast = parser.parse().unwrap();

        let result = runtime.execute(&ast).unwrap();
        if let Value::StepResult {
            success, output, ..
        } = result
        {
            assert!(success);
            assert!(output.contains("echo hello"));
        } else {
            panic!("Expected step result");
        }
    }
}
