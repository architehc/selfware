#![allow(dead_code, unused_imports, unused_variables)]
//! Built-in hook presets for common workflows.
//!
//! These are convenience constructors for frequently used hook configurations.

use super::{HookConfig, HookEvent};

/// Create a format-on-save hook that runs `cargo fmt` after file_write/file_edit.
pub fn format_on_save_rust() -> HookConfig {
    HookConfig {
        event: HookEvent::PostToolUse,
        command: "cargo fmt -- {path}".to_string(),
        match_tools: vec!["file_write".to_string(), "file_edit".to_string()],
        timeout_secs: 30,
    }
}

/// Create a lint-after-edit hook that runs `cargo clippy` after file changes.
pub fn lint_after_edit_rust() -> HookConfig {
    HookConfig {
        event: HookEvent::PostToolUse,
        command: "cargo clippy --quiet -- -D warnings 2>&1 | head -20".to_string(),
        match_tools: vec!["file_write".to_string(), "file_edit".to_string()],
        timeout_secs: 60,
    }
}

/// Create a test-on-stop hook that runs `cargo test` when the agent finishes.
pub fn test_on_stop_rust() -> HookConfig {
    HookConfig {
        event: HookEvent::Stop,
        command: "cargo test --quiet 2>&1 | tail -5".to_string(),
        match_tools: vec![],
        timeout_secs: 120,
    }
}

/// Create an auto-commit hook for file changes.
pub fn auto_commit() -> HookConfig {
    HookConfig {
        event: HookEvent::PostToolUse,
        command: "git add -- {path} && git commit -m '[selfware] auto-commit: '{tool}' '{path} --no-verify 2>/dev/null || true".to_string(),
        match_tools: vec!["file_write".to_string(), "file_edit".to_string()],
        timeout_secs: 15,
    }
}

/// Create a Python format-on-save hook.
pub fn format_on_save_python() -> HookConfig {
    HookConfig {
        event: HookEvent::PostToolUse,
        command: "ruff format -- {path} 2>/dev/null || black -- {path} 2>/dev/null || true"
            .to_string(),
        match_tools: vec!["file_write".to_string(), "file_edit".to_string()],
        timeout_secs: 15,
    }
}

/// Create a Node.js format-on-save hook.
pub fn format_on_save_node() -> HookConfig {
    HookConfig {
        event: HookEvent::PostToolUse,
        command: "npx prettier --write -- {path} 2>/dev/null || true".to_string(),
        match_tools: vec!["file_write".to_string(), "file_edit".to_string()],
        timeout_secs: 15,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/hooks/builtin/builtin_test.rs"]
mod tests;

#[cfg(test)]
mod auto_commit_quoting_regression_tests {
    use super::*;
    use crate::hooks::{HookAction, HookContext};

    #[test]
    fn test_auto_commit_and_format_hooks_include_double_dash_and_safe_quoting() {
        let hook = auto_commit();
        assert!(
            hook.command.contains("git add -- {path}"),
            "auto_commit must pass `--` before file operands: {}",
            hook.command
        );
        assert!(
            !hook.command.contains("\"[selfware]"),
            "auto_commit must not wrap {{tool}}/{{path}} inside double quotes: {}",
            hook.command
        );
        assert!(
            hook.command
                .contains("git commit -m '[selfware] auto-commit: '{tool}' '{path}"),
            "auto_commit should concatenate single-quoted placeholders safely: {}",
            hook.command
        );

        assert!(format_on_save_rust().command.contains("-- {path}"));
        assert!(format_on_save_python()
            .command
            .contains("ruff format -- {path}"));
        assert!(format_on_save_python().command.contains("black -- {path}"));
        assert!(format_on_save_node().command.contains("-- {path}"));
    }

    #[tokio::test]
    async fn test_auto_commit_executes_safely_with_spaces_and_metacharacters_in_path() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "test@example.com"][..],
            &["config", "user.name", "Test User"][..],
        ] {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .unwrap();
            assert!(status.success());
        }

        let sentinel = dir.path().join("injected_sentinel");
        let tricky_rel = format!(
            "dir with spaces/file $(touch {}) `id` 'quote'.rs",
            sentinel.display()
        );
        let tricky_full = repo.join(&tricky_rel);
        std::fs::create_dir_all(tricky_full.parent().unwrap()).unwrap();
        std::fs::write(&tricky_full, "fn main() {}\n").unwrap();

        let args_json = serde_json::json!({ "path": tricky_rel }).to_string();
        let ctx = HookContext::post_tool("file_write", &args_json, true, "ok");
        let root = crate::tools::workspace_root::WorkspaceRoot::fixed(repo.clone());

        let action =
            crate::hooks::shell_handler::execute_hook_in_root(&auto_commit(), &ctx, &root).await;
        assert!(matches!(action, HookAction::Continue));
        assert!(
            !sentinel.exists(),
            "shell metacharacters in path must not be evaluated"
        );

        let log_out = std::process::Command::new("git")
            .args(["log", "-1", "--pretty=%s"])
            .current_dir(&repo)
            .output()
            .unwrap();
        let subject = String::from_utf8_lossy(&log_out.stdout);
        assert!(
            subject.trim().contains(&tricky_rel),
            "commit subject should contain literal path, got: {subject}"
        );
    }
}
