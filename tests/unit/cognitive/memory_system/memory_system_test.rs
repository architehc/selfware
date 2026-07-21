use super::*;
use tempfile::tempdir;

// Windows: `dirs::home_dir()` reads `%USERPROFILE%`, not `$HOME`, so the
// env-var override below doesn't redirect the lookup. Setting USERPROFILE
// would race with other tests in the same process. The path-safety
// production fix is orthogonal to this env-var-channel mismatch.
#[cfg(not(target_os = "windows"))]
#[test]
fn test_discover_finds_files_up_to_home() {
    let temp = tempfile::tempdir().unwrap();
    // macOS canonicalizes `/var/folders/...` to `/private/var/folders/...`;
    // the path walked by `discover` follows the canonical form, so we must
    // anchor `home`/`project` on the same canonical root or the
    // PathBuf comparisons below silently miss.
    let temp_root = temp
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp.path().to_path_buf());
    let home = temp_root.join("home");
    let project = home.join("projects").join("myapp");
    let src = project.join("src");
    std::fs::create_dir_all(&src).unwrap();

    // Create two memory files: one in project root, one in home
    let project_memory = project.join(".selfware.md");
    let home_memory = home.join(".selfware.md");
    let outside_memory = temp_root.join(".selfware.md");

    std::fs::write(&project_memory, "project memory").unwrap();
    std::fs::write(&home_memory, "home memory").unwrap();
    std::fs::write(&outside_memory, "outside memory").unwrap();

    // Temporarily override home_dir by using a path inside our temp tree.
    // Since `dirs::home_dir()` reads env vars, we can set HOME.
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", &home);

    let files = MemorySystem::discover(&src);

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, project_memory);
    assert_eq!(files[0].content, "project memory");
    assert_eq!(files[1].path, home_memory);
    assert_eq!(files[1].content, "home memory");
}

#[test]
fn test_format_for_prompt_empty() {
    let formatted = MemorySystem::format_for_prompt(&[]);
    assert!(formatted.is_empty());
}

#[test]
fn test_format_for_prompt_with_files() {
    let files = vec![
        MemoryFile {
            path: PathBuf::from("/project/.selfware.md"),
            content: "Use Rust 2021 edition.".to_string(),
        },
        MemoryFile {
            path: PathBuf::from("/home/.selfware.md"),
            content: "Prefer anyhow for errors.".to_string(),
        },
    ];
    let formatted = MemorySystem::format_for_prompt(&files);
    assert!(formatted.starts_with("## Memory Files"));
    assert!(formatted.contains("### From `/project/.selfware.md`"));
    assert!(formatted.contains("Use Rust 2021 edition."));
    assert!(formatted.contains("### From `/home/.selfware.md`"));
    assert!(formatted.contains("Prefer anyhow for errors."));
}

// See `test_discover_finds_files_up_to_home` for the rationale on the
// Windows gate (USERPROFILE vs HOME).
#[cfg(not(target_os = "windows"))]
#[test]
fn test_discover_workspace_guidance_finds_agents_files_up_to_home() {
    let temp = tempfile::tempdir().unwrap();
    // Canonicalize on macOS where the temp dir resolves through /private.
    let temp_root = temp
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp.path().to_path_buf());
    let home = temp_root.join("home");
    let project = home.join("projects").join("myproject");
    let src = project.join("src");
    std::fs::create_dir_all(&src).unwrap();

    let project_guidance = project.join("AGENTS.md");
    let home_guidance = home.join("CLAUDE.md");
    let outside_guidance = temp_root.join("AGENTS.md");

    std::fs::write(&project_guidance, "project guidance").unwrap();
    std::fs::write(&home_guidance, "home guidance").unwrap();
    std::fs::write(&outside_guidance, "outside guidance").unwrap();

    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", &home);

    let files = MemorySystem::discover_workspace_guidance(&src);

    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, project_guidance);
    assert_eq!(files[0].content, "project guidance");
    assert_eq!(files[1].path, home_guidance);
    assert_eq!(files[1].content, "home guidance");
}

#[test]
fn test_format_workspace_guidance_for_prompt() {
    let files = vec![WorkspaceGuidanceFile {
        path: PathBuf::from("/project/AGENTS.md"),
        content: "Operate on /tmp/project.".to_string(),
    }];

    let formatted = MemorySystem::format_workspace_guidance_for_prompt(&files);
    assert!(formatted.contains("## Workspace Guidance"));
    assert!(formatted.contains("Follow the most local guidance file"));
    assert!(formatted.contains("### From `/project/AGENTS.md`"));
    assert!(formatted.contains("Operate on /tmp/project."));
}

#[test]
fn test_discover_workspace_guidance_truncates_large_files() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let project = home.join("project");
    std::fs::create_dir_all(&project).unwrap();

    let large_guidance = project.join("AGENTS.md");
    std::fs::write(
        &large_guidance,
        "a".repeat(MAX_WORKSPACE_GUIDANCE_BYTES + 128),
    )
    .unwrap();

    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", &home);

    let files = MemorySystem::discover_workspace_guidance(&project);

    assert_eq!(files.len(), 1);
    assert!(files[0].content.ends_with("... [truncated]"));
    assert!(files[0].content.len() <= MAX_WORKSPACE_GUIDANCE_BYTES + 32);
}

#[test]
fn test_project_key_from_path() {
    // Use platform-native absolute paths so component splitting works
    // identically: Unix uses `/` separators, Windows uses `\` plus a
    // drive letter prefix component.
    #[cfg(unix)]
    let abs = Path::new("/home/user/projects/myapp");
    #[cfg(windows)]
    let abs = Path::new(r"C:\Users\user\projects\myapp");

    let key = MemorySystem::project_key_from_path(abs);
    assert_eq!(key, "projects_myapp");

    let path = Path::new("myapp");
    let key = MemorySystem::project_key_from_path(path);
    assert_eq!(key, "myapp");

    // Root path returns the OS-native root as the key (single component).
    #[cfg(unix)]
    {
        let path = Path::new("/");
        let key = MemorySystem::project_key_from_path(path);
        assert_eq!(key, "/");
    }
}

// See `test_discover_finds_files_up_to_home`: HOME-vs-USERPROFILE makes
// this Windows-incompatible regardless of path normalization.
#[cfg(not(target_os = "windows"))]
#[test]
fn test_discover_consolidated_memory() {
    let temp = tempdir().unwrap();
    let temp_root = temp
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp.path().to_path_buf());
    let home = temp_root.join("home");
    let project = home.join("projects").join("myapp");
    let memory_base = home.join(".selfware").join("memory");

    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&memory_base).unwrap();

    // Create a consolidated memory file
    let memory_content = "# Project Memory\n\n## Facts\n- [2026-03-31] Test fact\n";
    std::fs::write(memory_base.join("projects_myapp_MEMORY.md"), memory_content).unwrap();

    // Set HOME to temp directory
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", &home);

    let memories = MemorySystem::discover_consolidated(&project);

    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].project_key, "projects_myapp");
    assert!(memories[0].content.contains("Test fact"));
}

#[test]
fn test_format_consolidated_for_prompt() {
    let memories = vec![ConsolidatedMemory {
        project_key: "test_project".to_string(),
        path: PathBuf::from("/tmp/test_MEMORY.md"),
        content: "- Fact 1\n- Fact 2".to_string(),
    }];

    let formatted = MemorySystem::format_consolidated_for_prompt(&memories);
    assert!(formatted.contains("## Consolidated Project Memory"));
    assert!(formatted.contains("### Project: test_project"));
    assert!(formatted.contains("- Fact 1"));
}

#[tokio::test]
async fn test_dream_integrated_memory_system_creation() {
    let temp = tempdir().unwrap();
    // Create a nested structure to avoid tempdir random prefix
    let parent = temp.path().join("projects");
    let project = parent.join("myapp");
    std::fs::create_dir_all(&project).unwrap();

    let system = DreamIntegratedMemorySystem::new(&project);
    // Should use "projects_myapp" as key (last two components)
    assert_eq!(system.project_key, "projects_myapp");
}

// See `test_discover_finds_files_up_to_home`: HOME-vs-USERPROFILE makes
// this Windows-incompatible regardless of path normalization.
#[cfg(not(target_os = "windows"))]
#[tokio::test]
async fn test_dream_integrated_memory_system_load_consolidated() {
    let temp = tempdir().unwrap();
    let temp_root = temp
        .path()
        .canonicalize()
        .unwrap_or_else(|_| temp.path().to_path_buf());
    let home = temp_root.join("home");
    let project = home.join("myapp");
    let memory_base = home.join(".selfware").join("memory");

    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&memory_base).unwrap();

    // Create a consolidated memory file
    let memory_content = "# Project Memory\n\n## Facts\n- [2026-03-31] Test fact\n";
    std::fs::write(memory_base.join("home_myapp_MEMORY.md"), memory_content).unwrap();

    // Set HOME to temp directory
    let env = crate::test_support::EnvGuard::capture(&["HOME"]);
    env.set("HOME", &home);

    let system = DreamIntegratedMemorySystem::new(&project);
    let memory = system.load_consolidated_memory();

    assert!(memory.is_some());
    assert!(memory.unwrap().content.contains("Test fact"));
}

#[tokio::test]
async fn test_dream_integrated_record_session_end() {
    use crate::cognitive::dream::DreamConfig;

    let temp = tempdir().unwrap();
    let memory_base = temp.path().join("memory");
    let project = temp.path().join("projects").join("myapp");
    std::fs::create_dir_all(&project).unwrap();

    // Create system with isolated dream config
    let dream_config = DreamConfig::new().with_base_dir(&memory_base);
    let system = DreamIntegratedMemorySystem::new(&project).with_dream_config(dream_config);

    // Record session end
    system.record_session_end().unwrap();

    // Verify state was updated
    let status = system.dream_status().await;
    assert_eq!(status.sessions_since_last_dream, 1);
}
