//! Selfware Prompt
//!
//! A warm, informative prompt for the workshop.

use nu_ansi_term::{Color, Style};
use reedline::{Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus};
use std::borrow::Cow;

/// The Selfware workshop prompt
pub struct SelfwarePrompt {
    /// Model name (for display)
    model: String,
    /// Current step number
    step: usize,
    /// Left prompt style
    #[allow(dead_code)] // For future styled prompts
    left_style: Style,
    /// Right prompt style
    #[allow(dead_code)] // For future styled prompts
    right_style: Style,
}

impl SelfwarePrompt {
    /// Create a new prompt
    pub fn new() -> Self {
        Self {
            model: String::new(),
            step: 0,
            left_style: Style::new().fg(Color::Rgb(212, 163, 115)), // Amber
            right_style: Style::new().fg(Color::Rgb(128, 128, 128)).dimmed(), // Stone
        }
    }

    /// Create a prompt with context
    pub fn with_context(model: &str, step: usize) -> Self {
        Self {
            model: model.to_string(),
            step,
            left_style: Style::new().fg(Color::Rgb(212, 163, 115)),
            right_style: Style::new().fg(Color::Rgb(128, 128, 128)).dimmed(),
        }
    }

    /// Get the fox glyph
    fn fox(&self) -> &'static str {
        "🦊"
    }

    /// Get the garden glyph based on step
    fn garden_glyph(&self) -> &'static str {
        match self.step % 4 {
            0 => "🌱",
            1 => "🌿",
            2 => "🍃",
            _ => "🌳",
        }
    }
}

impl Default for SelfwarePrompt {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(mismatched_lifetime_syntaxes)]
impl Prompt for SelfwarePrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        if self.step > 0 {
            Cow::Owned(format!("{} {} ", self.fox(), self.garden_glyph()))
        } else {
            Cow::Owned(format!("{} ", self.fox()))
        }
    }

    fn render_prompt_right(&self) -> Cow<str> {
        if !self.model.is_empty() {
            // Show abbreviated model name
            let short_model = if self.model.len() > 20 {
                format!("{}...", &self.model[..17])
            } else {
                self.model.clone()
            };
            Cow::Owned(format!("[{}]", short_model))
        } else {
            Cow::Borrowed("")
        }
    }

    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<str> {
        match edit_mode {
            PromptEditMode::Default | PromptEditMode::Emacs => Cow::Borrowed("❯ "),
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                reedline::PromptViMode::Normal => Cow::Borrowed("❮ "),
                reedline::PromptViMode::Insert => Cow::Borrowed("❯ "),
            },
            PromptEditMode::Custom(s) => Cow::Owned(format!("{} ", s)),
        }
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        Cow::Borrowed("  ⋮ ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<str> {
        let prefix = match history_search.status {
            PromptHistorySearchStatus::Passing => "🔍",
            PromptHistorySearchStatus::Failing => "❌",
        };
        Cow::Owned(format!("{} [{}]: ", prefix, history_search.term))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_creation() {
        let prompt = SelfwarePrompt::new();
        assert!(prompt.model.is_empty());
        assert_eq!(prompt.step, 0);
    }

    #[test]
    fn test_prompt_with_context() {
        let prompt = SelfwarePrompt::with_context("test-model", 5);
        assert_eq!(prompt.model, "test-model");
        assert_eq!(prompt.step, 5);
    }

    #[test]
    fn test_prompt_render() {
        let prompt = SelfwarePrompt::new();
        let left = prompt.render_prompt_left();
        assert!(left.contains("🦊"));
    }

    #[test]
    fn test_garden_glyph_rotation() {
        let p0 = SelfwarePrompt::with_context("", 0);
        let p1 = SelfwarePrompt::with_context("", 1);
        let p2 = SelfwarePrompt::with_context("", 2);
        let p3 = SelfwarePrompt::with_context("", 3);

        assert_eq!(p0.garden_glyph(), "🌱");
        assert_eq!(p1.garden_glyph(), "🌿");
        assert_eq!(p2.garden_glyph(), "🍃");
        assert_eq!(p3.garden_glyph(), "🌳");
    }

    #[test]
    fn test_prompt_default() {
        let prompt = SelfwarePrompt::default();
        assert!(prompt.model.is_empty());
        assert_eq!(prompt.step, 0);
    }

    #[test]
    fn test_fox_glyph() {
        let prompt = SelfwarePrompt::new();
        assert_eq!(prompt.fox(), "🦊");
    }

    #[test]
    fn test_garden_glyph_wraps_around() {
        let p4 = SelfwarePrompt::with_context("", 4);
        let p5 = SelfwarePrompt::with_context("", 5);
        let p8 = SelfwarePrompt::with_context("", 8);

        assert_eq!(p4.garden_glyph(), "🌱"); // 4 % 4 = 0
        assert_eq!(p5.garden_glyph(), "🌿"); // 5 % 4 = 1
        assert_eq!(p8.garden_glyph(), "🌱"); // 8 % 4 = 0
    }

    #[test]
    fn test_render_prompt_left_with_step() {
        let prompt = SelfwarePrompt::with_context("model", 1);
        let left = prompt.render_prompt_left();
        assert!(left.contains("🦊"));
        assert!(left.contains("🌿")); // Step 1 = 🌿
    }

    #[test]
    fn test_render_prompt_left_no_step() {
        let prompt = SelfwarePrompt::new();
        let left = prompt.render_prompt_left();
        assert!(left.contains("🦊"));
        // No garden glyph when step is 0
    }

    #[test]
    fn test_render_prompt_right_with_model() {
        let prompt = SelfwarePrompt::with_context("test-model", 1);
        let right = prompt.render_prompt_right();
        assert!(right.contains("test-model"));
        assert!(right.contains("["));
        assert!(right.contains("]"));
    }

    #[test]
    fn test_render_prompt_right_no_model() {
        let prompt = SelfwarePrompt::new();
        let right = prompt.render_prompt_right();
        assert!(right.is_empty());
    }

    #[test]
    fn test_render_prompt_right_long_model() {
        let long_model = "this-is-a-very-long-model-name-that-exceeds-twenty-characters";
        let prompt = SelfwarePrompt::with_context(long_model, 1);
        let right = prompt.render_prompt_right();
        assert!(right.contains("..."));
        assert!(right.len() < long_model.len() + 5); // Truncated
    }

    #[test]
    fn test_render_prompt_indicator_default() {
        let prompt = SelfwarePrompt::new();
        let indicator = prompt.render_prompt_indicator(PromptEditMode::Default);
        assert_eq!(indicator.as_ref(), "❯ ");
    }

    #[test]
    fn test_render_prompt_indicator_emacs() {
        let prompt = SelfwarePrompt::new();
        let indicator = prompt.render_prompt_indicator(PromptEditMode::Emacs);
        assert_eq!(indicator.as_ref(), "❯ ");
    }

    #[test]
    fn test_render_prompt_indicator_vi_normal() {
        let prompt = SelfwarePrompt::new();
        let indicator =
            prompt.render_prompt_indicator(PromptEditMode::Vi(reedline::PromptViMode::Normal));
        assert_eq!(indicator.as_ref(), "❮ ");
    }

    #[test]
    fn test_render_prompt_indicator_vi_insert() {
        let prompt = SelfwarePrompt::new();
        let indicator =
            prompt.render_prompt_indicator(PromptEditMode::Vi(reedline::PromptViMode::Insert));
        assert_eq!(indicator.as_ref(), "❯ ");
    }

    #[test]
    fn test_render_prompt_indicator_custom() {
        let prompt = SelfwarePrompt::new();
        let indicator =
            prompt.render_prompt_indicator(PromptEditMode::Custom("CUSTOM".to_string()));
        assert!(indicator.contains("CUSTOM"));
    }

    #[test]
    fn test_render_multiline_indicator() {
        let prompt = SelfwarePrompt::new();
        let indicator = prompt.render_prompt_multiline_indicator();
        assert_eq!(indicator.as_ref(), "  ⋮ ");
    }

    #[test]
    fn test_render_history_search_passing() {
        let prompt = SelfwarePrompt::new();
        let search = PromptHistorySearch {
            status: PromptHistorySearchStatus::Passing,
            term: "test".to_string(),
        };
        let indicator = prompt.render_prompt_history_search_indicator(search);
        assert!(indicator.contains("🔍"));
        assert!(indicator.contains("test"));
    }

    #[test]
    fn test_render_history_search_failing() {
        let prompt = SelfwarePrompt::new();
        let search = PromptHistorySearch {
            status: PromptHistorySearchStatus::Failing,
            term: "notfound".to_string(),
        };
        let indicator = prompt.render_prompt_history_search_indicator(search);
        assert!(indicator.contains("❌"));
        assert!(indicator.contains("notfound"));
    }
}
