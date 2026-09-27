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
