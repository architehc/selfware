use super::*;
use std::fs;
use std::process::Command as StdCommand;

#[test]
fn test_rsi_orchestrator_new_defaults() {
    let _env = crate::test_support::EnvGuard::capture(&[MAX_ITERATIONS_PER_RUN_ENV]);
    std::env::remove_var(MAX_ITERATIONS_PER_RUN_ENV);
    let orch = RSIOrchestrator::new(PathBuf::from("/tmp/test_project"));
    assert_eq!(orch.project_root, PathBuf::from("/tmp/test_project"));
    assert!(!orch.is_running);
    assert_eq!(orch.max_iterations, 100);
    assert_eq!(orch.consecutive_failures, 0);
    assert_eq!(orch.max_consecutive_failures, 5);
    // Per-run cost ceiling defaults to 10 iterations (20 paid e2e suites).
    assert_eq!(orch.max_iterations_per_run, 10);
}

#[test]
fn test_parse_max_iterations_per_run() {
    assert_eq!(parse_max_iterations_per_run(None), 10);
    assert_eq!(parse_max_iterations_per_run(Some("5")), 5);
    assert_eq!(parse_max_iterations_per_run(Some(" 7 ")), 7);
    // Zero / garbage / negative all fall back to the default — a ceiling
    // of 0 would silently disable the loop.
    assert_eq!(parse_max_iterations_per_run(Some("0")), 10);
    assert_eq!(parse_max_iterations_per_run(Some("abc")), 10);
    assert_eq!(parse_max_iterations_per_run(Some("-3")), 10);
    assert_eq!(parse_max_iterations_per_run(Some("")), 10);
}

#[test]
fn test_with_max_iterations_per_run_builder() {
    let orch =
        RSIOrchestrator::new(PathBuf::from("/tmp/test_project")).with_max_iterations_per_run(3);
    assert_eq!(orch.max_iterations_per_run, 3);
    // Clamped to at least 1 so the loop always does SOMETHING per run.
    let orch =
        RSIOrchestrator::new(PathBuf::from("/tmp/test_project")).with_max_iterations_per_run(0);
    assert_eq!(orch.max_iterations_per_run, 1);
}

// ── Trivial-mutation gate (whole-repo review, RSI P1) ──

fn write_pair(root: &Path, sandbox: &Path, rel: &str, old: &str, new: &str) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(sandbox.join("src")).unwrap();
    std::fs::write(root.join(rel), old).unwrap();
    std::fs::write(sandbox.join(rel), new).unwrap();
}

#[test]
fn test_mutation_is_trivial_comment_only_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let sandbox = dir.path().join("sandbox");
    write_pair(
        &root,
        &sandbox,
        "src/lib.rs",
        "pub fn f() -> usize {\n    // TODO: remove this marker\n    42\n}\n",
        "pub fn f() -> usize {\n    // Resolved: remove this marker\n    42\n}\n",
    );
    let edited = vec!["src/lib.rs".to_string()];
    assert!(mutation_is_trivial(&root, &sandbox, &edited));
}

#[test]
fn test_mutation_is_trivial_real_code_change_not_trivial() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let sandbox = dir.path().join("sandbox");
    write_pair(
        &root,
        &sandbox,
        "src/lib.rs",
        "pub fn f() -> usize {\n    // compute\n    42\n}\n",
        "pub fn f() -> usize {\n    // compute\n    43\n}\n",
    );
    let edited = vec!["src/lib.rs".to_string()];
    assert!(!mutation_is_trivial(&root, &sandbox, &edited));
}

#[test]
fn test_mutation_is_trivial_rust_attribute_change_not_trivial() {
    // `#[...]` lines are attributes (code), NOT comments: a mutation that
    // only changes an attribute must still be evaluated.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let sandbox = dir.path().join("sandbox");
    write_pair(
        &root,
        &sandbox,
        "src/lib.rs",
        "#[derive(Debug)]\npub struct S;\n",
        "#[derive(Debug, Clone)]\npub struct S;\n",
    );
    let edited = vec!["src/lib.rs".to_string()];
    assert!(!mutation_is_trivial(&root, &sandbox, &edited));
}

#[test]
fn test_mutation_is_trivial_hash_comment_in_toml() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let sandbox = dir.path().join("sandbox");
    write_pair(
        &root,
        &sandbox,
        "src/config.toml",
        "# old comment\n[package]\nname = \"x\"\n",
        "# new comment\n[package]\nname = \"x\"\n",
    );
    let edited = vec!["src/config.toml".to_string()];
    assert!(mutation_is_trivial(&root, &sandbox, &edited));
}

#[test]
fn test_mutation_is_trivial_new_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    let sandbox = dir.path().join("sandbox");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(sandbox.join("src")).unwrap();
    // New file containing only comments → trivial.
    std::fs::write(sandbox.join("src/notes.rs"), "// just a note\n").unwrap();
    let edited = vec!["src/notes.rs".to_string()];
    assert!(mutation_is_trivial(&root, &sandbox, &edited));
    // New file containing code → NOT trivial.
    std::fs::write(sandbox.join("src/notes.rs"), "pub fn new() {}\n").unwrap();
    assert!(!mutation_is_trivial(&root, &sandbox, &edited));
}

#[test]
fn test_mutation_is_trivial_empty_edit_list() {
    let dir = tempfile::tempdir().unwrap();
    assert!(mutation_is_trivial(dir.path(), dir.path(), &[]));
}

#[test]
fn test_rsi_orchestrator_stop() {
    let mut orch = RSIOrchestrator::new(PathBuf::from("/tmp/test_project"));
    // Initially not running
    assert!(!orch.is_running);

    // Simulate the state that run_loop sets
    orch.is_running = true;
    assert!(orch.is_running);

    orch.stop();
    assert!(!orch.is_running);
}

#[test]
fn test_rsi_orchestrator_stop_idempotent() {
    let mut orch = RSIOrchestrator::new(PathBuf::from("/tmp/test_project"));
    orch.stop();
    orch.stop(); // second call should be fine
    assert!(!orch.is_running);
}

#[test]
fn test_exponential_backoff_calculation() {
    // Test the exponential backoff formula from run_loop:
    // 60 * 2^(failures-1), capped at 3600
    let compute_backoff = |consecutive_failures: usize| -> u64 {
        std::cmp::min(
            60u64.saturating_mul(1u64 << (consecutive_failures - 1)),
            3600,
        )
    };

    assert_eq!(compute_backoff(1), 60); // 60 * 2^0 = 60
    assert_eq!(compute_backoff(2), 120); // 60 * 2^1 = 120
    assert_eq!(compute_backoff(3), 240); // 60 * 2^2 = 240
    assert_eq!(compute_backoff(4), 480); // 60 * 2^3 = 480
    assert_eq!(compute_backoff(5), 960); // 60 * 2^4 = 960
    assert_eq!(compute_backoff(6), 1920); // 60 * 2^5 = 1920
    assert_eq!(compute_backoff(7), 3600); // 60 * 2^6 = 3840, capped at 3600
}

#[test]
fn test_tsv_score_parsing_empty() {
    // Simulate TSV parsing logic from run_benchmark_and_get_score
    let tsv_content =
        "scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes\n";
    let (total_score, count) = parse_tsv_scores(tsv_content);
    assert_eq!(count, 0);
    assert_eq!(total_score, 0.0);
}

#[test]
fn test_tsv_score_parsing_single_row() {
    let tsv_content =
        "scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes\n\
                           test1|unit|easy|0.5|0.8|agent1|30|15|0.85|yes||ok\n";
    let (total_score, count) = parse_tsv_scores(tsv_content);
    assert_eq!(count, 1);
    assert!((total_score - 0.85).abs() < f64::EPSILON);
}

#[test]
fn test_tsv_score_parsing_multiple_rows() {
    let tsv_content =
        "scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes\n\
                           test1|unit|easy|0.5|0.8|agent1|30|15|0.80|yes||ok\n\
                           test2|unit|medium|0.3|0.7|agent1|60|30|0.90|yes||ok\n\
                           test3|unit|hard|0.1|0.5|agent1|120|60|0.70|no||fail\n";
    let (total_score, count) = parse_tsv_scores(tsv_content);
    assert_eq!(count, 3);
    let avg = total_score / count as f64;
    assert!((avg - 0.80).abs() < f64::EPSILON);
}

#[test]
fn test_tsv_score_parsing_invalid_score() {
    let tsv_content =
        "scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes\n\
                           test1|unit|easy|0.5|0.8|agent1|30|15|not_a_number|yes||ok\n";
    let (total_score, count) = parse_tsv_scores(tsv_content);
    assert_eq!(count, 0);
    assert_eq!(total_score, 0.0);
}

#[test]
fn test_tsv_score_parsing_short_row() {
    // Row with fewer than 9 columns should be skipped
    let tsv_content =
        "scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes\n\
                           test1|unit|easy\n";
    let (total_score, count) = parse_tsv_scores(tsv_content);
    assert_eq!(count, 0);
    assert_eq!(total_score, 0.0);
}

#[test]
fn test_consecutive_failures_tracking() {
    let mut orch = RSIOrchestrator::new(PathBuf::from("/tmp/test_project"));
    assert_eq!(orch.consecutive_failures, 0);

    // Simulate failure increments
    orch.consecutive_failures += 1;
    assert_eq!(orch.consecutive_failures, 1);

    orch.consecutive_failures += 1;
    assert_eq!(orch.consecutive_failures, 2);

    // Simulate reset on success
    orch.consecutive_failures = 0;
    assert_eq!(orch.consecutive_failures, 0);
}

#[test]
fn test_circuit_breaker_threshold() {
    let orch = RSIOrchestrator::new(PathBuf::from("/tmp/test_project"));
    // Verify the circuit breaker triggers at exactly max_consecutive_failures
    assert_eq!(orch.max_consecutive_failures, 5);

    // Simulate reaching threshold
    let mut failures = 0;
    let should_trip = |failures: usize, max: usize| failures >= max;

    for _ in 0..4 {
        failures += 1;
        assert!(
            !should_trip(failures, orch.max_consecutive_failures),
            "Should not trip at {} failures",
            failures
        );
    }
    failures += 1;
    assert!(
        should_trip(failures, orch.max_consecutive_failures),
        "Should trip at {} failures",
        failures
    );
}

#[test]
fn test_rsi_state_save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let project_root = dir.path().to_path_buf();
    let mut orch = RSIOrchestrator::new(project_root);
    orch.total_iterations = 42;
    orch.consecutive_failures = 3;
    orch.save_state().unwrap();

    // Load into a fresh orchestrator
    let orch2 = RSIOrchestrator::new(dir.path().to_path_buf());
    assert_eq!(orch2.total_iterations, 42);
    assert_eq!(orch2.consecutive_failures, 3);
}

#[test]
fn test_rsi_state_missing_file_is_ok() {
    let dir = tempfile::tempdir().unwrap();
    let orch = RSIOrchestrator::new(dir.path().to_path_buf());
    // Should start from defaults when no state file exists
    assert_eq!(orch.total_iterations, 0);
    assert_eq!(orch.consecutive_failures, 0);
}

#[test]
fn test_rsi_stop_saves_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut orch = RSIOrchestrator::new(dir.path().to_path_buf());
    orch.total_iterations = 10;
    orch.stop();
    // Verify state was persisted
    let state_path = RSIOrchestrator::default_state_path(dir.path());
    assert!(state_path.exists());
}

// ── Circuit breaker: non-improving cycles count as failures ──

#[test]
fn test_record_cycle_failure_increments_and_only_trips_at_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let project_root = dir.path().to_path_buf();
    let state_path = project_root.join(".selfware/rsi_state.json");
    let history_path = project_root.join(".selfware/history.json");
    let mut orch = RSIOrchestrator::with_paths(project_root, state_path, history_path);
    orch.max_consecutive_failures = 3;

    // Each non-improving cycle (Ok(false) path) increments the counter...
    orch.record_cycle_failure(1).unwrap();
    assert_eq!(orch.consecutive_failures, 1);
    orch.record_cycle_failure(1).unwrap();
    assert_eq!(orch.consecutive_failures, 2);

    // ...and the breaker trips exactly at the threshold.
    let err = orch.record_cycle_failure(2).unwrap_err();
    assert!(
        err.to_string().contains("3 consecutive failures"),
        "trip error must name the failure count, got: {}",
        err
    );

    // Tripping persists state so the failure count survives a restart.
    let saved = std::fs::read_to_string(RSIOrchestrator::default_state_path(dir.path())).unwrap();
    let state: RSIState = serde_json::from_str(&saved).unwrap();
    assert_eq!(state.consecutive_failures, 3);
}

#[test]
fn test_genuine_improvement_resets_consecutive_failures() {
    // run_loop resets the counter on Ok(true) only; simulate the exact
    // assignment it performs after a merged improvement.
    let dir = tempfile::tempdir().unwrap();
    let project_root = dir.path().to_path_buf();
    let state_path = project_root.join(".selfware/rsi_state.json");
    let history_path = project_root.join(".selfware/history.json");
    let mut orch = RSIOrchestrator::with_paths(project_root, state_path, history_path);

    orch.record_cycle_failure(1).unwrap();
    orch.record_cycle_failure(1).unwrap();
    assert_eq!(orch.consecutive_failures, 2);

    // Ok(true) arm in run_loop:
    orch.consecutive_failures = 0;
    assert_eq!(orch.consecutive_failures, 0);
}

// ── Atomic save_state ──

#[test]
fn test_save_state_is_atomic_and_leaves_no_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let project_root = dir.path().to_path_buf();
    let state_path = project_root.join(".selfware/rsi_state.json");
    let history_path = project_root.join(".selfware/history.json");
    let mut orch = RSIOrchestrator::with_paths(project_root, state_path.clone(), history_path);
    orch.total_iterations = 7;
    orch.consecutive_failures = 2;

    orch.save_state().unwrap();

    // The final file holds the full, valid JSON snapshot.
    let state: RSIState =
        serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
    assert_eq!(state.total_iterations, 7);
    assert_eq!(state.consecutive_failures, 2);

    // No temp files are left behind after the rename.
    let leftovers: Vec<_> = std::fs::read_dir(state_path.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(
        leftovers.is_empty(),
        "save_state must not leave temp files: {:?}",
        leftovers
    );

    // A second save (over an existing state file) works too.
    orch.total_iterations = 8;
    orch.save_state().unwrap();
    let state: RSIState =
        serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
    assert_eq!(state.total_iterations, 8);
}

// The fixture writes a `run_projecte2e.sh` shell script with a bash
// shebang and runs it via the RSI improvement cycle; Windows CI doesn't
// ship `bash` and `.sh` files aren't executable there. The path-safety
// production fix is orthogonal to that shell dependency.
#[cfg(unix)]
#[tokio::test]
// The state lock is intentionally held across awaits: it keeps the
// process-global cwd and HOME stable for the sandbox git-clone and for
// the rustup-shimmed `cargo` invocations in `sandbox.verify()`.
#[allow(clippy::await_holding_lock)]
async fn test_execute_improvement_cycle_applies_mutation_and_records_result() {
    let _state = crate::test_support::CwdGuard::hold();
    let dir = create_rsi_fixture_project();
    let project_root = dir.path().to_path_buf();
    let state_path = project_root.join(".selfware/rsi_state.json");
    let history_path = project_root.join(".selfware/history.json");
    let mut orch = RSIOrchestrator::with_paths(project_root.clone(), state_path, history_path);

    let improved = orch.execute_improvement_cycle().await.unwrap();
    assert!(improved, "RSI cycle should merge an improving mutation");

    let content = fs::read_to_string(project_root.join("src/lib.rs")).unwrap();
    assert!(content.contains("Resolved: remove this marker"));

    let history = orch.edit_orchestrator.history();
    assert_eq!(history.len(), 1);
    assert!(history[0].verified);
    assert!(!history[0].rolled_back);
    assert!(history[0].effectiveness_score > 0.0);
}

/// A whole-line TODO comment rewrite is a TRIVIAL mutation: the cycle
/// must skip the paid e2e evaluation entirely (no results.tsv produced),
/// return Ok(false), and record the attempt as rolled-back/unverified.
#[cfg(unix)]
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_execute_improvement_cycle_skips_trivial_comment_mutation() {
    let _state = crate::test_support::CwdGuard::hold();
    let dir = create_rsi_fixture_project_with_lib(
        r#"pub fn demo() -> usize {
    // TODO: remove this marker
    42
}

#[cfg(test)]
mod tests {
    #[test]
    fn demo_returns_answer() {
        assert_eq!(super::demo(), 42);
    }
}
"#,
    );
    let project_root = dir.path().to_path_buf();
    let state_path = project_root.join(".selfware/rsi_state.json");
    let history_path = project_root.join(".selfware/history.json");
    let mut orch = RSIOrchestrator::with_paths(project_root.clone(), state_path, history_path);

    let improved = orch.execute_improvement_cycle().await.unwrap();
    assert!(!improved, "trivial comment mutation must not be merged");

    // The TODO rewrite happened only in the (now cleaned) sandbox.
    let content = fs::read_to_string(project_root.join("src/lib.rs")).unwrap();
    assert!(content.contains("// TODO: remove this marker"));

    // No paid e2e suite ran: the benchmark script never produced a TSV.
    assert!(
        !project_root
            .join("system_tests/projecte2e/reports/latest/results.tsv")
            .exists(),
        "trivial mutation must not burn a paid e2e suite"
    );

    let history = orch.edit_orchestrator.history();
    assert_eq!(history.len(), 1);
    assert!(!history[0].verified);
    assert!(history[0].rolled_back);
}

/// Helper: replicates the TSV score-parsing logic from run_benchmark_and_get_score.
fn parse_tsv_scores(tsv_content: &str) -> (f64, usize) {
    let mut total_score = 0.0;
    let mut count = 0;

    for (i, line) in tsv_content.lines().enumerate() {
        if i == 0 {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() > 8 {
            if let Ok(score) = parts[8].parse::<f64>() {
                total_score += score;
                count += 1;
            }
        }
    }

    (total_score, count)
}

fn create_rsi_fixture_project() -> tempfile::TempDir {
    create_rsi_fixture_project_with_lib(
        r#"pub fn demo() -> usize {
    42 // TODO: remove this marker
}

#[cfg(test)]
mod tests {
    #[test]
    fn demo_returns_answer() {
        assert_eq!(super::demo(), 42);
    }
}
"#,
    )
}

fn create_rsi_fixture_project_with_lib(lib_src: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("system_tests/projecte2e")).unwrap();

    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "rsi_fixture"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
    )
    .unwrap();

    fs::write(root.join("src/lib.rs"), lib_src).unwrap();

    fs::write(
        root.join("system_tests/projecte2e/run_projecte2e.sh"),
        r#"#!/usr/bin/env bash
set -euo pipefail
mkdir -p system_tests/projecte2e/reports/latest
# Detect if running in sandbox (path contains .selfware-sandbox)
if pwd | grep -q ".selfware-sandbox"; then
    score="0.95"  # Higher score in sandbox to simulate improvement
else
    score="0.90"  # Baseline score
fi
cat > system_tests/projecte2e/reports/latest/results.tsv <<EOF
scenario|type|difficulty|baseline|post|agent|timeout|duration|score|changed|error|notes
todo_cleanup|unit|easy|0|0|selfware|0|0|${score}|yes||
EOF
"#,
    )
    .unwrap();

    init_git_repo(root);
    dir
}

fn init_git_repo(project_root: &Path) {
    run_git(project_root, &["init"]);
    run_git(project_root, &["config", "user.email", "codex@openai.com"]);
    run_git(project_root, &["config", "user.name", "Codex"]);
    run_git(project_root, &["add", "."]);
    run_git(project_root, &["commit", "-m", "initial"]);
}

fn run_git(project_root: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(args)
        .current_dir(project_root)
        .status()
        .unwrap();
    assert!(status.success(), "git {:?} should succeed", args);
}
