use super::*;
use crate::swl::parse_document;

#[test]
fn lower_code_review_produces_executor_workflow() {
    let source = std::fs::read_to_string("workflows/code_review.swl").unwrap();
    let doc = parse_document(&source).unwrap();
    let lowered = lower_document(&doc).unwrap();

    let workflow = lowered
        .workflows
        .iter()
        .find(|wf| wf.name == "full_review")
        .unwrap();
    assert!(!workflow.steps.is_empty());
    assert!(workflow.outputs.iter().any(|o| o.name == "staff_review"));
    assert!(lowered
        .warnings
        .iter()
        .any(|warning| warning.contains("guard")));
}

#[test]
fn lower_multi_agent_swarm_emits_reduce_output() {
    let source = std::fs::read_to_string("workflows/multi_agent_swarm.swl").unwrap();
    let doc = parse_document(&source).unwrap();
    let lowered = lower_document(&doc).unwrap();

    let workflow = lowered
        .workflows
        .iter()
        .find(|wf| wf.name == "swarm_with_consensus")
        .unwrap();
    assert!(workflow.outputs.iter().any(|o| o.name == "consensus_spec"));
}

#[test]
fn lower_legacy_test_execution_emits_log_steps() {
    let source = std::fs::read_to_string("workflows/test_execution.swl").unwrap();
    let doc = parse_document(&source).unwrap();
    let lowered = lower_document(&doc).unwrap();

    let workflow = lowered
        .workflows
        .iter()
        .find(|wf| wf.name == "main_flow")
        .unwrap();
    assert!(workflow
        .steps
        .iter()
        .any(|step| matches!(step.step_type, StepType::Log { .. })));
}
