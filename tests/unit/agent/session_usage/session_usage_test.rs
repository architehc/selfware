use super::*;

fn task(prompt: usize, completion: usize) -> TaskUsage {
    TaskUsage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: prompt + completion,
        cost_usd: None,
        cost_complete: true,
        unmetered_attempts: 1,
    }
}

#[test]
fn folding_tasks_sums_every_counter() {
    // 0.9.1 field test: /cost said 139287 and 156657, /quit 207115. One
    // fold across tasks is the only session total.
    let session = SessionUsage::default()
        .with_task(&task(100_000, 1_000))
        .with_task(&task(37_000, 400));
    assert_eq!(session.prompt_tokens, 137_000);
    assert_eq!(session.completion_tokens, 1_400);
    assert_eq!(session.total_tokens, 138_400);
    assert_eq!(session.tasks, 2);
    assert_eq!(session.unmetered_attempts, 2);
    assert_eq!(session.cost_usd, None);
}

#[test]
fn idle_task_does_not_count_or_break_billing_completeness() {
    let idle = TaskUsage {
        cost_complete: false,
        ..TaskUsage::default()
    };
    let session = SessionUsage::default().with_task(&idle);
    assert_eq!(session.tasks, 0);
    assert!(session.cost_complete);
    assert_eq!(session.render_quit_line(0), None);
}

#[test]
fn cost_is_summed_only_from_reported_amounts() {
    let billed = TaskUsage {
        cost_usd: Some(0.05),
        unmetered_attempts: 0,
        ..task(10, 5)
    };
    let unbilled = TaskUsage {
        cost_complete: false,
        ..task(10, 5)
    };
    let session = SessionUsage::default()
        .with_task(&billed)
        .with_task(&unbilled);
    assert_eq!(session.cost_usd, Some(0.05));
    assert!(!session.cost_complete, "one task lacked billing");
    assert!(session.cost_phrase().starts_with("known cost $0.0500"));
    assert_eq!(session.status_bar_cost().as_deref(), Some("≥$0.05"));

    let complete = SessionUsage::default().with_task(&billed);
    assert_eq!(complete.status_bar_cost().as_deref(), Some("$0.05"));
}

#[test]
fn status_bar_shows_no_dollar_figure_without_provider_cost() {
    // The status bar printed "$0.07" from a hard-coded price table while
    // /cost said "cost not tracked" (0.9.1 field test).
    let session = SessionUsage::default().with_task(&task(21_000, 700));
    assert_eq!(session.status_bar_cost(), None);
    assert_eq!(
        session.cost_phrase(),
        "cost not tracked (provider billing unavailable)"
    );
}

#[test]
fn cost_lines_and_quit_line_report_the_same_numbers() {
    let session = SessionUsage::default()
        .with_task(&task(200_000, 2_000))
        .with_task(&task(5_113, 2));
    let main_loop = 139_287;
    let lines = session.render_cost_lines(main_loop).join("\n");
    assert!(lines.contains("Prompt:         205113"), "{lines}");
    assert!(lines.contains("Completion:       2002"), "{lines}");
    assert!(lines.contains("Total:          207115"), "{lines}");
    assert!(lines.contains("main loop:      139287"), "{lines}");
    assert!(lines.contains("side calls:      67828"), "{lines}");
    assert!(lines.contains("2 tasks"), "{lines}");
    assert!(lines.contains("cost not tracked"), "{lines}");
    assert!(!lines.contains("Unsplit"), "{lines}");

    let quit = session.render_quit_line(main_loop).unwrap();
    assert_eq!(
        quit,
        "session tokens: 205113 prompt + 2002 completion = 207115 total \
         (main loop 139287 · side calls 67828) · cost not tracked (provider billing unavailable)"
    );
}

#[test]
fn unsplit_usage_is_shown_so_the_breakdown_adds_up() {
    let restored = TaskUsage {
        total_tokens: 1_500,
        ..task(1_000, 200)
    };
    let lines = SessionUsage::default()
        .with_task(&restored)
        .render_cost_lines(0)
        .join("\n");
    assert!(lines.contains("Unsplit:           300"), "{lines}");
    assert!(lines.contains("Total:            1500"), "{lines}");
}

#[test]
fn main_loop_share_never_exceeds_the_total() {
    let session = SessionUsage::default().with_task(&task(10, 5));
    assert_eq!(session.side_tokens(100), 0);
    let lines = session.render_cost_lines(100).join("\n");
    assert!(lines.contains("main loop:          15"), "{lines}");
}
