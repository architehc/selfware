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

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod value;

// Re-export all public types so the external API is unchanged.
pub use ast::AstNode;
pub use lexer::{Lexer, Token};
pub use parser::Parser;
pub use runtime::{ExecutionEvent, Runtime};
pub use value::Value;

/// Type alias for command executor callback
/// Returns (success, stdout, stderr)
pub type CommandExecutor = Box<dyn Fn(&str) -> (bool, String, String) + Send + Sync>;

/// Compile and run DSL source
pub fn run(source: &str) -> Result<Value, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();

    let mut parser = Parser::new(tokens);
    let ast = parser.parse()?;

    let mut runtime = Runtime::new();
    runtime.execute(&ast)
}
