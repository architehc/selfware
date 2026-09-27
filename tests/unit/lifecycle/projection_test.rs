use super::*;

fn r(id: &str, ts: &str, from: Option<&str>, to: &str, event: Option<&str>) -> TransitionRecord {
    let mut rec = TransitionRecord::now(Entity::Task, id, from, to, event, "why");
    rec.ts = ts.to_string();
    rec
}

fn sample() -> Vec<TransitionRecord> {
    let mut v = vec![
        r("aaaa-1", "2026-09-26T10:00:00.000Z", None, "queued", None),
        r(
            "aaaa-1",
            "2026-09-26T10:00:00.250Z",
            Some("queued"),
            "planning",
            Some("start"),
        ),
        r("bbbb-2", "2026-09-26T10:05:00.000Z", None, "queued", None),
        r(
            "aaaa-1",
            "2026-09-26T10:01:05.250Z",
            Some("planning"),
            "failed",
            Some("fail"),
        ),
    ];
    v[3].task_type = Some("bug_fix".into());
    // A resource record is not a task.
    let mut res = r("aaaa-9", "2026-09-26T11:00:00.000Z", None, "live", None);
    res.entity = Entity::Resource;
    v.push(res);
    v
}

#[test]
fn summaries_show_the_last_recorded_state_most_recent_first() {
    let s = task_summaries(&sample());
    assert_eq!(s.len(), 2, "resources are not tasks");
    assert_eq!(s[0].id, "bbbb-2");
    assert_eq!(s[0].state, "queued");
    assert_eq!(s[0].task_type, None, "no type recorded, none shown");
    assert_eq!(s[1].id, "aaaa-1");
    assert_eq!(s[1].state, "failed");
    assert_eq!(s[1].records, 3);
    assert_eq!(s[1].first_ts, "2026-09-26T10:00:00.000Z");
    assert_eq!(s[1].last_ts, "2026-09-26T10:01:05.250Z");
    assert_eq!(s[1].task_type.as_deref(), Some("bug_fix"));
}

#[test]
fn list_respects_the_limit_and_says_what_it_hid() {
    let s = task_summaries(&sample());
    let out = render_task_list(&s, 1);
    assert!(out.starts_with("ID"));
    assert!(out.contains("bbbb-2"));
    assert!(!out.contains("aaaa-1"));
    assert!(out.contains("(1 more; use --limit to show them)"), "{out}");
    let all = render_task_list(&s, 10);
    assert!(all.contains("bug_fix") && all.contains("failed"));
    assert!(!all.contains("more;"));
}

#[test]
fn resolves_full_ids_and_unique_prefixes() {
    let recs = sample();
    assert_eq!(resolve_task_id(&recs, "aaaa-1"), Ok("aaaa-1".to_string()));
    assert_eq!(resolve_task_id(&recs, "bb"), Ok("bbbb-2".to_string()));
    assert_eq!(resolve_task_id(&recs, "zz"), Err(ResolveError::NotFound));
    assert_eq!(resolve_task_id(&recs, ""), Err(ResolveError::NotFound));
    // "aaaa-9" is a resource: it never resolves as a task.
    assert_eq!(
        resolve_task_id(&recs, "aaaa-9"),
        Err(ResolveError::NotFound)
    );
    let mut more = recs.clone();
    more.push(r(
        "aaaa-3",
        "2026-09-26T12:00:00.000Z",
        None,
        "queued",
        None,
    ));
    assert_eq!(
        resolve_task_id(&more, "aaaa"),
        Err(ResolveError::Ambiguous(vec![
            "aaaa-1".to_string(),
            "aaaa-3".to_string()
        ]))
    );
}

#[test]
fn timeline_lists_transitions_with_measured_gaps() {
    let recs = sample();
    let tl = task_timeline(&recs, "aaaa-1");
    assert_eq!(tl.len(), 3);
    let out = render_timeline("aaaa-1", &tl);
    assert!(out.contains("type: bug_fix"), "{out}");
    assert!(
        out.contains("state: failed (at 2026-09-26T10:01:05.250Z)"),
        "{out}"
    );
    assert!(out.contains("(new) → queued"), "{out}");
    assert!(out.contains("queued → planning"), "{out}");
    assert!(out.contains("[start]"), "{out}");
    assert!(out.contains("+250ms"), "{out}");
    assert!(out.contains("+1m05s"), "{out}");
}

#[test]
fn command_outputs_say_where_they_read_and_what_they_did_not_find() {
    let dir = tempfile::tempdir().unwrap();
    let log = crate::lifecycle::EventLog::at(dir.path().join("events.jsonl"));
    let empty = tasks_command_output(&log, 20, false);
    assert!(
        empty.starts_with("No task transitions recorded in "),
        "{empty}"
    );
    assert!(task_show_output(&log, "abc")
        .unwrap_err()
        .starts_with("No task matching `abc`"));

    for rec in sample() {
        log.append(&rec, false);
    }
    let list = tasks_command_output(&log, 20, false);
    assert!(list.contains("aaaa-1") && list.contains("bbbb-2"), "{list}");
    assert!(list.contains("(from "), "{list}");
    let shown = task_show_output(&log, "aaaa-1").unwrap();
    assert!(shown.starts_with("Task aaaa-1"), "{shown}");

    let disabled = tasks_command_output(&crate::lifecycle::EventLog::disabled(), 5, false);
    assert!(
        disabled.contains("disabled via SELFWARE_EVENT_LOG"),
        "{disabled}"
    );
}

fn forked_sample() -> Vec<TransitionRecord> {
    let mut v = sample();
    let mut fork = r("cccc-3", "2026-09-26T12:00:00.000Z", None, "queued", None);
    fork.parent = Some("aaaa-1".into());
    v.push(fork);
    let mut done = r(
        "cccc-3",
        "2026-09-26T12:03:00.000Z",
        Some("executing"),
        "completed",
        Some("succeed"),
    );
    done.parent = Some("aaaa-1".into());
    done.usage = Some(crate::lifecycle::RecordedUsage {
        total_tokens: 212_000,
        main_tokens: Some(177_000),
        side_tokens: Some(35_000),
        cost_usd: None,
        cost_complete: false,
    });
    v.push(done);
    v
}

#[test]
fn tree_nests_forks_under_their_original() {
    let s = task_summaries(&forked_sample());
    let fork = s.iter().find(|t| t.id == "cccc-3").unwrap();
    assert_eq!(fork.parent.as_deref(), Some("aaaa-1"));
    assert_eq!(fork.usage.unwrap().total_tokens, 212_000);
    let tree = render_task_tree(&s, 10);
    let lines: Vec<&str> = tree.lines().collect();
    let orig = lines.iter().position(|l| l.starts_with("aaaa-1")).unwrap();
    assert!(lines[orig + 1].contains("└─ cccc-3"), "{tree}");
    assert_eq!(
        lines.iter().filter(|l| l.contains("cccc-3")).count(),
        1,
        "a fork is listed once, under its parent: {tree}"
    );
}

#[test]
fn a_parent_cycle_in_a_malformed_log_terminates() {
    let mut v = forked_sample();
    let mut back = r("aaaa-1", "2026-09-26T13:00:00.000Z", None, "queued", None);
    back.parent = Some("cccc-3".into());
    v.push(back);
    let s = task_summaries(&v);
    // Both have parents inside the log, so neither is a root; rendering
    // terminates and the unrelated task is still listed.
    let tree = render_task_tree(&s, 10);
    assert!(tree.contains("bbbb-2"), "{tree}");
    assert!(tree.lines().count() <= 3, "{tree}");
}

#[test]
fn show_names_parent_usage_and_forks_and_says_when_cost_was_not_reported() {
    let v = forked_sample();
    let timeline = task_timeline(&v, "cccc-3");
    let out = render_timeline("cccc-3", &timeline);
    assert!(out.contains("forked from: aaaa-1"), "{out}");
    assert!(out.contains("212000 (main 177000 · side 35000)"), "{out}");
    assert!(out.contains("cost: not reported"), "{out}");
    let (tokens, cost) = usage_line(None);
    assert_eq!(tokens, "not recorded");
    assert_eq!(cost, "not reported");
    let partial = crate::lifecycle::RecordedUsage {
        total_tokens: 10,
        main_tokens: None,
        side_tokens: None,
        cost_usd: Some(0.5),
        cost_complete: false,
    };
    let (tokens, cost) = usage_line(Some(&partial));
    assert!(tokens.contains("split not measured"));
    assert!(cost.starts_with("≥ $0.5000"));
}
// ---- agents ----------------------------------------------------------------

fn agent_rec(id: &str, ts: &str, to: &str, pid: u32) -> TransitionRecord {
    let mut rec = TransitionRecord::now(Entity::Agent, id, None, to, None, "why");
    rec.ts = ts.to_string();
    rec.pid = Some(pid);
    rec.agent_type = Some("main".into());
    rec
}

fn task_rec(id: &str, ts: &str, to: &str, owner: &str, tokens: Option<u64>) -> TransitionRecord {
    let mut rec = r(id, ts, None, to, None);
    rec.owner = Some(owner.to_string());
    rec.usage = tokens.map(|t| crate::lifecycle::RecordedUsage {
        total_tokens: t as usize,
        main_tokens: None,
        side_tokens: None,
        cost_usd: None,
        cost_complete: false,
    });
    rec.task_type = Some(format!("type-of-{id}"));
    rec
}

fn agent_sample() -> Vec<TransitionRecord> {
    vec![
        agent_rec("main-aaaa", "2026-09-26T10:00:00.000Z", "idle", 11),
        task_rec(
            "t1",
            "2026-09-26T10:00:01.000Z",
            "queued",
            "main-aaaa",
            None,
        ),
        agent_rec("main-aaaa", "2026-09-26T10:00:01.100Z", "working", 11),
        task_rec(
            "t1",
            "2026-09-26T10:02:00.000Z",
            "completed",
            "main-aaaa",
            Some(1200),
        ),
        agent_rec("main-aaaa", "2026-09-26T10:02:00.100Z", "idle", 11),
        // Interrupted, then resumed by the same agent and failed: counted
        // once, by its last segment, with the last terminal token total.
        task_rec(
            "t2",
            "2026-09-26T10:03:00.000Z",
            "interrupted",
            "main-aaaa",
            Some(50),
        ),
        task_rec(
            "t2",
            "2026-09-26T10:04:00.000Z",
            "failed",
            "main-aaaa",
            Some(300),
        ),
        // A provider that reported no usage: not recorded, not zero.
        task_rec(
            "t3",
            "2026-09-26T10:05:00.000Z",
            "completed",
            "main-aaaa",
            None,
        ),
        agent_rec("main-aaaa", "2026-09-26T10:05:00.100Z", "idle", 11),
        // Another agent whose process is gone while it was working.
        agent_rec("main-bbbb", "2026-09-26T09:00:00.000Z", "idle", 22),
        agent_rec("main-bbbb", "2026-09-26T09:00:01.000Z", "working", 22),
        task_rec(
            "t9",
            "2026-09-26T09:00:00.500Z",
            "planning",
            "main-bbbb",
            None,
        ),
    ]
}

#[test]
fn agent_summaries_count_tasks_by_their_last_segment_and_sum_recorded_tokens() {
    let s = agent_summaries(&agent_sample());
    assert_eq!(s.len(), 2);
    let a = &s[0];
    assert_eq!(a.id, "main-aaaa", "most recently active first");
    assert_eq!(a.agent_type.as_deref(), Some("main"));
    assert_eq!(a.state, "idle");
    assert_eq!(a.since.as_deref(), Some("2026-09-26T10:05:00.100Z"));
    assert_eq!(
        (a.tasks_completed, a.tasks_failed, a.tasks_stopped),
        (2, 1, 0)
    );
    assert_eq!(a.tokens, Some(1500), "1200 + t2's last terminal 300");
    assert_eq!(a.tasks_without_tokens, 1);
    assert_eq!(a.last_task.as_deref(), Some("t3"));
    assert_eq!(a.last_task_type.as_deref(), Some("type-of-t3"));
    assert_eq!(a.last_task_state.as_deref(), Some("completed"));
    assert_eq!(a.task_ids, vec!["t1", "t2", "t3"]);
    let b = &s[1];
    assert_eq!(b.state, "working");
    assert_eq!(b.tokens, None);
    assert_eq!((b.tasks_completed, b.tasks_failed), (0, 0));
    assert_eq!(b.last_task.as_deref(), Some("t9"));
}

#[test]
fn the_agent_table_shows_measured_numbers_and_says_what_was_not_recorded() {
    let s = agent_summaries(&agent_sample());
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-26T10:08:30.100Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    // One resource attributed to the agent, one owned by its task t3, one
    // owned by another agent's task.
    let unreleased = vec![
        ("session:x".to_string(), Some("main-aaaa".to_string())),
        ("t3".to_string(), None),
        ("t9".to_string(), None),
    ];
    let out = render_agent_list(&s, &unreleased, now, &|pid| pid == 11, 10);
    let line_a = out.lines().find(|l| l.starts_with("main-aaaa")).unwrap();
    assert!(line_a.contains("idle"), "{line_a}");
    assert!(
        line_a.contains("3m30s"),
        "time in state from the record: {line_a}"
    );
    assert!(line_a.contains("1500 (+1 not recorded)"), "{line_a}");
    assert!(line_a.contains("t3 (type-of-t3)"), "{line_a}");
    assert!(line_a.trim_end().ends_with('2'), "resources held: {line_a}");
    let line_b = out.lines().find(|l| l.starts_with("main-bbbb")).unwrap();
    assert!(
        line_b.contains("working (process ended)"),
        "a live state recorded by a dead process is not shown as live: {line_b}"
    );
    assert!(line_b.contains("not recorded"), "{line_b}");
    assert!(line_b.trim_end().ends_with('1'), "t9's resource: {line_b}");

    let limited = render_agent_list(&s, &unreleased, now, &|_| true, 1);
    assert!(limited.contains("(1 more; use --limit to show them)"));
}

#[test]
fn agents_output_names_the_log_when_empty() {
    let dir = tempfile::tempdir().unwrap();
    let log = super::super::EventLog::at(dir.path().join("events.jsonl"));
    let out = agents_command_output(&log, &[], &|_| true, 5);
    assert!(
        out.starts_with("No agent transitions recorded in "),
        "{out}"
    );
}

#[test]
fn task_show_lists_the_resources_the_task_owned() {
    let mut records = sample();
    let mut res = r(
        "res-1",
        "2026-09-26T10:01:06.000Z",
        Some("draining"),
        "leaked",
        Some("deadline_passed"),
    );
    res.entity = Entity::Resource;
    res.owner = Some("aaaa-1".into());
    res.cause = "teardown: still running after 10s".into();
    records.push(res);
    let out = render_task_resources(&records, "aaaa-1");
    assert!(out.contains("Resources owned"), "{out}");
    assert!(
        out.contains("res-1") && out.contains("draining → leaked"),
        "{out}"
    );
    assert!(
        out.contains("[deadline_passed] teardown: still running"),
        "{out}"
    );
    assert_eq!(render_task_resources(&records, "bbbb-2"), "");
}

#[test]
fn tasks_recorded_without_an_owner_belong_to_the_legacy_main_agent() {
    // `sample()` predates agent records: no owner on any task.
    let s = agent_summaries(&sample());
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].id, crate::lifecycle::control::MAIN_AGENT);
    assert_eq!(s[0].state, "not recorded", "no agent record, no state");
    assert_eq!(s[0].agent_type, None);
    assert_eq!(s[0].tasks_failed, 1);
    assert_eq!(s[0].tokens, None);
}

#[test]
fn paused_time_is_measured_from_pause_to_the_record_that_leaves_paused() {
    let t = |ts: &str, from: Option<&str>, to: &str, ev: Option<&str>| r("p-1", ts, from, to, ev);
    let records = vec![
        t("2026-09-26T10:00:00.000Z", None, "queued", None),
        t(
            "2026-09-26T10:00:01.000Z",
            Some("queued"),
            "executing",
            Some("start"),
        ),
        // First pause: 30s, closed by an edit + resume (the edit keeps it open).
        t(
            "2026-09-26T10:00:10.000Z",
            Some("executing"),
            "paused",
            Some("pause"),
        ),
        t(
            "2026-09-26T10:00:25.000Z",
            Some("paused"),
            "paused",
            Some("edit"),
        ),
        t(
            "2026-09-26T10:00:40.000Z",
            Some("paused"),
            "executing",
            Some("resume"),
        ),
        // Second pause: 45s, ended by a cancel.
        t(
            "2026-09-26T10:01:00.000Z",
            Some("executing"),
            "paused",
            Some("pause"),
        ),
        t(
            "2026-09-26T10:01:45.000Z",
            Some("paused"),
            "cancelled",
            Some("cancel"),
        ),
    ];
    let timeline = task_timeline(&records, "p-1");
    assert_eq!(paused_total_ms(&timeline), Some(75_000));
    assert_eq!(fmt_paused(75_000), "1m15s");
    let log_dir = tempfile::tempdir().unwrap();
    let log = crate::lifecycle::EventLog::at(log_dir.path().join("events.jsonl"));
    for rec in &records {
        log.append(rec, false);
    }
    // A resource the task owned: `task show` lists it next to the paused total.
    let mut res = r("res-1", "2026-09-26T10:00:05.000Z", None, "live", None);
    res.entity = Entity::Resource;
    res.owner = Some("p-1".into());
    log.append(&res, false);
    let shown = task_show_output(&log, "p-1").unwrap();
    assert!(
        shown.contains("paused: 1m15s (not counted against the wall-clock budget)"),
        "{shown}"
    );
    assert!(
        shown.contains("Resources owned") && shown.contains("res-1"),
        "{shown}"
    );

    // Never paused, or a pause still open: nothing reported.
    let never = task_timeline(&records[..2], "p-1");
    assert_eq!(paused_total_ms(&never), None);
    let open = task_timeline(&records[..4], "p-1");
    assert_eq!(paused_total_ms(&open), None);
}

#[test]
fn show_reports_the_edited_description_and_what_it_was_started_as() {
    let dir = tempfile::tempdir().unwrap();
    let log = crate::lifecycle::EventLog::at(dir.path().join("events.jsonl"));
    for rec in sample() {
        log.append(&rec, false);
    }
    let edited = |_: &str| {
        Some((
            "add max_words and max_len".to_string(),
            Some("add max_words".to_string()),
        ))
    };
    let shown = task_show_output_with(&log, "aaaa-1", &edited).unwrap();
    assert!(
        shown.contains(
            "  description: add max_words and max_len\n  (edited mid-run; started as: add max_words)\n"
        ),
        "{shown}"
    );
    // In the header, before the transition lines.
    assert!(shown.find("description:").unwrap() < shown.find("[start]").unwrap());
    let plain = |_: &str| Some(("fix the parser".to_string(), None));
    let shown = task_show_output_with(&log, "aaaa-1", &plain).unwrap();
    assert!(shown.contains("  description: fix the parser\n"), "{shown}");
    assert!(!shown.contains("edited"), "{shown}");
}
