//! Selfware Workflow Language (SWL).
//!
//! This is the initial implementation slice: parsing, semantic validation,
//! lightweight type/codegen checks, and a Rust stub generator.

pub mod codegen;
pub mod parser;
pub mod types;

pub use codegen::generate_rust_stub;
pub use parser::{
    parse_document, validate_document, CodeBlock, CodeLanguage, ParseError, SwlDocument,
    ValidationIssue, WorkflowType,
};
pub use types::{check_codegen_compatibility, FieldType, StateField, StateSchema, TypeIssue};
