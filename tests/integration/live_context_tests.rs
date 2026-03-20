//! Integration tests for the hierarchical context system.
//!
//! Non-ignored tests run without a live endpoint (pure logic tests on real files).
//! Ignored tests require a running sglang endpoint:
//!
//!   SELFWARE_LIVE_ENDPOINT=http://localhost:8000/v1 cargo test --test integration live_context -- --ignored

use std::path::Path;

use selfware::agent::context_map::{
    extract_rust_skeleton, ContextLevel, ContextMap, ContextModality,
};
use selfware::token_count::estimate_content_tokens;

// ─── Skeleton Extraction on Real Codebase ───────────────────────────────────

#[test]
fn test_skeleton_extraction_on_real_codebase() {
    let test_files = [
        "src/agent/context_map.rs",
        "src/agent/recovery.rs",
        "src/agent/execution.rs",
        "src/tools/mod.rs",
    ];

    for file in &test_files {
        let path = Path::new(file);
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(path).unwrap();
        let skeleton = extract_rust_skeleton(path, &content);

        assert!(
            !skeleton.items.is_empty(),
            "{} should have skeleton items",
            file
        );
        assert!(
            skeleton.token_count > 0,
            "{} should have non-zero token count",
            file
        );

        let rendered = skeleton.render();
        assert!(
            rendered.contains("fn "),
            "{} skeleton should contain function signatures",
            file
        );

        // Skeleton must be significantly smaller than full content.
        let full_tokens = estimate_content_tokens(&content);
        assert!(
            skeleton.token_count < full_tokens / 2,
            "{}: skeleton ({}) should be <50% of full ({}) — got {:.0}%",
            file,
            skeleton.token_count,
            full_tokens,
            (skeleton.token_count as f64 / full_tokens as f64) * 100.0,
        );
    }
}

// ─── Context Map Budget with Real Files ─────────────────────────────────────

#[test]
fn test_context_map_budget_with_real_files() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);

    let test_files = ["src/lib.rs", "src/main.rs", "src/token_count.rs"];
    for file in &test_files {
        let path = Path::new(file);
        if path.exists() {
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            map.register_tree_entry(path.to_path_buf(), size);
        }
    }

    assert!(map.file_count() > 0);
    assert!(map.remaining() > 0);

    // Load a real file at L3.
    let lib_path = Path::new("src/lib.rs");
    if lib_path.exists() {
        let content = std::fs::read_to_string(lib_path).unwrap();
        let before = map.total_tokens();
        map.load_full(lib_path, content);
        assert!(map.total_tokens() > before);
        assert_eq!(map.level_of(lib_path), Some(ContextLevel::Full));
    }
}

// ─── Skeleton Loading + Downgrade Cycle ─────────────────────────────────────

#[test]
fn test_skeleton_load_downgrade_cycle() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);

    let path = Path::new("src/token_count.rs");
    if !path.exists() {
        return;
    }

    let content = std::fs::read_to_string(path).unwrap();

    // Register at L1.
    map.register_tree_entry(path.to_path_buf(), content.len() as u64);
    assert_eq!(map.level_of(path), Some(ContextLevel::Tree));

    // Load skeleton (L2).
    let skeleton = extract_rust_skeleton(path, &content);
    map.load_skeleton(path, skeleton);
    assert_eq!(map.level_of(path), Some(ContextLevel::Skeleton));
    let l2_tokens = map.total_tokens();

    // Load full (L3).
    map.load_full(path, content);
    assert_eq!(map.level_of(path), Some(ContextLevel::Full));
    let l3_tokens = map.total_tokens();
    assert!(l3_tokens > l2_tokens, "L3 should use more tokens than L2");

    // Downgrade back to L2.
    let freed = map.downgrade_to_skeleton(path);
    assert!(freed > 0, "should free tokens on downgrade");
    assert_eq!(map.level_of(path), Some(ContextLevel::Skeleton));
    assert!(map.total_tokens() < l3_tokens);
}

// ─── Modality Detection ─────────────────────────────────────────────────────

#[test]
fn test_modality_from_real_tasks() {
    let test_cases = [
        ("review your own code, read all rust files", "Review"),
        ("fix the bug in context_management.rs", "Debug"),
        ("merge recovery.rs changes into execution.rs", "Merge"),
        ("refactor the agent module into smaller files", "Refactor"),
        ("create a brand new context_map module", "Greenfield"),
        ("add a new field to AgentConfig", "Implement"),
    ];

    for (task, expected) in &test_cases {
        let modality = ContextModality::from_task(task);
        let modality_name = format!("{:?}", modality);
        assert!(
            modality_name.starts_with(expected),
            "Task '{}' should be {} but got {}",
            task,
            expected,
            modality_name
        );
    }
}

// ─── Recommend Context ──────────────────────────────────────────────────────

#[test]
fn test_recommend_context_for_real_codebase() {
    let mut map = ContextMap::new(100_000, 0.75, 0.20, 0.05);

    // Register real .rs files.
    for entry in walkdir::WalkDir::new("src")
        .max_depth(5)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "rs"))
    {
        let path = entry.path().to_path_buf();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        map.register_tree_entry(path, size);
    }

    let rec = map.recommend_context("fix the no-action detection in recovery.rs");
    assert!(
        !rec.promote.is_empty() || !rec.evict.is_empty(),
        "should have some recommendations"
    );
    assert!(!rec.modality_description.is_empty());

    // recovery.rs should be in the promote list.
    let promotes_recovery = rec
        .promote
        .iter()
        .any(|s| s.path.to_string_lossy().contains("recovery"));
    assert!(
        promotes_recovery,
        "should recommend promoting recovery.rs for a recovery-related task"
    );
}

// ─── Auto-Optimize ──────────────────────────────────────────────────────────

#[test]
fn test_auto_optimize_evicts_stale() {
    let mut map = ContextMap::new(10_000, 0.75, 0.20, 0.05);

    let path = Path::new("src/lib.rs");
    if !path.exists() {
        return;
    }

    let content = std::fs::read_to_string(path).unwrap();
    let skeleton = extract_rust_skeleton(path, &content);
    map.register_tree_entry(path.to_path_buf(), content.len() as u64);
    map.load_skeleton(path, skeleton);
    map.load_full(path, content);
    assert_eq!(map.level_of(path), Some(ContextLevel::Full));

    // With 0 staleness, everything is stale immediately.
    let freed = map.auto_optimize(0);
    assert!(freed > 0, "should free stale L3 content");
    assert_ne!(map.level_of(path), Some(ContextLevel::Full));
}

// ─── Live Endpoint Tests ────────────────────────────────────────────────────

fn live_config() -> Option<selfware::config::Config> {
    let endpoint = std::env::var("SELFWARE_LIVE_ENDPOINT").ok()?;
    let model = std::env::var("SELFWARE_LIVE_MODEL").unwrap_or_else(|_| "qwen3.5-27b".to_string());

    Some(selfware::config::Config {
        endpoint,
        model,
        max_tokens: 900_000,
        agent: selfware::config::AgentConfig {
            max_iterations: 20,
            step_timeout_secs: 120,
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
    })
}

#[tokio::test]
#[ignore = "requires SELFWARE_LIVE_ENDPOINT"]
async fn test_live_review_with_skeletons() {
    let config = match live_config() {
        Some(c) => c,
        None => return,
    };

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let result = agent
        .run_task("review the code in src/token_count.rs and summarize what it does")
        .await;

    match result {
        Ok(()) => println!("PASS: review task completed"),
        Err(e) => println!("FAIL: {}", e),
    }
}

#[tokio::test]
#[ignore = "requires SELFWARE_LIVE_ENDPOINT"]
async fn test_live_context_tools() {
    let config = match live_config() {
        Some(c) => c,
        None => return,
    };

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let result = agent
        .run_task("use context_status to check your context window, then report the budget usage")
        .await;

    match result {
        Ok(()) => println!("PASS: context tools task completed"),
        Err(e) => println!("FAIL: {}", e),
    }
}

#[tokio::test]
#[ignore = "requires SELFWARE_LIVE_ENDPOINT"]
async fn test_live_skeleton_review() {
    let config = match live_config() {
        Some(c) => c,
        None => return,
    };

    let mut agent = selfware::agent::Agent::new(config).await.unwrap();
    let result = agent
        .run_task("read all rust files and give a brief summary of the project structure")
        .await;

    match result {
        Ok(()) => println!("PASS: skeleton-based review completed"),
        Err(e) => println!("FAIL: {}", e),
    }
}
