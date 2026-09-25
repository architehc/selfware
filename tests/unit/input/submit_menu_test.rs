use super::*;
use crate::input::SelfwareCompleter;
use reedline::{ColumnarMenu, MenuBuilder, UndoBehavior};

fn editor_with(text: &str) -> Editor {
    let mut editor = Editor::default();
    editor.edit_buffer(
        |lb| lb.set_buffer(text.to_string()),
        UndoBehavior::CreateUndoPoint,
    );
    editor
}

fn menu(flag: Arc<AtomicBool>) -> SubmitOnExactAcceptMenu<ColumnarMenu> {
    SubmitOnExactAcceptMenu::new(
        ColumnarMenu::default()
            .with_name("completion_menu")
            .with_columns(1),
        flag,
    )
}

fn completer() -> SelfwareCompleter {
    SelfwareCompleter::new(vec![], crate::input::command_registry::command_names())
}

/// Opens the menu over `text` and presses the accept (what reedline's Enter
/// does while a menu is active); returns (flag, buffer after).
fn accept(text: &str) -> (bool, String) {
    let flag = Arc::new(AtomicBool::new(false));
    let mut menu = menu(flag.clone());
    let mut editor = editor_with(text);
    let mut completer = completer();
    menu.menu_event(MenuEvent::Activate(false));
    menu.update_values(&mut editor, &mut completer);
    menu.replace_in_buffer(&mut editor);
    (flag.load(Ordering::SeqCst), editor.get_buffer().to_string())
}

#[test]
fn menu_keeps_its_name_so_keybindings_find_it() {
    let m = menu(Arc::new(AtomicBool::new(false)));
    assert_eq!(m.name(), "completion_menu");
}

#[test]
fn fully_typed_command_accept_is_exact_so_enter_submits() {
    let (exact, buffer) = accept("/quit");
    assert!(
        exact,
        "`/quit` + Enter must submit, not just complete (buffer after accept: {buffer:?})"
    );
    assert_eq!(buffer.trim_end(), "/quit");
}

#[test]
fn partial_command_accept_completes_without_submitting() {
    let (exact, buffer) = accept("/qu");
    assert!(
        !exact,
        "a completing accept must not submit (buffer {buffer:?})"
    );
    assert!(
        buffer.starts_with("/qu") && buffer.trim_end().len() > 3,
        "{buffer:?}"
    );
}

#[test]
fn enter_binding_accepts_then_asks_the_editor() {
    match super::super::enter_event() {
        reedline::ReedlineEvent::Multiple(events) => {
            assert_eq!(events.len(), 2);
            assert!(matches!(events[0], reedline::ReedlineEvent::Enter));
            assert!(matches!(
                &events[1],
                reedline::ReedlineEvent::ExecuteHostCommand(c) if c == MENU_ACCEPT_HOST_COMMAND
            ));
        }
        other => panic!("unexpected Enter binding: {other:?}"),
    }
}
