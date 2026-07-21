use super::*;
use crate::bench_harness::swebench_pro::catalog::{BackendProfile, ThinkingPolicy};

fn dummy_spec() -> QuantSpec {
    QuantSpec {
        label: "test-label".into(),
        gguf: "test.gguf".into(),
        alias: "test-alias".into(),
        mmproj: "mmproj.gguf".into(),
        name: "Test".into(),
        ctx: 262_144,
        max_parallel: 2,
        kv_cache_type: "q8_0".into(),
        tensor_split: Some("24,24".into()),
        temperature: 1.0,
        thinking_policy: ThinkingPolicy::Disable,
        backend: BackendProfile::LlamaCpp,
    }
}

#[test]
fn opts_default_uses_models_subdir() {
    // The default models dir is either the env override or
    // `~/models/qwen36-quants`; just make sure the path is nonempty.
    let opts = LlamaServerOpts::default();
    assert!(!opts.models_dir.as_os_str().is_empty());
    assert_eq!(opts.port, 8000);
    assert_eq!(opts.parallel, 2);
    assert_eq!(opts.ctx, 262_144);
}

#[test]
fn build_args_routes_ctx_and_parallel() {
    let opts = LlamaServerOpts {
        ctx: 65_536,
        parallel: 4,
        tensor_split: Some("12,12".into()),
        ..LlamaServerOpts::default()
    };
    let args = build_llama_server_args(&dummy_spec(), &opts, Path::new("/tmp/test.gguf"), None);

    // -c 65536 must appear with the right value.
    let c_pos = args
        .iter()
        .position(|a| a == "-c")
        .expect("-c flag missing");
    assert_eq!(args.get(c_pos + 1).map(|s| s.as_str()), Some("65536"));

    // --parallel 4 must appear with the right value.
    let p_pos = args
        .iter()
        .position(|a| a == "--parallel")
        .expect("--parallel flag missing");
    assert_eq!(args.get(p_pos + 1).map(|s| s.as_str()), Some("4"));

    // --tensor-split 12,12 must appear.
    let t_pos = args
        .iter()
        .position(|a| a == "--tensor-split")
        .expect("--tensor-split flag missing");
    assert_eq!(args.get(t_pos + 1).map(|s| s.as_str()), Some("12,12"));

    // --port still routes through.
    let port_pos = args
        .iter()
        .position(|a| a == "--port")
        .expect("--port flag missing");
    assert_eq!(
        args.get(port_pos + 1).map(|s| s.as_str()),
        Some(opts.port.to_string().as_str())
    );

    // --alias takes the spec's alias.
    let a_pos = args
        .iter()
        .position(|a| a == "--alias")
        .expect("--alias flag missing");
    assert_eq!(args.get(a_pos + 1).map(|s| s.as_str()), Some("test-alias"));
}

#[test]
fn build_args_omits_tensor_split_when_none() {
    let opts = LlamaServerOpts {
        tensor_split: None,
        ..LlamaServerOpts::default()
    };
    let args = build_llama_server_args(&dummy_spec(), &opts, Path::new("/tmp/test.gguf"), None);
    assert!(
        !args.iter().any(|a| a == "--tensor-split"),
        "should omit --tensor-split when None"
    );
}

#[test]
fn build_args_omits_chat_template_kwargs() {
    let opts = LlamaServerOpts::default();
    let args = build_llama_server_args(&dummy_spec(), &opts, Path::new("/tmp/test.gguf"), None);
    assert!(
        !args.iter().any(|a| a == "--chat-template-kwargs"),
        "should not hard-code --chat-template-kwargs; let the model profile / extra_body decide"
    );
}

#[test]
fn build_args_includes_mmproj_when_supplied() {
    let opts = LlamaServerOpts::default();
    let args = build_llama_server_args(
        &dummy_spec(),
        &opts,
        Path::new("/tmp/test.gguf"),
        Some(Path::new("/tmp/mm.gguf")),
    );
    let pos = args
        .iter()
        .position(|a| a == "--mmproj")
        .expect("--mmproj flag missing");
    assert_eq!(args.get(pos + 1).map(|s| s.as_str()), Some("/tmp/mm.gguf"));
}

#[test]
fn bench_selfware_config_uses_xml_tool_calls() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selfware_bench.toml");
    write_bench_selfware_config(&path, "http://127.0.0.1:8000/v1", "qwen3.6-27b-q4kp").unwrap();
    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains("model = \"qwen3.6-27b-q4kp\""));
    assert!(
        content.contains("native_function_calling = false"),
        "SWE-bench Pro prompts expect XML tool calls, so native FC must be disabled"
    );
    assert!(content.contains("read_loop_policy = \"force_mutation\""));
}

#[test]
fn cmd_targets_port_matches_exact_token() {
    let cmd: Vec<String> = ["llama-server", "--port", "8000", "--ctx", "4096"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert!(cmd_targets_port(&cmd, 8000));
    // a prefix/substring of the real port must NOT match
    assert!(!cmd_targets_port(&cmd, 80));
    assert!(!cmd_targets_port(&cmd, 800));
    assert!(!cmd_targets_port(&cmd, 9000));
    // no --port token at all
    let cmd2: Vec<String> = ["llama-server", "--ctx", "4096"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert!(!cmd_targets_port(&cmd2, 8000));
}

#[test]
fn resolve_llama_binary_honors_env() {
    // Use std::env carefully — set a sentinel we control then unset.
    // Note: env mutation in tests can race with other tests; this test
    // intentionally restricts itself to checking the env-priority code
    // path with a fixed sentinel.
    std::env::set_var("LLAMA_SERVER_BIN", "/sentinel/llama-server-test");
    let p = resolve_llama_binary();
    std::env::remove_var("LLAMA_SERVER_BIN");
    assert_eq!(p, PathBuf::from("/sentinel/llama-server-test"));
}

#[test]
fn default_llama_opts_bind_loopback() {
    assert_eq!(LlamaServerOpts::default().host, "127.0.0.1");
}

#[test]
fn capture_patch_excludes_harness_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    std::process::Command::new("git")
        .args(["-C"])
        .arg(repo)
        .arg("init")
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["config", "user.email", "test@example.com"])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["config", "user.name", "Test"])
        .status()
        .unwrap();

    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/app.py"), "print('old')\n").unwrap();
    std::process::Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["add", "."])
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["commit", "-m", "initial"])
        .status()
        .unwrap();

    std::fs::write(repo.join("src/app.py"), "print('new')\n").unwrap();
    std::fs::create_dir_all(repo.join("reports/swebench_pro/run")).unwrap();
    std::fs::write(repo.join("reports/swebench_pro/run/trace.jsonl"), "{}\n").unwrap();
    std::fs::write(repo.join("failure_mode.json"), "{}\n").unwrap();

    let patch = capture_patch(repo).unwrap();

    assert!(patch.contains("src/app.py"));
    assert!(!patch.contains("reports/swebench_pro"));
    assert!(!patch.contains("failure_mode.json"));

    // The real git index must not have been mutated.
    let real_index_diff = std::process::Command::new("git")
        .args(["-C"])
        .arg(repo)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .unwrap();
    assert!(
        real_index_diff.success(),
        "capture_patch mutated the real git index"
    );
}
