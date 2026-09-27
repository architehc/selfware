use super::*;
use crate::lifecycle::control::TaskConstraints;
use crate::lifecycle::RecordedUsage;
use crossterm::event::KeyEventKind;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const NOW: &str = "2026-09-26T10:10:00.000Z";

fn now() -> DateTime<Utc> {
    parse_ts(NOW).unwrap()
}

fn rec(id: &str, ts: &str, from: Option<&str>, to: &str, event: Option<&str>) -> TransitionRecord {
    let mut r = TransitionRecord::now(Entity::Task, id, from, to, event, "why");
    r.ts = ts.to_string();
    r.pid = Some(4242);
    r.task_type = Some("mutation".into());
    r
}

fn live_task() -> LiveTask {
    LiveTask {
        id: "01J9LIVETASK0001".into(),
        agent: MAIN_AGENT.into(),
        description: "add max_words to slugify()".into(),
        task_type: Some("mutation".into()),
        state: TaskState::Executing,
        state_since: now() - chrono::Duration::seconds(221),
        step: 12,
        constraints: TaskConstraints {
            max_turns: 40,
            turns_used: 12,
            token_budget: None,
            allowed_paths: vec!["./**".into()],
        },
        usage: Some(RecordedUsage {
            total_tokens: 212_000,
            main_tokens: Some(177_000),
            side_tokens: Some(35_000),
            cost_usd: None,
            cost_complete: false,
        }),
        parent: None,
        pause_pending: false,
    }
}

fn inputs() -> TasksInputs {
    let mut done = rec(
        "01J9DONETASK0002",
        "2026-09-26T09:00:03.000Z",
        Some("executing"),
        "completed",
        Some("succeed"),
    );
    done.usage = Some(RecordedUsage {
        total_tokens: 900,
        main_tokens: None,
        side_tokens: None,
        cost_usd: Some(0.0123),
        cost_complete: true,
    });
    let records = vec![
        rec(
            "01J9DONETASK0002",
            "2026-09-26T09:00:00.000Z",
            None,
            "queued",
            None,
        ),
        rec(
            "01J9DONETASK0002",
            "2026-09-26T09:00:00.500Z",
            Some("queued"),
            "planning",
            Some("start"),
        ),
        rec(
            "01J9DONETASK0002",
            "2026-09-26T09:00:02.000Z",
            Some("planning"),
            "executing",
            Some("planned"),
        ),
        done,
        rec(
            "01J9LIVETASK0001",
            "2026-09-26T10:06:00.000Z",
            None,
            "queued",
            None,
        ),
        rec(
            "01J9LIVETASK0001",
            "2026-09-26T10:06:00.000Z",
            Some("queued"),
            "planning",
            Some("start"),
        ),
        rec(
            "01J9LIVETASK0001",
            "2026-09-26T10:06:19.000Z",
            Some("planning"),
            "executing",
            Some("planned"),
        ),
    ];
    let resources = vec![
        ResourceRow {
            id: "res-1".into(),
            kind: "container".into(),
            state: "live".into(),
            handle: "container 3f2a1b9c0d11".into(),
            port: None,
            owner_task: "01J9LIVETASK0001".into(),
            owner_agent: None,
            label: "slugify-test".into(),
            note: None,
            drainable: true,
        },
        ResourceRow {
            id: "res-2".into(),
            kind: "server_port".into(),
            state: "live".into(),
            handle: "port 5000".into(),
            port: Some(5000),
            owner_task: "01J9LIVETASK0001".into(),
            owner_agent: None,
            label: String::new(),
            note: None,
            drainable: false,
        },
        ResourceRow {
            id: "res-3".into(),
            kind: "process".into(),
            state: "leaked".into(),
            handle: "pid 777".into(),
            port: None,
            owner_task: "01J9DONETASK0002".into(),
            owner_agent: None,
            label: "npm run dev".into(),
            note: Some("still running".into()),
            drainable: true,
        },
    ];
    let mut descriptions = HashMap::new();
    descriptions.insert("01J9DONETASK0002".to_string(), "fix the parser".to_string());
    TasksInputs {
        records,
        resources,
        live: Some(live_task()),
        descriptions,
        this_pid: 4242,
        live_pids: Default::default(),
        now: now(),
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

fn control_with(live: Option<LiveTask>) -> (TaskControl, Arc<AtomicBool>) {
    let token = Arc::new(AtomicBool::new(false));
    let c = TaskControl::new(Arc::clone(&token));
    if let Some(l) = live {
        c.publish(l);
    }
    (c, token)
}

fn field<'a>(view: &'a TasksView, name: &str) -> &'a str {
    view.fields
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or_else(|| panic!("no field {name}: {:?}", view.fields))
}

#[test]
fn root_lists_the_main_agent_with_its_last_task() {
    let inputs = inputs();
    let view = build_view(&inputs, &Nav::default());
    assert_eq!(view.breadcrumb, vec!["Session", "Agents"]);
    assert_eq!(view.items.len(), 1);
    assert!(view.items[0].label.starts_with("main"));
    assert!(
        view.items[0].label.contains("2 task(s)"),
        "{:?}",
        view.items
    );
}

#[test]
fn drill_down_and_back_restores_selection_and_scroll() {
    let inputs = inputs();
    let (control, _) = control_with(inputs.live.clone());
    let mut pane = TasksPane::default();
    pane.on_key(key(KeyCode::Enter), &inputs, &control); // → main's tasks
    assert_eq!(
        pane.nav.current.level,
        Level::Tasks {
            agent: "main".into()
        }
    );
    // Most recent first: the live task, then the finished one.
    pane.on_key(key(KeyCode::Down), &inputs, &control);
    pane.nav.current.scroll = 1;
    pane.on_key(key(KeyCode::Enter), &inputs, &control); // → finished task
    let view = build_view(&inputs, &pane.nav);
    assert_eq!(
        view.breadcrumb,
        vec!["Session", "Agents", "main", "Task 01J9DONETASK…"]
    );
    // Its leaked process is listed; open it.
    let idx = view
        .items
        .iter()
        .position(|i| i.label.contains("process pid 777"))
        .unwrap();
    for _ in 0..idx {
        pane.on_key(key(KeyCode::Down), &inputs, &control);
    }
    pane.on_key(key(KeyCode::Enter), &inputs, &control);
    let view = build_view(&inputs, &pane.nav);
    assert_eq!(view.breadcrumb.last().unwrap(), "process pid 777");
    assert_eq!(field(&view, "state"), "leaked");
    assert!(field(&view, "reap").contains("[r] drains it"));

    // Back twice: the task list again, same row selected, same scroll.
    pane.on_key(key(KeyCode::Esc), &inputs, &control);
    assert_eq!(pane.nav.current.selected, idx);
    pane.on_key(key(KeyCode::Backspace), &inputs, &control);
    assert_eq!(pane.nav.current.selected, 1);
    assert_eq!(pane.nav.current.scroll, 1);
    pane.on_key(key(KeyCode::Esc), &inputs, &control);
    assert_eq!(pane.nav.current.level, Level::Agents);
    assert_eq!(
        pane.on_key(key(KeyCode::Esc), &inputs, &control),
        Some(PaneAction::Close),
        "Esc at the root closes the pane"
    );
}

#[test]
fn live_task_detail_shows_measured_state_tokens_resources_and_timeline() {
    let inputs = inputs();
    let mut nav = Nav::default();
    nav.push(Level::Task {
        agent: "main".into(),
        id: "01J9LIVETASK0001".into(),
    });
    let view = build_view(&inputs, &nav);
    assert!(view.title.contains("\"add max_words to slugify()\""));
    assert_eq!(field(&view, "state"), "executing (step 12, 3m41s)");
    assert_eq!(field(&view, "type"), "mutation");
    assert_eq!(field(&view, "tokens"), "212000 (main 177000 · side 35000)");
    assert_eq!(field(&view, "cost"), "not reported");
    assert!(field(&view, "limits").contains("turns 12/40"));
    assert!(field(&view, "limits").contains("token budget unbounded"));
    assert_eq!(field(&view, "resources"), "2 recorded, 2 not released");
    assert_eq!(
        field(&view, "timeline"),
        "Queued 0s → Planning 19s → Executing …"
    );
    assert!(view.items.iter().any(|i| i
        .label
        .starts_with("● container container 3f2a1b9c0d11 (live)")));
    assert!(view.items.iter().any(|i| i.label.contains("(live, :5000)")));
    assert_eq!(view.keys, KEYS);
}

#[test]
fn finished_task_shows_reported_cost_and_unmeasured_split_honestly() {
    let inputs = inputs();
    let mut nav = Nav::default();
    nav.push(Level::Task {
        agent: "main".into(),
        id: "01J9DONETASK0002".into(),
    });
    let view = build_view(&inputs, &nav);
    assert!(field(&view, "state").starts_with("completed (at "));
    assert_eq!(field(&view, "tokens"), "900 (main/side split not measured)");
    assert_eq!(field(&view, "cost"), "$0.0123");
    assert_eq!(
        field(&view, "timeline"),
        "Queued 0s → Planning 1s → Executing 1s → Completed"
    );
    assert_eq!(field(&view, "description"), "fix the parser");
}

#[test]
fn a_live_task_of_another_process_is_named_as_such() {
    let mut inputs = inputs();
    inputs.live = None;
    inputs.this_pid = 1;
    let mut nav = Nav::default();
    nav.push(Level::Task {
        agent: "main".into(),
        id: "01J9LIVETASK0001".into(),
    });
    let view = build_view(&inputs, &nav);
    assert_eq!(
        field(&view, "state"),
        "executing for 3m41s (recorded by selfware pid 4242, not this session)"
    );
    assert_eq!(field(&view, "tokens"), "not recorded");
    let (control, _) = control_with(None);
    let mut pane = TasksPane {
        nav,
        ..Default::default()
    };
    pane.on_key(key(KeyCode::Char('p')), &inputs, &control);
    assert!(pane
        .status
        .as_deref()
        .unwrap()
        .contains("not supported yet"));
    pane.on_key(key(KeyCode::Char('e')), &inputs, &control);
    assert!(pane.editor.is_none());
}

#[test]
fn edit_pauses_then_submits_a_validated_edit() {
    let inputs = inputs();
    let (control, _) = control_with(inputs.live.clone());
    let mut pane = TasksPane::default();
    pane.nav.push(Level::Task {
        agent: "main".into(),
        id: "01J9LIVETASK0001".into(),
    });
    pane.on_key(key(KeyCode::Char('e')), &inputs, &control);
    assert!(
        control.pause_requested(),
        "e pauses at the next step boundary"
    );
    let editor = pane.editor.as_ref().unwrap();
    assert_eq!(
        editor.target,
        EditTarget::Live {
            id: "01J9LIVETASK0001".into()
        }
    );
    // Append to the description, then set max turns to 5 (below the 12 used).
    for c in " and max_len".chars() {
        pane.on_key(key(KeyCode::Char(c)), &inputs, &control);
    }
    pane.on_key(key(KeyCode::Tab), &inputs, &control);
    pane.on_key(key(KeyCode::Backspace), &inputs, &control);
    pane.on_key(key(KeyCode::Backspace), &inputs, &control);
    pane.on_key(key(KeyCode::Char('5')), &inputs, &control);
    pane.on_key(key(KeyCode::Enter), &inputs, &control);
    let err = pane.editor.as_ref().unwrap().error.clone().unwrap();
    assert!(err.contains("12 already used"), "{err}");
    // Fix it: 50.
    pane.on_key(key(KeyCode::Backspace), &inputs, &control);
    pane.on_key(key(KeyCode::Char('5')), &inputs, &control);
    pane.on_key(key(KeyCode::Char('0')), &inputs, &control);
    // Tab skips the read-only allowed paths field and wraps.
    pane.on_key(key(KeyCode::Tab), &inputs, &control);
    pane.on_key(key(KeyCode::Tab), &inputs, &control);
    assert_eq!(pane.editor.as_ref().unwrap().focus, 0);
    assert_eq!(pane.on_key(key(KeyCode::Enter), &inputs, &control), None);
    assert!(pane.editor.is_none());
    let edit = control.take_edit().expect("edit submitted");
    assert_eq!(edit.description, "add max_words to slugify() and max_len");
    assert_eq!(edit.max_turns, 50);
    assert_eq!(edit.token_budget, None);
}

#[test]
fn discarding_an_edit_withdraws_the_pause() {
    let inputs = inputs();
    let (control, _) = control_with(inputs.live.clone());
    let mut pane = TasksPane::default();
    pane.nav.push(Level::Task {
        agent: "main".into(),
        id: "01J9LIVETASK0001".into(),
    });
    pane.on_key(key(KeyCode::Char('e')), &inputs, &control);
    assert!(control.pause_requested());
    pane.on_key(key(KeyCode::Esc), &inputs, &control);
    assert!(pane.editor.is_none());
    assert!(!control.pause_requested());
    // The Esc went to the editor, not the back stack.
    assert!(matches!(pane.nav.current.level, Level::Task { .. }));
}

#[test]
fn editing_a_finished_task_forks_it() {
    let inputs = inputs();
    let (control, _) = control_with(inputs.live.clone());
    let mut pane = TasksPane::default();
    pane.nav.push(Level::Task {
        agent: "main".into(),
        id: "01J9DONETASK0002".into(),
    });
    pane.on_key(key(KeyCode::Char('e')), &inputs, &control);
    assert_eq!(
        pane.editor.as_ref().unwrap().target,
        EditTarget::Fork {
            parent: "01J9DONETASK0002".into()
        }
    );
    assert!(!control.pause_requested(), "a finished task is not paused");
    for c in " again".chars() {
        pane.on_key(key(KeyCode::Char(c)), &inputs, &control);
    }
    let action = pane.on_key(key(KeyCode::Enter), &inputs, &control);
    assert_eq!(
        action,
        Some(PaneAction::SubmitTask("fix the parser again".into()))
    );
    assert_eq!(
        control.take_fork_for("fix the parser again").as_deref(),
        Some("01J9DONETASK0002")
    );
}

#[test]
fn pause_toggles_and_cancel_needs_a_second_press() {
    let inputs = inputs();
    let (control, token) = control_with(inputs.live.clone());
    let mut pane = TasksPane::default();
    pane.on_key(key(KeyCode::Enter), &inputs, &control); // tasks list, live selected
    pane.on_key(key(KeyCode::Char('p')), &inputs, &control);
    assert!(control.pause_requested());
    let mut paused_inputs = inputs.clone();
    paused_inputs.live.as_mut().unwrap().pause_pending = true;
    pane.on_key(key(KeyCode::Char('p')), &paused_inputs, &control);
    assert!(!control.pause_requested(), "p again withdraws the pause");

    pane.on_key(key(KeyCode::Char('x')), &inputs, &control);
    assert!(!token.load(std::sync::atomic::Ordering::Relaxed));
    assert!(pane.status.as_deref().unwrap().contains("press x again"));
    pane.on_key(key(KeyCode::Char('x')), &inputs, &control);
    assert!(token.load(std::sync::atomic::Ordering::Relaxed));
    assert!(control.cancel_requested());
}

#[test]
fn reap_only_offers_what_was_left_behind() {
    let inputs = inputs();
    let (control, _) = control_with(inputs.live.clone());
    let mut pane = TasksPane::default();
    pane.nav.push(Level::Task {
        agent: "main".into(),
        id: "01J9LIVETASK0001".into(),
    });
    assert_eq!(
        pane.on_key(key(KeyCode::Char('r')), &inputs, &control),
        None
    );
    assert!(pane.status.as_deref().unwrap().contains("nothing to reap"));
    pane.nav.push(Level::Task {
        agent: "main".into(),
        id: "01J9DONETASK0002".into(),
    });
    assert_eq!(
        pane.on_key(key(KeyCode::Char('r')), &inputs, &control),
        Some(PaneAction::Reap(vec!["res-3".into()]))
    );
}

#[test]
fn edits_and_new_segments_show_in_the_compact_timeline() {
    let a = rec(
        "t",
        "2026-09-26T10:00:00.000Z",
        Some("executing"),
        "paused",
        Some("pause"),
    );
    let b = rec(
        "t",
        "2026-09-26T10:00:05.000Z",
        Some("paused"),
        "paused",
        Some("edit"),
    );
    let c = rec(
        "t",
        "2026-09-26T10:00:07.000Z",
        Some("paused"),
        "executing",
        Some("resume"),
    );
    let d = rec(
        "t",
        "2026-09-26T10:01:07.000Z",
        Some("executing"),
        "interrupted",
        Some("interrupt"),
    );
    let e = rec("t", "2026-09-26T11:00:00.000Z", None, "queued", None);
    let line = compact_timeline(&[&a, &b, &c, &d, &e]);
    assert_eq!(
        line,
        "Paused 5s (edited) → Paused 2s → Executing 1m00s → Interrupted 58m53s | new segment → Queued …"
    );
    assert_eq!(fmt_duration(3 * 3600 + 120), "3h02m");
}

fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
    let buf = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn renders_breadcrumb_fields_and_keys_on_a_test_backend() {
    let inputs = inputs();
    let mut nav = Nav::default();
    nav.push(Level::Tasks {
        agent: "main".into(),
    });
    nav.push(Level::Task {
        agent: "main".into(),
        id: "01J9LIVETASK0001".into(),
    });
    let view = build_view(&inputs, &nav);
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|f| render(f, f.area(), &view, None, Some("pause requested")))
        .unwrap();
    let text = buffer_text(&terminal);
    assert!(
        text.contains("Session › Agents › main › Task 01J9LIVETASK…"),
        "{text}"
    );
    assert!(text.contains("executing (step 12, 3m41s)"), "{text}");
    assert!(text.contains("212000 (main 177000 · side 35000)"), "{text}");
    assert!(text.contains("not reported"), "{text}");
    assert!(text.contains("pause requested"), "{text}");
    assert!(text.contains("[Enter] open  [e] edit"), "{text}");

    // With the editor open.
    let editor = TaskEditor::for_live(inputs.live.as_ref().unwrap());
    terminal
        .draw(|f| render(f, f.area(), &view, Some(&editor), None))
        .unwrap();
    let text = buffer_text(&terminal);
    assert!(text.contains("Edit task 01J9LIVETASK…"), "{text}");
    assert!(text.contains("allowed paths  (read-only)"), "{text}");
    assert!(text.contains("(empty = unbounded)"), "{text}");
}

#[test]
fn render_survives_tiny_terminals() {
    let inputs = inputs();
    let view = build_view(&inputs, &Nav::default());
    let editor = TaskEditor::for_live(inputs.live.as_ref().unwrap());
    for (w, h) in [(0u16, 0u16), (1, 1), (10, 4), (30, 6)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|f| render(f, f.area(), &view, Some(&editor), Some("s")))
            .unwrap_or_else(|e| panic!("{w}x{h}: {e}"));
    }
}

#[test]
fn the_agent_list_is_the_agents_projection_with_state_time_tasks_tokens_and_resources() {
    let mut inputs = inputs();
    // This process's agent owns the live task; its records are in the log.
    let agent_rec = |ts: &str, to: &str| {
        let mut r = TransitionRecord::now(Entity::Agent, "main-0a1b2c3d", None, to, None, "why");
        r.ts = ts.to_string();
        r.pid = Some(4242);
        r.agent_type = Some("main".into());
        r
    };
    inputs
        .records
        .push(agent_rec("2026-09-26T10:05:59.000Z", "idle"));
    inputs
        .records
        .push(agent_rec("2026-09-26T10:06:00.000Z", "working"));
    for r in inputs
        .records
        .iter_mut()
        .filter(|r| r.id == "01J9LIVETASK0001")
    {
        r.owner = Some("main-0a1b2c3d".into());
    }
    let mut live = live_task();
    live.agent = "main-0a1b2c3d".into();
    inputs.live = Some(live);

    let rows = agents_from(&inputs);
    let mine = rows.iter().find(|r| r.id == "main-0a1b2c3d").unwrap();
    assert_eq!(mine.agent_type, "main");
    assert_eq!(mine.state, "working");
    assert_eq!(mine.in_state, "4m00s", "now − the recorded timestamp");
    assert_eq!(mine.tasks, 1);
    assert_eq!(mine.last_task.as_deref(), Some("01J9LIVETASK0001"));
    assert_eq!(mine.last_state.as_deref(), Some("executing"));
    assert_eq!(mine.tokens, "not recorded", "the live task has not ended");
    assert_eq!(mine.resources, 2, "container + port of its live task");
    // The earlier, owner-less task stays with the legacy `main` agent, with
    // its measured usage.
    let legacy = rows.iter().find(|r| r.id == "main").unwrap();
    assert_eq!(legacy.state, "not recorded");
    assert_eq!((legacy.tasks, legacy.completed), (1, 1));
    assert_eq!(legacy.tokens, "900");
    assert_eq!(legacy.resources, 1, "its leaked process");

    let view = build_view(&inputs, &Nav::default());
    let label = &view
        .items
        .iter()
        .find(|i| i.label.starts_with("main-0a1b2c3d"))
        .unwrap()
        .label;
    assert!(label.contains("working 4m00s"), "{label}");
    assert!(label.contains("tokens not recorded"), "{label}");
    assert!(label.contains("resources 2"), "{label}");

    // A live state recorded by a process that is gone is not shown as live.
    for r in inputs
        .records
        .iter_mut()
        .filter(|r| r.entity == Entity::Agent)
    {
        r.pid = Some(999_999);
    }
    let rows = agents_from(&inputs);
    let mine = rows.iter().find(|r| r.id == "main-0a1b2c3d").unwrap();
    assert_eq!(mine.state, "working (process ended)");
}
