//! Turn-artifact decisions name what actually happened to a turn's tool
//! calls (2026-09-24 live validation: turns whose calls were rejected before
//! execution were labelled `executed_tools`, and the accepted final report
//! turn was labelled `no_tool_call`), and artifact files are never
//! overwritten by a later process.

use super::*;
use crate::testing::mock_api::MockLlmServer;

/// Mock-endpoint config with turn artifacts ENABLED (they are off by default).
fn artifact_config(endpoint: &str) -> crate::config::Config {
    let mut config = crate::test_support::mock_agent_config(endpoint);
    config.agent.disable_turn_artifacts = false;
    config
}

/// All `turn_NNNN.json` artifacts under `dir`, sorted by file name.
fn artifacts(dir: &std::path::Path) -> Vec<(String, serde_json::Value)> {
    let turns = dir.join(".selfware").join("turns");
    let mut out: Vec<(String, serde_json::Value)> = std::fs::read_dir(&turns)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if !(name.starts_with("turn_") && name.ends_with(".json")) {
                        return None;
                    }
                    let raw = std::fs::read_to_string(e.path()).ok()?;
                    Some((name, serde_json::from_str(&raw).ok()?))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The single artifact a one-step test wrote.
fn only_decision(dir: &std::path::Path) -> serde_json::Value {
    let all = artifacts(dir);
    assert_eq!(all.len(), 1, "expected exactly one artifact, got {:?}", all);
    all[0].1["agent_decision"].clone()
}

async fn run_one_step(response: &str, dir: &std::path::Path) -> (Agent, Result<bool>) {
    run_one_step_with(response, dir, |_| {}).await
}

async fn run_one_step_with(
    response: &str,
    _dir: &std::path::Path,
    tweak: impl FnOnce(&mut crate::config::Config),
) -> (Agent, Result<bool>) {
    let server = MockLlmServer::builder()
        .with_response(response)
        .build()
        .await;
    let mut config = artifact_config(&format!("{}/v1", server.url()));
    tweak(&mut config);
    let mut agent = Agent::new(config).await.unwrap();
    let result = agent.execute_step_internal(false).await;
    server.stop().await;
    (agent, result)
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn unknown_tool_is_recorded_as_rejected_not_executed() {
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    cwd.switch_to(dir.path());

    let (_agent, result) = run_one_step(
        "<tool>\n<name>no_such_tool</name>\n<arguments>{}</arguments>\n</tool>",
        dir.path(),
    )
    .await;
    assert!(result.is_ok(), "step must not error: {:?}", result.err());

    let decision = only_decision(dir.path());
    assert_eq!(decision["kind"], "rejected_tools", "{decision}");
    let rejected = decision["rejected_tools"].as_array().unwrap();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0]["name"], "no_such_tool");
    assert!(
        rejected[0]["reason"]
            .as_str()
            .unwrap()
            .contains("does not exist"),
        "the rejection names why: {decision}"
    );
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn executed_tool_is_listed_with_its_outcome_and_rejections_beside_it() {
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    cwd.switch_to(dir.path());
    std::fs::write(dir.path().join("notes.txt"), "hello\n").unwrap();

    let (_agent, result) = run_one_step(
        "<tool>\n<name>file_read</name>\n<arguments>{\"path\":\"notes.txt\"}</arguments>\n</tool>\n\
         <tool>\n<name>no_such_tool</name>\n<arguments>{}</arguments>\n</tool>",
        dir.path(),
    )
    .await;
    assert!(result.is_ok(), "step must not error: {:?}", result.err());

    let decision = only_decision(dir.path());
    assert_eq!(decision["kind"], "executed_tools", "{decision}");
    let executed = decision["tools"].as_array().unwrap();
    assert_eq!(
        executed.len(),
        1,
        "only the call that ran is executed: {decision}"
    );
    assert_eq!(executed[0]["name"], "file_read");
    assert_eq!(executed[0]["ok"], true);
    let rejected = decision["rejected_tools"].as_array().unwrap();
    assert_eq!(rejected.len(), 1, "{decision}");
    assert_eq!(rejected[0]["name"], "no_such_tool");
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn files_gate_discard_is_recorded_as_rejected() {
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    cwd.switch_to(dir.path());
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();

    let (_agent, result) = run_one_step(
        "<tool>\n<name>file_write</name>\n<arguments>{\"path\":\"src/main.rs\",\"content\":\"fn main() { }\"}</arguments>\n</tool>",
        dir.path(),
    )
    .await;
    assert_eq!(result.ok(), Some(false), "discarded write continues");

    let decision = only_decision(dir.path());
    assert_eq!(decision["kind"], "rejected_tools", "{decision}");
    let rejected = decision["rejected_tools"].as_array().unwrap();
    assert_eq!(rejected[0]["name"], "file_write");
    assert!(rejected[0]["reason"]
        .as_str()
        .unwrap()
        .contains("FILES: checklist"));
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn budget_stop_before_dispatch_is_not_recorded_as_execution() {
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    cwd.switch_to(dir.path());
    std::fs::write(dir.path().join("notes.txt"), "hello\n").unwrap();

    let (agent, result) = run_one_step_with(
        "<tool>\n<name>file_read</name>\n<arguments>{\"path\":\"notes.txt\"}</arguments>\n</tool>",
        dir.path(),
        |config| config.agent.max_budget_tokens = Some(1),
    )
    .await;
    let err = result.expect_err("an exhausted budget stops the step");
    assert!(err.to_string().contains("budget"), "{err}");

    let decision = only_decision(dir.path());
    assert_eq!(decision["kind"], "stopped_before_dispatch", "{decision}");
    assert!(
        decision["reason"].as_str().unwrap().contains("budget"),
        "{decision}"
    );
    assert_eq!(decision["tools"], serde_json::json!(["file_read"]));
    assert_eq!(agent.total_tool_call_count, 0, "nothing was dispatched");
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn accepted_final_answer_is_recorded_as_final_answer() {
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    cwd.switch_to(dir.path());

    let answer = "The module parses configuration files and validates every field before use.";
    let (_agent, result) = run_one_step(answer, dir.path()).await;
    assert_eq!(
        result.ok(),
        Some(true),
        "a substantial read-only answer completes"
    );

    let decision = only_decision(dir.path());
    assert_eq!(decision["kind"], "final_answer", "{decision}");
    assert_eq!(decision["text"], answer);
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn existing_turn_file_is_never_overwritten() {
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    cwd.switch_to(dir.path());
    // An earlier process already wrote turn 1 (and turn 2).
    let turns = dir.path().join(".selfware").join("turns");
    std::fs::create_dir_all(&turns).unwrap();
    std::fs::write(turns.join("turn_0001.json"), "{\"earlier\":1}").unwrap();
    std::fs::write(turns.join("turn_0002.json"), "{\"earlier\":2}").unwrap();

    let (agent, result) = run_one_step(
        "<tool>\n<name>no_such_tool</name>\n<arguments>{}</arguments>\n</tool>",
        dir.path(),
    )
    .await;
    assert!(result.is_ok(), "{:?}", result.err());

    assert_eq!(
        std::fs::read_to_string(turns.join("turn_0001.json")).unwrap(),
        "{\"earlier\":1}"
    );
    assert_eq!(
        std::fs::read_to_string(turns.join("turn_0002.json")).unwrap(),
        "{\"earlier\":2}"
    );
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(turns.join("turn_0003.json")).unwrap())
            .unwrap();
    assert_eq!(written["step"], 3, "the artifact records the slot it took");
    // The refinement (initial pending_dispatch -> rejected_tools) rewrote the
    // SAME slot instead of claiming another one.
    assert_eq!(written["agent_decision"]["kind"], "rejected_tools");
    assert!(!turns.join("turn_0004.json").exists());
    assert_eq!(agent.turn_artifact_seq, 3, "numbering continues after it");
}

/// D6: b3_review turn_0010 (verbatim qwen38-flash-next content). The first
/// call parses; the second (`<function=tool>` + `<name>` + `<arguments>`
/// closed by `</tool>`) matches no parser. 0.8.2 silently dropped it: not
/// executed, not in rejected_tools, no tool result. It must now be refused
/// through the dispatcher funnel so the model is told.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn d6_unparseable_call_in_mixed_batch_is_rejected_and_reported() {
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    cwd.switch_to(dir.path());
    std::fs::write(dir.path().join("notes.txt"), "hello\n").unwrap();

    let content = "\n\n<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"notes.txt\"}</arguments>\n</tool>\n</tool_call>\n<tool_call>\n<function=tool>\n<name>file_read</name>\n<arguments>{\"path\": \"src/agent/tool_validator.rs\"}</arguments>\n</tool>";
    let (agent, result) = run_one_step(content, dir.path()).await;
    assert!(result.is_ok(), "step must not error: {:?}", result.err());

    let decision = only_decision(dir.path());
    assert_eq!(decision["kind"], "executed_tools", "{decision}");
    assert_eq!(decision["tools"].as_array().unwrap().len(), 1, "{decision}");
    let rejected = decision["rejected_tools"].as_array().unwrap();
    assert_eq!(rejected.len(), 1, "{decision}");
    assert_eq!(rejected[0]["name"], "file_read");
    assert!(
        rejected[0]["reason"]
            .as_str()
            .unwrap()
            .contains("NOT executed"),
        "{decision}"
    );
    // The model sees a tool result for the rejected call.
    let told = agent.messages.iter().any(|m| {
        let text = m.content.text_all();
        text.contains("NOT executed") && text.contains("src/agent/tool_validator.rs")
    });
    assert!(told, "the rejection must reach the model as a tool result");
}

/// D6: b3_resume turn_0002 (verbatim): the response's only call is
/// malformed. The turn is a rejected tool call, not a final answer.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable on Windows CI"
)]
async fn d6_all_calls_unparseable_is_rejected_not_final_answer() {
    let cwd = crate::test_support::CwdGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    cwd.switch_to(dir.path());

    let content = "\n\nI'll start with stages 4 and 5. Let me read the checkpoint, replay, and recovery implementation bodies.\n\n<tool_call>\n<function=tool>\n<parameter=name>\nfile_read</name>\n<parameter=arguments>{\"path\": \"src/evolve/replay.rs\", \"line_range\": [1, 200]}\n</parameter>\n</tool>\n</tool_call>";
    let (agent, result) = run_one_step(content, dir.path()).await;
    assert!(
        matches!(result, Ok(false)),
        "the turn must continue, not complete: {:?}",
        result
    );
    let decision = only_decision(dir.path());
    assert_eq!(decision["kind"], "rejected_tools", "{decision}");
    assert_eq!(
        decision["rejected_tools"].as_array().unwrap().len(),
        1,
        "{decision}"
    );
    assert!(agent
        .messages
        .iter()
        .any(|m| m.content.text_all().contains("NOT executed")));
}
