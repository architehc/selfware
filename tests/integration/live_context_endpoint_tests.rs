//! Live endpoint probes for the hierarchical context system.
//!
//! Run explicitly against an sglang endpoint:
//! `SELFWARE_LIVE_ENDPOINT=http://localhost:8000/v1 cargo test --features integration --test integration live_context_endpoint -- --ignored`

fn live_config() -> selfware::config::Config {
    let endpoint = std::env::var("SELFWARE_LIVE_ENDPOINT")
        .expect("SELFWARE_LIVE_ENDPOINT must be set when running ignored live endpoint tests");
    let model = std::env::var("SELFWARE_LIVE_MODEL").unwrap_or_else(|_| "qwen3.5-27b".to_string());

    selfware::config::Config {
        endpoint,
        model,
        max_tokens: 900_000,
        agent: selfware::config::AgentConfig {
            max_iterations: 20,
            step_timeout_secs: 120,
            stream_stall_timeout_secs: None,
            token_budget: 900_000,
            streaming: true,
            native_function_calling: false,
            min_completion_steps: 0,
            require_verification_before_completion: false,
            ..Default::default()
        },
        safety: selfware::config::SafetyConfig {
            allowed_paths: vec!["./**".to_string(), "/**".to_string()],
            ..Default::default()
        },
        execution_mode: selfware::config::ExecutionMode::Yolo,
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "requires SELFWARE_LIVE_ENDPOINT"]
async fn test_live_review_with_skeletons() {
    let mut agent = selfware::agent::Agent::new(live_config()).await.unwrap();
    agent
        .run_task("review the code in src/token_count.rs and summarize what it does")
        .await
        .expect("live review task failed");
}

#[tokio::test]
#[ignore = "requires SELFWARE_LIVE_ENDPOINT"]
async fn test_live_context_tools() {
    let mut agent = selfware::agent::Agent::new(live_config()).await.unwrap();
    agent
        .run_task("use context_status to check your context window, then report the budget usage")
        .await
        .expect("live context-tools task failed");
}

#[tokio::test]
#[ignore = "requires SELFWARE_LIVE_ENDPOINT"]
async fn test_live_skeleton_review() {
    let mut agent = selfware::agent::Agent::new(live_config()).await.unwrap();
    agent
        .run_task("read all rust files and give a brief summary of the project structure")
        .await
        .expect("live skeleton review task failed");
}
