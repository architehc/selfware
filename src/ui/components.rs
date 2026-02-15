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
            owner_name: whoami::username(),
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
        format!("{} Homestead", Glyphs::HOME).garden_healthy()
    } else {
        format!("{} Remote", Glyphs::COMPASS).garden_wilting()
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
        Glyphs::CORNER_TL,
        Glyphs::HORIZ.repeat(width - 2),
        Glyphs::CORNER_TR
    );
    let bottom_border = format!(
        "{}{}{}",
        Glyphs::CORNER_BL,
        Glyphs::HORIZ.repeat(width - 2),
        Glyphs::CORNER_BR
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
        Glyphs::VERT.muted(),
        Glyphs::GEAR,
        mode_str,
        Glyphs::VERT.muted(),
        Glyphs::VERT.muted(),
        Glyphs::SPROUT,
        ctx.project_name.as_str().emphasis(),
        Glyphs::VERT.muted(),
        hosting,
        ctx.tasks_completed.to_string().garden_healthy(),
        bottom_border.muted(),
    )
}

/// Render a minimal status line
pub fn render_status_line(ctx: &WorkshopContext) -> String {
    let hosting = if ctx.is_local_model {
        format!("{} yours", Glyphs::HOME)
    } else {
        format!("{} remote", Glyphs::COMPASS)
    };

    format!(
        "{} {} {} {} {}",
        hosting.muted(),
        Glyphs::VERT.muted(),
        ctx.project_name.as_str().emphasis(),
        Glyphs::VERT.muted(),
        ctx.model_name.as_str().muted(),
    )
}

/// Render a task starting message
pub fn render_task_start(task: &str) -> String {
    format!(
        "\n{} {} beginning a new task in your garden...\n{} {}\n",
        Glyphs::SEEDLING,
        "Your companion is".craftsman_voice(),
        Glyphs::JOURNAL,
        task.emphasis()
    )
}

/// Render step progress
pub fn render_step(step: usize, phase: &str) -> String {
    let phase_glyph = match phase.to_lowercase().as_str() {
        "planning" => Glyphs::COMPASS,
        "executing" => Glyphs::HAMMER,
        "verifying" => Glyphs::MAGNIFIER,
        "reflecting" => Glyphs::JOURNAL,
        _ => Glyphs::GEAR,
    };

    format!(
        "{} {} Step {} · {}",
        phase_glyph,
        Glyphs::BRANCH.muted(),
        step.to_string().emphasis(),
        phase.craftsman_voice()
    )
}

/// Render tool execution
pub fn render_tool_call(tool_name: &str) -> String {
    let metaphor = super::style::tool_metaphor(tool_name);
    format!(
        "   {} {} {}...",
        Glyphs::WRENCH,
        metaphor.craftsman_voice(),
        format!("({})", tool_name).muted()
    )
}

/// Render tool success
pub fn render_tool_success(_tool_name: &str) -> String {
    format!(
        "   {} {}",
        Glyphs::BLOOM.garden_healthy(),
        "done".garden_healthy()
    )
}

/// Render tool failure
pub fn render_tool_error(_tool_name: &str, error: &str) -> String {
    format!(
        "   {} {} — {}",
        Glyphs::FROST,
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
        Glyphs::HARVEST,
        "Task complete.".garden_healthy(),
        time_str.muted()
    )
}

/// Render an error message
pub fn render_error(message: &str) -> String {
    format!(
        "\n{} {} {}\n",
        Glyphs::FROST,
        "A chill in the workshop:".garden_wilting(),
        message
    )
}

/// Format a raw error into a user-friendly message with actionable suggestions.
/// This translates technical error strings into language the user can act on.
pub fn format_user_friendly_error(error: &str) -> String {
    let error_lower = error.to_lowercase();

    // Connection refused
    if error_lower.contains("connection refused") || error_lower.contains("connrefused") {
        return format!(
            "{}\n   {} {}",
            "Could not connect to the API server.",
            Glyphs::BRANCH,
            "Suggestion: Check that your model server is running and the endpoint in your config is correct.".muted()
        );
    }

    // DNS / host resolution errors
    if error_lower.contains("dns error")
        || error_lower.contains("name or service not known")
        || error_lower.contains("no such host")
        || error_lower.contains("resolve")
    {
        return format!(
            "{}\n   {} {}",
            "Could not resolve the API server address.",
            Glyphs::BRANCH,
            "Suggestion: Verify the endpoint URL in your config. If using a local model, try http://localhost:<port>.".muted()
        );
    }

    // Rate limiting (check before generic status codes)
    if error_lower.contains("429") || error_lower.contains("too many requests") || error_lower.contains("rate limit") {
        return format!(
            "{}\n   {} {}",
            "Rate limited by the API server.",
            Glyphs::BRANCH,
            "Suggestion: Wait a moment and try again. The server is receiving too many requests.".muted()
        );
    }

    // Authentication errors
    if error_lower.contains("401") || error_lower.contains("unauthorized") || error_lower.contains("api key") {
        return format!(
            "{}\n   {} {}",
            "Authentication failed.",
            Glyphs::BRANCH,
            "Suggestion: Check that your API key is set correctly in the config or environment.".muted()
        );
    }

    // HTTP status code errors (check before generic "timeout" since "Gateway Timeout" contains "timeout")
    if error_lower.contains("504") || error_lower.contains("gateway timeout") {
        return format!(
            "{}\n   {} {}",
            "Gateway timeout -- the model took too long to respond.",
            Glyphs::BRANCH,
            "Suggestion: Try a simpler prompt or check if the model server is responsive.".muted()
        );
    }

    if error_lower.contains("503") || error_lower.contains("service unavailable") {
        return format!(
            "{}\n   {} {}",
            "The API service is temporarily unavailable.",
            Glyphs::BRANCH,
            "Suggestion: The server may be starting up or under maintenance. Retry shortly.".muted()
        );
    }

    if error_lower.contains("502") || error_lower.contains("bad gateway") {
        return format!(
            "{}\n   {} {}",
            "Bad gateway -- the API server's upstream is unreachable.",
            Glyphs::BRANCH,
            "Suggestion: The model backend may be restarting. Wait a moment and try again.".muted()
        );
    }

    if error_lower.contains("500") || error_lower.contains("internal server error") {
        return format!(
            "{}\n   {} {}",
            "The API server encountered an internal error.",
            Glyphs::BRANCH,
            "Suggestion: This is a server-side issue. Wait and retry, or check server logs.".muted()
        );
    }

    // Generic timeout (after specific HTTP timeouts)
    if error_lower.contains("timed out") || error_lower.contains("timeout") || error_lower.contains("deadline") {
        return format!(
            "{}\n   {} {}",
            "The request timed out waiting for a response.",
            Glyphs::BRANCH,
            "Suggestion: The model may be overloaded or the request too large. Try a shorter prompt or increase step_timeout_secs in config.".muted()
        );
    }

    // JSON parse errors
    if error_lower.contains("parse") && (error_lower.contains("json") || error_lower.contains("response")) {
        return format!(
            "{}\n   {} {}",
            "Received an unexpected response from the API.",
            Glyphs::BRANCH,
            "Suggestion: The API endpoint may not be OpenAI-compatible. Check your endpoint configuration.".muted()
        );
    }

    // File not found
    if error_lower.contains("no such file") || error_lower.contains("file not found") || error_lower.contains("notfound") {
        return format!(
            "{}\n   {} {}",
            "A file or path was not found.",
            Glyphs::BRANCH,
            "Suggestion: Double-check the file path. Use /analyze to survey your project structure.".muted()
        );
    }

    // Permission denied
    if error_lower.contains("permission denied") || error_lower.contains("access denied") {
        return format!(
            "{}\n   {} {}",
            "Permission denied.",
            Glyphs::BRANCH,
            "Suggestion: Check file permissions or run with appropriate access rights.".muted()
        );
    }

    // Default: return the original error without suggestion
    error.to_string()
}

/// Render a warning message
pub fn render_warning(message: &str) -> String {
    format!(
        "{} {} {}",
        Glyphs::WILT,
        "Note:".garden_wilting(),
        message.muted()
    )
}

/// Render checkpoint saved
pub fn render_checkpoint_saved(task_id: &str) -> String {
    format!(
        "{} {} · {}",
        Glyphs::BOOKMARK,
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
            format!("{} {}", Glyphs::BLOOM, "Complete".garden_healthy())
        } else {
            format!("{} {}", Glyphs::FROST, "Interrupted".garden_wilting())
        }
    }
}

/// Interactive prompt for the workshop
pub fn workshop_prompt() -> String {
    format!(
        "\n{} {} ",
        Glyphs::SPROUT,
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
        Glyphs::LANTERN,
        ctx.owner_name.as_str().emphasis(),
        Glyphs::SPROUT,
        ctx.companion_name.as_str().tool_name(),
        Glyphs::BOOKMARK,
        Glyphs::BRANCH.muted(),
        Glyphs::BRANCH.muted(),
        Glyphs::BRANCH.muted(),
        Glyphs::LEAF_BRANCH.muted(),
    )
}

/// Render the assistant's response
pub fn render_assistant_response(content: &str) -> String {
    format!(
        "\n{} {}\n\n{}\n",
        Glyphs::SPROUT,
        "Your companion says:".craftsman_voice(),
        content
    )
}

/// Render thinking/reasoning indicator
pub fn render_thinking() -> String {
    format!("{} {}", Glyphs::GEAR, "contemplating the garden...".muted())
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
        Glyphs::CORNER_TL,
        format!(" {} ", title).emphasis(),
        Glyphs::HORIZ.repeat(width.saturating_sub(title.len() + 5)),
    );

    let bottom = format!(
        "{}{}{}",
        Glyphs::CORNER_BL,
        Glyphs::HORIZ.repeat(width),
        Glyphs::CORNER_BR
    );

    let mut result = format!("{}\n", top);
    for line in lines {
        result.push_str(&format!(
            "{} {:<width$} {}\n",
            Glyphs::VERT,
            line,
            Glyphs::VERT,
            width = max_width
        ));
    }
    result.push_str(&bottom);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workshop_context_default() {
        let ctx = WorkshopContext::default();
        assert!(!ctx.owner_name.is_empty());
        assert_eq!(ctx.companion_name, "Selfware");
        assert!(!ctx.project_name.is_empty());
        assert!(!ctx.project_path.is_empty());
        assert_eq!(ctx.garden_age_days, 0);
        assert_eq!(ctx.tasks_completed, 0);
        assert_eq!(ctx.time_saved_hours, 0.0);
        assert!(ctx.is_local_model);
        assert_eq!(ctx.model_name, "local");
    }

    #[test]
    fn test_workshop_context_from_config_local() {
        let ctx = WorkshopContext::from_config("http://localhost:8080", "llama3");
        assert!(ctx.is_local_model);
        assert_eq!(ctx.model_name, "llama3");
    }

    #[test]
    fn test_workshop_context_from_config_remote() {
        let ctx = WorkshopContext::from_config("https://api.openai.com", "gpt-4");
        assert!(!ctx.is_local_model);
        assert_eq!(ctx.model_name, "gpt-4");
    }

    #[test]
    fn test_workshop_context_from_config_127() {
        let ctx = WorkshopContext::from_config("http://127.0.0.1:11434", "mistral");
        assert!(ctx.is_local_model);
    }

    #[test]
    fn test_render_header() {
        let ctx = WorkshopContext::default();
        let header = render_header(&ctx);
        assert!(header.contains("SELFWARE"));
        assert!(header.contains("WORKSHOP"));
    }

    #[test]
    fn test_render_header_remote() {
        let ctx = WorkshopContext::from_config("https://api.example.com", "remote-model");
        let header = render_header(&ctx);
        assert!(header.contains("SELFWARE"));
        assert!(header.contains("WORKSHOP"));
    }

    #[test]
    fn test_render_status_line_local() {
        let ctx = WorkshopContext::from_config("http://localhost:8080", "local-model");
        let status = render_status_line(&ctx);
        assert!(status.contains("yours"));
        assert!(status.contains("local-model"));
    }

    #[test]
    fn test_render_status_line_remote() {
        let ctx = WorkshopContext::from_config("https://api.example.com", "remote-model");
        let status = render_status_line(&ctx);
        assert!(status.contains("remote"));
    }

    #[test]
    fn test_render_task_start() {
        let task_msg = render_task_start("Fix the bug in login");
        assert!(task_msg.contains("Fix the bug in login"));
        assert!(task_msg.contains("companion"));
    }

    #[test]
    fn test_render_step() {
        let step = render_step(1, "planning");
        assert!(step.contains("Step 1"));
        assert!(step.contains("planning"));
    }

    #[test]
    fn test_render_step_phases() {
        // Test all phase types
        let phases = [
            "planning",
            "executing",
            "verifying",
            "reflecting",
            "unknown",
        ];
        for phase in phases {
            let step = render_step(1, phase);
            assert!(step.contains("Step 1"));
            assert!(step.contains(phase));
        }
    }

    #[test]
    fn test_render_tool_call() {
        let tool_msg = render_tool_call("file_read");
        assert!(tool_msg.contains("examining")); // metaphor for file_read
        assert!(tool_msg.contains("file_read"));
    }

    #[test]
    fn test_render_tool_success() {
        let success_msg = render_tool_success("file_read");
        assert!(success_msg.contains("done"));
    }

    #[test]
    fn test_render_tool_error() {
        let error_msg = render_tool_error("cargo_test", "tests failed");
        assert!(error_msg.contains("tests failed"));
        assert!(error_msg.contains("frost"));
    }

    #[test]
    fn test_render_task_complete() {
        let complete_msg = render_task_complete(Duration::from_secs(45));
        assert!(complete_msg.contains("complete"));
        assert!(complete_msg.contains("45s"));
    }

    #[test]
    fn test_render_task_complete_minutes() {
        let complete_msg = render_task_complete(Duration::from_secs(125));
        assert!(complete_msg.contains("2m 5s"));
    }

    #[test]
    fn test_render_error() {
        let error_msg = render_error("Something went wrong");
        assert!(error_msg.contains("Something went wrong"));
        assert!(error_msg.contains("chill"));
    }

    #[test]
    fn test_render_warning() {
        let warning_msg = render_warning("Be careful");
        assert!(warning_msg.contains("Be careful"));
        assert!(warning_msg.contains("Note"));
    }

    #[test]
    fn test_render_checkpoint_saved() {
        let checkpoint_msg = render_checkpoint_saved("task-123");
        assert!(checkpoint_msg.contains("task-123"));
        assert!(checkpoint_msg.contains("Journal"));
    }

    #[test]
    fn test_spinner() {
        let mut spinner = GardenSpinner::new("Testing");
        let frame1 = spinner.tick();
        let frame2 = spinner.tick();
        assert!(frame1.contains("Testing"));
        assert_ne!(frame1, frame2);
    }

    #[test]
    fn test_spinner_growth() {
        let mut spinner = GardenSpinner::growth();
        let frame1 = spinner.tick();
        let frame2 = spinner.tick();
        assert!(frame1.contains("Growing"));
        // Growth spinner should cycle through frames
        assert!(!frame1.is_empty());
        assert!(!frame2.is_empty());
    }

    #[test]
    fn test_spinner_finish_success() {
        let spinner = GardenSpinner::new("Task");
        let finish_msg = spinner.finish(true);
        assert!(finish_msg.contains("Complete"));
    }

    #[test]
    fn test_spinner_finish_failure() {
        let spinner = GardenSpinner::new("Task");
        let finish_msg = spinner.finish(false);
        assert!(finish_msg.contains("Interrupted"));
    }

    #[test]
    fn test_spinner_cycles() {
        let mut spinner = GardenSpinner::new("Cycling");
        // Tick through all frames and wrap around
        for _ in 0..16 {
            let frame = spinner.tick();
            assert!(frame.contains("Cycling"));
        }
    }

    #[test]
    fn test_workshop_prompt() {
        let prompt = workshop_prompt();
        assert!(prompt.contains("tend"));
    }

    #[test]
    fn test_render_welcome() {
        let ctx = WorkshopContext::default();
        let welcome = render_welcome(&ctx);
        assert!(welcome.contains("Welcome"));
        assert!(welcome.contains("workshop"));
        assert!(welcome.contains("/help"));
        assert!(welcome.contains("/status"));
        assert!(welcome.contains("/journal"));
        assert!(welcome.contains("/quit"));
    }

    #[test]
    fn test_render_assistant_response() {
        let response = render_assistant_response("Here is my answer");
        assert!(response.contains("Here is my answer"));
        assert!(response.contains("companion"));
    }

    #[test]
    fn test_render_thinking() {
        let thinking = render_thinking();
        assert!(thinking.contains("contemplating"));
    }

    #[test]
    fn test_render_box_simple() {
        let boxed = render_box("Title", "Content");
        assert!(boxed.contains("Title"));
        assert!(boxed.contains("Content"));
    }

    #[test]
    fn test_render_box_multiline() {
        let boxed = render_box("Multi", "Line 1\nLine 2\nLine 3");
        assert!(boxed.contains("Multi"));
        assert!(boxed.contains("Line 1"));
        assert!(boxed.contains("Line 2"));
        assert!(boxed.contains("Line 3"));
    }

    #[test]
    fn test_render_box_long_content() {
        let long_content = "x".repeat(100);
        let boxed = render_box("Long", &long_content);
        assert!(boxed.contains("Long"));
        assert!(boxed.contains(&long_content));
    }

    #[test]
    fn test_format_user_friendly_error_connection_refused() {
        let msg = format_user_friendly_error("Connection refused (os error 111)");
        assert!(msg.contains("Could not connect"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_timeout() {
        let msg = format_user_friendly_error("request timed out after 30s");
        assert!(msg.contains("timed out"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_rate_limit() {
        let msg = format_user_friendly_error("API error 429: too many requests");
        assert!(msg.contains("Rate limited"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_unauthorized() {
        let msg = format_user_friendly_error("API error 401: Unauthorized");
        assert!(msg.contains("Authentication failed"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_server_error() {
        let msg = format_user_friendly_error("API error 500: Internal Server Error");
        assert!(msg.contains("internal error"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_bad_gateway() {
        let msg = format_user_friendly_error("API error 502: Bad Gateway");
        assert!(msg.contains("Bad gateway"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_service_unavailable() {
        let msg = format_user_friendly_error("API error 503: Service Unavailable");
        assert!(msg.contains("temporarily unavailable"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_gateway_timeout() {
        let msg = format_user_friendly_error("API error 504: Gateway Timeout");
        assert!(msg.contains("Gateway timeout"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_dns() {
        let msg = format_user_friendly_error("dns error: Name or service not known");
        assert!(msg.contains("Could not resolve"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_json_parse() {
        let msg = format_user_friendly_error("Failed to parse response JSON");
        assert!(msg.contains("unexpected response"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_file_not_found() {
        let msg = format_user_friendly_error("No such file or directory");
        assert!(msg.contains("not found"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_permission_denied() {
        let msg = format_user_friendly_error("Permission denied");
        assert!(msg.contains("Permission denied"));
        assert!(msg.contains("Suggestion"));
    }

    #[test]
    fn test_format_user_friendly_error_unknown() {
        let msg = format_user_friendly_error("Some unknown random error");
        // Should return original error as-is
        assert_eq!(msg, "Some unknown random error");
    }
}
