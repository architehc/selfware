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
    pub tensor_split: String,
    pub boot_timeout: Duration,
    pub host: String,
}

impl Default for LlamaServerOpts {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            port: 8000,
            ctx: 262_144,
            parallel: 2,
            binary: PathBuf::from("/home/ivo/llama.cpp/build/bin/llama-server"),
            models_dir: home.join("models").join("qwen36-quants"),
            tensor_split: "24,24".into(),
            boot_timeout: Duration::from_secs(180),
            host: "0.0.0.0".into(),
        }
    }
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
        let gguf = opts.models_dir.join(spec.gguf);
        let mmproj = opts.models_dir.join(spec.mmproj);
        let gguf = gguf
            .canonicalize()
            .with_context(|| format!("missing GGUF: {}", gguf.display()))?;

        let mut cmd = Command::new(&opts.binary);
        cmd.arg("-m")
            .arg(&gguf)
            .arg("--jinja")
            .arg("-c")
            .arg(opts.ctx.to_string())
            .arg("-ngl")
            .arg("99")
            .arg("--tensor-split")
            .arg(&opts.tensor_split)
            .arg("-ctk")
            .arg("q8_0")
            .arg("-ctv")
            .arg("q8_0")
            .arg("--parallel")
            .arg(opts.parallel.to_string())
            .arg("--cont-batching")
            .arg("--chat-template-kwargs")
            .arg(r#"{"enable_thinking": false}"#)
            .arg("--host")
            .arg(&opts.host)
            .arg("--port")
            .arg(opts.port.to_string())
            .arg("--alias")
            .arg(spec.alias);

        if let Ok(canon_mmproj) = mmproj.canonicalize() {
            cmd.arg("--mmproj").arg(canon_mmproj);
        }

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
        let alias = spec.alias.to_string();
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
        bail!(
            "llama-server boot timed out (alias={}, pid={})",
            alias,
            pid
        );
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

/// Run `selfware -p PROMPT -C WORKDIR --yolo --no-tui --quiet` with a wall-clock
/// timeout.  Mirrors `run_selfware()` in run.py.
pub fn run_selfware(
    selfware_bin: &Path,
    workdir: &Path,
    prompt: &str,
    alias: &str,
    endpoint: &str,
    timeout: Duration,
    log_path: &Path,
) -> Result<RunOutcome> {
    let log = std::fs::File::create(log_path)
        .with_context(|| format!("opening agent log {}", log_path.display()))?;
    let log_err = log.try_clone()?;

    let started = Instant::now();
    let mut cmd = Command::new(selfware_bin);
    cmd.arg("-p")
        .arg(prompt)
        .arg("-C")
        .arg(workdir)
        .arg("--yolo")
        .arg("--no-tui")
        .arg("--quiet")
        .env("SELFWARE_ENDPOINT", endpoint)
        .env("SELFWARE_MODEL", alias)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawning {}", selfware_bin.display()))?;

    // Poll-with-timeout. `Child::wait` has no native timeout; we bound it.
    let deadline = started + timeout;
    loop {
        match child.try_wait()? {
            Some(status) => {
                let exit_code = status.code().unwrap_or(-1);
                return Ok(RunOutcome {
                    exit_code,
                    timed_out: false,
                    wall_secs: started.elapsed().as_secs_f64(),
                });
            }
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(RunOutcome {
                        exit_code: -1,
                        timed_out: true,
                        wall_secs: started.elapsed().as_secs_f64(),
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

    #[test]
    fn opts_default_uses_models_subdir() {
        let opts = LlamaServerOpts::default();
        assert!(opts
            .models_dir
            .ends_with(Path::new("models/qwen36-quants")));
        assert_eq!(opts.port, 8000);
        assert_eq!(opts.parallel, 2);
        assert_eq!(opts.ctx, 262_144);
    }
}
