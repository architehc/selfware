//! Selfware Prompt
//!
//! A warm, informative prompt for the workshop.

use reedline::{Prompt, PromptEditMode, PromptHistorySearch, PromptHistorySearchStatus};
use std::borrow::Cow;

/// The Selfware workshop prompt
pub struct SelfwarePrompt {
    /// Model name (for display)
    model: String,
    /// Current step number
    step: usize,
    /// Context usage percentage (0.0 - 100.0)
    context_pct: f64,
}

impl SelfwarePrompt {
    /// Create a new prompt
    pub fn new() -> Self {
        Self {
            model: String::new(),
            step: 0,
            context_pct: 0.0,
        }
    }

    /// Create a prompt with context
    pub fn with_context(model: &str, step: usize) -> Self {
        Self {
            model: model.to_string(),
            step,
            context_pct: 0.0,
        }
    }

    /// Create a prompt with full context including token usage
    pub fn with_full_context(model: &str, step: usize, context_pct: f64) -> Self {
        Self {
            model: model.to_string(),
            step,
            context_pct,
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
            // Show abbreviated model name + context percentage (like Qwen Code)
            let short_model = if self.model.len() > 20 {
                let safe_truncate: String = self.model.chars().take(17).collect();
                format!("{}...", safe_truncate)
            } else {
                self.model.clone()
            };
            if self.context_pct > 0.0 {
                Cow::Owned(format!("[{}] {:.1}% used", short_model, self.context_pct))
            } else {
                Cow::Owned(format!("[{}]", short_model))
            }
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
#[path = "../../tests/unit/input/prompt/prompt_test.rs"]
mod tests;
