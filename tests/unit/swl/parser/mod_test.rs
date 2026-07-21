use super::*;
use std::path::PathBuf;

fn workflow_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("workflows")
        .join(file_name)
}

#[test]
fn parse_document_reads_valid_yaml() {
    let source = r#"
version: "2.0"
name: review
agents:
  architect:
    model:
      provider: openai
      name: mock-model
      temperature: 0.2
    tools: [file_read]
    output_key: findings
workflows:
  parallel_review:
    type: map_reduce
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
    assert_eq!(doc.agents["architect"].model.name(), "mock-model");
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

#[test]
fn parse_bug_investigation_workflow() {
    let source = std::fs::read_to_string(workflow_path("bug_investigation.swl")).unwrap();
    parse_document(&source).unwrap();
}

#[test]
fn parse_code_review_workflow() {
    let source = std::fs::read_to_string(workflow_path("code_review.swl")).unwrap();
    parse_document(&source).unwrap();
}

#[test]
fn parse_documentation_update_workflow() {
    let source = std::fs::read_to_string(workflow_path("documentation_update.swl")).unwrap();
    parse_document(&source).unwrap();
}

#[test]
fn parse_multi_agent_swarm_workflow() {
    let source = std::fs::read_to_string(workflow_path("multi_agent_swarm.swl")).unwrap();
    parse_document(&source).unwrap();
}

#[test]
fn parse_product_sprint_workflow() {
    let source = std::fs::read_to_string(workflow_path("product_sprint.swl")).unwrap();
    parse_document(&source).unwrap();
}

#[test]
fn parse_security_audit_workflow() {
    let source = std::fs::read_to_string(workflow_path("security_audit.swl")).unwrap();
    parse_document(&source).unwrap();
}

#[test]
fn parse_test_simple_workflow() {
    let source = std::fs::read_to_string(workflow_path("test_simple.swl")).unwrap();
    parse_document(&source).unwrap();
}

#[test]
fn parse_test_execution_workflow() {
    let source = std::fs::read_to_string(workflow_path("test_execution.swl")).unwrap();
    parse_document(&source).unwrap();
}
