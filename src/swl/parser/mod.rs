pub mod ast;
pub mod validator;

use thiserror::Error;

pub use ast::{
    AggregateStage, CodeBlock, CodeLanguage, GuardCondition, Guardrail, ModelConfig, ModelSpec,
    ReduceStage, SwlDocument, WorkflowStep, WorkflowType,
};
pub use validator::{validate_document, ValidationIssue};

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("failed to parse SWL YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("SWL validation failed:\n{0}")]
    Validation(String),
}

pub fn parse_document(source: &str) -> Result<SwlDocument, ParseError> {
    let doc: SwlDocument = serde_yaml::from_str(source)?;
    let issues = validate_document(&doc);

    if issues.is_empty() {
        return Ok(doc);
    }

    Err(ParseError::Validation(format_issues(&issues)))
}

fn format_issues(issues: &[ValidationIssue]) -> String {
    issues
        .iter()
        .map(|issue| format!("- {}: {}", issue.path, issue.message))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "../../../tests/unit/swl/parser/mod_test.rs"]
mod tests;
