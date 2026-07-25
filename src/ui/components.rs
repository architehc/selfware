//! Selfware UI Components
//!
//! Reusable terminal components for the personal workshop aesthetic.

use std::time::Duration;

use super::style::{Glyphs, SelfwareStyle};
use crate::config::ExecutionMode;

/// Workshop context - your personal space
#[derive(Debug, Clone)]
pub struct WorkshopContext {
    pub owner_name: String,
    pub companion_name: String,
    pub project_name: String,
    pub project_path: String,
    pub garden_age_days: u64,
    pub tasks_completed: usize,
    pub time_saved_hours: f64,
    pub is_local_model: bool,
    pub model_name: String,
    pub execution_mode: ExecutionMode,
}

impl Default for WorkshopContext {
    fn default() -> Self {
        Self {
            owner_name: whoami::username().unwrap_or_else(|_| "friend".to_string()),
            companion_name: "Selfware".to_string(),
            project_name: std::env::current_dir()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "your project".to_string()),
            project_path: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            garden_age_days: 0,
            tasks_completed: 0,
            time_saved_hours: 0.0,
            is_local_model: true,
            model_name: "local".to_string(),
            execution_mode: ExecutionMode::Normal,
        }
    }
}

impl WorkshopContext {
    pub fn from_config(endpoint: &str, model: &str) -> Self {
        Self {
            is_local_model: endpoint.contains("localhost") || endpoint.contains("127.0.0.1"),
            model_name: model.to_string(),
            ..Default::default()
        }
    }

    pub fn with_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode;
        self
    }
}

/// Render the workshop header
pub fn render_header(ctx: &WorkshopContext) -> String {
    let hosting = if ctx.is_local_model {
        format!("{} Homestead", Glyphs::home()).garden_healthy()
    } else {
        format!("{} Remote", Glyphs::compass()).garden_wilting()
    };

    // Mode indicator with color
    let mode_str = match ctx.execution_mode {
        ExecutionMode::Normal => format!("[{}]", "normal".muted()),
        ExecutionMode::AutoEdit => format!("[{}]", "auto-edit".garden_healthy()),
        ExecutionMode::Yolo => format!("[{}]", "YOLO".garden_wilting()),
        ExecutionMode::Daemon => format!("[{}]", "DAEMON".tool_name()),
    };

    let width = 65;
    let top_border = format!(
        "{}{}{}",
        Glyphs::corner_tl(),
        Glyphs::horiz().repeat(width - 2),
        Glyphs::corner_tr()
    );
    let bottom_border = format!(
        "{}{}{}",
        Glyphs::corner_bl(),
        Glyphs::horiz().repeat(width - 2),
        Glyphs::corner_br()
    );

    format!(
        r#"
{}
{}  {} SELFWARE WORKSHOP {}                              {}
{}  {} Tending: {}
{}  {} · {} tasks completed
{}
"#,
        top_border.muted(),
        Glyphs::vert().muted(),
        Glyphs::gear(),
        mode_str,
        Glyphs::vert().muted(),
        Glyphs::vert().muted(),
        Glyphs::sprout(),
        ctx.project_name.as_str().emphasis(),
        Glyphs::vert().muted(),
        hosting,
        ctx.tasks_completed.to_string().garden_healthy(),
        bottom_border.muted(),
    )
}

/// Render a minimal status line
pub fn render_status_line(ctx: &WorkshopContext) -> String {
    let hosting = if ctx.is_local_model {
        format!("{} yours", Glyphs::home())
    } else {
        format!("{} remote", Glyphs::compass())
    };

    format!(
        "{} {} {} {} {}",
        hosting.muted(),
        Glyphs::vert().muted(),
        ctx.project_name.as_str().emphasis(),
        Glyphs::vert().muted(),
        ctx.model_name.as_str().muted(),
    )
}

/// Render a task starting message
pub fn render_task_start(task: &str) -> String {
    format!(
        "\n{} {} beginning a new task in your garden...\n{} {}\n",
        Glyphs::seedling(),
        "Your companion is".craftsman_voice(),
        Glyphs::journal(),
        task.emphasis()
    )
}

/// Render step progress
pub fn render_step(step: usize, phase: &str) -> String {
    let phase_glyph = match phase.to_lowercase().as_str() {
        "planning" => Glyphs::compass(),
        "executing" => Glyphs::hammer(),
        "verifying" => Glyphs::magnifier(),
        "reflecting" => Glyphs::journal(),
        _ => Glyphs::gear(),
    };

    format!(
        "{} {} Step {} · {}",
        phase_glyph,
        Glyphs::branch().muted(),
        step.to_string().emphasis(),
        phase.craftsman_voice()
    )
}

/// Render tool execution
pub fn render_tool_call(tool_name: &str) -> String {
    let metaphor = super::style::tool_metaphor(tool_name);
    format!(
        "   {} {} {}...",
        Glyphs::wrench(),
        metaphor.craftsman_voice(),
        format!("({})", tool_name).muted()
    )
}

/// Render tool success
pub fn render_tool_success(_tool_name: &str) -> String {
    format!(
        "   {} {}",
        Glyphs::bloom().garden_healthy(),
        "done".garden_healthy()
    )
}

/// Render tool failure
pub fn render_tool_error(_tool_name: &str, error: &str) -> String {
    format!(
        "   {} {} — {}",
        Glyphs::frost(),
        "a frost touched this".garden_wilting(),
        error.muted()
    )
}

/// Render task completion
pub fn render_task_complete(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let time_str = if seconds < 60 {
        format!("{}s", seconds)
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    };

    format!(
        "\n{} {} Your garden has been tended. ({})\n",
        Glyphs::harvest(),
        "Task complete.".garden_healthy(),
        time_str.muted()
    )
}

/// Render an error message
pub fn render_error(message: &str) -> String {
    format!(
        "\n{} {} {}\n",
        Glyphs::frost(),
        "A chill in the workshop:".garden_wilting(),
        message
    )
}

/// Render a warning message
pub fn render_warning(message: &str) -> String {
    format!(
        "{} {} {}",
        Glyphs::wilt(),
        "Note:".garden_wilting(),
        message.muted()
    )
}

/// Render checkpoint saved
pub fn render_checkpoint_saved(task_id: &str) -> String {
    format!(
        "{} {} · {}",
        Glyphs::bookmark(),
        "Journal entry saved".craftsman_voice(),
        task_id.muted()
    )
}

/// Progress spinner with garden metaphors
pub struct GardenSpinner {
    frames: Vec<&'static str>,
    current: usize,
    message: String,
}

impl GardenSpinner {
    pub fn new(message: &str) -> Self {
        Self {
            frames: vec!["◌ ", "◔ ", "◑ ", "◕ ", "● ", "◕ ", "◑ ", "◔ "],
            current: 0,
            message: message.to_string(),
        }
    }

    pub fn growth() -> Self {
        Self {
            frames: vec!["🌱", "🌱", "🌿", "🌿", "🌳", "🌳"],
            current: 0,
            message: "Growing...".to_string(),
        }
    }

    pub fn tick(&mut self) -> String {
        let frame = self.frames[self.current % self.frames.len()];
        self.current += 1;
        format!("{} {}", frame, self.message.as_str().craftsman_voice())
    }

    pub fn finish(&self, success: bool) -> String {
        if success {
            format!("{} {}", Glyphs::bloom(), "Complete".garden_healthy())
        } else {
            format!("{} {}", Glyphs::frost(), "Interrupted".garden_wilting())
        }
    }
}

/// Interactive prompt for the workshop
pub fn workshop_prompt() -> String {
    format!(
        "\n{} {} ",
        Glyphs::sprout(),
        "What shall we tend to?".craftsman_voice()
    )
}

/// Welcome message for interactive mode
pub fn render_welcome(ctx: &WorkshopContext) -> String {
    format!(
        r#"
{}

{} Welcome back to your workshop, {}.
{} {} stands ready to help tend your garden.

{} Type your request, or:
   {} /help    — workshop guide
   {} /status  — garden overview
   {} /journal — view saved states
   {} /quit    — close the workshop

"#,
        render_header(ctx),
        Glyphs::lantern(),
        ctx.owner_name.as_str().emphasis(),
        Glyphs::sprout(),
        ctx.companion_name.as_str().tool_name(),
        Glyphs::bookmark(),
        Glyphs::branch().muted(),
        Glyphs::branch().muted(),
        Glyphs::branch().muted(),
        Glyphs::leaf_branch().muted(),
    )
}

/// Render the assistant's response
pub fn render_assistant_response(content: &str) -> String {
    format!(
        "\n{} {}\n\n{}\n",
        Glyphs::sprout(),
        "Your companion says:".craftsman_voice(),
        content
    )
}

/// Render thinking/reasoning indicator
pub fn render_thinking() -> String {
    format!(
        "{} {}",
        Glyphs::gear(),
        "contemplating the garden...".muted()
    )
}

/// Box drawing for important content
pub fn render_box(title: &str, content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let max_width = lines
        .iter()
        .map(|l| l.len())
        .max()
        .unwrap_or(40)
        .max(title.len() + 4);
    let width = max_width + 4;

    let top = format!(
        "{} {} {}",
        Glyphs::corner_tl(),
        format!(" {} ", title).emphasis(),
        Glyphs::horiz().repeat(width.saturating_sub(title.len() + 5)),
    );

    let bottom = format!(
        "{}{}{}",
        Glyphs::corner_bl(),
        Glyphs::horiz().repeat(width),
        Glyphs::corner_br()
    );

    let mut result = format!("{}\n", top);
    for line in lines {
        result.push_str(&format!(
            "{} {:<width$} {}\n",
            Glyphs::vert(),
            line,
            Glyphs::vert(),
            width = max_width
        ));
    }
    result.push_str(&bottom);
    result
}

#[cfg(test)]
#[path = "../../tests/unit/ui/components/components_test.rs"]
mod tests;
