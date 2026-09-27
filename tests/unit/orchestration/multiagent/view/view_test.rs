use super::*;
use crate::api::types::Usage;

fn roles() -> Vec<AgentRole> {
    vec![AgentRole::Architect, AgentRole::Coder, AgentRole::Tester]
}

fn result(agent_id: usize, role: AgentRole, success: bool, content: &str) -> AgentResult {
    AgentResult {
        agent_id,
        agent_name: format!("Agent-{}-{}", agent_id, role.name()),
        role,
        content: content.to_string(),
        usage: success.then(|| Usage {
            prompt_tokens: 100,
            completion_tokens: 23,
            total_tokens: 123,
            ..Usage::default()
        }),
        duration: Duration::from_millis(1500),
        success,
        error: (!success).then(|| "boom".to_string()),
    }
}

// ── tags ────────────────────────────────────────────────────────────────

#[test]
fn agent_tag_is_role_and_one_based_number() {
    assert_eq!(agent_tag(1, AgentRole::Coder), "coder·2");
    assert_eq!(agent_tag(0, AgentRole::Architect), "architect·1");
    // Stable: same input, same tag and colour.
    assert_eq!(
        agent_tag(1, AgentRole::Coder),
        agent_tag(1, AgentRole::Coder)
    );
    assert_eq!(agent_color(3), agent_color(3));
    assert_eq!(agent_color_index(AGENT_COLOR_COUNT + 2), 2);
}

// ── per-agent buffering ─────────────────────────────────────────────────

#[test]
fn line_buffer_never_emits_a_partial_line() {
    let mut buf = AgentLineBuffer::new();
    assert!(buf.push(0, "hello wo").is_empty());
    assert!(buf.has_pending(0));
    assert_eq!(buf.push(0, "rld\nsecond"), vec!["hello world".to_string()]);
    assert_eq!(buf.flush(0), Some("second".to_string()));
    assert_eq!(buf.flush(0), None);
}

#[test]
fn interleaved_chunks_from_two_agents_stay_whole_per_line() {
    // Two agents' chunks arrive interleaved, split mid-line. Every emitted
    // line must be one agent's complete line — never a splice of both.
    let mut view = MultiChatView::from_roles(&roles());
    let chunks: [(usize, &str); 6] = [
        (0, "architect line one, part A "),
        (1, "coder line one, part A "),
        (0, "part B\narchitect line "),
        (1, "part B\r\ncoder line two"),
        (0, "two\n"),
        (1, "\n"),
    ];
    let mut emitted = Vec::new();
    for (id, chunk) in chunks {
        emitted.extend(view.push_output(id, chunk));
    }
    assert_eq!(
        emitted,
        vec![
            AgentLine {
                agent_id: 0,
                text: "architect line one, part A part B".into()
            },
            AgentLine {
                agent_id: 1,
                text: "coder line one, part A part B".into()
            },
            AgentLine {
                agent_id: 0,
                text: "architect line two".into()
            },
            AgentLine {
                agent_id: 1,
                text: "coder line two".into()
            },
        ]
    );
    // Each agent's own log is chronological and contains only its lines.
    assert_eq!(
        view.panels()[0].lines,
        vec!["architect line one, part A part B", "architect line two"]
    );
    assert_eq!(
        view.panels()[1].lines,
        vec!["coder line one, part A part B", "coder line two"]
    );
}

#[test]
fn completed_reply_becomes_whole_lines_in_that_agents_log() {
    let mut view = MultiChatView::from_roles(&roles());
    let now = Instant::now();
    view.apply(
        &MultiAgentEvent::AgentStarted {
            agent_id: 1,
            name: "Agent-1-Coder".into(),
            task: "t".into(),
        },
        now,
    );
    let lines = view.apply(
        &MultiAgentEvent::AgentCompleted {
            agent_id: 1,
            result: result(1, AgentRole::Coder, true, "first\nsecond (no newline)"),
        },
        now,
    );
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "first",
            "second (no newline)",
            "✓ done in 1.50s · 123 tokens"
        ]
    );
    assert!(lines.iter().all(|l| l.agent_id == 1));
    let coder = &view.panels()[1];
    assert_eq!(coder.status, AgentStatus::Completed);
    assert_eq!(coder.tokens, Some(123));
    assert_eq!(coder.lines[0], "▶ started");
    assert_eq!(coder.last_activity, "replied (2 lines)");
    // Other agents untouched.
    assert!(view.panels()[0].lines.is_empty());
}

#[test]
fn failure_is_reported_once_and_tokens_stay_unmeasured() {
    // The chat emits AgentFailed and then AgentCompleted(success=false) for
    // an API error; the log must say "failed" once, and tokens stay "—".
    let mut view = MultiChatView::from_roles(&roles());
    let now = Instant::now();
    let mut lines = view.apply(
        &MultiAgentEvent::AgentFailed {
            agent_id: 2,
            error: "boom".into(),
        },
        now,
    );
    lines.extend(view.apply(
        &MultiAgentEvent::AgentCompleted {
            agent_id: 2,
            result: result(2, AgentRole::Tester, false, ""),
        },
        now,
    ));
    let failed: Vec<_> = lines.iter().filter(|l| l.text.contains("failed")).collect();
    assert_eq!(failed.len(), 1);
    let tester = &view.panels()[2];
    assert_eq!(tester.status, AgentStatus::Failed);
    assert_eq!(tester.tokens, None);
    let status = MultiChatView::status_line(tester, now);
    assert!(status.contains(NOT_MEASURED), "{status}");
    assert!(status.contains("failed"), "{status}");
}

#[test]
fn status_line_shows_role_state_elapsed_tokens_activity() {
    let mut view = MultiChatView::from_roles(&roles());
    let t0 = Instant::now();
    view.apply(
        &MultiAgentEvent::AgentStarted {
            agent_id: 0,
            name: "Agent-0-Architect".into(),
            task: "t".into(),
        },
        t0,
    );
    let line = MultiChatView::status_line(&view.panels()[0], t0 + Duration::from_secs(2));
    assert!(line.starts_with("architect·1"), "{line}");
    assert!(line.contains("Architect"), "{line}");
    assert!(line.contains("working"), "{line}");
    assert!(line.contains("2.00s"), "{line}");
    assert!(line.contains("waiting for response"), "{line}");
    // No usage yet → not measured, never an estimate.
    assert!(line.contains(NOT_MEASURED), "{line}");
}

#[test]
fn begin_task_separates_runs_in_every_agent_log() {
    let mut view = MultiChatView::from_roles(&roles());
    let lines = view.begin_task("design a cache");
    assert_eq!(lines.len(), 3);
    for panel in view.panels() {
        assert_eq!(panel.lines, vec!["── task: design a cache"]);
        assert_eq!(panel.status, AgentStatus::Idle);
    }
}

// ── tab switching ───────────────────────────────────────────────────────

#[test]
fn number_keys_select_tabs_and_ignore_out_of_range() {
    let mut view = MultiChatView::from_roles(&roles());
    assert_eq!(view.tab(), ViewTab::All);
    assert!(view.select_number(2));
    assert_eq!(view.tab(), ViewTab::Agent(1));
    assert_eq!(view.tab_index(), 2);
    assert!(!view.select_number(4), "only 3 agents");
    assert_eq!(view.tab(), ViewTab::Agent(1));
    assert!(view.select_number(0));
    assert_eq!(view.tab(), ViewTab::All);
}

#[test]
fn tab_and_shift_tab_cycle_through_all_and_agents() {
    let mut view = MultiChatView::from_roles(&roles());
    let mut seen = vec![view.tab()];
    for _ in 0..4 {
        view.next_tab();
        seen.push(view.tab());
    }
    assert_eq!(
        seen,
        vec![
            ViewTab::All,
            ViewTab::Agent(0),
            ViewTab::Agent(1),
            ViewTab::Agent(2),
            ViewTab::All
        ]
    );
    view.prev_tab();
    assert_eq!(view.tab(), ViewTab::Agent(2));
    view.prev_tab();
    view.prev_tab();
    assert_eq!(view.tab(), ViewTab::Agent(0));
    view.prev_tab();
    assert_eq!(view.tab(), ViewTab::All);
}

#[test]
fn tabs_with_no_agents_stay_on_all() {
    let mut view = MultiChatView::from_roles(&[]);
    view.next_tab();
    assert_eq!(view.tab(), ViewTab::All);
    view.prev_tab();
    assert_eq!(view.tab(), ViewTab::All);
    assert!(!view.select_number(1));
}

#[test]
fn tab_titles_number_matches_tag() {
    let view = MultiChatView::from_roles(&roles());
    assert_eq!(
        view.tab_titles(),
        vec!["0 All", "1 architect·1", "2 coder·2", "3 tester·3"]
    );
}

// ── summary table ───────────────────────────────────────────────────────

#[test]
fn summary_table_uses_measured_values_only() {
    let mut skipped = result(2, AgentRole::Tester, false, "");
    skipped.error = Some("skipped to stay within --max-budget-tokens=10".into());
    // Completion order, not id order — the table sorts by agent.
    let results = vec![
        skipped,
        result(1, AgentRole::Coder, false, ""),
        result(0, AgentRole::Architect, true, "ok"),
    ];
    let rows = summary_table_rows(&results);
    assert_eq!(
        rows[0],
        [
            "architect·1".to_string(),
            "Architect".into(),
            "ok".into(),
            "1.50s".into(),
            "123 (100+23)".into()
        ]
    );
    assert_eq!(rows[1][2], "failed");
    assert_eq!(rows[1][4], NOT_MEASURED);
    assert_eq!(rows[2][2], "skipped");
    assert_eq!(rows[2][4], NOT_MEASURED);

    let table = render_summary_table(&results);
    assert_eq!(table.len(), 2 + 3);
    assert!(table[0].starts_with("agent"));
    // Every row is aligned: the duration column ends at the same char offset.
    let col_end = |line: &str, needle: &str| {
        let byte = line.find(needle).unwrap() + needle.len();
        line[..byte].chars().count()
    };
    assert_eq!(col_end(&table[2], "1.50s"), col_end(&table[3], "1.50s"));
    assert_eq!(col_end(&table[0], "duration"), col_end(&table[2], "1.50s"));
}

#[test]
fn format_duration_switches_to_minutes() {
    assert_eq!(format_duration(Duration::from_millis(3210)), "3.21s");
    assert_eq!(format_duration(Duration::from_secs(125)), "2m05s");
}
