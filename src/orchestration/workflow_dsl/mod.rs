//! Workflow DSL -- a small imperative language for defining agent workflows.
//!
//! The DSL lets you express multi-step build/test/deploy pipelines as
//! structured programs rather than shell scripts.  Source text goes through
//! three stages:
//!
//! 1. **Lexing** ([`lexer`]) -- source text to [`Token`] stream
//! 2. **Parsing** ([`parser`]) -- tokens to [`AstNode`] tree
//! 3. **Execution** ([`runtime`]) -- tree-walking interpreter with a [`Value`] result
//!
//! The convenience function [`run`] chains all three stages.
//!
//! # Language features
//!
//! - `workflow` / `step` -- named workflow definitions with named steps
//! - `parallel { ... }` -- concurrent step execution (real threads)
//! - `sequence { ... }` -- explicit sequential grouping
//! - `if` / `else`, `for .. in`, `while` -- control flow
//! - `fn` / `return` -- user-defined functions
//! - `let` -- variable binding
//! - `|` -- pipeline operator (chains step output as `_input`)
//! - `on error` -- error handler registration
//! - Literals: integers, floats, booleans, strings, arrays
//! - Binary/unary operators with standard arithmetic precedence
//! - Property access (`step.success`, `step.output`, `step.error`)
//! - Built-ins: `print`, `len`, `range`, `env`
//!
//! # Syntax example
//!
//! ```text
//! workflow build_project {
//!     step check = "cargo check";
//!     if check.success {
//!         step test = "cargo test";
//!     }
//!     parallel {
//!         step lint = "cargo clippy";
//!         step fmt = "cargo fmt --check";
//!     }
//! }
//! ```
//!
//! # Programmatic usage
//!
//! ```ignore
//! use selfware::orchestration::workflow_dsl::{run, Runtime, Lexer, Parser};
//!
//! // One-shot:
//! let result = run("let x = 1 + 2; x")?;
//!
//! // With a custom command executor:
//! let mut lexer = Lexer::new(source);
//! let tokens = lexer.tokenize();
//! let ast = Parser::new(tokens).parse()?;
//! let mut rt = Runtime::new()
//!     .with_executor(|cmd| { /* run cmd, return (ok, stdout, stderr) */ });
//! let result = rt.execute(&ast)?;
//! ```

#![allow(dead_code, unused_imports, unused_variables)]

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
