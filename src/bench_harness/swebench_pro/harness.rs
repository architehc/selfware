//! `llama-server` boot/teardown and selfware subprocess invocation.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};

use super::catalog::QuantSpec;

#[derive(Clone, Debug)]
pub struct LlamaServerOpts {
    pub port: u16,
    pub ctx: u32,
    pub parallel: u32,
    pub binary: PathBuf,
    pub models_dir: PathBuf,
    /// `--tensor-split` value.  When `None`, the flag is omitted entirely
    /// and llama-server picks defaults itself.
    pub tensor_split: Option<String>,
    pub boot_timeout: Duration,
    pub host: String,
}

impl Default for LlamaServerOpts {
    fn default() -> Self {
        Self {
            port: 8000,
            ctx: 262_144,
            parallel: 2,
            binary: resolve_llama_binary(),
            models_dir: resolve_models_dir(),
            // Preserve the user's working setup as the default so existing
            // invocations keep working without flags.  CLI/env overrides set
            // this to `None` (auto) when explicitly requested.
            tensor_split: Some("24,24".into()),
            boot_timeout: Duration::from_secs(180),
            host: "0.0.0.0".into(),
        }
    }
}

/// Resolve the `llama-server` binary path with this priority:
///   1. `LLAMA_SERVER_BIN` environment variable.
///   2. `which llama-server` on `$PATH`.
///   3. Hard-coded fallback the user's machine ships with.
pub fn resolve_llama_binary() -> PathBuf {
    if let Ok(p) = std::env::var("LLAMA_SERVER_BIN") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(out) = Command::new("which").arg("llama-server").output() {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                return PathBuf::from(path);
            }
        }
    }
    PathBuf::from("/home/ivo/llama.cpp/build/bin/llama-server")
}

/// Resolve the models directory with this priority:
///   1. `SWEBENCH_MODELS_DIR` environment variable.
///   2. `~/models/qwen36-quants` (existing user default).
pub fn resolve_models_dir() -> PathBuf {
    if let Ok(p) = std::env::var("SWEBENCH_MODELS_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join("models").join("qwen36-quants")
}

/// Build the argv passed to `llama-server`.  Extracted into its own function so
/// unit tests can exercise the wiring of `ctx`/`parallel`/`tensor_split` without
/// actually spawning the process.
///
/// Returns the args in the same order they would be passed; the first element
/// is *not* the binary path (callers should set that on the `Command`).
pub fn build_llama_server_args(
    spec: &QuantSpec,
    opts: &LlamaServerOpts,
    gguf: &Path,
    mmproj: Option<&Path>,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-m".into(),
        gguf.to_string_lossy().into_owned(),
        "--jinja".into(),
        "-c".into(),
        opts.ctx.to_string(),
        "-ngl".into(),
        "99".into(),
    ];
    if let Some(ts) = opts.tensor_split.as_ref() {
        if !ts.trim().is_empty() {
            args.push("--tensor-split".into());
            args.push(ts.clone());
        }
    }
    args.extend([
        "-ctk".into(),
        "q8_0".into(),
        "-ctv".into(),
        "q8_0".into(),
        "--parallel".into(),
        opts.parallel.to_string(),
        "--cont-batching".into(),
        "--chat-template-kwargs".into(),
        r#"{"enable_thinking": false}"#.into(),
        "--host".into(),
        opts.host.clone(),
        "--port".into(),
        opts.port.to_string(),
        "--alias".into(),
        spec.alias.clone(),
    ]);
    if let Some(p) = mmproj {
        args.push("--mmproj".into());
        args.push(p.to_string_lossy().into_owned());
    }
    args
}

pub struct LlamaServer {
    pub child: Option<Child>,
    pub alias: String,
    pub port: u16,
}

impl LlamaServer {
    /// Best-effort kill of any existing `llama-server` and a short pause to let
    /// the kernel reclaim the GPU.  Mirrors `stop_llama_server()` in run.py.
    pub fn stop_existing() {
        let _ = Command::new("pkill")
            .args(["-f", "llama-server"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        std::thread::sleep(Duration::from_secs(2));
    }

    /// Boot llama-server for the given quant; block until it answers
    /// `/v1/models` or `boot_timeout` elapses.
    pub fn boot(spec: &QuantSpec, opts: &LlamaServerOpts) -> Result<Self> {
        let gguf = opts.models_dir.join(&spec.gguf);
        let mmproj = opts.models_dir.join(&spec.mmproj);
        let gguf = gguf
            .canonicalize()
            .with_context(|| format!("missing GGUF: {}", gguf.display()))?;

        let canon_mmproj = mmproj.canonicalize().ok();
        let argv = build_llama_server_args(spec, opts, &gguf, canon_mmproj.as_deref());
        let mut cmd = Command::new(&opts.binary);
        cmd.args(&argv);

        let log_path = format!("/tmp/llama-{}.log", spec.alias);
        let log = std::fs::File::create(&log_path)
            .with_context(|| format!("opening log file {}", log_path))?;
        let log_err = log.try_clone()?;

        cmd.stdout(Stdio::from(log)).stderr(Stdio::from(log_err));
        // Use a fresh process group so killing the parent doesn't tear down
        // the bench mid-run.  Mirror Python's `start_new_session=True`.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let child = cmd
            .spawn()
            .with_context(|| format!("spawning {}", opts.binary.display()))?;
        let pid = child.id();

        let port = opts.port;
        let alias = spec.alias.clone();
        let server = LlamaServer {
            child: Some(child),
            alias: alias.clone(),
            port,
        };

        let deadline = Instant::now() + opts.boot_timeout;
        while Instant::now() < deadline {
            if probe_endpoint(port) {
                return Ok(server);
            }
            std::thread::sleep(Duration::from_secs(2));
        }

        // Boot failed — drop the server and propagate.
        drop(server);
        bail!("llama-server boot timed out (alias={}, pid={})", alias, pid);
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn probe_endpoint(port: u16) -> bool {
    Command::new("curl")
        .args([
            "-sf",
            "-m",
            "1",
            &format!("http://127.0.0.1:{}/v1/models", port),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Detect the backend engine by querying `/v1/models` and inspecting headers
/// plus body content.  Returns a lower-case label like `"llama.cpp"`,
/// `"sglang"`, `"vllm"`, or `"unknown"`.
pub fn detect_backend(port: u16) -> Result<String> {
    let url = format!("http://127.0.0.1:{}/v1/models", port);
    let output = Command::new("curl")
        .args(["-sf", "-m", "3", "-i", &url])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("spawning curl for backend detection")?;

    if !output.status.success() {
        bail!("curl probe failed for {}", url);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let lower = text.to_lowercase();

    // Header hints
    if lower.contains("server: llama.cpp") || lower.contains("server: llamacpp") {
        return Ok("llama.cpp".into());
    }
    if lower.contains("server: sglang") {
        return Ok("sglang".into());
    }
    if lower.contains("server: vllm") {
        return Ok("vllm".into());
    }

    // Body hints
    if lower.contains("llama.cpp") || lower.contains("llamacpp") {
        return Ok("llama.cpp".into());
    }
    if lower.contains("sglang") {
        return Ok("sglang".into());
    }
    if lower.contains("vllm") {
        return Ok("vllm".into());
    }

    Ok("unknown".into())
}

/// Parse the final JSON object from stdout text.
///
/// The agent may emit intermediate progress lines (in stream-json mode) or
/// diagnostic text; we scan from the last non-empty line backward until we
/// find a line that parses as `SessionResult`.
fn parse_session_result_from_stdout(text: &str) -> Option<crate::cli::headless::SessionResult> {
    for line in text
        .lines()
        .rev()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
    {
        if let Ok(result) = serde_json::from_str::<crate::cli::headless::SessionResult>(line) {
            return Some(result);
        }
    }
    None
}

/// Run `selfware -p PROMPT -C WORKDIR --yolo --no-tui --quiet --output-format json`
/// with a wall-clock timeout.  Mirrors `run_selfware()` in run.py.
///
/// `result_dir` is exported as `SELFWARE_RESULT_DIR` so the agent's
/// `failure_mode.json` lands next to this trial's `result.json` instead of
/// under `~/.selfware/checkpoints/...`.
pub fn run_selfware(
    selfware_bin: &Path,
    workdir: &Path,
    prompt: &str,
    alias: &str,
    endpoint: &str,
    timeout: Duration,
    log_path: &Path,
    result_dir: &Path,
) -> Result<RunOutcome> {
    let log = std::fs::File::create(log_path)
        .with_context(|| format!("opening agent log {}", log_path.display()))?;

    let started = Instant::now();
    let mut cmd = Command::new(selfware_bin);
    cmd.arg("-p")
        .arg(prompt)
        .arg("-C")
        .arg(workdir)
        .arg("--yolo")
        .arg("--no-tui")
        .arg("--quiet")
        .arg("--output-format")
        .arg("json")
        .env("SELFWARE_ENDPOINT", endpoint)
        .env("SELFWARE_MODEL", alias)
        .env("SELFWARE_RESULT_DIR", result_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log));

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawning {}", selfware_bin.display()))?;

    let stdout = child.stdout.take().unwrap();
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::BufReader::new(stdout), &mut buf);
        buf
    });

    // Poll-with-timeout. `Child::wait` has no native timeout; we bound it.
    let deadline = started + timeout;
    loop {
        match child.try_wait()? {
            Some(status) => {
                let stdout_text = stdout_handle.join().unwrap_or_default();
                let exit_code = status.code().unwrap_or(-1);

                // Try to parse structured output from stdout.
                if let Some(result) = parse_session_result_from_stdout(&stdout_text) {
                    return Ok(RunOutcome {
                        exit_code: result.exit_status,
                        timed_out: false,
                        wall_secs: result.duration_ms as f64 / 1000.0,
                        parsed_result: Some(result),
                    });
                }

                // Fall back to current behavior if JSON parse fails.
                return Ok(RunOutcome {
                    exit_code,
                    timed_out: false,
                    wall_secs: started.elapsed().as_secs_f64(),
                    parsed_result: None,
                });
            }
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_handle.join();
                    return Ok(RunOutcome {
                        exit_code: -1,
                        timed_out: true,
                        wall_secs: started.elapsed().as_secs_f64(),
                        parsed_result: None,
                    });
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunOutcome {
    pub exit_code: i32,
    pub timed_out: bool,
    pub wall_secs: f64,
    /// Parsed `SessionResult` when `--output-format json` was used and parsing
    /// succeeded.  `None` when the agent ran in text mode or JSON parsing failed.
    pub parsed_result: Option<crate::cli::headless::SessionResult>,
}

/// Capture `git diff` from `workdir`, including newly added files and excluding
/// selfware-internal scratch directories.
///
/// Mirrors the Python harness's exclusion list exactly so a Rust run produces
/// the same patch as `scripts/swebench_pro/run.py`:
///   `.selfware/`, `.claude/`, `__pycache__/`, `selfware.toml`, `*.bak`,
///   `**/*.bak`.
pub fn capture_patch(workdir: &Path) -> Result<String> {
    let _ = Command::new("git")
        .args(["-C"])
        .arg(workdir)
        .args(["add", "-A"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let out = Command::new("git")
        .args(["-C"])
        .arg(workdir)
        .args([
            "diff",
            "--cached",
            "HEAD",
            "--",
            ".",
            ":(exclude).selfware/**",
            ":(exclude).claude/**",
            ":(exclude)__pycache__/**",
            ":(exclude)**/__pycache__/**",
            ":(exclude)selfware.toml",
            ":(exclude)*.bak",
            ":(exclude)**/*.bak",
        ])
        .output()
        .with_context(|| format!("running git diff in {}", workdir.display()))?;

    if !out.status.success() {
        bail!(
            "git diff failed (status={:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8(out.stdout).map_err(|e| anyhow!("non-UTF8 patch: {}", e))
}

/// Shallow-clone `repo` at `base_commit` into `dest`.  Mirrors the
/// fetch-then-fall-back-to-clone strategy in run.py.
pub fn clone_instance(repo: &str, base_commit: &str, dest: &Path) -> Result<()> {
    if dest.exists() {
        std::fs::remove_dir_all(dest)
            .with_context(|| format!("removing existing dest {}", dest.display()))?;
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let url = format!("https://github.com/{}.git", repo);

    // First strategy: init + fetch a single SHA.
    let init = Command::new("git")
        .arg("init")
        .arg(dest)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| "git init")?;

    if init.success() {
        let add_remote = Command::new("git")
            .args(["-C"])
            .arg(dest)
            .args(["remote", "add", "origin", &url])
            .status()?;
        let fetch = Command::new("git")
            .args(["-C"])
            .arg(dest)
            .args(["fetch", "--depth", "1", "origin", base_commit])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        let checkout = Command::new("git")
            .args(["-C"])
            .arg(dest)
            .args(["checkout", "FETCH_HEAD"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;

        if add_remote.success() && fetch.success() && checkout.success() {
            return Ok(());
        }
    }

    // Fallback: full shallow clone, then checkout.
    if dest.exists() {
        let _ = std::fs::remove_dir_all(dest);
    }
    let clone_status = Command::new("git")
        .args(["clone", "--filter=blob:none", "--depth", "200", &url])
        .arg(dest)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| "git clone fallback")?;
    if !clone_status.success() {
        bail!(
            "git clone fallback failed for {} (status={:?})",
            url,
            clone_status.code()
        );
    }
    let checkout_status = Command::new("git")
        .args(["-C"])
        .arg(dest)
        .args(["checkout", base_commit])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| "git checkout fallback")?;
    if !checkout_status.success() {
        bail!(
            "git checkout fallback failed for {}@{} (status={:?})",
            url,
            base_commit,
            checkout_status.code()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn detect_backend_returns_unknown_for_unresponsive_port() {
        // Use a port that is almost certainly closed.
        let result = detect_backend(59999);
        assert!(result.is_err() || result.unwrap() == "unknown");
    }
}
