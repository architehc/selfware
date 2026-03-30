use super::ast::{
    GuardCondition, Guardrail, ReduceStage, SwlDocument, WorkflowDefinition, WorkflowStep,
    WorkflowType,
};

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

    for (agent_name, agent) in &doc.agents {
        if agent.model.name().trim().is_empty() {
            issues.push(ValidationIssue::new(
                format!("agents.{agent_name}.model"),
                "model must not be empty",
            ));
        }

        for (tool_idx, tool) in agent.tools.iter().enumerate() {
            if tool.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("agents.{agent_name}.tools.{tool_idx}"),
                    "tool name must not be empty",
                ));
            }
        }

        if let Some(output_key) = &agent.output_key {
            if output_key.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("agents.{agent_name}.output_key"),
                    "output_key must not be empty",
                ));
            }
        }
    }

    for (workflow_name, workflow) in &doc.workflows {
        let path = format!("workflows.{workflow_name}");
        validate_workflow(doc, workflow, &path, &mut issues);
    }

    for (idx, guardrail) in doc.guardrails.iter().enumerate() {
        let path = format!("guardrails.{idx}");

        if guardrail
            .name
            .as_ref()
            .is_none_or(|name| name.trim().is_empty())
        {
            issues.push(ValidationIssue::new(
                format!("{path}.name"),
                "guardrail name must not be empty",
            ));
        }

        if guardrail
            .guardrail_type
            .as_ref()
            .is_none_or(|guardrail_type| guardrail_type.trim().is_empty())
        {
            issues.push(ValidationIssue::new(
                format!("{path}.type"),
                "guardrail type must not be empty",
            ));
        }

        validate_guardrail(guardrail, &path, &mut issues);
    }

    if let Some(state) = &doc.state {
        for (idx, field) in state.fields.iter().enumerate() {
            if field.name.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("state.fields.{idx}.name"),
                    "state field name must not be empty",
                ));
            }
        }
    }

    if let Some(telemetry) = &doc.telemetry {
        if let Some(export) = &telemetry.export {
            if export.export_type.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    "telemetry.export.type",
                    "telemetry export type must not be empty",
                ));
            }

            if export
                .path
                .as_deref()
                .is_some_and(|path| path.trim().is_empty())
            {
                issues.push(ValidationIssue::new(
                    "telemetry.export.path",
                    "telemetry export path must not be empty",
                ));
            }
        }
    }

    issues
}

fn validate_workflow(
    doc: &SwlDocument,
    workflow: &WorkflowDefinition,
    path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
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

    for (idx, step) in workflow.steps.iter().enumerate() {
        validate_step(doc, step, &format!("{path}.steps.{idx}"), issues);
    }

    if let Some(map) = &workflow.map {
        if map.targets.is_empty() && map.parallel.is_none() {
            issues.push(ValidationIssue::new(
                format!("{path}.map"),
                "map stage must define targets or parallel branches",
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

        if let Some(parallel) = &map.parallel {
            if parallel.branches.is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{path}.map.parallel.branches"),
                    "parallel branches must not be empty",
                ));
            }

            for (idx, step) in parallel.branches.iter().enumerate() {
                validate_step(
                    doc,
                    step,
                    &format!("{path}.map.parallel.branches.{idx}"),
                    issues,
                );
            }
        }
    }

    if let Some(reduce) = &workflow.reduce {
        match reduce {
            ReduceStage::Code(block) => {
                if block.code.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        format!("{path}.reduce.code"),
                        "reduce code must not be empty",
                    ));
                }
            }
            ReduceStage::Aggregate(stage) => {
                if !doc.agents.contains_key(&stage.agent) {
                    issues.push(ValidationIssue::new(
                        format!("{path}.reduce.agent"),
                        format!("unknown agent target '{}'", stage.agent),
                    ));
                }
            }
        }
    }

    if let Some(merge) = &workflow.merge {
        if !doc.agents.contains_key(&merge.agent) {
            issues.push(ValidationIssue::new(
                format!("{path}.merge.agent"),
                format!("unknown agent target '{}'", merge.agent),
            ));
        }
    }
}

fn validate_step(
    doc: &SwlDocument,
    step: &WorkflowStep,
    path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    let kinds = usize::from(step.delegate.is_some())
        + usize::from(step.agent.is_some())
        + usize::from(step.parallel.is_some())
        + usize::from(step.guard.is_some());

    if kinds == 0 {
        issues.push(ValidationIssue::new(
            path,
            "step must define one of delegate, agent, parallel, or guard",
        ));
        return;
    }

    if kinds > 1 {
        issues.push(ValidationIssue::new(
            path,
            "step must not combine delegate, agent, parallel, and guard definitions",
        ));
    }

    if let Some(delegate) = &step.delegate {
        if !doc.agents.contains_key(delegate) {
            issues.push(ValidationIssue::new(
                format!("{path}.delegate"),
                format!("unknown agent target '{delegate}'"),
            ));
        }
    }

    if let Some(agent) = &step.agent {
        if !doc.agents.contains_key(agent) {
            issues.push(ValidationIssue::new(
                format!("{path}.agent"),
                format!("unknown agent target '{agent}'"),
            ));
        }

        if step
            .action
            .as_ref()
            .is_none_or(|action| action.trim().is_empty())
        {
            issues.push(ValidationIssue::new(
                format!("{path}.action"),
                "legacy agent steps must define a non-empty action",
            ));
        }
    }

    if let Some(parallel) = &step.parallel {
        if parallel.branches.is_empty() {
            issues.push(ValidationIssue::new(
                format!("{path}.parallel.branches"),
                "parallel branches must not be empty",
            ));
        }

        for (idx, branch) in parallel.branches.iter().enumerate() {
            validate_step(
                doc,
                branch,
                &format!("{path}.parallel.branches.{idx}"),
                issues,
            );
        }
    }

    if let Some(guard) = &step.guard {
        validate_guardrail(guard, &format!("{path}.guard"), issues);
    }
}

fn validate_guardrail(guardrail: &Guardrail, path: &str, issues: &mut Vec<ValidationIssue>) {
    match &guardrail.condition {
        GuardCondition::Inline(condition) => {
            if condition.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{path}.condition"),
                    "guard condition must not be empty",
                ));
            }
        }
        GuardCondition::Code(block) => {
            if block.code.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{path}.condition.code"),
                    "guard condition code must not be empty",
                ));
            }
        }
    }

    if guardrail.on_violation.trim().is_empty() {
        issues.push(ValidationIssue::new(
            format!("{path}.on_violation"),
            "on_violation must not be empty",
        ));
    }
}

#[cfg(test)]
mod tests {
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
}
