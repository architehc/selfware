//! Parser -- transforms a token stream into an [`AstNode`] tree.

use super::ast::AstNode;
use super::lexer::Token;

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
    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::workflow_dsl::Lexer;

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
}
