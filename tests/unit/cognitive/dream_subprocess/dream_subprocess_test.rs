use super::*;
use tempfile::tempdir;

#[tokio::test]
async fn test_autodream_config_default() {
    let config = AutoDreamConfig::default();
    assert_eq!(config.model, "qwen3.5-9b");
    assert_eq!(config.timeout_secs, 300);
    assert!(config.prefer_local);
}

#[tokio::test]
async fn test_orient_phase() {
    let dir = tempdir().unwrap();

    // Create some session files
    tokio::fs::create_dir(dir.path().join(".selfware"))
        .await
        .unwrap();
    tokio::fs::write(dir.path().join(".selfware").join("session_1.json"), "{}")
        .await
        .unwrap();
    tokio::fs::write(dir.path().join(".selfware").join("memory_log.jsonl"), "")
        .await
        .unwrap();

    let files = orient_phase(dir.path()).await.unwrap();
    assert_eq!(files.len(), 2);
}

#[tokio::test]
async fn test_gather_phase_empty() {
    let files: Vec<PathBuf> = Vec::new();
    let memories = gather_phase(&files, 5).await.unwrap();
    assert!(memories.is_empty());
}

#[tokio::test]
async fn test_prune_and_index_creates_memory_file() {
    let dir = tempdir().unwrap();
    let dream_config = DreamConfig::new().with_base_dir(dir.path());

    let count = prune_and_index_phase("test_project", &dream_config, None)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // MEMORY.md should be created
    let memory_path = dream_config.memory_file_path("test_project");
    assert!(memory_path.exists());
}

#[test]
fn test_dream_result_builder() {
    let result = DreamResult::success(vec![DreamPhase::Orient])
        .with_phase(DreamPhase::Gather)
        .with_consolidated(10)
        .with_pruned(5);

    assert!(result.success);
    assert_eq!(result.phases_completed.len(), 2);
    assert_eq!(result.memories_consolidated, 10);
    assert_eq!(result.memories_pruned, 5);
}

// ── /dream force backend fixes (whole-repo review, P2) ──

#[test]
fn test_from_user_config_picks_up_user_backend() {
    let _env = crate::test_support::EnvGuard::capture(&["SELFWARE_CONFIG"]);
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("selfware.toml");
    std::fs::write(
        &config_path,
        "endpoint = \"http://127.0.0.1:9999/v1\"\nmodel = \"dream-test-model\"\n",
    )
    .unwrap();
    _env.set("SELFWARE_CONFIG", &config_path);

    let config = AutoDreamConfig::from_user_config();
    assert_eq!(config.endpoint, "http://127.0.0.1:9999/v1");
    assert_eq!(config.model, "dream-test-model");
}

#[test]
fn test_count_consolidated_entries() {
    let content = "# Project Memory\n\n## Facts (consolidated)\n- fact A\n- fact B\n\n## Preferences (user-defined)\n- pref C\n";
    assert_eq!(count_consolidated_entries(content), 3);
    assert_eq!(count_consolidated_entries(""), 0);
    assert_eq!(count_consolidated_entries("no sections\n- orphan\n"), 0);
}

/// A dream fixture: project with a parseable session log + a mock LLM
/// serving the consolidation response.
async fn dream_fixture(
    response: &str,
) -> (
    tempfile::TempDir,
    tempfile::TempDir,
    AutoDreamConfig,
    DreamConfig,
    crate::testing::mock_api::MockLlmServer,
) {
    let project = tempdir().unwrap();
    let memory_dir = tempdir().unwrap();
    tokio::fs::create_dir(project.path().join(".selfware"))
        .await
        .unwrap();
    tokio::fs::write(
        project.path().join(".selfware").join("session_1.json"),
        "- [2026-07-01] Project uses Rust\n- [2026-07-02] Tests are thorough\n",
    )
    .await
    .unwrap();

    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response(response)
        .build()
        .await;
    let auto_config = AutoDreamConfig::default()
        .with_endpoint(format!("{}/v1", server.url()))
        .with_model("mock-dream-model");
    let dream_config = DreamConfig::new().with_base_dir(memory_dir.path());
    (project, memory_dir, auto_config, dream_config, server)
}

#[tokio::test]
async fn test_run_dream_consolidation_uses_configured_backend_and_keeps_content() {
    let (project, memory_dir, auto_config, dream_config, server) =
        dream_fixture("## Facts (consolidated)\n- Project uses Rust\n- Tests are thorough\n").await;

    let result =
        run_dream_consolidation(project.path(), "test_project", &auto_config, &dream_config)
            .await
            .unwrap();

    assert!(
        result.success,
        "dream should succeed against the mock backend: {:?}",
        result.errors
    );
    assert_eq!(result.memories_consolidated, 2);
    // The mock only answers /v1/chat/completions — a hit proves the
    // correct URL was used (the old code POSTed to the bare endpoint).
    assert_eq!(server.captured_request_bodies().await.len(), 1);

    // The consolidation content is NOT discarded: it lands in MEMORY.md.
    let memory_path = dream_config.memory_file_path("test_project");
    let memory = std::fs::read_to_string(&memory_path).unwrap();
    assert!(
        memory.contains("Project uses Rust"),
        "consolidated content persisted: {}",
        memory
    );
    let _ = memory_dir;
}

#[tokio::test]
async fn test_run_dream_consolidation_backend_error_reports_failure() {
    let project = tempdir().unwrap();
    let memory_dir = tempdir().unwrap();
    tokio::fs::create_dir(project.path().join(".selfware"))
        .await
        .unwrap();
    tokio::fs::write(
        project.path().join(".selfware").join("session_1.json"),
        "- [2026-07-01] Project uses Rust\n",
    )
    .await
    .unwrap();

    // 400 is terminal (no retry backoff) — the point is that ANY backend
    // error must surface as an honest failure, not a discarded-call
    // "success".
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_error(400, "bad request")
        .build()
        .await;
    let auto_config = AutoDreamConfig::default()
        .with_endpoint(format!("{}/v1", server.url()))
        .with_model("mock-dream-model");
    let dream_config = DreamConfig::new().with_base_dir(memory_dir.path());

    let result =
        run_dream_consolidation(project.path(), "test_project", &auto_config, &dream_config)
            .await
            .unwrap();

    assert!(
        !result.success,
        "backend failure must be reported as failure, not success"
    );
    assert!(
        result.errors.iter().any(|e| e.contains("Consolidate")),
        "consolidation error recorded: {:?}",
        result.errors
    );
}

#[tokio::test]
async fn test_spawn_autodream_runs_in_process_and_returns_real_result() {
    let (project, memory_dir, auto_config, dream_config, server) =
        dream_fixture("## Facts (consolidated)\n- Project uses Rust\n").await;

    let mut handle = spawn_autodream(project.path(), "test_project", &auto_config, &dream_config)
        .await
        .unwrap();
    let result = handle
        .wait_with_timeout(Duration::from_secs(30))
        .await
        .unwrap();

    // The handle returns the REAL DreamResult — not a fabricated
    // all-phases success with a hardcoded 0 consolidation count.
    assert!(result.success, "spawn result: {:?}", result.errors);
    assert_eq!(result.memories_consolidated, 1);
    assert!(!handle.is_running().await);
    let _ = (memory_dir, server);
}
