//! Boot-assistant model: download + integrity verification of the tiny local
//! model (`--chat`), and `llama-server` discovery/spawn.
//!
//! The model is OPTIONAL and freeform-only — it never emits config (recipe
//! cards do that). A sha256 mismatch fails loudly and deletes the file rather
//! than serving a corrupted model.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

/// Qwen3-0.6B Q8_0 GGUF — small enough to run anywhere, big enough for setup Q&A.
pub const MODEL_URL: &str =
    "https://huggingface.co/Qwen/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q8_0.gguf";
pub const MODEL_SHA256: &str = "9465e63a22add5354d9bb4b99e90117043c7124007664907259bd16d043bb031";
pub const MODEL_SIZE_BYTES: u64 = 639_446_688;
pub const MODEL_FILE_NAME: &str = "Qwen3-0.6B-Q8_0.gguf";

/// Alias the spawned server gives the model; the chat client targets this id.
pub const SERVER_ALIAS: &str = "boot-assistant";

/// Port the boot assistant's llama-server binds. Picked away from the usual
/// vLLM/LM Studio/Ollama ports so it never collides with a user's server.
pub const SERVER_PORT: u16 = 8787;

/// `~/.local/share/selfware/boot` (platform data dir).
pub fn boot_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("selfware")
        .join("boot")
}

pub fn model_path_in(dir: &Path) -> PathBuf {
    dir.join(MODEL_FILE_NAME)
}

/// sha256 hex digest of a file, streamed (the model is 639MB — never read it
/// into memory).
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Whether the file at `path` is exactly the pinned model: size gate first
/// (cheap), then the full digest.
pub fn verify_model_file(path: &Path) -> Result<bool> {
    let meta = std::fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if meta.len() != MODEL_SIZE_BYTES {
        return Ok(false);
    }
    Ok(sha256_file(path)? == MODEL_SHA256)
}

/// Pure resolution rule for the `llama-server` binary: the `LLAMA_SERVER`
/// env var wins, then whatever a PATH lookup found. Extracted from
/// [`find_llama_server`] so tests don't depend on the host machine.
pub fn resolve_llama_server(env_val: Option<String>, path_hit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(p) = env_val {
        if !p.trim().is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    path_hit
}

/// Find `llama-server`: `LLAMA_SERVER` env var, then `which llama-server`.
pub fn find_llama_server() -> Option<PathBuf> {
    let env_val = std::env::var("LLAMA_SERVER").ok();
    let path_hit = Command::new("which")
        .arg("llama-server")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (!path.is_empty()).then_some(PathBuf::from(path))
        });
    resolve_llama_server(env_val, path_hit)
}

/// Where to get llama.cpp when no server binary is available.
pub const LLAMA_INSTALL_HINT: &str = "llama-server not found. Install llama.cpp (https://github.com/ggml-org/llama.cpp — package `llama.cpp` on most distros, or build and add `llama-server` to PATH), or point LLAMA_SERVER at the binary.";

/// Ensure the model exists under `dir` and passes integrity verification.
/// Downloads (to a `.partial` file, then renames) when missing; an existing
/// file that fails verification is deleted and re-downloaded once — a second
/// mismatch fails loudly.
pub async fn ensure_model(dir: &Path) -> Result<PathBuf> {
    let path = model_path_in(dir);
    if path.is_file() {
        if verify_model_file(&path)? {
            return Ok(path);
        }
        eprintln!(
            "  boot model at {} failed sha256 verification — deleting and re-downloading.",
            path.display()
        );
        std::fs::remove_file(&path)?;
    }
    download_model(dir, &path).await?;
    if !verify_model_file(&path)? {
        std::fs::remove_file(&path).ok();
        bail!(
            "downloaded boot model failed sha256 verification (expected {}, size {} bytes) — refusing to use it. File deleted; check your network/mirror and retry.",
            MODEL_SHA256,
            MODEL_SIZE_BYTES
        );
    }
    Ok(path)
}

async fn download_model(dir: &Path, dest: &Path) -> Result<()> {
    use futures::StreamExt;

    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let partial = dest.with_extension("partial");
    println!(
        "  Downloading boot-assistant model ({} MB) from HuggingFace...",
        MODEL_SIZE_BYTES / 1_000_000
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()?;
    let resp = client
        .get(MODEL_URL)
        .send()
        .await
        .context("starting model download")?;
    if !resp.status().is_success() {
        bail!("model download returned HTTP {}", resp.status());
    }
    let mut file = std::fs::File::create(&partial)
        .with_context(|| format!("creating {}", partial.display()))?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut next_report: u64 = 64 * 1024 * 1024;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("model download stream")?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        if downloaded >= next_report {
            println!(
                "    {} MB / {} MB",
                downloaded / 1_000_000,
                MODEL_SIZE_BYTES / 1_000_000
            );
            next_report += 256 * 1024 * 1024;
        }
    }
    file.flush()?;
    drop(file);
    std::fs::rename(&partial, dest)
        .with_context(|| format!("moving {} into place", dest.display()))?;
    Ok(())
}

/// argv for the boot-assistant server (without the binary path). CPU-safe
/// defaults: no `-ngl` (works on machines without a GPU), a modest 4k context
/// (setup Q&A, not coding), jinja for Qwen3's chat template.
pub fn build_server_args(model: &Path, port: u16) -> Vec<String> {
    vec![
        "-m".into(),
        model.to_string_lossy().into_owned(),
        "--alias".into(),
        SERVER_ALIAS.into(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "-c".into(),
        "4096".into(),
        "--jinja".into(),
    ]
}

/// Running llama-server child. Killing on drop means a Ctrl-C or early return
/// never leaks a model server holding RAM.
pub struct LlamaServerGuard {
    child: Child,
    pub port: u16,
}

impl LlamaServerGuard {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }
}

impl Drop for LlamaServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Spawn llama-server and wait until its `/health` answers (or the boot
/// timeout elapses). stdout/stderr are muted unless it exits early, in which
/// case the hint points at `--check`.
pub async fn spawn_server(binary: &Path, model: &Path, port: u16) -> Result<LlamaServerGuard> {
    let mut child = Command::new(binary)
        .args(build_server_args(model, port))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawning {}", binary.display()))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;
    let health = format!("http://127.0.0.1:{}/health", port);
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        if let Some(status) = child.try_wait()? {
            bail!(
                "llama-server exited during boot ({}) — run `selfware boot --check` for diagnostics",
                status
            );
        }
        if let Ok(resp) = client.get(&health).send().await {
            if resp.status().is_success() {
                return Ok(LlamaServerGuard { child, port });
            }
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            bail!("llama-server did not become healthy within 120s");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/boot/model_test.rs"]
mod tests;
