//! Completion menu wrapper that lets Enter submit an already-complete line.
//!
//! Reedline's Enter, while a completion menu is open, only ACCEPTS the
//! selected suggestion (replaces it into the buffer and closes the menu); it
//! never submits. Typing `/` opens the slash-command menu, so `/quit` + Enter
//! turned the line into `/quit ` and waited for a second Enter — a chat
//! session driven by a single Enter (a pty harness, a user who typed the whole
//! command) never exited and had to be killed 45 s later.
//!
//! [`SubmitOnExactAcceptMenu`] wraps the menu and records whether an accept
//! left the line unchanged apart from trailing whitespace (the user had
//! already typed the whole suggestion, or there was nothing to accept). The
//! Enter binding follows the accept with [`MENU_ACCEPT_HOST_COMMAND`]; the
//! editor then submits the line when the accept was a no-op, and keeps
//! editing otherwise (`/qu` + Enter still completes to `/quit ` without
//! running it).

use reedline::{Completer, Editor, Menu, MenuEvent, MenuSettings, Painter, Suggestion};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Host command emitted by Enter right after the menu branch of reedline's
/// Enter handler ran. Never reaches the REPL: `SelfwareEditor::read_line`
/// consumes it.
pub(crate) const MENU_ACCEPT_HOST_COMMAND: &str = "__menu_accept__";

/// A menu whose accept records "the line was already complete".
pub struct SubmitOnExactAcceptMenu<M: Menu> {
    inner: M,
    exact_accept: Arc<AtomicBool>,
}

impl<M: Menu> SubmitOnExactAcceptMenu<M> {
    pub fn new(inner: M, exact_accept: Arc<AtomicBool>) -> Self {
        Self {
            inner,
            exact_accept,
        }
    }
}

impl<M: Menu> Menu for SubmitOnExactAcceptMenu<M> {
    fn settings(&self) -> &MenuSettings {
        self.inner.settings()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn indicator(&self) -> &str {
        self.inner.indicator()
    }

    fn is_active(&self) -> bool {
        self.inner.is_active()
    }

    fn menu_event(&mut self, event: MenuEvent) {
        self.inner.menu_event(event)
    }

    fn can_quick_complete(&self) -> bool {
        self.inner.can_quick_complete()
    }

    fn can_partially_complete(
        &mut self,
        values_updated: bool,
        editor: &mut Editor,
        completer: &mut dyn Completer,
    ) -> bool {
        self.inner
            .can_partially_complete(values_updated, editor, completer)
    }

    fn update_values(&mut self, editor: &mut Editor, completer: &mut dyn Completer) {
        self.inner.update_values(editor, completer)
    }

    fn update_working_details(
        &mut self,
        editor: &mut Editor,
        completer: &mut dyn Completer,
        painter: &Painter,
    ) {
        self.inner
            .update_working_details(editor, completer, painter)
    }

    fn replace_in_buffer(&self, editor: &mut Editor) {
        let before = editor.get_buffer().trim_end().to_string();
        self.inner.replace_in_buffer(editor);
        let unchanged = editor.get_buffer().trim_end() == before;
        self.exact_accept.store(unchanged, Ordering::SeqCst);
    }

    fn menu_required_lines(&self, terminal_columns: u16) -> u16 {
        self.inner.menu_required_lines(terminal_columns)
    }

    fn menu_string(&self, available_lines: u16, use_ansi_coloring: bool) -> String {
        self.inner.menu_string(available_lines, use_ansi_coloring)
    }

    fn min_rows(&self) -> u16 {
        self.inner.min_rows()
    }

    fn get_values(&self) -> &[Suggestion] {
        self.inner.get_values()
    }

    fn set_cursor_pos(&mut self, pos: (u16, u16)) {
        self.inner.set_cursor_pos(pos)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/input/submit_menu_test.rs"]
mod tests;
