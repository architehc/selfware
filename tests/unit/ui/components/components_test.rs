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
    assert!(step.contains("Step"));
    assert!(step.contains("1"));
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
        assert!(step.contains("Step"));
        assert!(step.contains("1"));
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
