pub mod ast;
pub mod validator;

use thiserror::Error;

pub use ast::{CodeBlock, CodeLanguage, SwlDocument, WorkflowType};
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
mod tests {
    use super::*;

    #[test]
    fn parse_document_reads_valid_yaml() {
        let source = r#"
version: "2.0"
name: review
agents:
  architect:
    model: mock-model
workflows:
  parallel_review:
    type: map-reduce
    map:
      targets: [architect]
    reduce:
      language: rust
      code: |
        fn merge() {}
"#;

        let doc = parse_document(source).unwrap();
        assert_eq!(doc.name, "review");
        assert!(doc.agents.contains_key("architect"));
    }

    #[test]
    fn parse_document_surfaces_validation_errors() {
        let source = r#"
version: "2.0"
name: broken
agents: {}
workflows:
  parallel_review:
    type: map-reduce
    map:
      targets: [missing]
"#;

        let err = parse_document(source).unwrap_err().to_string();
        assert!(err.contains("unknown agent"));
        assert!(err.contains("requires a reduce stage"));
    }
}
