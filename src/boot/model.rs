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

/// LiquidAI LFM3-1.7B GSQ 1GB (`kaitchup/LFM3-1.7B-GSQ-GGUF` `IQ4_XS_RCO`) —
/// 1.00 GB mixed-precision GGUF CPU helper model.
pub const LFM3_1GB_MODEL_URL: &str =
    "https://huggingface.co/kaitchup/LFM3-1.7B-GSQ-GGUF/resolve/main/LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf";
pub const LFM3_1GB_MODEL_SHA256: &str =
    "e606ed8376b95afc09553f424560fb33e47ca95ae0caaedb33073f9b2ad4129f";
pub const LFM3_1GB_MODEL_SIZE_BYTES: u64 = 999_984_736;
pub const LFM3_1GB_MODEL_FILE_NAME: &str = "LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf";

/// LiquidAI LFM3-1.7B GSQ + SLQ (`kaitchup/LFM3-1.7B-GSQ-GGUF` `Q6_K_SLQ`) —
/// 1.59 GB SLQ+GSQ mixed-precision GGUF CPU helper model.
pub const LFM3_SLQ_MODEL_URL: &str =
    "https://huggingface.co/kaitchup/LFM3-1.7B-GSQ-GGUF/resolve/main/LFM3-1.7B-GSQ-Q6_K_SLQ.gguf";
pub const LFM3_SLQ_MODEL_SHA256: &str =
    "9541a534e5b88e0b9bd180b885b16bb1fd3e252e40a0c6aef95f858c1b2e9506";
pub const LFM3_SLQ_MODEL_SIZE_BYTES: u64 = 1_587_721_664;
pub const LFM3_SLQ_MODEL_FILE_NAME: &str = "LFM3-1.7B-GSQ-Q6_K_SLQ.gguf";

/// Pinned local CPU helper model specification (URL + exact byte size + SHA256).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootModelSpec {
    pub id: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    pub description: &'static str,
}

/// Verified catalog of local CPU helper models supported by the bundled `llama-server` runner.
pub const BOOT_MODELS: &[BootModelSpec] = &[
    BootModelSpec {
        id: "lfm3-1.7b-gsq-1gb",
        file_name: LFM3_1GB_MODEL_FILE_NAME,
        url: LFM3_1GB_MODEL_URL,
        sha256: LFM3_1GB_MODEL_SHA256,
        size_bytes: LFM3_1GB_MODEL_SIZE_BYTES,
        description: "LiquidAI LFM3-1.7B GSQ 1GB (IQ4_XS_RCO, 1.00 GB) — fast 1GB CPU helper model",
    },
    BootModelSpec {
        id: "lfm3-1.7b-gsq-slq",
        file_name: LFM3_SLQ_MODEL_FILE_NAME,
        url: LFM3_SLQ_MODEL_URL,
        sha256: LFM3_SLQ_MODEL_SHA256,
        size_bytes: LFM3_SLQ_MODEL_SIZE_BYTES,
        description: "LiquidAI LFM3-1.7B GSQ+SLQ (Q6_K_SLQ, 1.59 GB) — higher-precision SLQ+GSQ CPU helper model",
    },
    BootModelSpec {
        id: "boot-assistant",
        file_name: MODEL_FILE_NAME,
        url: MODEL_URL,
        sha256: MODEL_SHA256,
        size_bytes: MODEL_SIZE_BYTES,
        description: "Qwen3-0.6B Q8_0 (639 MB) — compact setup assistant model",
    },
];

/// Look up a pinned local CPU helper model by id or filename.
pub fn find_boot_model_spec(id_or_file: &str) -> Option<&'static BootModelSpec> {
    let needle = id_or_file.trim();
    BOOT_MODELS
        .iter()
        .find(|m| m.id.eq_ignore_ascii_case(needle) || m.file_name.eq_ignore_ascii_case(needle))
}

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

/// Verify `path` against a specific [`BootModelSpec`]: regular-file and size
/// gates first (cheap), then the full streamed sha256 digest.
pub fn verify_model_file_spec(path: &Path, spec: &BootModelSpec) -> Result<bool> {
    let meta =
        std::fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if meta.file_type().is_symlink() || !meta.file_type().is_file() {
        return Ok(false);
    }
    if meta.len() != spec.size_bytes {
        return Ok(false);
    }
    Ok(sha256_file(path)? == spec.sha256)
}

/// Whether the file at `path` matches any pinned model in [`BOOT_MODELS`].
pub fn verify_any_boot_model_file(path: &Path) -> Result<Option<&'static BootModelSpec>> {
    let meta =
        std::fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if meta.file_type().is_symlink() || !meta.file_type().is_file() {
        return Ok(None);
    }
    let len = meta.len();
    let Some(spec) = BOOT_MODELS.iter().find(|s| s.size_bytes == len) else {
        return Ok(None);
    };
    if sha256_file(path)? == spec.sha256 {
        Ok(Some(spec))
    } else {
        Ok(None)
    }
}

/// Whether the file at `path` is a verified pinned boot/helper model:
/// regular-file and size gates first (cheap), then the full digest.
pub fn verify_model_file(path: &Path) -> Result<bool> {
    Ok(verify_any_boot_model_file(path)?.is_some())
}

/// Platform binary name for `llama-server` (`llama-server.exe` on Windows).
pub const LLAMA_SERVER_BIN_NAME: &str = if cfg!(windows) {
    "llama-server.exe"
} else {
    "llama-server"
};

/// Candidate locations for a bundled `llama-server` binary shipped alongside
/// `selfware` (e.g. `<exe_dir>/bin/llama-server.exe` or `<exe_dir>/llama-server.exe`)
/// or installed under `boot_dir`.
pub fn bundled_llama_server_candidates(exe_dir: Option<&Path>, boot_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let names: &[&str] = if cfg!(windows) {
        &["llama-server.exe", "llama-server"]
    } else {
        &["llama-server", "llama-server.exe"]
    };
    if let Some(dir) = exe_dir {
        for name in names {
            out.push(dir.join("bin").join(name));
            out.push(dir.join(name));
        }
    }
    for name in names {
        out.push(boot_dir.join("bin").join(name));
        out.push(boot_dir.join(name));
    }
    out
}

/// Candidate locations for a pre-bundled `Qwen3-0.6B-Q8_0.gguf` model shipped
/// inside a release archive (`<exe_dir>/models/Qwen3-0.6B-Q8_0.gguf`, etc.)
/// or pointed to by `SELFWARE_BOOT_MODEL`.
pub fn bundled_model_candidates(
    exe_dir: Option<&Path>,
    env_override: Option<&str>,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = env_override {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            out.push(PathBuf::from(trimmed));
        }
    }
    if let Some(dir) = exe_dir {
        out.push(dir.join("models").join(MODEL_FILE_NAME));
        out.push(dir.join(MODEL_FILE_NAME));
        out.push(dir.join("boot").join(MODEL_FILE_NAME));
        for spec in BOOT_MODELS {
            if spec.file_name != MODEL_FILE_NAME {
                out.push(dir.join("models").join(spec.file_name));
                out.push(dir.join(spec.file_name));
                out.push(dir.join("boot").join(spec.file_name));
            }
        }
    }
    out
}

/// Locate a verified bundled model file next to the executable (or via
/// `SELFWARE_BOOT_MODEL`) so packaged Windows/macOS/Linux bundles work
/// offline without re-downloading 639 MB.
pub fn find_bundled_model() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let env_override = std::env::var("SELFWARE_BOOT_MODEL").ok();
    bundled_model_candidates(exe_dir.as_deref(), env_override.as_deref())
        .into_iter()
        .find(|candidate| verify_model_file(candidate).unwrap_or(false))
}

/// Pure `PATH` lookup for `llama-server` / `llama-server.exe` that works on
/// both Windows and Unix without shelling out to `which`.
pub fn find_llama_server_in_path(path_env: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let path_env = path_env?;
    let names: &[&str] = if cfg!(windows) {
        &["llama-server.exe", "llama-server"]
    } else {
        &["llama-server"]
    };
    for dir in std::env::split_paths(path_env) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Pure resolution rule for the `llama-server` binary: the `LLAMA_SERVER`
/// env var wins, then whatever a bundled/PATH lookup found. Extracted from
/// [`find_llama_server`] so tests don't depend on the host machine.
pub fn resolve_llama_server(env_val: Option<String>, path_hit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(p) = env_val {
        if !p.trim().is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    path_hit
}

/// Find `llama-server`:
/// 1. `LLAMA_SERVER` env var
/// 2. Bundled beside the `selfware` executable (`<exe_dir>/bin/llama-server[.exe]` or `<exe_dir>/llama-server[.exe]`)
/// 3. Installed under `boot_dir()` (`~/.local/share/selfware/boot/bin/llama-server[.exe]`)
/// 4. `PATH` lookup (`llama-server.exe` on Windows, `llama-server` on Unix)
pub fn find_llama_server() -> Option<PathBuf> {
    let env_val = std::env::var("LLAMA_SERVER").ok();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let bundled_hit = bundled_llama_server_candidates(exe_dir.as_deref(), &boot_dir())
        .into_iter()
        .find(|p| p.is_file());
    let path_hit =
        bundled_hit.or_else(|| find_llama_server_in_path(std::env::var_os("PATH").as_deref()));
    resolve_llama_server(env_val, path_hit)
}

/// Where to get llama.cpp when no server binary is available.
pub const LLAMA_INSTALL_HINT: &str = "llama-server not found. Use the bundled release archive (which includes bin/llama-server), install llama.cpp (https://github.com/ggml-org/llama.cpp), or point LLAMA_SERVER at the binary.";

/// Ensure a specific [`BootModelSpec`] exists (either pre-bundled next to the
/// executable or under `dir`) and passes integrity verification.
pub async fn ensure_model_spec(dir: &Path, spec: &BootModelSpec) -> Result<PathBuf> {
    let path = dir.join(spec.file_name);
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_file() && verify_model_file_spec(&path, spec)? {
            return Ok(path);
        }
        eprintln!(
            "  boot model at {} failed sha256 verification — deleting and re-downloading.",
            path.display()
        );
        std::fs::remove_file(&path)?;
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let env_override = std::env::var("SELFWARE_BOOT_MODEL").ok();
    if let Some(bundled) = bundled_model_candidates(exe_dir.as_deref(), env_override.as_deref())
        .into_iter()
        .find(|candidate| verify_model_file_spec(candidate, spec).unwrap_or(false))
    {
        return Ok(bundled);
    }
    download_model_spec(dir, &path, spec).await?;
    if !verify_model_file_spec(&path, spec)? {
        std::fs::remove_file(&path).ok();
        bail!(
            "downloaded boot model {} failed sha256 verification (expected {}, size {} bytes) — refusing to use it. File deleted; check your network/mirror and retry.",
            spec.file_name,
            spec.sha256,
            spec.size_bytes
        );
    }
    Ok(path)
}

/// Ensure the model exists (either pre-bundled next to the executable or under
/// `dir`) and passes integrity verification.
/// Downloads (to a `.partial` file, then renames) when missing; an existing
/// file that fails verification is deleted and re-downloaded once — a second
/// mismatch fails loudly.
pub async fn ensure_model(dir: &Path) -> Result<PathBuf> {
    let path = model_path_in(dir);
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_file() && verify_model_file(&path)? {
            return Ok(path);
        }
        eprintln!(
            "  boot model at {} failed sha256 verification — deleting and re-downloading.",
            path.display()
        );
        std::fs::remove_file(&path)?;
    }
    if let Some(bundled) = find_bundled_model() {
        return Ok(bundled);
    }
    let default_spec = find_boot_model_spec(SERVER_ALIAS).expect("boot-assistant spec exists");
    download_model_spec(dir, &path, default_spec).await?;
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

pub(crate) fn check_download_progress(
    downloaded: u64,
    chunk_len: usize,
    expected_size: u64,
) -> Result<u64> {
    let next = downloaded.saturating_add(chunk_len as u64);
    if next > expected_size {
        bail!(
            "model download exceeded expected size ({} > {} bytes); aborting stream",
            next,
            expected_size
        );
    }
    Ok(next)
}

pub(crate) fn check_download_complete(downloaded: u64, expected_size: u64) -> Result<()> {
    if downloaded != expected_size {
        bail!(
            "model download truncated ({} < {} bytes); refusing partial stream",
            downloaded,
            expected_size
        );
    }
    Ok(())
}

async fn download_model_spec(dir: &Path, dest: &Path, spec: &BootModelSpec) -> Result<()> {
    use futures::StreamExt;

    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    println!(
        "  Downloading {} ({} MB) from HuggingFace...",
        spec.file_name,
        spec.size_bytes / 1_000_000
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()?;
    let resp = client
        .get(spec.url)
        .send()
        .await
        .context("starting model download")?;
    if !resp.status().is_success() {
        bail!("model download returned HTTP {}", resp.status());
    }
    if let Some(content_len) = resp.content_length() {
        if content_len != spec.size_bytes {
            bail!(
                "model download reported unexpected Content-Length {} (expected {} bytes)",
                content_len,
                spec.size_bytes
            );
        }
    }
    let (partial, mut file) = crate::session::checkpoint::create_atomic_temp_file(dest, 0o600)
        .with_context(|| format!("creating a temporary model beside {}", dest.display()))?;
    let download_result = async {
        let mut stream = resp.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut next_report: u64 = 64 * 1024 * 1024;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("model download stream")?;
            downloaded = check_download_progress(downloaded, chunk.len(), spec.size_bytes)?;
            file.write_all(&chunk)?;
            if downloaded >= next_report {
                println!(
                    "    {} MB / {} MB",
                    downloaded / 1_000_000,
                    spec.size_bytes / 1_000_000
                );
                next_report += 256 * 1024 * 1024;
            }
        }
        check_download_complete(downloaded, spec.size_bytes)?;
        file.sync_all()?;
        Ok::<(), anyhow::Error>(())
    }
    .await;
    drop(file);
    if let Err(error) = download_result {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    if let Err(error) = crate::session::checkpoint::replace_atomically(&partial, dest) {
        let _ = std::fs::remove_file(&partial);
        return Err(error).with_context(|| format!("moving {} into place", dest.display()));
    }
    Ok(())
}

/// Select the best pinned CPU helper model that fits comfortably inside
/// `available_ram_mb` (leaving headroom for OS + `llama-server` KV cache + `selfware`):
/// - `< 1500` MB: `boot-assistant` (`Qwen3-0.6B-Q8_0.gguf`, 639 MB weights, ~860 MB RSS)
/// - `1500..2600` MB (2 GB RAM class): `lfm3-1.7b-gsq-1gb` (`LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf`, 1.00 GB weights, ~1.22 GB RSS)
/// - `>= 2600` MB: `lfm3-1.7b-gsq-slq` (`LFM3-1.7B-GSQ-Q6_K_SLQ.gguf`, 1.59 GB weights, ~1.85 GB RSS)
pub fn pick_boot_model_for_memory(available_ram_mb: u64) -> &'static BootModelSpec {
    if available_ram_mb < 1500 {
        find_boot_model_spec("boot-assistant").expect("boot-assistant spec exists")
    } else if available_ram_mb < 2600 {
        find_boot_model_spec("lfm3-1.7b-gsq-1gb").expect("lfm3-1.7b-gsq-1gb spec exists")
    } else {
        find_boot_model_spec("lfm3-1.7b-gsq-slq").expect("lfm3-1.7b-gsq-slq spec exists")
    }
}

/// Detect available system RAM in MiB via `sysinfo` (falls back to 2048 MiB on error).
pub fn detect_available_memory_mb() -> u64 {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    let avail = sys.available_memory();
    let total = sys.total_memory();
    let effective_bytes = if avail > 0 { avail } else { total };
    if effective_bytes == 0 {
        2048
    } else {
        effective_bytes / (1024 * 1024)
    }
}

/// argv for the boot-assistant server (without the binary path). CPU-safe
/// 2GB-RAM-friendly defaults: no `-ngl` (works on machines without a GPU),
/// 8k context with `q8_0` quantized KV cache (`-ctk q8_0 -ctv q8_0`) and
/// bounded micro-batch (`-b 256 -ub 128`) so total RSS stays under 2 GB.
pub fn build_server_args(model: &Path, port: u16) -> Vec<String> {
    build_server_args_with_alias(model, SERVER_ALIAS, port)
}

/// argv for `llama-server` with an explicit model alias (e.g. `lfm3-1.7b-gsq-1gb`,
/// `lfm3-1.7b-gsq-slq`, or `boot-assistant`).
pub fn build_server_args_with_alias(model: &Path, alias: &str, port: u16) -> Vec<String> {
    vec![
        "-m".into(),
        model.to_string_lossy().into_owned(),
        "--alias".into(),
        alias.into(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "-c".into(),
        "8192".into(),
        "-ctk".into(),
        "q8_0".into(),
        "-ctv".into(),
        "q8_0".into(),
        "-b".into(),
        "256".into(),
        "-ub".into(),
        "128".into(),
        "--jinja".into(),
    ]
}

/// Whether `endpoint` targets the local bundled boot-assistant server port (`8787`).
pub fn is_boot_server_endpoint(endpoint: &str) -> bool {
    let trimmed = endpoint.trim().trim_end_matches('/');
    matches!(
        trimmed,
        "http://127.0.0.1:8787/v1"
            | "http://localhost:8787/v1"
            | "http://127.0.0.1:8787"
            | "http://localhost:8787"
    )
}

/// Fast health probe against a running local `llama-server`.
pub async fn is_boot_server_healthy(port: u16) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(500))
        .build()
    else {
        return false;
    };
    let health = format!("http://127.0.0.1:{}/health", port);
    client
        .get(&health)
        .send()
        .await
        .map(|resp| resp.status().is_success())
        .unwrap_or(false)
}

static BACKGROUND_BOOT_SERVER: std::sync::OnceLock<std::sync::Mutex<Option<LlamaServerGuard>>> =
    std::sync::OnceLock::new();

fn background_server_slot() -> &'static std::sync::Mutex<Option<LlamaServerGuard>> {
    BACKGROUND_BOOT_SERVER.get_or_init(|| std::sync::Mutex::new(None))
}

/// Ensure the bundled/boot `llama-server` on port 8787 is running in the
/// background for the lifetime of the current process.
/// Returns the OpenAI-compatible base URL (`http://127.0.0.1:8787/v1`).
pub async fn ensure_background_boot_server() -> Result<String> {
    ensure_background_boot_server_for_model(None).await
}

/// Ensure the bundled/boot `llama-server` is running for the requested
/// `model_id` (e.g. `Some("lfm3-1.7b-gsq-1gb")`, `Some("lfm3-1.7b-gsq-slq")`,
/// or `Some("boot-assistant")`).
pub async fn ensure_background_boot_server_for_model(model_id: Option<&str>) -> Result<String> {
    if is_boot_server_healthy(SERVER_PORT).await {
        return Ok(format!("http://127.0.0.1:{}/v1", SERVER_PORT));
    }
    let Some(server_bin) = find_llama_server() else {
        bail!("{}", LLAMA_INSTALL_HINT);
    };
    let (model_path, alias) = if let Some(spec) = model_id.and_then(find_boot_model_spec) {
        (ensure_model_spec(&boot_dir(), spec).await?, spec.id)
    } else if let Some(bundled) = find_bundled_model() {
        let alias = verify_any_boot_model_file(&bundled)
            .ok()
            .flatten()
            .map(|s| s.id)
            .unwrap_or(SERVER_ALIAS);
        (bundled, alias)
    } else {
        let spec = pick_boot_model_for_memory(detect_available_memory_mb());
        (ensure_model_spec(&boot_dir(), spec).await?, spec.id)
    };
    let guard = spawn_server_with_alias(&server_bin, &model_path, alias, SERVER_PORT).await?;
    let base_url = guard.base_url();
    if let Ok(mut slot) = background_server_slot().lock() {
        *slot = Some(guard);
    }
    Ok(base_url)
}

/// Cleanly terminate the background `llama-server` child if one was started
/// by [`ensure_background_boot_server`].
pub fn shutdown_background_boot_server() {
    if let Some(mutex) = BACKGROUND_BOOT_SERVER.get() {
        if let Ok(mut slot) = mutex.lock() {
            let _ = slot.take();
        }
    }
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
    spawn_server_with_alias(binary, model, SERVER_ALIAS, port).await
}

/// Spawn llama-server with an explicit model alias and wait until `/health` answers.
pub async fn spawn_server_with_alias(
    binary: &Path,
    model: &Path,
    alias: &str,
    port: u16,
) -> Result<LlamaServerGuard> {
    use crate::safety::process_env::SanitizedEnvExt;

    let mut child = Command::new(binary)
        .sanitized_env()
        .args(build_server_args_with_alias(model, alias, port))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawning {}", binary.display()))?;

    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
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
            let _ = child.wait();
            bail!("llama-server did not become healthy within 120s");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/boot/model_test.rs"]
mod tests;
