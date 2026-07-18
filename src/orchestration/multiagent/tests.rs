//! Multi-Agent System Tests

use std::time::Duration;

use tokio::sync::mpsc;

use crate::config::Config;
use crate::orchestration::multiagent::{
    AgentInstance, AgentResult, AgentStatus, MultiAgentChat, MultiAgentConfig, MultiAgentEvent,
    MultiAgentFailurePolicy, MAX_CONCURRENT_AGENTS,
};
use crate::swarm::AgentRole;

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_multiagent_config_default() {
    let config = MultiAgentConfig::default();
    assert_eq!(config.max_concurrency, 4);
    assert_eq!(config.roles.len(), 4);
}

#[test]
fn test_multiagent_config_with_concurrency() {
    let config = MultiAgentConfig::default().with_concurrency(16);
    assert_eq!(config.max_concurrency, 16);

    // Should cap at MAX_CONCURRENT_AGENTS
    let config = MultiAgentConfig::default().with_concurrency(100);
    assert_eq!(config.max_concurrency, MAX_CONCURRENT_AGENTS);
}

#[test]
fn test_multiagent_config_with_roles() {
    let roles = vec![AgentRole::Coder, AgentRole::Tester];
    let config = MultiAgentConfig::default().with_roles(roles.clone());
    assert_eq!(config.roles.len(), 2);
    assert!(config.roles.contains(&AgentRole::Coder));
    assert!(config.roles.contains(&AgentRole::Tester));
}

#[test]
fn test_failure_policy_default_is_fail_fast() {
    let policy = MultiAgentFailurePolicy::default();
    assert_eq!(policy, MultiAgentFailurePolicy::FailFast);
}

#[test]
fn test_failure_policy_variants_not_equal() {
    assert_ne!(
        MultiAgentFailurePolicy::BestEffort,
        MultiAgentFailurePolicy::FailFast
    );
}

#[test]
fn test_failure_policy_debug() {
    let best_effort = format!("{:?}", MultiAgentFailurePolicy::BestEffort);
    let fail_fast = format!("{:?}", MultiAgentFailurePolicy::FailFast);
    assert_eq!(best_effort, "BestEffort");
    assert_eq!(fail_fast, "FailFast");
}

#[test]
fn test_config_default_failure_policy() {
    let config = MultiAgentConfig::default();
    // The config default explicitly sets BestEffort (distinct from the enum default of FailFast)
    assert_eq!(config.failure_policy, MultiAgentFailurePolicy::BestEffort);
}

#[test]
fn test_config_default_roles_are_architect_coder_tester_reviewer() {
    let config = MultiAgentConfig::default();
    assert_eq!(config.roles.len(), 4);
    assert_eq!(config.roles[0], AgentRole::Architect);
    assert_eq!(config.roles[1], AgentRole::Coder);
    assert_eq!(config.roles[2], AgentRole::Tester);
    assert_eq!(config.roles[3], AgentRole::Reviewer);
}

#[test]
fn test_with_concurrency_clamps_zero_to_one() {
    let config = MultiAgentConfig::default().with_concurrency(0);
    assert_eq!(config.max_concurrency, 1);
}

#[test]
fn test_with_concurrency_clamps_large_to_max() {
    let config = MultiAgentConfig::default().with_concurrency(1000);
    assert_eq!(config.max_concurrency, MAX_CONCURRENT_AGENTS);
}

// ============================================================================
// Agent Instance Tests
// ============================================================================

#[test]
fn test_agent_instance() {
    let agent = AgentInstance {
        id: 0,
        role: AgentRole::Coder,
        name: "Test Agent".to_string(),
        messages: vec![],
        status: AgentStatus::Idle,
        last_heartbeat: std::time::Instant::now(),
    };
    assert_eq!(agent.status, AgentStatus::Idle);
}

#[test]
fn test_agent_status_variants() {
    assert_eq!(AgentStatus::Idle, AgentStatus::Idle);
    assert_eq!(AgentStatus::Working, AgentStatus::Working);
    assert_eq!(AgentStatus::Completed, AgentStatus::Completed);
    assert_eq!(AgentStatus::Failed, AgentStatus::Failed);
    assert_ne!(AgentStatus::Idle, AgentStatus::Working);
}

#[test]
fn test_agent_status_debug() {
    let status = AgentStatus::Idle;
    let debug_str = format!("{:?}", status);
    assert_eq!(debug_str, "Idle");
}

// ============================================================================
// Agent Result Tests
// ============================================================================

#[test]
fn test_agent_result() {
    let result = AgentResult {
        agent_id: 0,
        agent_name: "Test".to_string(),
        role: AgentRole::Coder,
        content: "Hello".to_string(),
        usage: None,
        duration: Duration::from_secs(1),
        success: true,
        error: None,
    };
    assert!(result.success);
}

#[test]
fn test_aggregate_results() {
    let results = vec![
        AgentResult {
            agent_id: 0,
            agent_name: "Agent-0".to_string(),
            role: AgentRole::Coder,
            content: "Code here".to_string(),
            usage: None,
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        },
        AgentResult {
            agent_id: 1,
            agent_name: "Agent-1".to_string(),
            role: AgentRole::Tester,
            content: "Tests here".to_string(),
            usage: None,
            duration: Duration::from_secs(2),
            success: true,
            error: None,
        },
    ];

    let summary = MultiAgentChat::aggregate_results(&results);
    assert!(summary.contains("Agent-0"));
    assert!(summary.contains("Agent-1"));
    assert!(summary.contains("Code here"));
    assert!(summary.contains("Tests here"));
}

#[test]
fn test_aggregate_results_with_failures() {
    let results = vec![
        AgentResult {
            agent_id: 0,
            agent_name: "Success".to_string(),
            role: AgentRole::Coder,
            content: "Good output".to_string(),
            usage: None,
            duration: Duration::from_secs(1),
            success: true,
            error: None,
        },
        AgentResult {
            agent_id: 1,
            agent_name: "Failure".to_string(),
            role: AgentRole::Tester,
            content: "".to_string(),
            usage: None,
            duration: Duration::from_secs(2),
            success: false,
            error: Some("Error occurred".to_string()),
        },
    ];

    let summary = MultiAgentChat::aggregate_results(&results);
    assert!(summary.contains("Success"));
    assert!(summary.contains("Good output"));
    assert!(summary.contains("Failure"));
    assert!(summary.contains("FAILED"));
    assert!(summary.contains("Error occurred"));
}

#[test]
fn test_aggregate_results_empty() {
    let results: Vec<AgentResult> = vec![];
    let summary = MultiAgentChat::aggregate_results(&results);
    assert!(summary.contains("Summary"));
    assert!(!summary.contains("###")); // No agent sections
}

// ============================================================================
// Event Tests
// ============================================================================

#[test]
fn test_multiagent_event_started() {
    let event = MultiAgentEvent::AgentStarted {
        agent_id: 0,
        name: "Test".to_string(),
        task: "Do something".to_string(),
    };
    if let MultiAgentEvent::AgentStarted {
        agent_id,
        name,
        task,
    } = event
    {
        assert_eq!(agent_id, 0);
        assert_eq!(name, "Test");
        assert_eq!(task, "Do something");
    }
}

#[test]
fn test_total_usage_sums_tokens_and_cost() {
    use crate::api::types::Usage;

    let make = |usage: Option<Usage>| AgentResult {
        agent_id: 0,
        agent_name: "A".to_string(),
        role: AgentRole::Coder,
        content: String::new(),
        usage,
        duration: Duration::from_secs(1),
        success: true,
        error: None,
    };

    let results = vec![
        make(Some(Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cost: Some(0.001),
        })),
        make(Some(Usage {
            prompt_tokens: 2,
            completion_tokens: 3,
            total_tokens: 5,
            cost: None,
        })),
        make(None),
    ];

    let total = MultiAgentChat::total_usage(&results);
    assert_eq!(total.prompt_tokens, 12);
    assert_eq!(total.completion_tokens, 8);
    assert_eq!(total.total_tokens, 20);
    // Cost sums only over results whose provider reported one.
    assert!((total.cost.unwrap() - 0.001).abs() < 1e-12);
}

#[test]
fn test_total_usage_cost_none_when_unreported() {
    let results = vec![AgentResult {
        agent_id: 0,
        agent_name: "A".to_string(),
        role: AgentRole::Coder,
        content: String::new(),
        usage: Some(crate::api::types::Usage {
            prompt_tokens: 1,
            completion_tokens: 1,
            total_tokens: 2,
            cost: None,
        }),
        duration: Duration::from_secs(1),
        success: true,
        error: None,
    }];

    let total = MultiAgentChat::total_usage(&results);
    assert_eq!(total.total_tokens, 2);
    assert_eq!(total.cost, None);
}

#[test]
fn test_multiagent_event_completed() {
    let result = AgentResult {
        agent_id: 0,
        agent_name: "Agent-0".to_string(),
        role: AgentRole::Coder,
        content: "Done".to_string(),
        usage: None,
        duration: Duration::from_secs(10),
        success: true,
        error: None,
    };
    let event = MultiAgentEvent::AgentCompleted {
        agent_id: 0,
        result: result.clone(),
    };
    if let MultiAgentEvent::AgentCompleted {
        agent_id,
        result: r,
    } = event
    {
        assert_eq!(agent_id, 0);
        assert!(r.success);
    }
}

#[test]
fn test_multiagent_event_failed() {
    let event = MultiAgentEvent::AgentFailed {
        agent_id: 3,
        error: "Network error".to_string(),
    };
    if let MultiAgentEvent::AgentFailed { agent_id, error } = event {
        assert_eq!(agent_id, 3);
        assert!(error.contains("Network"));
    }
}

#[test]
fn test_multiagent_event_all_completed() {
    let results = vec![AgentResult {
        agent_id: 0,
        agent_name: "A".to_string(),
        role: AgentRole::Coder,
        content: "".to_string(),
        usage: None,
        duration: Duration::from_secs(1),
        success: true,
        error: None,
    }];
    let event = MultiAgentEvent::AllCompleted {
        results: results.clone(),
        total_duration: Duration::from_secs(5),
    };
    if let MultiAgentEvent::AllCompleted {
        results: r,
        total_duration,
    } = event
    {
        assert_eq!(r.len(), 1);
        assert_eq!(total_duration.as_secs(), 5);
    }
}

// ============================================================================
// MultiAgentChat Construction Tests
// ============================================================================

#[test]
fn test_multiagent_chat_new_succeeds() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config);
    assert!(chat.is_ok());
}

#[test]
fn test_multiagent_chat_new_with_custom_concurrency() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default().with_concurrency(8);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    assert_eq!(chat.config.max_concurrency, 8);
}

#[test]
fn test_multiagent_chat_with_events() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    let (tx, _rx) = mpsc::channel::<MultiAgentEvent>(100);
    let chat = chat.with_events(tx);
    assert!(chat.event_tx.is_some());
}

#[test]
fn test_semaphore_permits_match_clamped_concurrency() {
    let config = Config::default();

    // Concurrency 1
    let agent_config = MultiAgentConfig::default().with_concurrency(1);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    assert_eq!(chat.semaphore.available_permits(), 1);

    // Concurrency 8
    let agent_config = MultiAgentConfig::default().with_concurrency(8);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    assert_eq!(chat.semaphore.available_permits(), 8);

    // Concurrency 16 (max)
    let agent_config = MultiAgentConfig::default().with_concurrency(16);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    assert_eq!(chat.semaphore.available_permits(), 16);

    // Concurrency 100 (clamped to 16)
    let agent_config = MultiAgentConfig::default().with_concurrency(100);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();
    assert_eq!(chat.semaphore.available_permits(), 16);
}

// ============================================================================
// Async Tests
// ============================================================================

#[tokio::test]
async fn test_initialize_agents_creates_agents() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    let agents = chat.agents.read().await;
    assert_eq!(agents.len(), 4);
}

#[tokio::test]
async fn test_initialize_agents_sets_correct_roles() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    let agents = chat.agents.read().await;
    assert_eq!(agents[0].role, AgentRole::Architect);
    assert_eq!(agents[1].role, AgentRole::Coder);
    assert_eq!(agents[2].role, AgentRole::Tester);
    assert_eq!(agents[3].role, AgentRole::Reviewer);
}

#[tokio::test]
async fn test_initialize_agents_all_idle() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    let agents = chat.agents.read().await;
    for agent in agents.iter() {
        assert_eq!(agent.status, AgentStatus::Idle);
    }
}

#[tokio::test]
async fn test_initialize_agents_clears_previous() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    // Initialize twice -- second call should reset
    chat.initialize_agents().await.unwrap();
    chat.initialize_agents().await.unwrap();

    let agents = chat.agents.read().await;
    assert_eq!(agents.len(), 4);
}

#[tokio::test]
async fn test_is_agent_healthy_idle_agent() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    // Fresh idle agents should be healthy
    assert!(chat.is_agent_healthy(0).await);
    assert!(chat.is_agent_healthy(1).await);
    assert!(chat.is_agent_healthy(2).await);
    assert!(chat.is_agent_healthy(3).await);
}

#[tokio::test]
async fn test_is_agent_healthy_nonexistent_agent() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    // Agent ID out of range should not be healthy
    assert!(!chat.is_agent_healthy(999).await);
}

#[tokio::test]
async fn test_is_agent_healthy_failed_agent() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.initialize_agents().await.unwrap();

    // Mark agent as failed
    {
        let mut agents = chat.agents.write().await;
        agents[0].status = AgentStatus::Failed;
    }

    assert!(!chat.is_agent_healthy(0).await);
    // Other agents should still be healthy
    assert!(chat.is_agent_healthy(1).await);
}

#[tokio::test]
async fn test_results_initially_empty() {
    let config = Config::default();
    let agent_config = MultiAgentConfig::default();
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    let results = chat.results.lock().await;
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_initialize_agents_concurrent_safe() {
    use std::sync::Arc;

    let config = Config::default();
    let agent_config =
        MultiAgentConfig::default().with_roles(vec![AgentRole::Coder, AgentRole::Tester]);
    let chat = Arc::new(MultiAgentChat::new(&config, agent_config).unwrap());

    // Initialize from two concurrent tasks
    let chat1 = Arc::clone(&chat);
    let chat2 = Arc::clone(&chat);
    let (r1, r2) = tokio::join!(async move { chat1.initialize_agents().await }, async move {
        chat2.initialize_agents().await
    },);
    r1.unwrap();
    r2.unwrap();

    // After concurrent initialization, we should have exactly 2 agents
    let agents = chat.agents.read().await;
    assert_eq!(agents.len(), 2);
}

// ============================================================================
// run_task fan-out tests (HTTP-level MockLlmServer)
//
// These cover the actual execution path: fan-out, config inheritance,
// history persistence, budget guardrails, failure policies, and timeouts.
// ============================================================================

use crate::testing::mock_api::MockLlmServer;

/// Config pointing at the mock server, with retries disabled so error
/// responses surface immediately instead of backing off.
fn mock_fanout_config(endpoint: String) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        context_length: 500_000,
        max_tokens: 8192,
        temperature: 0.25,
        retry: crate::config::RetrySettings {
            max_retries: 0,
            ..Default::default()
        },
        agent: crate::config::AgentConfig {
            step_timeout_secs: 30,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn message_roles(body: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(body).unwrap();
    v["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_run_task_fanout_all_agents_complete() {
    let server = MockLlmServer::builder().build().await;
    let config = mock_fanout_config(format!("{}/v1", server.url()));
    let chat = MultiAgentChat::new(&config, MultiAgentConfig::default()).unwrap();

    let results = chat.run_task("say hi").await.unwrap();

    assert_eq!(results.len(), 4);
    for r in &results {
        assert!(r.success, "agent {} failed: {:?}", r.agent_id, r.error);
        assert_eq!(r.content, "Hello from MockLlmServer");
        // Usage is surfaced per agent (mock reports 10/5/15).
        let usage = r.usage.as_ref().expect("usage must be populated");
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);
        assert_eq!(usage.total_tokens, 15);
    }

    let total = MultiAgentChat::total_usage(&results);
    assert_eq!(total.prompt_tokens, 40);
    assert_eq!(total.completion_tokens, 20);
    assert_eq!(total.total_tokens, 60);
    assert_eq!(total.cost, None); // mock reports no cost

    // Exactly one paid completion per agent, and no tools are ever offered.
    let bodies = server.captured_request_bodies().await;
    assert_eq!(bodies.len(), 4);
    for body in &bodies {
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        assert!(
            v.get("tools").is_none() || v["tools"].is_null(),
            "multi-chat must not send tool definitions: {body}"
        );
        assert_ne!(v["stream"].as_bool(), Some(true));
    }

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_run_task_persists_user_and_assistant_turns() {
    let server = MockLlmServer::builder().build().await;
    let config = mock_fanout_config(format!("{}/v1", server.url()));
    let chat = MultiAgentChat::new(&config, MultiAgentConfig::default()).unwrap();

    chat.run_task("first question").await.unwrap();
    chat.run_task("second question").await.unwrap();

    let bodies = server.captured_request_bodies().await;
    assert_eq!(bodies.len(), 8, "two turns x four agents");

    // Turn 1: [system, user]
    for body in &bodies[..4] {
        assert_eq!(message_roles(body), ["system", "user"]);
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(v["messages"][1]["content"], "first question");
    }

    // Turn 2: [system, user, assistant, user] — the user turn must persist
    // alongside the assistant reply, with correct role alternation.
    for body in &bodies[4..] {
        assert_eq!(
            message_roles(body),
            ["system", "user", "assistant", "user"],
            "turn 2 must include the full prior conversation: {body}"
        );
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(v["messages"][1]["content"], "first question");
        assert_eq!(v["messages"][2]["content"], "Hello from MockLlmServer");
        assert_eq!(v["messages"][3]["content"], "second question");
    }

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_run_task_inherits_user_sampling_config() {
    let server = MockLlmServer::builder().build().await;
    // Distinctive values; the old hardcoded defaults were 1.0 / 65536.
    let mut config = mock_fanout_config(format!("{}/v1", server.url()));
    config.temperature = 0.25;
    config.max_tokens = 4321;
    let chat = MultiAgentChat::new(&config, MultiAgentConfig::default()).unwrap();

    chat.run_task("hi").await.unwrap();

    let bodies = server.captured_request_bodies().await;
    assert_eq!(bodies.len(), 4);
    for body in &bodies {
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(v["max_tokens"].as_u64(), Some(4321));
        assert!(
            (v["temperature"].as_f64().unwrap() - 0.25).abs() < 1e-6,
            "user temperature must hit the wire: {body}"
        );
    }

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_run_task_explicit_sampling_overrides_win() {
    let server = MockLlmServer::builder().build().await;
    let config = mock_fanout_config(format!("{}/v1", server.url()));
    let agent_config = MultiAgentConfig {
        temperature: Some(0.5),
        max_tokens: Some(111),
        ..Default::default()
    };
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    chat.run_task("hi").await.unwrap();

    let bodies = server.captured_request_bodies().await;
    assert_eq!(bodies.len(), 4);
    for body in &bodies {
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(v["max_tokens"].as_u64(), Some(111));
        assert!((v["temperature"].as_f64().unwrap() - 0.5).abs() < 1e-6);
    }

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_run_task_budget_tokens_blocks_paid_calls() {
    let server = MockLlmServer::builder().build().await;
    let mut config = mock_fanout_config(format!("{}/v1", server.url()));
    // One token of budget cannot fit even one call at max_tokens=8192:
    // every agent must be skipped before any paid request is made.
    config.agent.max_budget_tokens = Some(1);
    let chat = MultiAgentChat::new(&config, MultiAgentConfig::default()).unwrap();

    let results = chat.run_task("hi").await.unwrap();

    assert_eq!(results.len(), 4);
    for r in &results {
        assert!(!r.success);
        let err = r.error.as_deref().unwrap_or("");
        assert!(
            err.contains("--max-budget-tokens"),
            "skip reason must name the limit: {err}"
        );
        assert!(r.usage.is_none());
    }
    assert!(
        server.captured_request_bodies().await.is_empty(),
        "no paid call may leave the process"
    );

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_run_task_zero_budget_errors_before_any_call() {
    let server = MockLlmServer::builder().build().await;
    let mut config = mock_fanout_config(format!("{}/v1", server.url()));
    config.agent.max_budget_tokens = Some(0);
    let chat = MultiAgentChat::new(&config, MultiAgentConfig::default()).unwrap();

    let err = chat.run_task("hi").await.unwrap_err();
    assert!(
        err.to_string().contains("--max-budget-tokens"),
        "error must name the exhausted limit: {err}"
    );
    assert!(server.captured_request_bodies().await.is_empty());

    // Same for a non-positive cost cap.
    let mut config = mock_fanout_config(format!("{}/v1", server.url()));
    config.agent.max_budget_tokens = None;
    config.agent.max_cost_usd = Some(0.0);
    let chat = MultiAgentChat::new(&config, MultiAgentConfig::default()).unwrap();

    let err = chat.run_task("hi").await.unwrap_err();
    assert!(err.to_string().contains("--max-cost-usd"));
    assert!(server.captured_request_bodies().await.is_empty());

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_run_task_failfast_cancels_remaining_agents() {
    // Latency guarantees the first agent's error lands while later agents
    // are still waiting, so FailFast cancellation is observable.
    let server = MockLlmServer::builder()
        .with_error(500, r#"{"error":"boom"}"#)
        .with_latency(200)
        .build()
        .await;
    let config = mock_fanout_config(format!("{}/v1", server.url()));
    let agent_config = MultiAgentConfig {
        failure_policy: MultiAgentFailurePolicy::FailFast,
        ..Default::default()
    }
    .with_concurrency(1);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    let results = chat.run_task("hi").await.unwrap();

    assert_eq!(
        results.len(),
        1,
        "FailFast must stop after the first failure"
    );
    assert!(!results[0].success);
    let bodies = server.captured_request_bodies().await;
    assert!(
        bodies.len() < 4,
        "remaining agents must be cancelled, got {} requests",
        bodies.len()
    );

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_run_task_best_effort_keeps_successes_when_one_agent_fails() {
    let server = MockLlmServer::builder()
        .with_error(500, r#"{"error":"boom"}"#)
        .build()
        .await;
    let config = mock_fanout_config(format!("{}/v1", server.url()));
    let agent_config = MultiAgentConfig {
        failure_policy: MultiAgentFailurePolicy::BestEffort,
        ..Default::default()
    }
    .with_concurrency(1);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    let results = chat.run_task("hi").await.unwrap();

    assert_eq!(results.len(), 4, "all agents must produce a result");
    let failures = results.iter().filter(|r| !r.success).count();
    let successes = results.iter().filter(|r| r.success).count();
    assert_eq!(failures, 1);
    assert_eq!(successes, 3);
    for r in results.iter().filter(|r| r.success) {
        assert_eq!(r.content, "Hello from MockLlmServer");
    }
    assert_eq!(server.captured_request_bodies().await.len(), 4);

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_run_task_timeout_records_failure() {
    let server = MockLlmServer::builder()
        .with_response("too slow")
        .with_latency(3000)
        .build()
        .await;
    let config = mock_fanout_config(format!("{}/v1", server.url()));
    let agent_config = MultiAgentConfig {
        timeout_secs: Some(1),
        ..Default::default()
    }
    .with_roles(vec![AgentRole::Coder]);
    let chat = MultiAgentChat::new(&config, agent_config).unwrap();

    let results = chat.run_task("hi").await.unwrap();

    assert_eq!(results.len(), 1);
    assert!(!results[0].success);
    assert!(
        results[0]
            .error
            .as_deref()
            .unwrap_or("")
            .contains("timed out"),
        "timeout must be reported as such: {:?}",
        results[0].error
    );
    assert!(results[0].usage.is_none());

    server.stop().await;
}
