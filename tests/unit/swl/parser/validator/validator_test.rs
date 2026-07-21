use super::*;
use crate::swl::parser::ast::{
    AgentDefinition, CodeBlock, CodeLanguage, MapStage, ModelSpec, ReduceStage, SwlDocument,
    WorkflowDefinition,
};
use std::collections::BTreeMap;

fn valid_doc() -> SwlDocument {
    let mut agents = BTreeMap::new();
    agents.insert(
        "architect".to_string(),
        AgentDefinition {
            model: ModelSpec::Simple("mock-model".to_string()),
            role: Some("Review architecture".to_string()),
            instruction: None,
            tools: vec![],
            output_key: None,
            sub_agents: vec![],
        },
    );

    let mut workflows = BTreeMap::new();
    workflows.insert(
        "review".to_string(),
        WorkflowDefinition {
            workflow_type: WorkflowType::MapReduce,
            description: None,
            steps: vec![],
            map: Some(MapStage {
                targets: vec!["architect".to_string()],
                input: None,
                parallel: None,
            }),
            reduce: Some(ReduceStage::Code(CodeBlock {
                language: CodeLanguage::Rust,
                code: "fn merge() {}".to_string(),
            })),
            merge: None,
        },
    );

    SwlDocument {
        version: "2.0".to_string(),
        name: "code_review".to_string(),
        description: None,
        metadata: None,
        agents,
        workflows,
        guardrails: Vec::new(),
        telemetry: None,
        dashboard: None,
        state: None,
    }
}

#[test]
fn validate_document_accepts_minimal_valid_doc() {
    assert!(validate_document(&valid_doc()).is_empty());
}

#[test]
fn validate_document_rejects_unknown_map_targets() {
    let mut doc = valid_doc();
    doc.workflows
        .get_mut("review")
        .unwrap()
        .map
        .as_mut()
        .unwrap()
        .targets = vec!["missing".to_string()];

    let issues = validate_document(&doc);
    assert!(issues
        .iter()
        .any(|issue| issue.message.contains("unknown agent")));
}

#[test]
fn validate_document_accepts_object_model_specs() {
    let source = r#"
version: "1.0"
name: review
agents:
  architect:
    model:
      provider: openai
      name: txn545/Qwen3.5-122B-A10B-NVFP4
      temperature: 0.2
workflows:
  quick_review:
    type: sequential
    steps:
      - delegate: architect
"#;

    let doc = crate::swl::parser::parse_document(source).unwrap();
    assert!(validate_document(&doc).is_empty());
}
