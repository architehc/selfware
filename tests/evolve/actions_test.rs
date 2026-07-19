use selfware::evolve::actions::{Action, ActionEngine};

#[test]
fn test_action_engine_creates_branch_name() {
    let action = Action::Extend {
        component: "agent".to_string(),
    };
    let name = ActionEngine::branch_name(&action);
    assert!(name.starts_with("evolve/extend-agent-"));
}
