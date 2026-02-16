//! Lexer -- tokenizes DSL source text into a stream of [`Token`]s.

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
}
