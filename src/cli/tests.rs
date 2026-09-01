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

// ── Theme / HeadlessOutputFormat enum tests ──

#[test]
fn theme_default_is_amber() {
    let theme: Theme = Default::default();
    assert!(matches!(theme, Theme::Amber));
}

#[test]
fn headless_output_format_default_is_text() {
    let fmt: HeadlessOutputFormat = Default::default();
    assert!(matches!(fmt, HeadlessOutputFormat::Text));
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
    assert!(
        cli.yolo,
        "trailing -y after subcommand should set the global yolo flag"
    );
    assert!(matches!(cli.command.unwrap(), Commands::Chat));
}

#[test]
fn cli_chat_yolo_long_flag() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "chat", "--yolo"]).unwrap();
    assert!(cli.yolo);
    assert!(matches!(cli.command.unwrap(), Commands::Chat));
}

#[test]
fn cli_chat_no_yolo() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "chat"]).unwrap();
    assert!(!cli.yolo, "chat without -y should leave yolo=false");
    assert!(matches!(cli.command.unwrap(), Commands::Chat));
}

#[test]
fn cli_run_yolo_flag() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "run", "-y", "fix bug"]).unwrap();
    assert!(cli.yolo, "run -y should set the global yolo flag");
    match cli.command.unwrap() {
        Commands::Run { task, .. } => assert_eq!(task.as_deref(), Some("fix bug")),
        other => panic!("Expected Run, got {:?}", other),
    }
}

#[test]
fn cli_multichat_yolo_flag() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "multi-chat", "-y"]).unwrap();
    assert!(cli.yolo);
    assert!(matches!(cli.command.unwrap(), Commands::MultiChat { .. }));
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
    // No subcommand → defaults to Chat
    assert!(cli.command.is_none());
}

#[test]
fn cli_command_tree_has_no_duplicate_aliases() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}

// ── P0-1 regression: `selfware status` must parse (clap TypeId panic) ──
//
// The Status subcommand used to declare its own `output_format` arg of a
// different enum type than the global `--output-format`, which made clap
// panic with "Mismatch between definition and access of 'output_format'"
// (exit 101) on every `selfware status` invocation. The per-subcommand arg
// was removed; the global flag now covers it.

#[test]
fn cli_status_parses_without_args() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "status"]).unwrap();
    assert!(matches!(cli.command.unwrap(), Commands::Status));
}

#[test]
fn cli_status_uses_global_output_format() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "status", "--output-format", "json"]).unwrap();
    assert!(matches!(cli.command.unwrap(), Commands::Status));
    assert_eq!(cli.output_format, HeadlessOutputFormat::Json);
}

#[test]
fn cli_every_subcommand_help_is_valid() {
    // debug_assert validates the whole command tree recursively (arg
    // conflicts, duplicate shorts/longs from global propagation, etc.).
    // This catches the P0-1 class of bug at test time instead of at runtime.
    use clap::CommandFactory;
    Cli::command().debug_assert();
}

// ── P0-3 regression: flags must be accepted AFTER the subcommand ──
//
// Only `--output-format` was global, so `selfware run "task" -m yolo` died
// with "unexpected argument '-m'" (exit 2). The common flags are now global.

#[test]
fn cli_trailing_mode_and_max_turns_after_run() {
    use clap::Parser;
    let cli =
        Cli::try_parse_from(["selfware", "run", "x", "-m", "yolo", "--max-turns", "1"]).unwrap();
    assert_eq!(cli.mode, Some(ExecutionMode::Yolo));
    assert_eq!(cli.max_turns, Some(1));
    match cli.command.unwrap() {
        Commands::Run { task, .. } => assert_eq!(task.as_deref(), Some("x")),
        other => panic!("Expected Run, got {:?}", other),
    }
}

#[test]
fn cli_trailing_yolo_after_run_task() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "run", "fix the bug", "-y"]).unwrap();
    assert!(cli.yolo);
    match cli.command.unwrap() {
        Commands::Run { task, .. } => assert_eq!(task.as_deref(), Some("fix the bug")),
        other => panic!("Expected Run, got {:?}", other),
    }
}

#[test]
fn cli_trailing_config_workdir_quiet_verbose_after_subcommand() {
    use clap::Parser;
    let cli = Cli::try_parse_from([
        "selfware", "status", "-q", "-v", "-c", "my.toml", "-C", "/tmp",
    ])
    .unwrap();
    assert!(cli.quiet);
    assert!(cli.verbose);
    assert_eq!(cli.config.as_deref(), Some("my.toml"));
    assert_eq!(cli.workdir.as_deref(), Some("/tmp"));
}

#[test]
fn cli_trailing_max_budget_flags_after_run() {
    use clap::Parser;
    let cli = Cli::try_parse_from([
        "selfware",
        "run",
        "x",
        "--max-budget-tokens",
        "1000",
        "--max-wall-secs",
        "60",
        "--max-cost-usd",
        "0.5",
    ])
    .unwrap();
    assert_eq!(cli.max_budget_tokens, Some(1000));
    assert_eq!(cli.max_wall_secs, Some(60));
    assert_eq!(cli.max_cost_usd, Some(0.5));
}

#[test]
fn cli_hidden_dev_commands_still_parse() {
    // Hidden from --help but still functional.
    use clap::Parser;
    for argv in [
        &["selfware", "test"][..],
        &["selfware", "bench"][..],
        &["selfware", "long-test"][..],
        &["selfware", "lsp"][..],
        &["selfware", "batch", "-f", "tasks.txt"][..],
        &["selfware", "swe-bench", "diagnose", "/tmp/out"][..],
    ] {
        assert!(
            Cli::try_parse_from(argv).is_ok(),
            "hidden command should still parse: {:?}",
            argv
        );
    }
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

fn clear_config_env() -> crate::test_support::EnvGuard {
    crate::test_support::EnvGuard::clear_selfware_env()
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
                assert_eq!(args.official_eval_script.as_deref(), Some("/tmp/eval.py"));
                assert_eq!(
                    args.official_eval_raw_sample_path.as_deref(),
                    Some("/tmp/sample.jsonl")
                );
                assert_eq!(
                    args.official_eval_scripts_dir.as_deref(),
                    Some("/tmp/run_scripts")
                );
                assert_eq!(args.official_eval_dockerhub_username, "example");
                assert_eq!(args.official_eval_num_workers, 2);
                assert!(args.official_eval_modal);
                assert!(args.official_eval_redo);
                assert!(args.official_eval_block_network);
            }
        },
        other => panic!("expected Bench, got {:?}", other),
    }
}

#[test]
fn official_eval_requires_explicit_paths() {
    use args::SwebenchProArgs;

    // --official-eval without the three paths must fail with a helpful error.
    let args = SwebenchProArgs {
        official_eval: true,
        ..Default::default()
    };
    let err = crate::cli::resolve_official_eval_paths(&args).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("--official-eval-script"), "got: {msg}");
    assert!(msg.contains("--official-eval-scripts-dir"), "got: {msg}");

    // Without --official-eval the paths are unused — empty triple is fine.
    let args = SwebenchProArgs::default();
    assert!(crate::cli::resolve_official_eval_paths(&args).is_ok());
}

#[test]
fn endpoint_is_local_detects_localhost_variants() {
    assert!(endpoint_is_local("http://localhost:8000/v1"));
    assert!(endpoint_is_local("http://127.0.0.1:1234/v1"));
    assert!(endpoint_is_local("http://0.0.0.0:8080/v1"));
    assert!(endpoint_is_local("http://[::1]:8000/v1"));
    assert!(!endpoint_is_local("https://openrouter.ai/api/v1"));
    assert!(!endpoint_is_local("https://api.example.com/v1"));
}

// ── P1-5: --coordinator / --profile are global (usable after the subcommand) ──
//
// `selfware multi-chat --coordinator` used to be a clap parse error because
// both flags were top-level-only, unlike every other common flag.

#[test]
fn cli_multi_chat_trailing_coordinator_flag() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "multi-chat", "--coordinator"]).unwrap();
    assert!(cli.coordinator);
    assert!(matches!(cli.command.unwrap(), Commands::MultiChat { .. }));
}

#[test]
fn cli_leading_coordinator_before_multi_chat() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "--coordinator", "multi-chat"]).unwrap();
    assert!(cli.coordinator);
    assert!(matches!(cli.command.unwrap(), Commands::MultiChat { .. }));
}

#[test]
fn cli_profile_before_multi_chat() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "--profile", "swarm-8", "multi-chat"]).unwrap();
    assert_eq!(cli.profile.as_deref(), Some("swarm-8"));
    assert!(matches!(cli.command.unwrap(), Commands::MultiChat { .. }));
}

#[test]
fn cli_trailing_profile_after_multi_chat() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "multi-chat", "--profile", "quick"]).unwrap();
    assert_eq!(cli.profile.as_deref(), Some("quick"));
}

// ── P1-4: multi-chat one-shot task parsing ──

#[test]
fn cli_multi_chat_positional_task() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "multi-chat", "fix the bug"]).unwrap();
    match cli.command.unwrap() {
        Commands::MultiChat { task, concurrency } => {
            assert_eq!(task.as_deref(), Some("fix the bug"));
            assert_eq!(concurrency, DEFAULT_MULTI_CHAT_CONCURRENCY);
        }
        other => panic!("Expected MultiChat, got {:?}", other),
    }
}

#[test]
fn cli_multi_chat_without_task_is_interactive() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "multi-chat"]).unwrap();
    match cli.command.unwrap() {
        Commands::MultiChat { task, .. } => assert!(task.is_none()),
        other => panic!("Expected MultiChat, got {:?}", other),
    }
}

#[test]
fn cli_multi_chat_task_with_coordinator_and_concurrency() {
    use clap::Parser;
    let cli = Cli::try_parse_from([
        "selfware",
        "multi-chat",
        "ship it",
        "--coordinator",
        "-n",
        "8",
    ])
    .unwrap();
    assert!(cli.coordinator);
    match cli.command.unwrap() {
        Commands::MultiChat { task, concurrency } => {
            assert_eq!(task.as_deref(), Some("ship it"));
            assert_eq!(concurrency, 8);
        }
        other => panic!("Expected MultiChat, got {:?}", other),
    }
}

#[test]
fn cli_prompt_combined_with_multi_chat_parses() {
    use clap::Parser;
    // The one-shot routing happens in run(); this only checks the parse shape.
    let cli = Cli::try_parse_from(["selfware", "-p", "do it", "multi-chat"]).unwrap();
    assert_eq!(cli.prompt.as_deref(), Some("do it"));
    match cli.command.unwrap() {
        Commands::MultiChat { task, .. } => assert!(task.is_none()),
        other => panic!("Expected MultiChat, got {:?}", other),
    }
}

// ── P1-4: one-shot fan-out against a mock LLM endpoint ──

/// Minimal config pointed at the mock server; multi-chat makes no tool
/// calls, so no safety/yolo setup is needed.
fn mock_multi_chat_config(endpoint: String) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        ..Default::default()
    }
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn multi_chat_one_shot_success_returns_ok() {
    use crate::testing::mock_api::MockLlmServer;

    let server = MockLlmServer::builder()
        .with_default_response(crate::testing::mock_api::MockResponse::Text(
            "mock answer".to_string(),
        ))
        .build()
        .await;
    let config = mock_multi_chat_config(format!("{}/v1", server.url()));
    let ctx = WorkshopContext::from_config(&config.endpoint, &config.model)
        .with_mode(ExecutionMode::Normal);

    // quiet=true keeps human output out of the test log; exit semantics are
    // what matters: all agents succeed → Ok.
    let result = run_multi_chat_one_shot(
        &config,
        &ctx,
        4,
        false,
        "say hi",
        HeadlessOutputFormat::Text,
        true,
    )
    .await;
    assert!(
        result.is_ok(),
        "one-shot should succeed: {:?}",
        result.err()
    );

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn multi_chat_one_shot_coordinator_assigns_and_runs() {
    use crate::testing::mock_api::MockLlmServer;

    let server = MockLlmServer::builder()
        .with_default_response(crate::testing::mock_api::MockResponse::Text(
            "mock answer".to_string(),
        ))
        .build()
        .await;
    let config = mock_multi_chat_config(format!("{}/v1", server.url()));
    let ctx = WorkshopContext::from_config(&config.endpoint, &config.model)
        .with_mode(ExecutionMode::Normal);

    // Coordinator mode: the swarm mirrors the role fleet 1:1, so the
    // assignment gate passes and the fan-out executes.
    let result = run_multi_chat_one_shot(
        &config,
        &ctx,
        4,
        true,
        "say hi",
        HeadlessOutputFormat::Text,
        true,
    )
    .await;
    assert!(
        result.is_ok(),
        "coordinator one-shot should succeed: {:?}",
        result.err()
    );

    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn multi_chat_one_shot_any_failure_returns_err() {
    use crate::testing::mock_api::{MockLlmServer, MockResponse};

    // 401 is non-retryable, so every agent fails immediately.
    let server = MockLlmServer::builder()
        .with_default_response(MockResponse::Error {
            status: 401,
            body: r#"{"error":"unauthorized"}"#.to_string(),
        })
        .build()
        .await;
    let config = mock_multi_chat_config(format!("{}/v1", server.url()));
    let ctx = WorkshopContext::from_config(&config.endpoint, &config.model)
        .with_mode(ExecutionMode::Normal);

    let result = run_multi_chat_one_shot(
        &config,
        &ctx,
        4,
        false,
        "say hi",
        HeadlessOutputFormat::Text,
        true,
    )
    .await;
    let err = result.expect_err("any agent failure must make the one-shot fail");
    assert!(
        format!("{err}").contains("agents failed"),
        "error should report the failure count: {err}"
    );

    server.stop().await;
}

#[test]
fn multi_agent_result_json_shape() {
    // The machine-readable one-shot output: per-agent array entries carry
    // identity, success, content, error, and provider-reported usage.
    let result = multiagent::AgentResult {
        agent_id: 0,
        agent_name: "Agent-0-Coder".to_string(),
        role: crate::swarm::AgentRole::Coder,
        content: "answer".to_string(),
        usage: Some(crate::api::types::Usage {
            prompt_tokens: 30,
            completion_tokens: 12,
            total_tokens: 42,
            cost: Some(0.001),
        }),
        duration: std::time::Duration::from_millis(1500),
        success: true,
        error: None,
    };
    let v = multi_agent_result_json(&result);
    assert_eq!(v["agent_name"], "Agent-0-Coder");
    assert_eq!(v["role"], "Coder");
    assert_eq!(v["success"], true);
    assert_eq!(v["usage"]["total_tokens"], 42);
    assert_eq!(v["usage"]["cost"], 0.001);
    assert!(v["error"].is_null());
}

// ── garden banner / structured output tests ──

#[test]
fn garden_banner_printed_for_text_output_when_not_quiet() {
    assert!(should_print_garden_banner(
        false,
        HeadlessOutputFormat::Text
    ));
}

#[test]
fn garden_banner_suppressed_for_machine_readable_output() {
    // A banner preceding the JSON object on stdout breaks json.load(stdout).
    assert!(!should_print_garden_banner(
        false,
        HeadlessOutputFormat::Json
    ));
    assert!(!should_print_garden_banner(
        false,
        HeadlessOutputFormat::StreamJson
    ));
}

#[test]
fn garden_banner_suppressed_when_quiet() {
    assert!(!should_print_garden_banner(
        true,
        HeadlessOutputFormat::Text
    ));
}

// ── TUI launch guard tests ──

#[test]
fn tui_launch_allowed_with_both_ttys() {
    assert!(tui_launch_block_reason(true, true).is_none());
}

#[test]
fn tui_launch_blocked_without_a_terminal() {
    // CI/pipe/script launches must fail loudly (non-zero) instead of
    // silently exiting 0 having done nothing.
    for (stdin_tty, stdout_tty) in [(false, true), (true, false), (false, false)] {
        let reason =
            tui_launch_block_reason(stdin_tty, stdout_tty).expect("non-TTY launch must be blocked");
        assert!(
            reason.contains("requires a terminal") && reason.contains("-p"),
            "reason must name the headless alternative: {reason}"
        );
    }
}

// ── resolve_preset_task tests ──

#[test]
fn resolve_preset_task_renders_known_preset() {
    let id = crate::evolve::presets::presets()[0].id.to_string();
    let task = resolve_preset_task(Some(id), None).unwrap();
    assert!(task.contains("Invariants"));
}

#[test]
fn resolve_preset_task_errors_on_unknown_id_with_available_list() {
    let err = resolve_preset_task(Some("nope".to_string()), None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("unknown preset 'nope'"));
    assert!(msg.contains("available:"));
}

#[test]
fn resolve_preset_task_passthrough_plain_task() {
    let task = resolve_preset_task(None, Some("do the thing".to_string())).unwrap();
    assert_eq!(task, "do the thing");
}

// ── journal_title tests ──

#[test]
fn journal_title_takes_first_non_empty_line() {
    assert_eq!(
        journal_title("\n\n   \nReview the auth module\nand report findings", 60),
        "Review the auth module"
    );
}

#[test]
fn journal_title_truncates_long_lines_with_ellipsis() {
    let long = "x".repeat(100);
    let title = journal_title(&long, 60);
    assert_eq!(title.chars().count(), 60);
    assert!(title.ends_with("..."));
}

#[test]
fn journal_title_empty_prompt_is_honest_placeholder() {
    assert_eq!(journal_title("", 60), "(untitled task)");
    assert_eq!(journal_title("\n  \n\t\n", 60), "(untitled task)");
}

// ── render_run_summary tests ──

fn sample_summary() -> crate::agent::RunSummary {
    crate::agent::RunSummary {
        iterations: 12,
        max_iterations: 30,
        budget_extended: false,
        files_changed: vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
            "src/d.rs".to_string(),
        ],
        verification: Some((true, 4)),
        total_tokens: 123_456,
        cost_usd: Some(0.0123),
    }
}

#[test]
fn render_run_summary_completed_run() {
    let rendered = render_run_summary(&sample_summary(), None);
    assert!(rendered.contains("outcome: completed"), "{rendered}");
    assert!(rendered.contains("iterations: 12/30"), "{rendered}");
    assert!(
        rendered.contains("files changed: 4 (src/a.rs, src/b.rs, src/c.rs, +1 more)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("verification: passed (4 checks)"),
        "{rendered}"
    );
    assert!(
        rendered.contains("tokens: 123456 total, cost $0.0123"),
        "{rendered}"
    );
    assert!(!rendered.contains("budget extended"), "{rendered}");
}

#[test]
fn render_run_summary_failed_run_with_extension_and_no_verification() {
    let mut summary = sample_summary();
    summary.iterations = 31;
    summary.max_iterations = 45;
    summary.budget_extended = true;
    summary.files_changed = Vec::new();
    summary.verification = None;
    summary.cost_usd = None;
    let rendered = render_run_summary(&summary, Some("Max iterations exceeded"));
    assert!(
        rendered.contains("outcome: failed — Max iterations exceeded"),
        "{rendered}"
    );
    assert!(
        rendered.contains("iterations: 31/45 (budget extended)"),
        "{rendered}"
    );
    assert!(rendered.contains("files changed: none"), "{rendered}");
    assert!(
        rendered.contains("verification: not performed"),
        "{rendered}"
    );
    // No invented cost line when nothing was billed (rule 3).
    assert!(rendered.contains("tokens: 123456 total"), "{rendered}");
    assert!(!rendered.contains("cost $"), "{rendered}");
}

// ── Harness-alignment flags: --model, exec alias, mcp list ──

#[test]
fn cli_global_model_flag_parses() {
    use clap::Parser;
    let cli =
        Cli::try_parse_from(["selfware", "--model", "qwen3.8-max", "run", "fix bug"]).unwrap();
    assert_eq!(cli.model.as_deref(), Some("qwen3.8-max"));
    match cli.command.unwrap() {
        Commands::Run { task, .. } => assert_eq!(task.as_deref(), Some("fix bug")),
        other => panic!("Expected Run, got {:?}", other),
    }
}

#[test]
fn cli_short_m_stays_mode_not_model() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "-m", "yolo", "run", "fix bug"]).unwrap();
    assert!(
        cli.model.is_none(),
        "-m must stay --mode; only --model sets the model override"
    );
    assert!(matches!(cli.mode, Some(crate::config::ExecutionMode::Yolo)));
}

#[test]
fn cli_exec_alias_routes_to_run() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "exec", "fix the bug"]).unwrap();
    match cli.command.unwrap() {
        Commands::Run { task, .. } => assert_eq!(task.as_deref(), Some("fix the bug")),
        other => panic!("Expected Run via exec alias, got {:?}", other),
    }
}

#[test]
fn cli_mcp_list_parses() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "mcp", "list"]).unwrap();
    match cli.command.unwrap() {
        Commands::Mcp { command } => assert!(matches!(command, args::McpCommands::List)),
        other => panic!("Expected Mcp, got {:?}", other),
    }
}

// ── Slash-command handler rendering ──

#[test]
fn slash_help_lists_all_session_commands() {
    let help = slash_help_text();
    for command in [
        "/help", "/status", "/compact", "/cost", "/model", "/doctor", "/mcp", "/clear", "/plan",
        "/quit", "/exit",
    ] {
        assert!(help.contains(command), "help missing {command}: {help}");
    }
}

#[test]
fn render_cost_line_honest_about_missing_billing() {
    let mut summary = crate::agent::RunSummary {
        iterations: 1,
        max_iterations: 30,
        budget_extended: false,
        files_changed: Vec::new(),
        verification: None,
        total_tokens: 12_345,
        cost_usd: None,
    };
    let rendered = render_cost_line(&summary);
    assert!(rendered.contains("tokens: 12345 total"), "{rendered}");
    assert!(rendered.contains("cost not tracked"), "{rendered}");
    assert!(!rendered.contains('$'), "no invented cost: {rendered}");

    summary.cost_usd = Some(0.0123);
    let rendered = render_cost_line(&summary);
    assert!(rendered.contains("cost $0.0123"), "{rendered}");
}

#[test]
fn mcp_servers_text_covers_empty_and_configured() {
    let mut config = crate::config::Config::default();
    assert!(mcp_servers_text(&config).contains("no MCP servers configured"));

    config.mcp.servers.push(crate::mcp::McpServerConfig {
        name: "docs".to_string(),
        command: "uvx".to_string(),
        args: vec!["docs-mcp".to_string()],
        env: Default::default(),
        init_timeout_secs: 30,
        framing: crate::mcp::transport::Framing::default(),
    });
    let rendered = mcp_servers_text(&config);
    assert!(
        rendered.contains("1 configured MCP server(s)"),
        "{rendered}"
    );
    assert!(rendered.contains("docs — uvx docs-mcp"), "{rendered}");
    assert!(
        rendered.contains("not live connectivity") || rendered.contains("configured"),
        "{rendered}"
    );
}

// ── Round E2: attachments, mcp config edits, --continue, render helpers ──

#[test]
fn expand_attachments_resolves_files_and_ignores_non_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let file = temp.path().join("note.txt");
    std::fs::write(&file, "hello world").expect("write");
    let input = format!(
        "review @{} and @/nonexistent/missing.rs now",
        file.display()
    );
    let attachments = expand_attachments(&input);
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].content, "hello world");
    assert!(!attachments[0].truncated);
}

#[test]
fn expand_attachments_marks_truncation_honestly() {
    let temp = tempfile::tempdir().expect("tempdir");
    let file = temp.path().join("big.txt");
    std::fs::write(&file, "x".repeat(ATTACHMENT_MAX_CHARS + 100)).expect("write");
    let attachments = expand_attachments(&format!("@{}", file.display()));
    assert_eq!(attachments.len(), 1);
    assert!(attachments[0].truncated);
    assert_eq!(attachments[0].content.chars().count(), ATTACHMENT_MAX_CHARS);
}

#[test]
fn mcp_add_remove_round_trip_through_the_config_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = temp.path().join("config.toml");

    let added = edit_mcp_servers(&path, |servers| {
        let mut entry = toml::value::Table::new();
        entry.insert("name".to_string(), toml::Value::String("docs".to_string()));
        entry.insert(
            "command".to_string(),
            toml::Value::String("uvx".to_string()),
        );
        servers.push(toml::Value::Table(entry));
        Ok("added".to_string())
    })
    .expect("add");
    assert_eq!(added, "added");

    // The file parses back with the server present.
    let parsed: toml::Value =
        toml::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
    let servers = parsed["mcp"]["servers"].as_array().expect("servers array");
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["name"].as_str(), Some("docs"));
    assert_eq!(servers[0]["command"].as_str(), Some("uvx"));

    let removed = edit_mcp_servers(&path, |servers| {
        servers.retain(|s| s.get("name").and_then(|n| n.as_str()) != Some("docs"));
        Ok("removed".to_string())
    })
    .expect("remove");
    assert_eq!(removed, "removed");
    let parsed: toml::Value =
        toml::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
    assert!(parsed["mcp"]["servers"]
        .as_array()
        .expect("servers array")
        .is_empty());
}

#[test]
fn cli_continue_flag_parses_and_keeps_dash_c_for_config() {
    use clap::Parser;
    let cli = Cli::try_parse_from(["selfware", "--continue"]).unwrap();
    assert!(cli.continue_flag);
    let cli = Cli::try_parse_from(["selfware", "-c", "custom.toml", "run", "x"]).unwrap();
    assert!(!cli.continue_flag);
    assert_eq!(cli.config.as_deref(), Some("custom.toml"));
}

#[test]
fn cli_mcp_add_remove_parse() {
    use clap::Parser;
    let cli = Cli::try_parse_from([
        "selfware",
        "mcp",
        "add",
        "docs",
        "--command",
        "uvx",
        "--args",
        "docs-mcp",
    ])
    .unwrap();
    match cli.command.unwrap() {
        Commands::Mcp { command } => match command {
            args::McpCommands::Add {
                name,
                command,
                args,
            } => {
                assert_eq!(name, "docs");
                assert_eq!(command, "uvx");
                assert_eq!(args, vec!["docs-mcp".to_string()]);
            }
            other => panic!("Expected Mcp Add, got {:?}", other),
        },
        other => panic!("Expected Mcp, got {:?}", other),
    }

    let cli = Cli::try_parse_from(["selfware", "mcp", "remove", "docs"]).unwrap();
    match cli.command.unwrap() {
        Commands::Mcp { command } => match command {
            args::McpCommands::Remove { name } => assert_eq!(name, "docs"),
            other => panic!("Expected Mcp Remove, got {:?}", other),
        },
        other => panic!("Expected Mcp, got {:?}", other),
    }
}

#[test]
fn slash_help_covers_round_e2_commands() {
    let help = slash_help_text();
    for command in [
        "/undo",
        "/redo",
        "/agents",
        "/resume",
        "/permissions",
        "/journal",
        "/memory",
        "/tools",
        "/garden",
        "/analyze",
        "/review",
        "/bug",
        "/skills",
        "!cmd",
        "@path",
    ] {
        assert!(help.contains(command), "help missing {command}: {help}");
    }
}

#[test]
fn agents_status_text_is_honest_roster() {
    let text = agents_status_text();
    assert!(text.contains("Archie"), "{text}");
    assert!(text.contains("no live agents are tracked"), "{text}");
}

#[test]
fn untrusted_repo_config_is_not_the_trust_default() {
    // The mcp add/remove target gate relies on this predicate: a repo-local
    // selfware.toml is untrusted until `selfware trust` records it.
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_config = temp.path().join("selfware.toml");
    std::fs::write(&repo_config, "").expect("write");
    assert!(!crate::config::trust::is_config_trusted(&repo_config));
}
