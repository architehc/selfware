use super::*;
use crate::lifecycle::Entity;

fn rec(id: &str, from: Option<&str>, to: &str, pid: u32) -> TransitionRecord {
    let mut r = TransitionRecord::now(Entity::Task, id, from, to, None, "why");
    r.pid = Some(pid);
    r
}

fn records() -> Vec<TransitionRecord> {
    vec![
        rec("done-1", None, "queued", 10),
        rec("done-1", Some("executing"), "completed", 10),
        rec("live-2", None, "queued", 20),
        rec("live-2", Some("queued"), "planning", 20),
        rec("intr-3", Some("executing"), "interrupted", 30),
    ]
}

fn log() -> EventLog {
    EventLog::disabled()
}

#[test]
fn control_of_a_task_in_another_live_process_says_it_is_not_supported() {
    let out = control_output(&records(), &log(), "live", ControlVerb::Pause, &|_| true);
    let msg = out.unwrap_err();
    assert!(msg.starts_with("Not done:"), "{msg}");
    assert!(msg.contains("selfware process 20"), "{msg}");
    assert!(msg.contains("not supported yet"), "{msg}");
    assert!(msg.contains("Ctrl+T"), "{msg}");
}

#[test]
fn control_of_a_task_whose_process_is_gone_says_nothing_runs_it() {
    let msg = control_output(&records(), &log(), "live-2", ControlVerb::Cancel, &|_| {
        false
    })
    .unwrap_err();
    assert!(msg.contains("no longer running"), "{msg}");
    assert!(msg.contains("selfware resume live-2"), "{msg}");
}

#[test]
fn finished_tasks_cannot_be_paused_but_can_be_forked() {
    let msg =
        control_output(&records(), &log(), "done", ControlVerb::Pause, &|_| true).unwrap_err();
    assert!(msg.contains("already finished (completed)"), "{msg}");
    assert!(msg.contains("selfware task edit done-1"), "{msg}");
    // An interrupted task resumes through `selfware resume`.
    let ok = control_output(&records(), &log(), "intr", ControlVerb::Resume, &|_| true).unwrap();
    assert!(ok.contains("selfware resume intr-3"), "{ok}");
}

#[test]
fn edit_forks_a_finished_task_and_refuses_a_live_one() {
    let desc = |id: &str| (id == "done-1").then(|| "add max_words".to_string());
    let (task, original) = plan_edit(&records(), &log(), "done", &desc, &|_| true).unwrap();
    assert_eq!(task.id, "done-1");
    assert_eq!(original.as_deref(), Some("add max_words"));
    let msg = plan_edit(&records(), &log(), "live", &desc, &|_| true).unwrap_err();
    assert!(msg.contains("not supported yet"), "{msg}");
    let text = fork_instructions(&task, "add max_words and max_len; don't touch 'x'");
    assert!(text.contains("history is kept"), "{text}");
    assert!(
        text.contains("selfware run --fork-of done-1 \"add max_words and max_len; don't touch 'x'\"")
            || text.contains("selfware run --fork-of done-1 'add max_words and max_len; don'\\''t touch '\\''x'\\'''"),
        "{text}"
    );
}

#[test]
fn fork_of_needs_a_finished_task_and_resolves_prefixes() {
    assert_eq!(fork_parent(&records(), &log(), "done").unwrap(), "done-1");
    assert!(fork_parent(&records(), &log(), "live")
        .unwrap_err()
        .contains("not finished"));
    assert!(fork_parent(&records(), &log(), "zzz")
        .unwrap_err()
        .contains("No task matching"));
}

#[test]
fn the_edited_text_is_what_is_above_the_cut_line() {
    let text = format!("# Heading kept\nline two\n\n{EDIT_CUT_LINE}\nhelp text\n");
    assert_eq!(above_cut_line(&text), "# Heading kept\nline two");
    assert_eq!(above_cut_line("  only this  "), "only this");
}
