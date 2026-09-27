use super::*;

fn key(p: &str) -> String {
    p.trim_start_matches("./").to_string()
}

fn read_result(first: usize, lines: &[&str]) -> String {
    let numbered = crate::tools::line_numbers::number_lines(&(lines.join("\n") + "\n"), first);
    serde_json::json!({"content": numbered, "line_numbers": true}).to_string()
}

fn read_keys(path: &str, first: usize, lines: &[&str]) -> Vec<u64> {
    let args = serde_json::json!({"path": path, "line_range": [first, first + lines.len() - 1]});
    observation_keys(
        "file_read",
        &args.to_string(),
        &read_result(first, lines),
        true,
        &key,
    )
}

/// Drive one read-only turn on a green, unchanged tree; returns its verdict.
fn stall_turn(fs: &mut FinishStall, turn: usize, keys: &[u64]) -> TurnVerdict {
    fs.begin_turn(turn, true, 1);
    fs.record_call(false, false, None, keys);
    fs.end_turn(true, 1)
}

/// The edit + passing-check turn that establishes the green point.
fn green_turn(fs: &mut FinishStall, turn: usize) {
    fs.begin_turn(turn, false, 0);
    fs.record_call(true, false, None, &[]);
    fs.record_call(false, true, Some("cargo_check"), &[42]);
    assert_eq!(fs.end_turn(true, 1), TurnVerdict::Continue);
}

#[test]
fn identical_line_content_is_seen_across_tools_and_ranges() {
    let whole = read_keys("./src/a.rs", 1, &["fn a() {}", "fn b() {}", "fn c() {}"]);
    let sub = read_keys("src/a.rs", 2, &["fn b() {}"]);
    let mut fs = FinishStall::default();
    assert!(fs.is_novel(&whole));
    fs.record_call(false, false, None, &whole);
    assert!(
        !fs.is_novel(&sub),
        "a sub-range of delivered lines is not new"
    );

    let grep = serde_json::json!({"count": 1, "matches": [
        {"file": "src/a.rs", "line": 3, "content": "fn c() {}",
         "context_before": ["fn b() {}"], "context_after": []}
    ]})
    .to_string();
    let grep_keys = observation_keys("grep_search", r#"{"pattern":"c"}"#, &grep, true, &key);
    assert!(
        !fs.is_novel(&grep_keys),
        "grep matches of delivered lines are not new"
    );

    // The same text at a SHIFTED line (an edit above it) is new: the model
    // needs the new line numbers.
    let shifted = read_keys("src/a.rs", 3, &["fn b() {}"]);
    assert!(fs.is_novel(&shifted));
    // A different file is new.
    assert!(fs.is_novel(&read_keys("src/b.rs", 1, &["fn a() {}"])));
}

#[test]
fn directive_after_k_stall_turns_then_refusal_after_k_more() {
    let mut fs = FinishStall::default();
    let lines = read_keys("src/a.rs", 1, &["x"]);
    green_turn(&mut fs, 10);
    // First read after the check delivers new content: not a stall.
    assert_eq!(stall_turn(&mut fs, 11, &lines), TurnVerdict::Continue);
    assert_eq!(fs.streak(), 0);
    for turn in 12..12 + FINISH_STALL_TURNS - 1 {
        assert_eq!(stall_turn(&mut fs, turn, &lines), TurnVerdict::Continue);
    }
    assert_eq!(
        stall_turn(&mut fs, 12 + FINISH_STALL_TURNS - 1, &lines),
        TurnVerdict::Nudge
    );
    assert!(fs.nudged() && !fs.refusing());
    let base = 12 + FINISH_STALL_TURNS;
    for turn in base..base + FINISH_STALL_TURNS - 1 {
        assert_eq!(stall_turn(&mut fs, turn, &lines), TurnVerdict::Continue);
    }
    assert_eq!(
        stall_turn(&mut fs, base + FINISH_STALL_TURNS - 1, &lines),
        TurnVerdict::StartRefusing
    );
    // Now a seen read is refused, a novel one is not.
    fs.begin_turn(99, true, 1);
    assert!(fs.should_refuse(false, false, fs.is_novel(&lines)));
    let fresh = read_keys("src/other.rs", 1, &["new"]);
    assert!(!fs.should_refuse(false, false, fs.is_novel(&fresh)));
    // A check re-run on the unchanged green tree adds nothing: refused.
    assert!(fs.should_refuse(false, true, true));
    // An edit attempt is never refused.
    assert!(!fs.should_refuse(true, false, false));
    let refusal = fs.refusal();
    assert!(refusal.contains(FINISH_STALL_REFUSAL_KEY), "{refusal}");
    assert!(
        refusal.contains("cargo_check passed at turn 10"),
        "{refusal}"
    );
}

#[test]
fn directive_names_files_and_the_passing_check() {
    let mut fs = FinishStall::default();
    green_turn(&mut fs, 30);
    let text = fs.directive(&["docs/NOTES.md".to_string(), "src/a.rs".to_string()]);
    assert!(text.contains(FINISH_STALL_DIRECTIVE_MARKER), "{text}");
    assert!(text.contains("docs/NOTES.md, src/a.rs"), "{text}");
    assert!(text.contains("cargo_check passed at turn 30"), "{text}");
}

#[test]
fn novel_reads_never_count_and_reset_the_streak() {
    let mut fs = FinishStall::default();
    green_turn(&mut fs, 5);
    for turn in 6..30 {
        // Every turn reads a different range of a large file: all new.
        let keys = read_keys("src/big.rs", turn * 10, &["line"]);
        assert_eq!(stall_turn(&mut fs, turn, &keys), TurnVerdict::Continue);
        assert!(!fs.nudged());
    }
    assert!(!fs.blocks_budget_extension());
    assert!(fs.outcome_clause().is_none());
    assert!(fs.summary_detail().is_none());
}

#[test]
fn a_mutation_or_a_red_tree_resets_everything() {
    let mut fs = FinishStall::default();
    let lines = read_keys("src/a.rs", 1, &["x"]);
    green_turn(&mut fs, 1);
    fs.record_call(false, false, None, &lines);
    for turn in 2..2 + FINISH_STALL_TURNS {
        stall_turn(&mut fs, turn, &lines);
    }
    assert!(fs.nudged());
    // An edit moves the tree: the old green point and the phase are gone.
    fs.begin_turn(20, true, 1);
    fs.record_call(true, false, None, &[]);
    assert_eq!(fs.end_turn(true, 2), TurnVerdict::Continue);
    assert!(!fs.nudged());
    assert_eq!(fs.green_point().map(|g| g.turn), Some(20));
    // A failing check on the tree: not green, nothing to finish.
    fs.begin_turn(21, true, 2);
    fs.record_call(false, true, None, &[7]);
    assert_eq!(fs.end_turn(false, 2), TurnVerdict::Continue);
    assert!(fs.green_point().is_none());
    assert!(!fs.blocks_budget_extension());
}

#[test]
fn turns_that_are_not_green_at_start_never_stall() {
    // Read-only turns BEFORE any verified state (exploration) are never
    // counted, however repetitive.
    let mut fs = FinishStall::default();
    let lines = read_keys("src/a.rs", 1, &["x"]);
    for turn in 1..20 {
        fs.begin_turn(turn, false, 0);
        fs.record_call(false, false, None, &lines);
        assert_eq!(fs.end_turn(false, 0), TurnVerdict::Continue);
    }
    assert!(!fs.nudged());
    assert!(!fs.blocks_budget_extension());
}

#[test]
fn budget_extension_is_withheld_after_a_stall_turn_and_reported() {
    let mut fs = FinishStall::default();
    let lines = read_keys("src/a.rs", 1, &["x"]);
    green_turn(&mut fs, 38);
    stall_turn(&mut fs, 39, &lines); // new content
    assert!(!fs.blocks_budget_extension());
    stall_turn(&mut fs, 40, &lines); // seen
    assert!(fs.blocks_budget_extension());
    assert!(fs.note_extension_withheld());
    assert!(!fs.note_extension_withheld(), "reported once");
    let clause = fs.outcome_clause().expect("clause");
    assert!(
        clause.contains("verified at turn 38")
            && clause.contains("cargo_check passed")
            && clause.contains("no final answer was given"),
        "{clause}"
    );
    let detail = fs.summary_detail().expect("detail");
    assert!(
        detail.contains("iteration budget NOT extended at turn 40"),
        "{detail}"
    );
}

#[test]
fn failed_results_and_unparsed_output_hash_whole() {
    let a = observation_keys("git_status", "{}", "clean", true, &key);
    let b = observation_keys("git_status", "{}", "clean", true, &key);
    let c = observation_keys("git_status", "{}", "dirty", true, &key);
    assert_eq!(a, b);
    assert_ne!(a, c);
    let err = observation_keys("file_read", r#"{"path":"x"}"#, "not found", false, &key);
    assert_eq!(err.len(), 1);
}

// ── Agent runs against a scripted mock LLM ──

mod agent_runs {
    use super::*;
    use crate::agent::Agent;
    use crate::config::Config;
    use crate::testing::mock_api::MockLlmServer;

    fn config(endpoint: String) -> Config {
        Config {
            endpoint,
            model: "mock-model".to_string(),
            context_length: 500_000,
            max_tokens: 8192,
            agent: crate::config::AgentConfig {
                max_iterations: 20,
                step_timeout_secs: 120,
                stream_stall_timeout_secs: None,
                streaming: false,
                native_function_calling: false,
                min_completion_steps: 0,
                require_verification_before_completion: false,
                ..Default::default()
            },
            safety: crate::config::SafetyConfig {
                allowed_paths: vec!["./**".to_string(), "/**".to_string()],
                ..Default::default()
            },
            execution_mode: crate::config::ExecutionMode::Yolo,
            ..Default::default()
        }
    }

    /// A scratch directory under the repo (inside the task root, so the
    /// check is in scope), removed on drop.
    struct Scratch {
        dir: std::path::PathBuf,
        rel: String,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let rel = format!("docs/.sw_finish_stall_{tag}_{}", std::process::id());
            let dir = std::env::current_dir().expect("cwd").join(&rel);
            std::fs::create_dir_all(&dir).expect("scratch dir");
            Self { dir, rel }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn tool(name: &str, args: serde_json::Value) -> String {
        format!("<tool>\n<name>{name}</name>\n<arguments>{args}</arguments>\n</tool>")
    }

    fn history(agent: &Agent) -> Vec<String> {
        agent
            .messages
            .iter()
            .map(|m| m.content.text().to_string())
            .collect()
    }

    const BODY: &str = "def add(a, b):\n    return a + b\n\n\ndef sub(a, b):\n    return a - b\n";

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn dithering_after_a_verified_edit_is_nudged_once_then_refused_and_an_answer_completes() {
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("dither");
        let file = format!("{}/calc.py", scratch.rel);
        let read = |range: Option<[u64; 2]>| match range {
            Some(r) => tool(
                "file_read",
                serde_json::json!({"path": file, "line_range": r}),
            ),
            None => tool("file_read", serde_json::json!({"path": file})),
        };
        // Distinct arguments, identical output: no duplicate-call guard
        // treats them as the same call, only the content says "seen".
        let grep = |max: u64| {
            tool(
                "grep_search",
                serde_json::json!({"path": file, "pattern": "def ", "max_matches": max}),
            )
        };
        let server = MockLlmServer::builder()
            .with_response(format!(
                "FILES: {file}\n\n{}",
                tool("file_write", serde_json::json!({"path": file, "content": BODY}))
            ))
            .with_response(tool(
                "shell_exec",
                serde_json::json!({"command": format!("python3 -m py_compile {file}")}),
            ))
            .with_response(read(None)) // new: first read-back of the edit
            .with_response(grep(10)) // seen 1
            .with_response(read(Some([1, 2]))) // seen 2
            // Consumed by the step-5 reflection side call, not a turn.
            .with_response("Reflection: re-reading the same file adds nothing.")
            .with_response(grep(11)) // seen 3 -> directive
            .with_response(read(Some([5, 6]))) // seen 1
            .with_response(grep(12)) // seen 2
            .with_response(grep(13)) // seen 3 -> refusal armed
            .with_response(grep(14)) // refused
            .with_response("Final answer: calc.py has add and sub; py_compile passed.")
            .build()
            .await;
        let mut agent = Agent::new(config(format!("{}/v1", server.url())))
            .await
            .unwrap();
        let task = format!(
            "Fix task: create {file} with add and sub functions, then verify it with python3 -m py_compile."
        );
        let result = agent.run_task(&task).await;
        let messages = history(&agent);
        let joined = messages.join("\n---\n");
        assert!(result.is_ok(), "{:?}\n{joined}", result.err());
        let directives = messages
            .iter()
            .filter(|m| m.contains(FINISH_STALL_DIRECTIVE_MARKER))
            .count();
        assert_eq!(directives, 1, "the directive fires exactly once:\n{joined}");
        let refusals = messages
            .iter()
            .filter(|m| m.contains(FINISH_STALL_REFUSAL_KEY))
            .count();
        assert_eq!(
            refusals, 1,
            "exactly the one call after the armed turn:\n{joined}"
        );
        assert_eq!(agent.finish_stall.refused(), 1);
        // The directive precedes the refusal, and nothing was refused before
        // the directive.
        let d = messages
            .iter()
            .position(|m| m.contains(FINISH_STALL_DIRECTIVE_MARKER))
            .unwrap();
        let r = messages
            .iter()
            .position(|m| m.contains(FINISH_STALL_REFUSAL_KEY))
            .unwrap();
        assert!(d < r, "{joined}");
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn reading_new_content_after_a_verified_edit_is_never_nudged_or_refused() {
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("novel");
        let file = format!("{}/calc.py", scratch.rel);
        let other = format!("{}/other.py", scratch.rel);
        let mut builder = MockLlmServer::builder()
            .with_response(format!(
                "FILES: {file}\n\n{}",
                tool(
                    "file_write",
                    serde_json::json!({"path": file, "content": BODY})
                )
            ))
            .with_response(tool(
                "shell_exec",
                serde_json::json!({"command": format!("python3 -m py_compile {file}")}),
            ));
        // Seven read-only turns, each returning lines never returned before.
        for range in [[1, 1], [2, 2], [3, 3], [4, 4], [5, 5], [6, 6]] {
            builder = builder.with_response(tool(
                "file_read",
                serde_json::json!({"path": file, "line_range": range}),
            ));
        }
        // An edit, then the same ranges again: new content (the tree moved).
        builder = builder
            .with_response(format!(
                "FILES: {other}\n\n{}",
                tool(
                    "file_write",
                    serde_json::json!({"path": other, "content": "X = 1\n"})
                )
            ))
            .with_response(tool("file_read", serde_json::json!({"path": other})))
            .with_response("Final answer: done.");
        let server = builder.build().await;
        let mut agent = Agent::new(config(format!("{}/v1", server.url())))
            .await
            .unwrap();
        let task = format!(
            "Fix task: create {file} with add and sub functions and {other} with X = 1, then verify with python3 -m py_compile."
        );
        let result = agent.run_task(&task).await;
        let joined = history(&agent).join("\n---\n");
        assert!(result.is_ok(), "{:?}\n{joined}", result.err());
        assert!(
            !joined.contains(FINISH_STALL_DIRECTIVE_MARKER),
            "new content is never nudged:\n{joined}"
        );
        assert!(
            !joined.contains(FINISH_STALL_REFUSAL_KEY),
            "new content is never refused:\n{joined}"
        );
        assert_eq!(agent.finish_stall.refused(), 0);
        assert!(
            scratch.dir.join("other.py").exists(),
            "the later edit ran:\n{joined}"
        );
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn re_reads_of_a_verified_tree_at_the_cap_earn_no_budget_extension() {
        // c24: the cap tripped during finish-stall re-reads and the adaptive
        // extension granted 40→50 more of them. Every turn here succeeds
        // with distinct arguments (so `productive_streak` alone would grant
        // it); the finish stall withholds it and the summary says why.
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("cap");
        let file = format!("{}/calc.py", scratch.rel);
        let grep = |max: u64| {
            tool(
                "grep_search",
                serde_json::json!({"path": file, "pattern": "def ", "max_matches": max}),
            )
        };
        let mut builder = MockLlmServer::builder()
            .with_response(format!(
                "FILES: {file}\n\n{}",
                tool(
                    "file_write",
                    serde_json::json!({"path": file, "content": BODY})
                )
            ))
            .with_response(tool(
                "shell_exec",
                serde_json::json!({"command": format!("python3 -m py_compile {file}")}),
            ))
            .with_response(tool("file_read", serde_json::json!({"path": file})));
        for max in 10..30 {
            builder = builder.with_response(grep(max));
        }
        let server = builder.build().await;
        let mut cfg = config(format!("{}/v1", server.url()));
        cfg.agent.max_iterations = 7;
        let mut agent = Agent::new(cfg).await.unwrap();
        let task = format!(
            "Fix task: create {file} with add and sub functions, then verify it with python3 -m py_compile."
        );
        let _ = agent.run_task(&task).await;
        let summary = agent.run_summary();
        let joined = history(&agent).join("\n---\n");
        assert!(!summary.budget_extended, "{joined}");
        let detail = summary.finish_stall_detail.clone().unwrap_or_default();
        assert!(
            detail.contains("iteration budget NOT extended"),
            "detail: {detail}\n{joined}"
        );
        server.stop().await;
    }

    #[tokio::test]
    async fn max_iterations_on_a_finish_stall_does_not_advise_more_iterations() {
        use crate::agent::failure_mode::{
            FailureKind, FailureMode, RunOutcome, FINISH_STALL_ADVICE,
        };
        let server = MockLlmServer::builder().with_response("done").build().await;
        let agent_cfg = config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(agent_cfg).await.unwrap();
        let outcome = || RunOutcome::Failed {
            reason: crate::agent::loop_control::MAX_ITERATIONS_STOP_REASON.to_string(),
        };
        let plain = FailureMode::classify(&agent, outcome());
        assert_eq!(plain.kind, FailureKind::MaxIterations);
        assert_ne!(plain.advice, FINISH_STALL_ADVICE);

        let lines = read_keys("src/a.rs", 1, &["x"]);
        green_turn(&mut agent.finish_stall, 38);
        stall_turn(&mut agent.finish_stall, 39, &lines);
        stall_turn(&mut agent.finish_stall, 40, &lines);
        let fm = FailureMode::classify(&agent, outcome());
        assert_eq!(
            fm.kind,
            FailureKind::MaxIterations,
            "the typed stop is unchanged"
        );
        assert_eq!(fm.advice, FINISH_STALL_ADVICE);
        assert!(
            fm.evidence.contains("verified at turn 38") && fm.evidence.contains("no final answer"),
            "{}",
            fm.evidence
        );
        server.stop().await;
    }
}
