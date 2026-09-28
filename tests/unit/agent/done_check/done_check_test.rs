use super::*;

const C24_TASK: &str = "Multi-step documentation task in this Rust repo. Do the steps in order.
1. Read src/agent/context.rs in full.
2. Read src/agent/compression.rs in full.
3. Read src/agent/context_management.rs in full.
4. Create docs/CONTEXT_NOTES.md containing one section per file (context.rs, compression.rs, context_management.rs). In each section list every `pub fn` / `pub async fn` defined in that file as a bullet: `name` (line N) - one-sentence description.
5. In src/agent/context.rs, add a one-line `///` doc comment directly above every `pub fn` / `pub async fn` that does not already have a doc comment. Do not change any code other than adding comments.
6. Finish with a short summary saying how many functions you documented in step 5 and how many bullets are in docs/CONTEXT_NOTES.md.
Do not re-read a file you have already read unless you need to verify an edit.";

fn key(p: &str) -> String {
    p.trim_start_matches("./").to_string()
}

fn c24_ledger() -> Ledger {
    let mut ledger = Ledger {
        mutation_task: true,
        ..Ledger::default()
    };
    for p in ["docs/CONTEXT_NOTES.md", "src/agent/context.rs"] {
        ledger.changed.insert(p.to_string(), p.to_string());
    }
    for p in [
        "src/agent/context.rs",
        "src/agent/compression.rs",
        "src/agent/context_management.rs",
    ] {
        ledger.read.insert(p.to_string());
    }
    ledger.checks.push(CheckRun {
        name: "cargo_check".to_string(),
        passed: true,
        current: true,
    });
    ledger
        .line_counts
        .insert("src/agent/context.rs".to_string(), 800);
    ledger
}

fn c24_claim(done: bool) -> DoneClaim {
    let item = |id: &str, evidence: &str| ClaimItem {
        id: id.to_string(),
        met: true,
        evidence: evidence.to_string(),
    };
    DoneClaim {
        done,
        items: vec![
            item("R1", "read src/agent/context.rs"),
            item("R2", "read src/agent/compression.rs"),
            item("R3", "read src/agent/context_management.rs"),
            item("R4", "docs/CONTEXT_NOTES.md: 44 bullets"),
            item("R5", "src/agent/context.rs:120 and 19 other doc comments"),
            item("R6", "final answer"),
        ],
        remaining: vec![],
        final_answer_ready: true,
        final_answer: Some("Documented 20 functions; 44 bullets.".to_string()),
    }
}

// ── requirements ──

#[test]
fn numbered_steps_become_requirements_with_kinds() {
    let reqs = task_requirements(C24_TASK, true);
    let kinds: Vec<_> = reqs.iter().map(|r| (r.id.as_str(), r.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            ("R1", RequirementKind::Read),
            ("R2", RequirementKind::Read),
            ("R3", RequirementKind::Read),
            ("R4", RequirementKind::Change),
            ("R5", RequirementKind::Change),
            ("R6", RequirementKind::Answer),
        ]
    );
    // The preamble and the trailing rule are not requirements.
    assert!(reqs.iter().all(|r| !r.text.contains("Do the steps")));
    assert!(reqs.iter().all(|r| !r.text.contains("re-read")));
}

#[test]
fn a_task_without_a_list_is_one_requirement() {
    let reqs = task_requirements("Add a max_words option to slugify and a test for it.", true);
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].id, "R1");
    assert_eq!(reqs[0].kind, RequirementKind::Change);
    let reqs = task_requirements("hi", false);
    assert_eq!(reqs[0].kind, RequirementKind::Answer);
}

#[test]
fn more_items_than_the_cap_are_folded_not_dropped() {
    let task: String = (1..=15)
        .map(|i| format!("{i}. add item{i}.txt\n"))
        .collect();
    let reqs = task_requirements(&task, true);
    assert_eq!(reqs.len(), MAX_REQUIREMENTS);
    assert!(reqs.last().unwrap().text.contains("item15.txt"));
}

#[test]
fn check_and_read_requirements_are_classified() {
    assert_eq!(
        classify_requirement("Run pytest and make sure it passes", true),
        RequirementKind::Check
    );
    assert_eq!(
        classify_requirement("Review src/lib.rs", true),
        RequirementKind::Read
    );
    assert_eq!(
        classify_requirement("Report how many files changed", true),
        RequirementKind::Answer
    );
}

// ── parsing ──

#[test]
fn parses_fenced_json_with_prose_and_think_blocks() {
    let text = "<think>let me see</think>Sure:\n```json\n{\"status\": \"DONE\", \"requirements\": [{\"id\": 1, \"met\": \"true\", \"evidence\": \"a.rs:3\"}], \"remaining\": [], \"final_answer_ready\": true, \"final_answer\": \"Did it {really}.\"}\n```\nthanks";
    let claim = parse_done_claim(text).unwrap();
    assert!(claim.done);
    assert_eq!(claim.items[0].id, "R1");
    assert!(claim.items[0].met);
    assert_eq!(claim.items[0].evidence, "a.rs:3");
    assert_eq!(claim.final_answer.as_deref(), Some("Did it {really}."));
    assert!(claim.final_answer_ready);
}

#[test]
fn parses_not_done_and_remaining() {
    let claim = parse_done_claim(
        r#"{"status":"not done","requirements":[{"id":"R2","met":false,"evidence":""}],"remaining":["write the notes"],"final_answer_ready":false,"final_answer":""}"#,
    )
    .unwrap();
    assert!(!claim.done);
    assert_eq!(claim.remaining, vec!["write the notes".to_string()]);
    assert_eq!(claim.final_answer, None);
}

#[test]
fn rejects_replies_without_a_verdict() {
    assert!(parse_done_claim("I think I am done.").is_err());
    assert!(parse_done_claim(r#"{"requirements": []}"#).is_err());
    assert!(parse_done_claim(r#"{"status": "MAYBE"}"#).is_err());
    // A brace inside a string does not end the object early.
    assert!(parse_done_claim(r#"{"status": "DONE", "final_answer": "}"}"#).is_ok());
}

// ── verification ──

#[test]
fn c24_claim_with_evidence_is_verified_done() {
    let reqs = task_requirements(C24_TASK, true);
    let v = verify_claim(&reqs, &c24_claim(true), &c24_ledger(), &key);
    assert_eq!(v.status, DoneStatus::Verified, "{}", v.breakdown());
    assert_eq!(v.verified_count(), 6);
    assert!(v.label().starts_with("VERIFIED DONE (6/6"));
}

#[test]
fn a_claimed_change_to_a_file_never_changed_is_not_accepted() {
    let reqs = task_requirements(C24_TASK, true);
    let mut ledger = c24_ledger();
    ledger.changed.remove("docs/CONTEXT_NOTES.md");
    let v = verify_claim(&reqs, &c24_claim(true), &ledger, &key);
    assert_eq!(v.status, DoneStatus::ClaimedUnverified);
    let unverified = v.unverified();
    assert_eq!(unverified.len(), 1);
    assert_eq!(unverified[0].0.id, "R4");
    assert!(
        unverified[0].1.contains("docs/CONTEXT_NOTES.md"),
        "{}",
        unverified[0].1
    );
    let msg = feedback_message(&v, 30);
    assert!(
        msg.contains(DONE_CHECK_MARKER) && msg.contains("R4"),
        "{msg}"
    );
}

#[test]
fn a_cited_line_past_the_end_of_the_file_is_not_evidence() {
    let reqs = task_requirements(C24_TASK, true);
    let mut claim = c24_claim(true);
    claim.items[4].evidence = "src/agent/context.rs:9000".to_string();
    let v = verify_claim(&reqs, &claim, &c24_ledger(), &key);
    assert_eq!(v.status, DoneStatus::ClaimedUnverified);
    assert!(v.unverified()[0].1.contains("800 line"));
}

#[test]
fn a_read_requirement_needs_a_recorded_read() {
    let reqs = task_requirements(C24_TASK, true);
    let mut ledger = c24_ledger();
    ledger.read.remove("src/agent/compression.rs");
    let v = verify_claim(&reqs, &c24_claim(true), &ledger, &key);
    assert_eq!(v.status, DoneStatus::ClaimedUnverified);
    assert_eq!(v.unverified()[0].0.id, "R2");
}

#[test]
fn the_answer_requirement_needs_an_answer() {
    let reqs = task_requirements(C24_TASK, true);
    let mut claim = c24_claim(true);
    claim.final_answer = None;
    let v = verify_claim(&reqs, &claim, &c24_ledger(), &key);
    assert_eq!(v.status, DoneStatus::ClaimedUnverified);
    assert_eq!(v.unverified()[0].0.id, "R6");
}

#[test]
fn task_level_blockers_keep_done_unverified() {
    let reqs = task_requirements(C24_TASK, true);
    let mut ledger = c24_ledger();
    ledger.blocking_failure = Some("cargo check".to_string());
    ledger.open_findings = 2;
    let v = verify_claim(&reqs, &c24_claim(true), &ledger, &key);
    assert_eq!(v.status, DoneStatus::ClaimedUnverified);
    assert_eq!(v.blockers.len(), 2);
    assert!(v.breakdown().contains("cargo check"));
}

#[test]
fn a_check_requirement_needs_a_pass_on_the_current_tree() {
    let reqs = task_requirements("1. Add a.py\n2. Run pytest", true);
    let mut ledger = Ledger {
        mutation_task: true,
        ..Ledger::default()
    };
    ledger.changed.insert("a.py".into(), "a.py".into());
    ledger.checks.push(CheckRun {
        name: "python3 -m pytest".into(),
        passed: true,
        current: false,
    });
    let claim = DoneClaim {
        done: true,
        items: vec![
            ClaimItem {
                id: "R1".into(),
                met: true,
                evidence: "a.py".into(),
            },
            ClaimItem {
                id: "R2".into(),
                met: true,
                evidence: "pytest passed".into(),
            },
        ],
        remaining: vec![],
        final_answer_ready: true,
        final_answer: Some("done".into()),
    };
    let v = verify_claim(&reqs, &claim, &ledger, &key);
    assert_eq!(v.status, DoneStatus::ClaimedUnverified, "stale pass");
    ledger.checks[0].current = true;
    let v = verify_claim(&reqs, &claim, &ledger, &key);
    assert_eq!(v.status, DoneStatus::Verified, "{}", v.breakdown());
}

#[test]
fn not_done_feeds_remaining_and_unanswered_requirements_back() {
    let reqs = task_requirements(C24_TASK, true);
    let mut claim = c24_claim(false);
    claim.items.retain(|i| i.id != "R5");
    claim.remaining = vec!["document the rest of context.rs".to_string()];
    let v = verify_claim(&reqs, &claim, &c24_ledger(), &key);
    assert_eq!(v.status, DoneStatus::NotDone);
    let msg = feedback_message(&v, 12);
    assert!(msg.contains("document the rest of context.rs"), "{msg}");
    assert!(
        msg.contains("requirements not met: R5"),
        "an unanswered requirement is named: {msg}"
    );
    // With no list from the model, the requirement texts are the list.
    claim.remaining.clear();
    let v = verify_claim(&reqs, &claim, &c24_ledger(), &key);
    let msg = feedback_message(&v, 12);
    assert!(msg.contains("R5: In src/agent/context.rs"), "{msg}");
}

#[test]
fn done_with_a_requirement_marked_unmet_is_not_done() {
    let reqs = task_requirements(C24_TASK, true);
    let mut claim = c24_claim(true);
    claim.items[3].met = false;
    let v = verify_claim(&reqs, &claim, &c24_ledger(), &key);
    assert_eq!(v.status, DoneStatus::NotDone);
}

#[test]
fn no_change_on_a_mutation_task_blocks_done() {
    let reqs = task_requirements("Add a docstring to a.py", true);
    let claim = DoneClaim {
        done: true,
        items: vec![ClaimItem {
            id: "R1".into(),
            met: true,
            evidence: "a.py:1".into(),
        }],
        remaining: vec![],
        final_answer_ready: true,
        final_answer: Some("done".into()),
    };
    let v = verify_claim(
        &reqs,
        &claim,
        &Ledger {
            mutation_task: true,
            ..Ledger::default()
        },
        &key,
    );
    assert_eq!(v.status, DoneStatus::ClaimedUnverified);
    assert!(v.blockers[0].contains("no file was changed"));
}

#[test]
fn ledger_answer_names_only_verified_requirements() {
    let reqs = task_requirements(C24_TASK, true);
    let mut claim = c24_claim(true);
    claim.final_answer = None;
    let mut v = verify_claim(&reqs, &claim, &c24_ledger(), &key);
    v.items.truncate(5);
    let answer = ledger_answer(&v, 41);
    assert!(answer.contains("turn 41") && answer.contains("done-check"));
    assert!(answer.contains("R4") && !answer.contains("R6"));
}

// ── triggers ──

#[test]
fn cap_and_cooldown_bound_the_checks() {
    let mut s = DoneCheckState::default();
    let rec = |turn| DoneCheckRecord {
        turn,
        trigger: DoneTrigger::FinishStall,
        verdict: Ok("NOT DONE".to_string()),
        breakdown: None,
    };
    assert!(s.may_fire(DoneTrigger::FinishStall, 10, 3));
    s.record(rec(10), 3);
    assert!(!s.may_fire(DoneTrigger::NearCap, 11, 3), "cooldown");
    assert!(!s.may_fire(DoneTrigger::AtCap, 11, 3), "at cap, same tree");
    assert!(
        s.may_fire(DoneTrigger::AtCap, 11, 4),
        "at cap, tree changed"
    );
    assert!(s.may_fire(DoneTrigger::NearCap, 10 + DONE_CHECK_COOLDOWN_TURNS, 3));
    s.record(rec(20), 3);
    s.record(rec(30), 3);
    assert_eq!(s.asked(), MAX_DONE_CHECKS_PER_TASK);
    assert!(!s.may_fire(DoneTrigger::AtCap, 99, 9), "per-task cap");
}

#[test]
fn near_cap_window_is_the_tail_of_a_large_enough_budget_once_per_cap() {
    let mut s = DoneCheckState::default();
    assert!(!s.near_cap_due(36, 40));
    assert!(s.near_cap_due(38, 40));
    assert!(s.near_cap_due(40, 40));
    assert!(!s.near_cap_due(6, 7), "small caps never get the window");
    s.mark_near_cap(40);
    assert!(!s.near_cap_due(39, 40), "once per cap value");
    assert!(s.near_cap_due(48, 50), "an extension re-arms it");
}

#[test]
fn summary_says_what_was_verified_and_who_wrote_the_answer() {
    let mut s = DoneCheckState::default();
    assert!(s.summary_line().is_none());
    s.record(
        DoneCheckRecord {
            turn: 38,
            trigger: DoneTrigger::NearCap,
            verdict: Ok("VERIFIED DONE (6/6 requirement(s) with evidence)".into()),
            breakdown: Some("verified: R1, R2".into()),
        },
        2,
    );
    s.mark_completed(38, false);
    let line = s.summary_line().unwrap();
    assert!(line.starts_with("done-check: 1 asked"), "{line}");
    assert!(line.contains("verified DONE at turn 38"), "{line}");
    assert!(
        line.contains("semantic completeness not verified"),
        "{line}"
    );
    assert!(s.failure_clause().is_none());
    let report = s.report().unwrap();
    assert_eq!(report.verified_done_at, Some(38));
    assert!(!report.answer_synthesized);
}

#[test]
fn failure_clause_carries_the_breakdown() {
    let mut s = DoneCheckState::default();
    s.record(
        DoneCheckRecord {
            turn: 41,
            trigger: DoneTrigger::AtCap,
            verdict: Ok("NOT DONE (4 of 6 with evidence; 2 not met)".into()),
            breakdown: Some("verified: R1, R2, R3, R4; unverified: none; not met: R5, R6".into()),
        },
        2,
    );
    let clause = s.failure_clause().unwrap();
    assert!(
        clause.contains("turn 41") && clause.contains("not met: R5, R6"),
        "{clause}"
    );
}

// ── Agent runs against a scripted mock LLM ──

mod agent_runs {
    use crate::agent::Agent;
    use crate::config::Config;
    use crate::testing::mock_api::MockLlmServer;

    use super::super::{DONE_CHECK_MARKER, DONE_CHECK_SYSTEM};

    fn config(endpoint: String) -> Config {
        Config {
            endpoint,
            model: "mock-model".to_string(),
            context_length: 500_000,
            max_tokens: 8192,
            agent: crate::config::AgentConfig {
                // Off by default; these runs measure it switched on.
                done_check: true,
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

    struct Scratch {
        dir: std::path::PathBuf,
        rel: String,
    }

    impl Scratch {
        fn new(tag: &str) -> Self {
            let rel = format!("docs/.sw_done_check_{tag}_{}", std::process::id());
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

    /// Marker of a done-check request body (its system prompt).
    fn is_done_check_request(body: &str) -> bool {
        body.contains("You check whether a coding task is finished")
    }

    /// The c24 shape: the edit is written and verified, then the model only
    /// re-reads it. Responses up to the finish-stall detection (turn 6),
    /// with the step-5 reflection side call in between.
    fn stalled_run(file: &str) -> crate::testing::mock_api::MockLlmServerBuilder {
        let read = |range: Option<[u64; 2]>| match range {
            Some(r) => tool(
                "file_read",
                serde_json::json!({"path": file, "line_range": r}),
            ),
            None => tool("file_read", serde_json::json!({"path": file})),
        };
        let grep = |max: u64| {
            tool(
                "grep_search",
                serde_json::json!({"path": file, "pattern": "def ", "max_matches": max}),
            )
        };
        MockLlmServer::builder()
            .with_response(format!(
                "FILES: {file}\n\n{}",
                tool("file_write", serde_json::json!({"path": file, "content": BODY}))
            ))
            .with_response(tool(
                "shell_exec",
                serde_json::json!({"command": format!("python3 -m py_compile {file}")}),
            ))
            .with_response(read(None)) // new: first read-back
            .with_response(grep(10)) // seen 1
            .with_response(read(Some([1, 2]))) // seen 2
            .with_response("Reflection: re-reading the same file adds nothing.")
            .with_response(grep(11)) // seen 3 -> done-check requested
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn c24_stall_gets_one_done_check_and_a_verified_done_completes_with_its_answer() {
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("verified");
        let file = format!("{}/calc.py", scratch.rel);
        let answer = "Created calc.py with add and sub; py_compile passed.";
        let server = stalled_run(&file)
            .with_response(
                serde_json::json!({
                    "status": "DONE",
                    "requirements": [{"id": "R1", "met": true, "evidence": format!("{file}:1")}],
                    "remaining": [],
                    "final_answer_ready": true,
                    "final_answer": answer,
                })
                .to_string(),
            )
            // Never reached: the done-check ends the run.
            .with_response(tool("grep_search", serde_json::json!({"path": file, "pattern": "x"})))
            .build()
            .await;
        let mut agent = Agent::new(config(format!("{}/v1", server.url())))
            .await
            .unwrap();
        let task = format!(
            "Fix task: create {file} with add and sub functions, then verify it with python3 -m py_compile."
        );
        let result = agent.run_task(&task).await;
        let joined = history(&agent).join("\n---\n");
        assert!(result.is_ok(), "{:?}\n{joined}", result.err());
        assert_eq!(agent.last_assistant_response, answer);
        assert_eq!(agent.done_check.asked(), 1, "{joined}");
        assert!(agent.done_check.completed_at().is_some());
        assert!(
            !joined.contains(super::super::super::finish_stall::FINISH_STALL_DIRECTIVE_MARKER),
            "the directive is only the fallback:\n{joined}"
        );
        let bodies = server.captured_request_bodies().await;
        assert_eq!(
            bodies.iter().filter(|b| is_done_check_request(b)).count(),
            1
        );
        let summary = agent.run_summary();
        let line = summary.done_check.clone().unwrap_or_default();
        assert!(line.contains("verified DONE at turn"), "{line}");
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn a_false_done_claim_about_an_unchanged_file_is_not_accepted() {
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("false");
        let file = format!("{}/calc.py", scratch.rel);
        let other = format!("{}/other.py", scratch.rel);
        let server = stalled_run(&file)
            .with_response(
                serde_json::json!({
                    "status": "DONE",
                    "requirements": [
                        {"id": "R1", "met": true, "evidence": file},
                        {"id": "R2", "met": true, "evidence": format!("{other}:1")},
                        {"id": "R3", "met": true, "evidence": "python3 -m py_compile passed"},
                    ],
                    "remaining": [],
                    "final_answer_ready": true,
                    "final_answer": "Both files created.",
                })
                .to_string(),
            )
            .with_response("Final answer: only calc.py was created; other.py is still missing.")
            .build()
            .await;
        let mut agent = Agent::new(config(format!("{}/v1", server.url())))
            .await
            .unwrap();
        let task = format!(
            "Fix task:\n1. Create {file} with add and sub functions.\n2. Create {other} containing X = 1.\n3. Verify with python3 -m py_compile."
        );
        let result = agent.run_task(&task).await;
        let messages = history(&agent);
        let joined = messages.join("\n---\n");
        assert!(result.is_ok(), "{:?}\n{joined}", result.err());
        assert!(
            agent.done_check.completed_at().is_none(),
            "the false claim did not end the run:\n{joined}"
        );
        assert_ne!(agent.last_assistant_response, "Both files created.");
        let feedback = messages
            .iter()
            .find(|m| m.contains(DONE_CHECK_MARKER))
            .unwrap_or_else(|| panic!("no done-check feedback:\n{joined}"));
        assert!(
            feedback.contains("R2") && feedback.contains("other.py"),
            "{feedback}"
        );
        assert!(!feedback.contains("R1 ("), "R1 has evidence: {feedback}");
        let record = &agent.done_check.records()[0];
        assert!(
            record
                .verdict
                .as_ref()
                .is_ok_and(|l| l.starts_with("DONE claimed, NOT verified")),
            "{record:?}"
        );
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn not_done_feeds_remaining_back_as_the_next_focus() {
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("notdone");
        let file = format!("{}/calc.py", scratch.rel);
        let server = stalled_run(&file)
            .with_response(
                serde_json::json!({
                    "status": "NOT_DONE",
                    "requirements": [{"id": "R1", "met": false, "evidence": ""}],
                    "remaining": ["add a docstring to sub in calc.py"],
                    "final_answer_ready": false,
                    "final_answer": "",
                })
                .to_string(),
            )
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
        let feedback: Vec<&String> = messages
            .iter()
            .filter(|m| m.contains(DONE_CHECK_MARKER))
            .collect();
        assert_eq!(feedback.len(), 1, "one message:\n{joined}");
        assert!(
            feedback[0].contains("NOT finished")
                && feedback[0].contains("add a docstring to sub in calc.py"),
            "{}",
            feedback[0]
        );
        assert!(agent.done_check.completed_at().is_none());
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn a_run_that_answers_on_its_own_never_gets_a_done_check() {
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("normal");
        let file = format!("{}/calc.py", scratch.rel);
        let server = MockLlmServer::builder()
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
        let joined = history(&agent).join("\n---\n");
        assert!(result.is_ok(), "{:?}\n{joined}", result.err());
        assert_eq!(agent.done_check.asked(), 0);
        assert!(agent.run_summary().done_check.is_none());
        let bodies = server.captured_request_bodies().await;
        assert!(!bodies.iter().any(|b| is_done_check_request(b)));
        assert!(!joined.contains(DONE_CHECK_MARKER));
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn at_the_cap_a_verified_done_without_an_answer_completes_with_a_ledger_answer() {
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("cap");
        let file = format!("{}/calc.py", scratch.rel);
        let missing = |i: usize| {
            tool(
                "shell_exec",
                serde_json::json!({"command": format!("ls {}/missing{i}.py", scratch.rel)}),
            )
        };
        // Failing observational calls are not a productive streak (no
        // budget extension, no auto-continue), and each names a different
        // path, so its output is new (no finish stall).
        let server = MockLlmServer::builder()
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
            .with_response(missing(1))
            .with_response(missing(2))
            .with_response(missing(3))
            .with_response("Reflection: nothing new.")
            .with_response(missing(4))
            .with_response(missing(5))
            .with_default_response(crate::testing::mock_api::MockResponse::Text(
                serde_json::json!({
                    "status": "DONE",
                    "requirements": [{"id": "R1", "met": true, "evidence": file}],
                    "remaining": [],
                    "final_answer_ready": false,
                    "final_answer": "",
                })
                .to_string(),
            ))
            .build()
            .await;
        let mut cfg = config(format!("{}/v1", server.url()));
        cfg.agent.max_iterations = 6;
        let mut agent = Agent::new(cfg).await.unwrap();
        let task = format!(
            "Fix task: create {file} with add and sub functions, then verify it with python3 -m py_compile."
        );
        let result = agent.run_task(&task).await;
        let joined = history(&agent).join("\n---\n");
        assert!(result.is_ok(), "{:?}\n{joined}", result.err());
        assert!(
            agent.done_check.completed_at().is_some(),
            "{:?}\n{joined}",
            agent.done_check.records()
        );
        assert!(
            agent
                .last_assistant_response
                .contains("produced by the done-check"),
            "{}",
            agent.last_assistant_response
        );
        let report = agent.done_check_report().unwrap();
        assert!(report.answer_synthesized);
        assert!(report.line.contains("from the ledger"), "{}", report.line);
        let _ = DONE_CHECK_SYSTEM;
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn at_the_cap_a_verified_done_the_gate_refuses_fails_with_the_breakdown() {
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("cap");
        let file = format!("{}/calc.py", scratch.rel);
        let missing = |i: usize| {
            tool(
                "shell_exec",
                serde_json::json!({"command": format!("ls {}/missing{i}.py", scratch.rel)}),
            )
        };
        // Failing observational calls are not a productive streak (no
        // budget extension, no auto-continue), and each names a different
        // path, so its output is new (no finish stall).
        let server = MockLlmServer::builder()
            .with_response(format!(
                "FILES: {file}\n\n{}",
                tool("file_write", serde_json::json!({"path": file, "content": BODY}))
            ))
            // No check runs: a fresh green check at the cap is completed
            // by the existing cap completion gate, without a done-check.
            .with_response(missing(0))
            .with_response(missing(1))
            .with_response(missing(2))
            .with_response(missing(3))
            .with_response("Reflection: nothing new.")
            .with_response(missing(4))
            .with_response(missing(5))
            .with_default_response(crate::testing::mock_api::MockResponse::Text(
                serde_json::json!({
                    "status": "DONE",
                    "requirements": [{"id": "R1", "met": true, "evidence": file}],
                    "remaining": [],
                    "final_answer_ready": false,
                    "final_answer": "",
                })
                .to_string(),
            ))
            .build()
            .await;
        let mut cfg = config(format!("{}/v1", server.url()));
        cfg.agent.max_iterations = 6;
        let mut agent = Agent::new(cfg).await.unwrap();
        let task = format!("Fix task: create {file} with add and sub functions.");
        let result = agent.run_task(&task).await;
        let joined = history(&agent).join("\n---\n");
        assert!(result.is_err(), "the gate refused the answer:\n{joined}");
        assert!(agent.done_check.completed_at().is_none());
        let clause = agent.done_check_failure_clause().unwrap_or_default();
        assert!(
            clause.contains("VERIFIED DONE") && clause.contains("refused by the completion gate"),
            "{clause}"
        );
        let summary = agent.run_summary();
        assert!(
            summary
                .done_check
                .as_deref()
                .is_some_and(|l| l.contains("refused by the completion gate")),
            "{:?}",
            summary.done_check
        );
        server.stop().await;
    }

    /// The c24 stall with `[agent] done_check` at its default (off): the
    /// 0.9.5 behaviour, byte for byte in what reaches the model — no
    /// done-check request, the finish-stall directive at the same point, the
    /// same responses consumed in the same order, nothing new in the summary,
    /// the JSON result or the failure evidence.
    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn switched_off_the_stall_gets_the_0_9_5_directive_and_no_done_check() {
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("off");
        let file = format!("{}/calc.py", scratch.rel);
        let answer = "Final answer: calc.py has add and sub; py_compile passed.";
        // Exactly the responses the 0.9.5 run consumes: the stall, then the
        // answer right after the directive (no side call in between).
        let server = stalled_run(&file).with_response(answer).build().await;
        let mut cfg = config(format!("{}/v1", server.url()));
        cfg.agent.done_check = false;
        assert!(
            !crate::config::AgentConfig::default().done_check,
            "off by default"
        );
        let mut agent = Agent::new(cfg).await.unwrap();
        let task = format!(
            "Fix task: create {file} with add and sub functions, then verify it with python3 -m py_compile."
        );
        let result = agent.run_task(&task).await;
        let messages = history(&agent);
        let joined = messages.join("\n---\n");
        assert!(result.is_ok(), "{:?}\n{joined}", result.err());
        assert_eq!(agent.last_assistant_response, answer);
        // The directive fired once, as the last pushed message before the
        // answer turn.
        let marker = super::super::super::finish_stall::FINISH_STALL_DIRECTIVE_MARKER;
        assert_eq!(
            messages.iter().filter(|m| m.contains(marker)).count(),
            1,
            "{joined}"
        );
        assert!(!joined.contains(DONE_CHECK_MARKER), "{joined}");
        let bodies = server.captured_request_bodies().await;
        assert!(!bodies.iter().any(|b| is_done_check_request(b)));
        assert!(!bodies.iter().any(|b| b.contains("DONE-CHECK")));
        // 8 scripted responses (stall + answer) + the reflection = 8 requests:
        // nothing extra was sent.
        assert_eq!(bodies.len(), 8, "every request is one the 0.9.5 run made");
        // No side call went out with a workload thinking override.
        assert_eq!(
            crate::api::client::SideCall::new("x").thinking_workload,
            None
        );
        assert_eq!(agent.done_check.asked(), 0);
        let summary = agent.run_summary();
        assert!(summary.done_check.is_none());
        assert!(agent.done_check_report().is_none());
        assert!(agent.done_check_failure_clause().is_none());
        let detail = summary.finish_stall_detail.unwrap_or_default();
        assert!(
            detail.contains("told to give the final answer after turn"),
            "the 0.9.5 wording: {detail}"
        );
        server.stop().await;
    }

    /// Switched off, the iteration cap never asks either.
    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
    )]
    async fn switched_off_the_cap_fails_as_before_with_no_done_check() {
        let _g = crate::test_support::ExecGuard::hold();
        let scratch = Scratch::new("offcap");
        let file = format!("{}/calc.py", scratch.rel);
        let missing = |i: usize| {
            tool(
                "shell_exec",
                serde_json::json!({"command": format!("ls {}/missing{i}.py", scratch.rel)}),
            )
        };
        let mut builder = MockLlmServer::builder().with_response(format!(
            "FILES: {file}\n\n{}",
            tool(
                "file_write",
                serde_json::json!({"path": file, "content": BODY})
            )
        ));
        for i in 0..30 {
            builder = builder.with_response(missing(i));
        }
        let server = builder.build().await;
        let mut cfg = config(format!("{}/v1", server.url()));
        cfg.agent.done_check = false;
        cfg.agent.max_iterations = 12;
        let mut agent = Agent::new(cfg).await.unwrap();
        let result = agent
            .run_task(&format!(
                "Fix task: create {file} with add and sub functions."
            ))
            .await;
        assert!(result.is_err());
        let bodies = server.captured_request_bodies().await;
        assert!(!bodies.iter().any(|b| is_done_check_request(b)));
        assert_eq!(agent.done_check.asked(), 0);
        let fm = agent.last_run_failure_mode.clone().expect("classified");
        assert!(!fm.evidence.contains("done-check"), "{}", fm.evidence);
        server.stop().await;
    }
}

#[test]
fn a_requested_check_is_taken_once_with_its_fallback() {
    let mut s = DoneCheckState::default();
    assert!(s.take_pending().is_none());
    s.request(DoneTrigger::WrapUp, None);
    let (trigger, fallback) = s.take_pending().unwrap();
    assert_eq!(trigger, DoneTrigger::WrapUp);
    assert!(fallback.is_none());
    assert!(s.take_pending().is_none());
    s.request(DoneTrigger::FinishStall, Some("FINISH NOW".into()));
    assert_eq!(s.take_pending().unwrap().1.as_deref(), Some("FINISH NOW"));
    assert_eq!(
        DoneTrigger::WrapUp.label(),
        "at the deadline/budget wrap-up"
    );
}
