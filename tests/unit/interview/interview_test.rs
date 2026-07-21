use super::*;

#[test]
fn test_analyze_task_web_api() {
    let hints = analyze_task("Build a REST API for user management");
    assert!(hints.suggests_web_api);
    assert!(!hints.suggests_cli);
}

#[test]
fn test_analyze_task_cli() {
    let hints = analyze_task("Create a CLI tool that converts images");
    assert!(hints.suggests_cli);
    assert!(!hints.suggests_web_api);
}

#[test]
fn test_analyze_task_language_mention() {
    let hints = analyze_task("Write a Python script to parse CSV files");
    assert_eq!(hints.mentioned_languages, vec!["Python"]);
    assert!(hints.suggests_script);
}

#[test]
fn test_analyze_task_multiple_languages() {
    let hints = analyze_task("Port this Rust library to Go");
    assert!(hints.mentioned_languages.contains(&"Rust".to_string()));
    assert!(hints.mentioned_languages.contains(&"Go".to_string()));
}

#[test]
fn test_detect_existing_project_rust() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
    assert_eq!(
        detect_existing_project(dir.path()),
        Some("Rust".to_string())
    );
}

#[test]
fn test_detect_existing_project_typescript() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), "{}").unwrap();
    std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
    assert_eq!(
        detect_existing_project(dir.path()),
        Some("TypeScript".to_string())
    );
}

#[test]
fn test_detect_existing_project_node() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), "{}").unwrap();
    assert_eq!(
        detect_existing_project(dir.path()),
        Some("JavaScript/Node.js".to_string())
    );
}

#[test]
fn test_detect_existing_project_python() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("pyproject.toml"), "[project]").unwrap();
    assert_eq!(
        detect_existing_project(dir.path()),
        Some("Python".to_string())
    );
}

#[test]
fn test_detect_existing_project_go() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("go.mod"), "module example").unwrap();
    assert_eq!(detect_existing_project(dir.path()), Some("Go".to_string()));
}

#[test]
fn test_detect_existing_project_none() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(detect_existing_project(dir.path()), None);
}

#[test]
fn test_parse_project_type() {
    assert_eq!(parse_project_type("CLI tool"), ProjectType::CliTool);
    assert_eq!(parse_project_type("Web API / backend"), ProjectType::WebApi);
    assert_eq!(
        parse_project_type("Script / automation"),
        ProjectType::Script
    );
    assert_eq!(parse_project_type("Library / crate"), ProjectType::Library);
}

#[test]
fn test_parse_testing_preference() {
    assert_eq!(
        parse_testing_preference("Full TDD (tests first)"),
        TestingPreference::Tdd
    );
    assert_eq!(
        parse_testing_preference("Tests after implementation"),
        TestingPreference::TestsAfter
    );
    assert_eq!(
        parse_testing_preference("Minimal tests (just critical paths)"),
        TestingPreference::Minimal
    );
    assert_eq!(
        parse_testing_preference("No tests"),
        TestingPreference::None
    );
}

#[test]
fn test_parse_scope() {
    assert_eq!(
        parse_scope("Quick script (< 100 lines)"),
        ProjectScope::Quick
    );
    assert_eq!(
        parse_scope("Small project (100-500 lines)"),
        ProjectScope::Small
    );
    assert_eq!(
        parse_scope("Medium project (500-2000 lines)"),
        ProjectScope::Medium
    );
    assert_eq!(
        parse_scope("Large project (2000+ lines)"),
        ProjectScope::Large
    );
}

#[test]
fn test_interview_context_to_system_prompt_section() {
    let ctx = InterviewContext {
        language: Some("Rust".to_string()),
        framework: Some("axum".to_string()),
        project_type: Some(ProjectType::WebApi),
        testing_preference: Some(TestingPreference::Tdd),
        output_dir: Some(".".to_string()),
        scope: Some(ProjectScope::Medium),
        extra_notes: vec![],
        task: "Build a REST API".to_string(),
    };
    let section = ctx.to_system_prompt_section();
    assert!(section.contains("Rust"));
    assert!(section.contains("axum"));
    assert!(section.contains("Web API"));
    assert!(section.contains("TDD"));
    assert!(section.contains("Medium project"));
}

#[test]
fn test_interview_context_empty() {
    let ctx = InterviewContext {
        language: None,
        framework: None,
        project_type: None,
        testing_preference: None,
        output_dir: None,
        scope: None,
        extra_notes: vec![],
        task: "do something".to_string(),
    };
    assert!(ctx.is_empty());
    let section = ctx.to_system_prompt_section();
    assert!(section.contains("no preferences specified"));
}

#[test]
fn test_interview_context_not_empty() {
    let ctx = InterviewContext {
        language: Some("Python".to_string()),
        framework: None,
        project_type: None,
        testing_preference: None,
        output_dir: None,
        scope: None,
        extra_notes: vec![],
        task: "do something".to_string(),
    };
    assert!(!ctx.is_empty());
}

#[test]
fn test_framework_options_for_rust() {
    let opts = framework_options_for("Rust");
    assert!(opts.len() >= 5);
    assert!(opts.iter().any(|o| o.label.contains("axum")));
}

#[test]
fn test_framework_options_for_python() {
    let opts = framework_options_for("Python");
    assert!(opts.iter().any(|o| o.label.contains("FastAPI")));
}

#[test]
fn test_framework_options_for_typescript() {
    let opts = framework_options_for("TypeScript / Node.js");
    assert!(opts.iter().any(|o| o.label.contains("Next.js")));
}

#[test]
fn test_framework_options_for_go() {
    let opts = framework_options_for("Go");
    assert!(opts.iter().any(|o| o.label.contains("gin")));
}

#[test]
fn test_framework_options_for_unknown() {
    let opts = framework_options_for("Haskell");
    assert_eq!(opts.len(), 1);
    assert!(opts[0].label.contains("specify"));
}

#[test]
fn test_infer_project_type_default_web_api() {
    let hints = analyze_task("Build a REST API");
    assert_eq!(infer_project_type_default(&hints), Some('b'));
}

#[test]
fn test_infer_project_type_default_cli() {
    let hints = analyze_task("Create a CLI tool");
    assert_eq!(infer_project_type_default(&hints), Some('a'));
}

#[test]
fn test_infer_scope_default_script() {
    let hints = analyze_task("Write a quick script");
    assert_eq!(infer_scope_default(&hints), Some('a'));
}

#[test]
fn test_display_impls() {
    assert_eq!(format!("{}", ProjectType::CliTool), "CLI tool");
    assert_eq!(
        format!("{}", TestingPreference::Tdd),
        "Full TDD (tests first)"
    );
    assert_eq!(
        format!("{}", ProjectScope::Medium),
        "Medium project (500-2000 lines)"
    );
}

#[test]
fn test_session_to_context() {
    let session = InterviewSession {
        language: Some("Rust".to_string()),
        framework: None,
        project_type: Some(ProjectType::CliTool),
        testing_preference: None,
        output_dir: None,
        scope: None,
        extra_notes: vec!["keep it simple".to_string()],
    };
    let ctx = session_to_context(session, "build something");
    assert_eq!(ctx.task, "build something");
    assert_eq!(ctx.language.as_deref(), Some("Rust"));
    assert_eq!(ctx.project_type, Some(ProjectType::CliTool));
    assert_eq!(ctx.extra_notes, vec!["keep it simple"]);
}
