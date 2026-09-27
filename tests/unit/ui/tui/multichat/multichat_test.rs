use super::*;

fn state() -> MultiChatTuiState {
    MultiChatTuiState::new(&[AgentRole::Architect, AgentRole::Coder, AgentRole::Tester])
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn press(state: &mut MultiChatTuiState, code: KeyCode) -> MultiChatKeyAction {
    handle_key(state, key(code))
}

#[test]
fn digits_switch_tabs_when_input_is_empty() {
    let mut s = state();
    press(&mut s, KeyCode::Char('2'));
    assert_eq!(s.view.tab(), ViewTab::Agent(1));
    press(&mut s, KeyCode::Char('0'));
    assert_eq!(s.view.tab(), ViewTab::All);
    assert!(s.input.is_empty(), "tab keys are not typed");
}

#[test]
fn out_of_range_digit_starts_a_task_instead() {
    let mut s = state();
    press(&mut s, KeyCode::Char('9'));
    assert_eq!(s.view.tab(), ViewTab::All);
    assert_eq!(s.input, "9");
}

#[test]
fn digits_are_typed_once_a_task_is_being_written() {
    let mut s = state();
    for c in "list 3".chars() {
        press(&mut s, KeyCode::Char(c));
    }
    assert_eq!(s.input, "list 3");
    assert_eq!(s.view.tab(), ViewTab::All);
    // Alt+digit still switches tabs mid-input.
    handle_key(&mut s, KeyEvent::new(KeyCode::Char('3'), KeyModifiers::ALT));
    assert_eq!(s.view.tab(), ViewTab::Agent(2));
    assert_eq!(s.input, "list 3");
}

#[test]
fn tab_and_backtab_cycle_and_reset_scroll() {
    let mut s = state();
    press(&mut s, KeyCode::Tab);
    assert_eq!(s.view.tab(), ViewTab::Agent(0));
    s.scroll = 5;
    press(&mut s, KeyCode::Tab);
    assert_eq!(s.view.tab(), ViewTab::Agent(1));
    assert_eq!(s.scroll, 0);
    press(&mut s, KeyCode::BackTab);
    press(&mut s, KeyCode::BackTab);
    assert_eq!(s.view.tab(), ViewTab::All);
    press(&mut s, KeyCode::BackTab);
    assert_eq!(s.view.tab(), ViewTab::Agent(2));
}

#[test]
fn enter_submits_trimmed_task_and_is_refused_while_running() {
    let mut s = state();
    for c in " hello ".chars() {
        press(&mut s, KeyCode::Char(c));
    }
    assert_eq!(
        press(&mut s, KeyCode::Enter),
        MultiChatKeyAction::Submit("hello".into())
    );
    assert!(s.input.is_empty());

    s.running = true;
    press(&mut s, KeyCode::Char('x'));
    assert_eq!(press(&mut s, KeyCode::Enter), MultiChatKeyAction::None);
    assert_eq!(s.input, "x", "input kept for after the run");
}

#[test]
fn quit_keys() {
    let mut s = state();
    assert_eq!(
        handle_key(
            &mut s,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
        ),
        MultiChatKeyAction::Quit
    );
    press(&mut s, KeyCode::Char('a'));
    assert_eq!(press(&mut s, KeyCode::Esc), MultiChatKeyAction::None);
    assert!(s.input.is_empty());
    assert_eq!(press(&mut s, KeyCode::Esc), MultiChatKeyAction::Quit);
    for c in "exit".chars() {
        press(&mut s, KeyCode::Char(c));
    }
    assert_eq!(press(&mut s, KeyCode::Enter), MultiChatKeyAction::Quit);
}

#[test]
fn wrap_and_window_keep_the_tail_visible() {
    let lines = vec!["abcdef".to_string(), String::new(), "gh".to_string()];
    let wrapped = wrap_lines(&lines, 4);
    assert_eq!(wrapped, vec!["abcd", "ef", "", "gh"]);
    assert_eq!(visible_window(4, 2, 0), (2, 4));
    assert_eq!(visible_window(4, 2, 1), (1, 3));
    // Scrolling past the top clamps.
    assert_eq!(visible_window(4, 2, 99), (0, 2));
    // Fewer lines than rows.
    assert_eq!(visible_window(1, 5, 3), (0, 1));
    // Multi-byte chars wrap by char, not byte.
    assert_eq!(wrap_lines(&["·—·—".to_string()], 3), vec!["·—·", "—"]);
}

#[test]
fn render_draws_overview_and_agent_tab() {
    use ratatui::{backend::TestBackend, Terminal};
    let mut s = state();
    s.view.apply(
        &MultiAgentEvent::AgentStarted {
            agent_id: 1,
            name: "Agent-1-Coder".into(),
            task: "t".into(),
        },
        Instant::now(),
    );
    let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
    term.draw(|f| render(f, &s, Instant::now())).unwrap();
    let text: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(text.contains("coder·2"), "{text}");
    assert!(text.contains("working"), "{text}");

    press(&mut s, KeyCode::Char('2'));
    term.draw(|f| render(f, &s, Instant::now())).unwrap();
    let text: String = term
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(text.contains("▶ started"), "{text}");
}
