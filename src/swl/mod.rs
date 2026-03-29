//! Selfware Workflow Language (SWL) — EXPERIMENTAL.
//!
//! Parsing, semantic validation, lightweight type/codegen checks, and a
//! Rust stub generator. Not yet production-ready.
//!
//! ## Guardrail Enforcement
//!
//! SWL provides runtime guardrail enforcement at multiple checkpoints:
//! - `pre_agent`: Before agent execution
//! - `post_agent`: After agent execution
//! - `pre_tool`: Before tool execution
//! - `post_tool`: After tool execution
//! - `pre_workflow`: Before workflow execution
//! - `post_workflow`: After workflow execution
//!
//! Guardrails support multiple violation actions:
//! - `block`: Halt execution
//! - `warn`: Log warning and continue
//! - `log`: Silent logging
//! - `alert`: Notify external systems

#![allow(dead_code, unused_imports, unused_variables)]

pub mod codegen;
pub mod guardrails;
pub mod lowering;
pub mod parser;
pub mod runtime;
pub mod state;
pub mod types;

pub use codegen::generate_rust_stub;
pub use lowering::{lower_document, LoweredSwl};
pub use parser::{
    parse_document, validate_document, CodeBlock, CodeLanguage, ParseError, SwlDocument,
    ValidationIssue, WorkflowType,
};
pub use runtime::{
    AgentTelemetry, ExecutionContext, ExecutionEvent, ExecutionResult, ExecutionStatus,
    SwlRuntime, WorkflowTelemetry,
};
pub use state::{
    backend::{StateBackend, StateBackendType},
    validation::{validate_state_against_schema, ValidationError},
    StateManager,
};
pub use types::{check_codegen_compatibility, FieldType, StateField, StateSchema, TypeIssue};
