use super::*;
use std::fs;

/// A workspace with a few real files at known lines.
fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src/agent");
    fs::create_dir_all(&src).unwrap();
    let mut lib = String::new();
    for n in 1..=40 {
        match n {
            10 => lib.push_str("pub fn alpha_helper() -> u32 {\n"),
            25 => lib.push_str("pub struct BetaState {\n"),
            _ => lib.push_str(&format!("// line {n}\n")),
        }
    }
    fs::write(src.join("widget.rs"), lib).unwrap();
    fs::write(dir.path().join("README.md"), "# title\nsecond\nthird\n").unwrap();
    dir
}

fn cite(path: &str, start: usize, end: usize, symbol: Option<&str>) -> Citation {
    Citation {
        path: path.to_string(),
        start,
        end,
        symbol: symbol.map(str::to_string),
    }
}

// ── parser ───────────────────────────────────────────────────────────────

#[test]
fn parses_line_range_hash_and_dash_forms() {
    let text = "See src/a.rs:12 and `b.rs:3-9`, also c.py:4–7 (en dash), \
                d.go:5—6, docs/e.md#L8 and f.ts#L2-L4.";
    let got = parse_citations(text);
    assert_eq!(
        got,
        vec![
            cite("src/a.rs", 12, 12, None),
            cite("b.rs", 3, 9, None),
            cite("c.py", 4, 7, None),
            cite("d.go", 5, 6, None),
            cite("docs/e.md", 8, 8, None),
            cite("f.ts", 2, 4, None),
        ]
    );
}

#[test]
fn parses_symbol_next_to_citation() {
    let text = "Tested by `check_id_preserves` (`verification_scope.rs:1046-1078`) and \
                `second_test` at tests/x_test.rs:80. The `AgentDecision` enum \
                (`turn_artifacts.rs:22-36`: …), `MAX_LEN = 16_384` (`last_tool.rs:12`), \
                `FailureKind::Unknown` (`failure_mode.rs:99`), `render()` (`scope.rs:850-884`).";
    let got = parse_citations(text);
    let syms: Vec<(String, Option<String>)> = got
        .iter()
        .map(|c| (c.path.clone(), c.symbol.clone()))
        .collect();
    assert_eq!(
        syms,
        vec![
            (
                "verification_scope.rs".into(),
                Some("check_id_preserves".into())
            ),
            ("tests/x_test.rs".into(), Some("second_test".into())),
            ("turn_artifacts.rs".into(), Some("AgentDecision".into())),
            ("last_tool.rs".into(), Some("MAX_LEN".into())),
            ("failure_mode.rs".into(), Some("Unknown".into())),
            ("scope.rs".into(), Some("render".into())),
        ]
    );
}

#[test]
fn prose_and_expressions_name_no_symbol() {
    // A phrase / expression in a code span is not a checkable identifier,
    // and a code span separated from the citation by prose is not attached.
    let got = parse_citations(
        "the rule `blocked>=2 && x` (`f.rs:4`); run `cargo test --lib` (`g.rs:5`); \
         `far_away` is discussed later in text (`h.rs:6`)",
    );
    assert!(got.iter().all(|c| c.symbol.is_none()), "{got:?}");

    // Suffix shorthand and format strings (both seen in the evidence answer).
    let got = parse_citations(
        "`render_turn_decision_kv`/`_no_detail` (`progress_test.rs:44-72`); writes \
         `turn_{step:04}.json` (`turn_artifacts.rs:245`)",
    );
    assert_eq!(got.len(), 2);
    assert!(got.iter().all(|c| c.symbol.is_none()), "{got:?}");
}

#[test]
fn urls_hosts_and_non_citations_are_ignored() {
    let got = parse_citations(
        "https://example.com/src/a.rs:12 localhost:8080 example.com:443 12:30 \
         version 1.2.3 lines 1340-1400 foo.rs without line",
    );
    assert!(got.is_empty(), "{got:?}");
}

#[test]
fn duplicate_citations_are_counted_once() {
    let got = parse_citations("`a_fn` (`x.rs:1`) ... again `a_fn` (`x.rs:1`) and x.rs:1");
    assert_eq!(got.len(), 2, "{got:?}"); // with symbol, and bare
}

// ── verification verdicts ───────────────────────────────────────────────

#[test]
fn verified_wrong_line_missing_out_of_range_and_symbol_missing() {
    let ws = workspace();
    let mut r = CitationResolver::new(ws.path());

    // Verified: symbol at line 10, cited 9-12.
    assert_eq!(
        r.check(&cite("src/agent/widget.rs", 9, 12, Some("alpha_helper"))),
        CitationVerdict::Verified {
            file: "src/agent/widget.rs".into()
        }
    );
    // Within tolerance (cited 13, actual 10: 3 lines off).
    assert!(matches!(
        r.check(&cite("src/agent/widget.rs", 13, 13, Some("alpha_helper"))),
        CitationVerdict::Verified { .. }
    ));
    // Symbol moved: cited 30-35, actually at 10.
    assert_eq!(
        r.check(&cite("src/agent/widget.rs", 30, 35, Some("alpha_helper"))),
        CitationVerdict::WrongLine {
            file: "src/agent/widget.rs".into(),
            actual_line: 10
        }
    );
    // Missing file.
    assert_eq!(
        r.check(&cite("src/agent/nope.rs", 1, 2, Some("alpha_helper"))),
        CitationVerdict::MissingFile
    );
    // Out of range (file has 40 lines) and malformed range.
    assert_eq!(
        r.check(&cite("src/agent/widget.rs", 38, 90, None)),
        CitationVerdict::OutOfRange {
            file: "src/agent/widget.rs".into(),
            line_count: 40
        }
    );
    assert!(matches!(
        r.check(&cite("src/agent/widget.rs", 0, 0, None)),
        CitationVerdict::OutOfRange { .. }
    ));
    // Symbol not in the file at all.
    assert_eq!(
        r.check(&cite("src/agent/widget.rs", 1, 5, Some("gamma_missing"))),
        CitationVerdict::SymbolNotFound {
            file: "src/agent/widget.rs".into()
        }
    );
    // Range exists, no symbol: unverifiable (neither confirmed nor refuted).
    assert!(matches!(
        r.check(&cite("README.md", 1, 2, None)),
        CitationVerdict::Unverifiable { .. }
    ));
}

#[test]
fn bare_file_name_resolves_by_suffix() {
    let ws = workspace();
    let mut r = CitationResolver::new(ws.path());
    assert_eq!(
        r.check(&cite("widget.rs", 25, 25, Some("BetaState"))),
        CitationVerdict::Verified {
            file: "src/agent/widget.rs".into()
        }
    );
    assert_eq!(
        r.check(&cite("agent/widget.rs", 1, 3, Some("BetaState"))),
        CitationVerdict::WrongLine {
            file: "src/agent/widget.rs".into(),
            actual_line: 25
        }
    );
}

#[test]
fn ambiguous_bare_name_is_unverifiable_unless_the_text_names_the_path() {
    let ws = workspace();
    let other = ws.path().join("tests");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("widget.rs"), "one\ntwo\n").unwrap();

    let mut r = CitationResolver::new(ws.path());
    let report = r.verify_text("`BetaState` (`widget.rs:1-2`)", ANSWER_SOURCE);
    assert_eq!(report.unverifiable, 1, "{report:?}");
    assert_eq!(report.problem_count(), 0);

    // Naming the full path elsewhere in the answer disambiguates.
    let report = r.verify_text(
        "Area: src/agent/widget.rs. `BetaState` (`widget.rs:1-2`)",
        ANSWER_SOURCE,
    );
    assert_eq!(report.wrong_line.len(), 1, "{report:?}");
}

#[test]
fn locate_symbol_prefers_the_definition() {
    let lines: Vec<String> = ["call foo_bar();", "", "fn foo_bar() {", "}"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(locate_symbol(&lines, "foo_bar"), Some(3));
    assert_eq!(locate_symbol(&lines, "foo"), None, "whole words only");
}

#[test]
fn report_counts_and_grounding_line() {
    let ws = workspace();
    let mut r = CitationResolver::new(ws.path());
    let report = r.verify_text(
        "`alpha_helper` (`widget.rs:10`), `BetaState` (`widget.rs:3`), \
         `x_fn` (`gone.rs:1`), `widget.rs:99`, `README.md:2`",
        ANSWER_SOURCE,
    );
    assert_eq!(report.total, 5);
    assert_eq!(report.verified, 1);
    assert_eq!(report.wrong_line.len(), 1);
    assert_eq!(report.missing_file.len(), 1);
    assert_eq!(report.out_of_range.len(), 1);
    assert_eq!(report.unverifiable, 1);
    assert_eq!(report.problem_count(), 3);

    let status = GroundingStatus::from_report(&report, 2, vec![]);
    assert_eq!(status.unverified_count(), 4);
    assert_eq!(
        status.grounding_line(),
        "Grounding: 1 verified citations, 4 unverified (3 wrong, 1 without a checkable symbol)"
    );
    assert_eq!(
        status.unverified_note(),
        "citations: 3 of 5 could not be verified"
    );
    assert!(status.problems[0].contains("but found at src/agent/widget.rs:25"));
}

#[test]
fn deliverable_paths_are_doc_like_only() {
    assert!(is_deliverable_path("REVIEW.md"));
    assert!(is_deliverable_path("out/notes.TXT"));
    assert!(!is_deliverable_path("src/lib.rs"));
    assert!(!is_deliverable_path("Makefile"));
}

// ── the evidence case (context-validation run 2026-09-24) ───────────────

/// A workspace laid out like the reviewed checkout, holding fixture copies
/// (at checkout 686810b2) of the two files the wrong citations point into.
fn evidence_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let scope = dir.path().join("src/agent");
    let fm = dir.path().join("tests/unit/agent/failure_mode");
    fs::create_dir_all(&scope).unwrap();
    fs::create_dir_all(&fm).unwrap();
    fs::write(
        scope.join("verification_scope.rs"),
        include_str!("fixtures/verification_scope.rs.txt"),
    )
    .unwrap();
    fs::write(
        fm.join("failure_mode_test.rs"),
        include_str!("fixtures/failure_mode_test.rs.txt"),
    )
    .unwrap();
    dir
}

#[test]
fn evidence_review_answer_is_flagged_with_the_actual_lines() {
    let ws = evidence_workspace();
    let answer = include_str!("fixtures/review_answer_350000.md");
    let mut r = CitationResolver::new(ws.path());
    let report = r.verify_text(answer, ANSWER_SOURCE);

    let wrong = |sym: &str| {
        report
            .wrong_line
            .iter()
            .find(|c| c.citation.symbol.as_deref() == Some(sym))
            .unwrap_or_else(|| panic!("{sym} not flagged: {:#?}", report.wrong_line))
            .clone()
    };
    let a = wrong("check_id_preserves_test_selectors_and_drops_flags");
    assert_eq!((a.citation.start, a.citation.end), (1046, 1078));
    assert_eq!(
        a.verdict,
        CitationVerdict::WrongLine {
            file: "src/agent/verification_scope.rs".into(),
            actual_line: 493
        }
    );
    let b = wrong("check_id_keeps_every_subset_selector_and_strips_redirections");
    assert_eq!((b.citation.start, b.citation.end), (1080, 1110));
    assert_eq!(
        b.verdict,
        CitationVerdict::WrongLine {
            file: "src/agent/verification_scope.rs".into(),
            actual_line: 556
        }
    );
    let c = wrong("scattered_safety_blocks_do_not_relabel_a_real_timeout");
    assert_eq!((c.citation.start, c.citation.end), (300, 326));
    assert_eq!(
        c.verdict,
        CitationVerdict::WrongLine {
            file: "tests/unit/agent/failure_mode/failure_mode_test.rs".into(),
            actual_line: 475
        }
    );
    // The directive names the correction the model must make.
    let directive = correction_directive(&report, 1);
    assert!(directive.contains(
        "`check_id_preserves_test_selectors_and_drops_flags` cited at \
         verification_scope.rs:1046-1078 but found at src/agent/verification_scope.rs:493"
    ));
    assert!(report.problem_count() > 0);
}

// ── gate: bounded correction rounds, then an explicit warning ───────────

async fn gate_agent(root: &Path) -> crate::agent::Agent {
    let mut config = crate::config::Config::default();
    config.agent.min_completion_steps = 0;
    let mut agent = crate::agent::Agent::new(config)
        .await
        .expect("agent should build");
    agent
        .tools
        .set_workspace_root(crate::tools::workspace_root::WorkspaceRoot::fixed(root));
    agent.task_is_read_only = true;
    agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
        "cite".to_string(),
        "Review src/agent and report findings. Do not edit files.".to_string(),
    ));
    agent
}

fn answer(agent: &mut crate::agent::Agent, step: usize, text: &str) {
    agent.loop_control.restore_progress(step, step);
    agent
        .messages
        .push(crate::api::types::Message::assistant(text.to_string()));
}

const WRONG_ANSWER: &str = "Findings: `alpha_helper` (`src/agent/widget.rs:30-35`) \
    returns a u32; `BetaState` (`widget.rs:25`) holds state.";
const FIXED_ANSWER: &str = "Findings: `alpha_helper` (`src/agent/widget.rs:10`) \
    returns a u32; `BetaState` (`widget.rs:25`) holds state.";

#[tokio::test]
async fn gate_feeds_back_wrong_citations_and_accepts_the_fix() {
    let ws = workspace();
    let mut agent = gate_agent(ws.path()).await;
    answer(&mut agent, 1, WRONG_ANSWER);
    let directive = agent
        .citation_gate(true)
        .expect("wrong citation must block");
    assert!(directive.contains("CITATION CHECK"));
    assert!(directive.contains(
        "`alpha_helper` cited at src/agent/widget.rs:30-35 but found at src/agent/widget.rs:10"
    ));
    // Repeat probes in the same turn do not spend another round.
    assert!(agent.citation_gate(true).is_some());
    assert_eq!(agent.citation_gate.lock().unwrap().rejections, 1);

    answer(&mut agent, 2, FIXED_ANSWER);
    assert_eq!(agent.citation_gate(true), None);
    let status = agent.grounding_status().expect("status recorded");
    assert_eq!(
        (status.total, status.verified, status.problem_count()),
        (2, 2, 0)
    );
    assert_eq!(status.correction_rounds, 1);
}

#[tokio::test]
async fn gate_is_bounded_then_steps_aside_with_the_count_recorded() {
    let ws = workspace();
    let mut agent = gate_agent(ws.path()).await;
    for round in 1..=CITATION_GATE_REJECTION_BOUND {
        answer(&mut agent, round, WRONG_ANSWER);
        assert!(
            agent.citation_gate(true).is_some(),
            "round {round} must still block"
        );
    }
    answer(&mut agent, CITATION_GATE_REJECTION_BOUND + 1, WRONG_ANSWER);
    assert_eq!(
        agent.citation_gate(true),
        None,
        "past the bound the gate steps aside"
    );
    let status = agent.grounding_status().expect("status recorded");
    assert_eq!(status.problem_count(), 1);
    assert_eq!(status.correction_rounds, CITATION_GATE_REJECTION_BOUND);
    assert_eq!(
        status.unverified_note(),
        "citations: 1 of 2 could not be verified"
    );
    // Through the full completion gate as well: no further rejection.
    assert_eq!(agent.check_completion_gate().await, None);

    // The verdict is not a clean pass: the banner warns and names the count.
    let base = crate::agent::failure_mode::FailureMode {
        restored_files: Vec::new(),
        kind: crate::agent::failure_mode::FailureKind::NoChange,
        evidence: "completed naturally with 0 mutating tool calls".to_string(),
        advice: "-".to_string(),
    };
    let fm = crate::agent::failure_mode::with_citation_status(base, Some(&status));
    assert!(fm
        .evidence
        .contains("citations: 1 of 2 could not be verified"));
    let banner = fm.cli_banner();
    assert!(banner.starts_with("⚠️"), "{banner}");
    assert!(!banner.contains('✅'), "{banner}");

    // A new task starts clean.
    agent.reset_citation_gate();
    assert!(agent.grounding_status().is_none());
}

#[tokio::test]
async fn gate_checks_citations_in_written_deliverables() {
    let ws = workspace();
    let mut agent = gate_agent(ws.path()).await;
    fs::write(
        ws.path().join("REVIEW.md"),
        "# Review\n`BetaState` (`src/agent/widget.rs:2`) is the state.\n",
    )
    .unwrap();
    agent
        .current_checkpoint
        .as_mut()
        .unwrap()
        .log_tool_call(crate::checkpoint::ToolCallLog {
            timestamp: chrono::Utc::now(),
            tool_name: "file_write".to_string(),
            arguments: serde_json::json!({"path": "REVIEW.md", "content": "..."}).to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(1),
        });
    answer(&mut agent, 1, "Wrote the review to REVIEW.md.");
    let directive = agent
        .citation_gate(true)
        .expect("the deliverable's wrong citation blocks");
    assert!(directive.contains("found at src/agent/widget.rs:25 [in REVIEW.md]"));
    let status = agent.grounding_status().unwrap();
    assert_eq!(status.checked_files, vec!["REVIEW.md".to_string()]);
}

#[tokio::test]
async fn gate_ignores_uncited_mutation_answers_but_labels_read_only_ones() {
    let ws = workspace();
    let mut agent = gate_agent(ws.path()).await;
    answer(
        &mut agent,
        1,
        "Done: the helper now returns early on empty input.",
    );
    assert_eq!(agent.citation_gate(false), None);
    assert!(agent.grounding_status().is_none());

    answer(&mut agent, 2, "The module looks fine; nothing to report.");
    assert_eq!(agent.citation_gate(true), None);
    let status = agent
        .grounding_status()
        .expect("read-only answer is labelled");
    assert_eq!(status.total, 0);
    assert!(status.grounding_line().contains("no path:line citations"));
}
