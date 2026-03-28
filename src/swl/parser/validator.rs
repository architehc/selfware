use super::ast::{CodeLanguage, SwlDocument, WorkflowType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
}

impl ValidationIssue {
    fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

pub fn validate_document(doc: &SwlDocument) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    if doc.version.trim().is_empty() {
        issues.push(ValidationIssue::new("version", "version must not be empty"));
    }
    if doc.name.trim().is_empty() {
        issues.push(ValidationIssue::new("name", "name must not be empty"));
    }
    if doc.agents.is_empty() {
        issues.push(ValidationIssue::new(
            "agents",
            "document must define at least one agent",
        ));
    }
    if doc.workflows.is_empty() {
        issues.push(ValidationIssue::new(
            "workflows",
            "document must define at least one workflow",
        ));
    }

    for (workflow_name, workflow) in &doc.workflows {
        let path = format!("workflows.{workflow_name}");

        if matches!(workflow.workflow_type, WorkflowType::MapReduce) {
            if workflow.map.is_none() {
                issues.push(ValidationIssue::new(
                    format!("{path}.map"),
                    "map-reduce workflow requires a map stage",
                ));
            }
            if workflow.reduce.is_none() {
                issues.push(ValidationIssue::new(
                    format!("{path}.reduce"),
                    "map-reduce workflow requires a reduce stage",
                ));
            }
        }

        if let Some(map) = &workflow.map {
            if map.targets.is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{path}.map.targets"),
                    "map stage must target at least one agent",
                ));
            }

            for target in &map.targets {
                if !doc.agents.contains_key(target) {
                    issues.push(ValidationIssue::new(
                        format!("{path}.map.targets"),
                        format!("unknown agent target '{target}'"),
                    ));
                }
            }
        }

        if let Some(reduce) = &workflow.reduce {
            if reduce.code.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{path}.reduce.code"),
                    "reduce code must not be empty",
                ));
            }

            match reduce.language {
                CodeLanguage::Rust | CodeLanguage::Python => {}
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swl::parser::ast::{
        AgentDefinition, CodeBlock, DashboardConfig, MapStage, SwlDocument, TelemetryConfig,
        WorkflowDefinition,
    };
    use std::collections::BTreeMap;

    fn valid_doc() -> SwlDocument {
        let mut agents = BTreeMap::new();
        agents.insert(
            "architect".to_string(),
            AgentDefinition {
                model: "mock-model".to_string(),
                role: Some("Review architecture".to_string()),
                instruction: None,
            },
        );

        let mut workflows = BTreeMap::new();
        workflows.insert(
            "review".to_string(),
            WorkflowDefinition {
                workflow_type: WorkflowType::MapReduce,
                map: Some(MapStage {
                    targets: vec!["architect".to_string()],
                    input: Some("${file://src/*.rs}".to_string()),
                }),
                reduce: Some(CodeBlock {
                    language: CodeLanguage::Rust,
                    code: "fn merge() {}".to_string(),
                }),
            },
        );

        SwlDocument {
            version: "2.0".to_string(),
            name: "code_review".to_string(),
            agents,
            workflows,
            guardrails: Vec::new(),
            telemetry: Some(TelemetryConfig { traces: vec![] }),
            dashboard: Some(DashboardConfig {
                layout: Some("grid".to_string()),
                refresh: Some("100ms".to_string()),
            }),
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
}
