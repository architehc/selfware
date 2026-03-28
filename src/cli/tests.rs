use super::*;

use std::path::Path;

    // ── truncate_with_ellipsis tests ──

    #[test]
    fn truncate_with_ellipsis_short_string_unchanged() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn truncate_with_ellipsis_adds_dots_when_over_limit() {
        // max_chars=8 means keep 5 chars + "..."
        assert_eq!(truncate_with_ellipsis("hello world", 8), "hello...");
    }

    #[test]
    fn truncate_with_ellipsis_empty_string() {
        assert_eq!(truncate_with_ellipsis("", 10), "");
        assert_eq!(truncate_with_ellipsis("", 0), "");
    }

    #[test]
    fn truncate_with_ellipsis_unicode_chars() {
        // Each emoji is 1 char but multiple bytes. "ab" = 2 chars, max=3 means no truncation needed for "ab"
        assert_eq!(truncate_with_ellipsis("ab", 3), "ab");
        // 5 chars total, max=4 => keep 1 + "..."
        let result = truncate_with_ellipsis("abcde", 4);
        assert_eq!(result, "a...");
    }

    #[test]
    fn truncate_with_ellipsis_max_less_than_three() {
        // max_chars=2, keep_chars = 2.saturating_sub(3) = 0, so just "..."
        assert_eq!(truncate_with_ellipsis("hello", 2), "...");
        assert_eq!(truncate_with_ellipsis("hello", 0), "...");
    }

    // ── take_prefix_chars tests ──

    #[test]
    fn take_prefix_chars_basic() {
        assert_eq!(take_prefix_chars("abcdef", 3), "abc");
        assert_eq!(take_prefix_chars("abcdef", 0), "");
        assert_eq!(take_prefix_chars("abcdef", 100), "abcdef");
    }

    #[test]
    fn take_prefix_chars_empty_string() {
        assert_eq!(take_prefix_chars("", 5), "");
    }

    // ── default_workflow_name tests ──

    #[test]
    fn default_workflow_name_extracts_stem() {
        assert_eq!(
            default_workflow_name(Path::new("my_workflow.yaml")),
            "my_workflow"
        );
        assert_eq!(
            default_workflow_name(Path::new("/path/to/deploy.yml")),
            "deploy"
        );
    }

    #[test]
    fn default_workflow_name_no_extension() {
        assert_eq!(default_workflow_name(Path::new("Makefile")), "Makefile");
    }

    #[test]
    fn default_workflow_name_falls_back_for_empty_path() {
        // Path with no file stem returns the default
        assert_eq!(default_workflow_name(Path::new("/")), DEFAULT_WORKFLOW_NAME);
    }

    // ── Theme / OutputFormat enum tests ──

    #[test]
    fn theme_default_is_amber() {
        let theme: Theme = Default::default();
        assert!(matches!(theme, Theme::Amber));
    }

    #[test]
    fn output_format_default_is_text() {
        let fmt: OutputFormat = Default::default();
        assert!(matches!(fmt, OutputFormat::Text));
    }

    // ── Constants sanity checks ──

    #[test]
    fn constants_have_reasonable_values() {
        let concurrency = DEFAULT_MULTI_CHAT_CONCURRENCY;
        assert!((1..=64).contains(&concurrency));
        let desc_max: usize = JOURNAL_DESC_MAX_CHARS;
        assert_ne!(desc_max, 0);
        let hash_prefix: usize = COMMIT_HASH_PREFIX_CHARS;
        assert_ne!(hash_prefix, 0);
        let max_errors: usize = MAX_JOURNAL_ERRORS_DISPLAY;
        assert_ne!(max_errors, 0);
        assert_ne!(DEFAULT_WORKFLOW_NAME, "");
    }

    // ── CLI flag parsing tests ──

    #[test]
    fn cli_chat_yolo_short_flag() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "chat", "-y"]).unwrap();
        match cli.command.unwrap() {
            Commands::Chat { yolo } => assert!(yolo, "chat -y should set yolo=true"),
            other => panic!("Expected Chat, got {:?}", other),
        }
    }

    #[test]
    fn cli_chat_yolo_long_flag() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "chat", "--yolo"]).unwrap();
        match cli.command.unwrap() {
            Commands::Chat { yolo } => assert!(yolo),
            other => panic!("Expected Chat, got {:?}", other),
        }
    }

    #[test]
    fn cli_chat_no_yolo() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "chat"]).unwrap();
        match cli.command.unwrap() {
            Commands::Chat { yolo } => assert!(!yolo, "chat without -y should be yolo=false"),
            other => panic!("Expected Chat, got {:?}", other),
        }
    }

    #[test]
    fn cli_run_yolo_flag() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "run", "-y", "fix bug"]).unwrap();
        match cli.command.unwrap() {
            Commands::Run { yolo, task } => {
                assert!(yolo, "run -y should set yolo=true");
                assert_eq!(task, "fix bug");
            }
            other => panic!("Expected Run, got {:?}", other),
        }
    }

    #[test]
    fn cli_multichat_yolo_flag() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "multi-chat", "-y"]).unwrap();
        match cli.command.unwrap() {
            Commands::MultiChat { yolo, .. } => assert!(yolo),
            other => panic!("Expected MultiChat, got {:?}", other),
        }
    }

    #[test]
    fn cli_global_yolo_still_works() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware", "-y", "chat"]).unwrap();
        assert!(cli.yolo, "global -y flag should still work");
    }

    #[test]
    fn cli_default_command_is_chat() {
        use clap::Parser;
        let cli = Cli::try_parse_from(["selfware"]).unwrap();
        // No subcommand → defaults to Chat { yolo: false }
        assert!(cli.command.is_none());
    }

    #[test]
    fn cli_command_tree_has_no_duplicate_aliases() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    // ── resolve_config_path tests ──

    #[test]
    fn resolve_config_path_no_flags_returns_none() {
        // No --config, no -C → None (Config::load does normal search)
        let result = resolve_config_path(None, false, Some(Path::new("/home/user/project")));
        assert!(result.is_none());
    }

    #[test]
    fn resolve_config_path_explicit_absolute_config() {
        // --config /etc/selfware.toml → returned as-is regardless of cwd or -C
        let result = resolve_config_path(
            Some("/etc/selfware.toml"),
            false,
            Some(Path::new("/home/user/project")),
        );
        assert_eq!(result.as_deref(), Some("/etc/selfware.toml"));
    }

    #[test]
    #[cfg(not(windows))] // Uses Unix paths
    fn resolve_config_path_explicit_relative_config_uses_original_cwd() {
        // --config my.toml with original cwd → absolutified against original cwd
        let result = resolve_config_path(
            Some("my.toml"),
            false,
            Some(Path::new("/home/user/project")),
        );
        assert_eq!(result.as_deref(), Some("/home/user/project/my.toml"));
    }

    #[test]
    #[cfg(not(windows))] // Uses Unix paths
    fn resolve_config_path_explicit_relative_config_with_workdir_uses_original_cwd() {
        // --config my.toml -C /other/dir → absolutified against ORIGINAL cwd, not /other/dir
        let result =
            resolve_config_path(Some("my.toml"), true, Some(Path::new("/home/user/project")));
        assert_eq!(result.as_deref(), Some("/home/user/project/my.toml"));
    }

    #[test]
    fn resolve_config_path_workdir_without_config_checks_original_cwd() {
        // -C /other/dir (no --config) → checks for selfware.toml in original cwd
        let tmp = tempfile::tempdir().unwrap();
        let config_file = tmp.path().join("selfware.toml");
        std::fs::write(&config_file, "[model]\nname = \"test\"\n").unwrap();

        let result = resolve_config_path(None, true, Some(tmp.path()));
        assert_eq!(
            result.as_deref(),
            Some(config_file.to_str().unwrap()),
            "should find selfware.toml in original cwd when -C is used"
        );
    }

    #[test]
    fn resolve_config_path_workdir_without_config_no_selfware_toml_returns_none() {
        // -C /other/dir (no --config), no selfware.toml in original cwd → None
        let tmp = tempfile::tempdir().unwrap();
        // Don't create selfware.toml

        let result = resolve_config_path(None, true, Some(tmp.path()));
        assert!(
            result.is_none(),
            "should return None when no selfware.toml in original cwd"
        );
    }

    #[test]
    fn resolve_config_path_no_original_cwd_falls_back_gracefully() {
        // Edge case: original_cwd is None (couldn't be determined)
        let result = resolve_config_path(Some("my.toml"), true, None);
        // Should still return the path, just not absolutified
        assert_eq!(result.as_deref(), Some("my.toml"));
    }
