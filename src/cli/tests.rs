use super::*;

use std::path::Path;

// ── parse_task_file tests ──

#[test]
fn parse_task_file_skips_blanks_and_comments() {
    let contents = "# This is a comment\n\ntask 1\n  \n# another comment\ntask 2\n";
    let tasks = parse_task_file(contents);
    assert_eq!(tasks, vec!["task 1", "task 2"]);
}

#[test]
fn parse_task_file_empty_contents() {
    assert!(parse_task_file("").is_empty());
}

#[test]
fn parse_task_file_all_comments() {
    let contents = "# comment 1\n# comment 2\n";
    assert!(parse_task_file(contents).is_empty());
}

#[test]
fn parse_task_file_trims_whitespace() {
    let contents = "  task with spaces  \n\t  tabbed task  \n";
    let tasks = parse_task_file(contents);
    assert_eq!(tasks, vec!["task with spaces", "tabbed task"]);
}

#[test]
fn parse_task_file_single_task_no_trailing_newline() {
    let tasks = parse_task_file("only task");
    assert_eq!(tasks, vec!["only task"]);
}

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
    let _guard = clear_config_env();
    // No --config, no -C → None (Config::load does normal search)
    let result = resolve_config_path(None, false, Some(Path::new("/home/user/project")));
    assert!(result.is_none());
}

#[test]
fn resolve_config_path_explicit_absolute_config() {
    let _guard = clear_config_env();
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
    let _guard = clear_config_env();
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
    let _guard = clear_config_env();
    // --config my.toml -C /other/dir → absolutified against ORIGINAL cwd, not /other/dir
    let result = resolve_config_path(Some("my.toml"), true, Some(Path::new("/home/user/project")));
    assert_eq!(result.as_deref(), Some("/home/user/project/my.toml"));
}

#[test]
fn resolve_config_path_workdir_without_config_checks_original_cwd() {
    let _guard = clear_config_env();
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
    let _guard = clear_config_env();
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
    let _guard = clear_config_env();
    // Edge case: original_cwd is None (couldn't be determined)
    let result = resolve_config_path(Some("my.toml"), true, None);
    // Should still return the path, just not absolutified
    assert_eq!(result.as_deref(), Some("my.toml"));
}

// ── SELFWARE_CONFIG + -C regression tests ──
//
// Bug history: setting `SELFWARE_CONFIG=/abs/path/selfware.toml` together with
// `-C /workdir` previously caused `resolve_config_path` to return an absolute
// path for `<original_cwd>/selfware.toml` (when one existed), shadowing the
// env-var setting and silently loading the wrong file. The fix returns `None`
// from `resolve_config_path` whenever `SELFWARE_CONFIG` is set, letting
// `Config::load` honour the env var.

fn clear_config_env() -> crate::config::test_helpers::EnvGuard {
    crate::config::test_helpers::clear_env()
}

#[test]
fn resolve_config_path_workdir_with_selfware_config_env_returns_none() {
    let _guard = clear_config_env();

    let tmp = tempfile::tempdir().unwrap();
    let original_cfg = tmp.path().join("selfware.toml");
    std::fs::write(&original_cfg, "model = \"A\"\n").unwrap();

    std::env::set_var("SELFWARE_CONFIG", "/some/other/path.toml");
    let result = resolve_config_path(None, true, Some(tmp.path()));
    std::env::remove_var("SELFWARE_CONFIG");

    assert!(
        result.is_none(),
        "SELFWARE_CONFIG must take precedence over original_cwd/selfware.toml; got {:?}",
        result
    );
}

#[test]
fn config_load_selfware_config_env_overrides_local_selfware_toml_with_workdir() {
    let _guard = clear_config_env();

    let dir_a = tempfile::tempdir().unwrap();
    let cfg_a = dir_a.path().join("selfware.toml");
    std::fs::write(
        &cfg_a,
        "model = \"A-model\"\nendpoint = \"http://a:1/v1\"\n",
    )
    .unwrap();

    let dir_b = tempfile::tempdir().unwrap();
    let cfg_b = dir_b.path().join("selfware.toml");
    std::fs::write(
        &cfg_b,
        "model = \"B-model\"\nendpoint = \"http://b:1/v1\"\n",
    )
    .unwrap();

    std::env::set_var("SELFWARE_CONFIG", cfg_b.to_str().unwrap());

    let resolved = resolve_config_path(None, false, Some(dir_a.path()));
    assert!(resolved.is_none());
    let cfg = Config::load(resolved.as_deref()).unwrap();
    assert_eq!(
        cfg.model, "B-model",
        "SELFWARE_CONFIG must override local selfware.toml; loaded {:?}",
        cfg.model
    );

    let resolved = resolve_config_path(None, true, Some(dir_a.path()));
    assert!(
        resolved.is_none(),
        "with -C and SELFWARE_CONFIG set, resolve must yield None to let the env var win; got {:?}",
        resolved
    );
    let cfg = Config::load(resolved.as_deref()).unwrap();
    assert_eq!(
        cfg.model, "B-model",
        "BUG: SELFWARE_CONFIG + -C silently loaded the wrong file (got {:?})",
        cfg.model
    );

    let dir_c = tempfile::tempdir().unwrap();
    let cfg_c = dir_c.path().join("explicit.toml");
    std::fs::write(&cfg_c, "model = \"C-model\"\n").unwrap();

    let resolved = resolve_config_path(Some(cfg_c.to_str().unwrap()), true, Some(dir_a.path()));
    let resolved_path = resolved.unwrap();
    let cfg = Config::load(Some(&resolved_path)).unwrap();
    assert_eq!(
        cfg.model, "C-model",
        "CLI --config must override SELFWARE_CONFIG"
    );

    std::env::remove_var("SELFWARE_CONFIG");
}

#[test]
fn config_show_renders_provenance_lines() {
    let _guard = clear_config_env();

    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("selfware.toml");
    std::fs::write(
        &cfg_path,
        "model = \"qwen-test\"\nendpoint = \"http://localhost:1234/v1\"\ntemperature = 0.7\n",
    )
    .unwrap();

    let cfg = Config::load(Some(cfg_path.to_str().unwrap())).unwrap();

    let model_src = cfg.source_of("model");
    assert!(
        matches!(model_src, crate::config::ConfigSource::ConfigFile(_)),
        "model source should be ConfigFile, got {:?}",
        model_src
    );

    let unset = cfg.source_of("agent.native_function_calling");
    assert!(
        matches!(unset, crate::config::ConfigSource::Default),
        "untouched key should be Default, got {:?}",
        unset
    );

    super::config_show(&cfg, false).unwrap();
    super::config_show(&cfg, true).unwrap();
}

// ── `selfware bench <subcommand>` parsing tests ──

#[test]
fn bench_legacy_form_still_parses() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "bench", "--suite", "throughput"]).unwrap();
    match cli.command.unwrap() {
        Commands::Bench {
            command,
            suite,
            concurrent,
            ..
        } => {
            assert!(
                command.is_none(),
                "legacy form should leave subcommand=None"
            );
            assert_eq!(suite, "throughput");
            assert_eq!(concurrent, 4); // default
        }
        other => panic!("expected Bench, got {:?}", other),
    }
}

#[test]
fn bench_swebench_pro_basic_parsing() {
    use args::BenchCommand;
    use clap::Parser;
    let cli = Cli::try_parse_from([
        "selfware",
        "bench",
        "swebench-pro",
        "--quants",
        "Q4_K_P,Q8_K_P",
        "--instances",
        "10",
        "--scenario-timeout",
        "1800",
        "--ctx",
        "262144",
        "--parallel",
        "1",
        "--concurrency",
        "1",
        "--trials",
        "3",
        "--output",
        "reports/swebench_pro/test",
    ])
    .unwrap();

    match cli.command.unwrap() {
        Commands::Bench { command, .. } => match command.unwrap() {
            BenchCommand::SwebenchPro(args) => {
                assert_eq!(args.quants, "Q4_K_P,Q8_K_P");
                assert_eq!(args.instances, 10);
                assert_eq!(args.scenario_timeout, 1800);
                assert_eq!(args.ctx, 262_144);
                assert_eq!(args.parallel, 1);
                assert_eq!(args.concurrency, 1);
                assert_eq!(args.trials, 3);
                assert_eq!(args.output.as_deref(), Some("reports/swebench_pro/test"));
                assert!(!args.skip_existing);
                assert!(!args.resume);
                assert!(!args.force_rerun);
            }
            other => panic!("expected SwebenchPro, got {:?}", other),
        },
        other => panic!("expected Bench, got {:?}", other),
    }
}

#[test]
fn bench_swebench_pro_defaults() {
    use args::BenchCommand;
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "bench", "swebench-pro"]).unwrap();
    match cli.command.unwrap() {
        Commands::Bench { command, .. } => match command.unwrap() {
            BenchCommand::SwebenchPro(args) => {
                // Defaults should match the documented values.
                assert_eq!(args.instances, 3);
                assert_eq!(args.scenario_timeout, 900);
                assert_eq!(args.ctx, 262_144);
                assert_eq!(args.parallel, 2);
                assert_eq!(args.trials, 1);
                assert!(args.quants.contains("Q4_K_P"));
                assert!(!args.resume);
                assert!(!args.force_rerun);
                assert_eq!(args.prompt_mode, "official");
            }
            other => panic!("expected SwebenchPro, got {:?}", other),
        },
        other => panic!("expected Bench, got {:?}", other),
    }
}

#[test]
fn bench_swebench_pro_instance_ids_overrides_count() {
    use args::BenchCommand;
    use clap::Parser;
    let cli = Cli::try_parse_from([
        "selfware",
        "bench",
        "swebench-pro",
        "--instance-ids",
        "foo-1,foo-2",
    ])
    .unwrap();
    match cli.command.unwrap() {
        Commands::Bench { command, .. } => match command.unwrap() {
            BenchCommand::SwebenchPro(args) => {
                assert_eq!(args.instance_ids.as_deref(), Some("foo-1,foo-2"));
            }
            other => panic!("expected SwebenchPro, got {:?}", other),
        },
        other => panic!("expected Bench, got {:?}", other),
    }
}

#[test]
fn bench_swebench_pro_official_eval_flags_parse() {
    use args::BenchCommand;
    use clap::Parser;
    let cli = Cli::try_parse_from([
        "selfware",
        "bench",
        "swebench-pro",
        "--official-eval",
        "--prompt-mode",
        "official",
        "--official-eval-script",
        "/tmp/eval.py",
        "--official-eval-raw-sample-path",
        "/tmp/sample.jsonl",
        "--official-eval-scripts-dir",
        "/tmp/run_scripts",
        "--official-eval-dockerhub-username",
        "example",
        "--official-eval-num-workers",
        "2",
        "--official-eval-modal",
        "--official-eval-redo",
        "--official-eval-block-network",
    ])
    .unwrap();
    match cli.command.unwrap() {
        Commands::Bench { command, .. } => match command.unwrap() {
            BenchCommand::SwebenchPro(args) => {
                assert!(args.official_eval);
                assert_eq!(args.prompt_mode, "official");
                assert_eq!(args.official_eval_script, "/tmp/eval.py");
                assert_eq!(args.official_eval_raw_sample_path, "/tmp/sample.jsonl");
                assert_eq!(args.official_eval_scripts_dir, "/tmp/run_scripts");
                assert_eq!(args.official_eval_dockerhub_username, "example");
                assert_eq!(args.official_eval_num_workers, 2);
                assert!(args.official_eval_modal);
                assert!(args.official_eval_redo);
                assert!(args.official_eval_block_network);
            }
            other => panic!("expected SwebenchPro, got {:?}", other),
        },
        other => panic!("expected Bench, got {:?}", other),
    }
}

#[test]
fn bench_other_subcommands_parse_for_help_visibility() {
    use args::BenchCommand;
    use clap::Parser;
    for sub in ["throughput", "multilang", "browser", "long-run"] {
        let cli = Cli::try_parse_from(["selfware", "bench", sub])
            .unwrap_or_else(|e| panic!("failed to parse `bench {}`: {}", sub, e));
        match cli.command.unwrap() {
            Commands::Bench { command, .. } => {
                let cmd = command.expect("subcommand parsed");
                let label = match cmd {
                    BenchCommand::Throughput => "throughput",
                    BenchCommand::Multilang => "multilang",
                    BenchCommand::Browser => "browser",
                    BenchCommand::LongRun => "long-run",
                    BenchCommand::SwebenchPro(_) => "swebench-pro",
                };
                assert_eq!(label, sub);
            }
            other => panic!("expected Bench, got {:?}", other),
        }
    }
}
