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

/// Google FunctionGemma 270M IT (`unsloth/functiongemma-270m-it-GGUF` `UD-Q6_K_XL`) —
/// 273 MiB ultra-lightweight 32k-context function-calling & setup assistant GGUF.
/// Replaces `Qwen3-0.6B-Q8_0.gguf` as the base `boot-assistant` model (~470 MiB RSS, 238 tok/s).
pub const MODEL_URL: &str =
    "https://huggingface.co/unsloth/functiongemma-270m-it-GGUF/resolve/main/functiongemma-270m-it-UD-Q6_K_XL.gguf";
pub const MODEL_SHA256: &str = "906f29fc5343b33a8d7d5ccd7433a82d8d3cefe61893825ca0e504c402add759";
pub const MODEL_SIZE_BYTES: u64 = 286_162_144;
pub const MODEL_FILE_NAME: &str = "functiongemma-270m-it-UD-Q6_K_XL.gguf";

/// Explicit FunctionGemma 270M IT aliases for callers referencing the base model directly.
pub const FUNCTIONGEMMA_270M_MODEL_URL: &str = MODEL_URL;
pub const FUNCTIONGEMMA_270M_MODEL_SHA256: &str = MODEL_SHA256;
pub const FUNCTIONGEMMA_270M_MODEL_SIZE_BYTES: u64 = MODEL_SIZE_BYTES;
pub const FUNCTIONGEMMA_270M_MODEL_FILE_NAME: &str = MODEL_FILE_NAME;
pub const FUNCTIONGEMMA_CTX_32K: usize = 32_768;

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

/// Supported context window sizes for LiquidAI LFM3-1.7B-GSQ (native `n_ctx_train = 262144`):
/// - `65_536` (64k): ~1.49 GiB RSS (`-np 1 --no-repack -ctk q8_0 -ctv q8_0`)
/// - `131_072` (128k): ~1.89 GiB RSS (`-np 1 --no-repack -ctk q8_0 -ctv q8_0`)
/// - `262_144` (256k): ~1.95 GiB RSS (`-np 1 --no-repack -ctk q4_0 -ctv q4_0`)
pub const LFM3_CTX_64K: usize = 65_536;
pub const LFM3_CTX_128K: usize = 131_072;
pub const LFM3_CTX_256K: usize = 262_144;
pub const LFM3_SUPPORTED_CONTEXTS: &[usize] = &[LFM3_CTX_64K, LFM3_CTX_128K, LFM3_CTX_256K];

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
        description: "LiquidAI LFM3-1.7B GSQ 1GB (IQ4_XS_RCO, 1.00 GB) — fast 1GB CPU helper model (64k/128k/256k ctx)",
    },
    BootModelSpec {
        id: "lfm3-1.7b-gsq-slq",
        file_name: LFM3_SLQ_MODEL_FILE_NAME,
        url: LFM3_SLQ_MODEL_URL,
        sha256: LFM3_SLQ_MODEL_SHA256,
        size_bytes: LFM3_SLQ_MODEL_SIZE_BYTES,
        description: "LiquidAI LFM3-1.7B GSQ+SLQ (Q6_K_SLQ, 1.59 GB) — higher-precision SLQ+GSQ CPU helper model (64k/128k/256k ctx)",
    },
    BootModelSpec {
        id: "boot-assistant",
        file_name: MODEL_FILE_NAME,
        url: MODEL_URL,
        sha256: MODEL_SHA256,
        size_bytes: MODEL_SIZE_BYTES,
        description: "Google FunctionGemma 270M IT (UD-Q6_K_XL, 273 MB) — ultra-lightweight 32k-ctx function-calling setup assistant",
    },
];

/// Pinned cross-platform `llama.cpp` (`b12110`) runtime archive specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlamaServerRuntimeSpec {
    pub os: &'static str,
    pub arch: &'static str,
    pub archive_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
}

/// Official `ggml-org/llama.cpp` release `b12110` archives (with LFM3 + GSQ/SLQ support)
/// for macOS (`aarch64` & `x86_64`), Linux (`x86_64`), and Windows (`x86_64`).
pub const LLAMA_SERVER_RUNTIMES: &[LlamaServerRuntimeSpec] = &[
    LlamaServerRuntimeSpec {
        os: "macos",
        arch: "aarch64",
        archive_name: "llama-b12110-bin-macos-arm64.tar.gz",
        url: "https://github.com/ggml-org/llama.cpp/releases/download/b12110/llama-b12110-bin-macos-arm64.tar.gz",
        sha256: "4f5b8de12e0b9516658ca6a5764c6ffb3d7c659c3dd538556b1413c7b4050662",
        size_bytes: 16_650_691,
    },
    LlamaServerRuntimeSpec {
        os: "macos",
        arch: "x86_64",
        archive_name: "llama-b12110-bin-macos-x64.tar.gz",
        url: "https://github.com/ggml-org/llama.cpp/releases/download/b12110/llama-b12110-bin-macos-x64.tar.gz",
        sha256: "c8094268b6bf62f4998f0e45656fd9f5e2d4afb6b1cd0044d761db64be1dd47d",
        size_bytes: 12_388_601,
    },
    LlamaServerRuntimeSpec {
        os: "linux",
        arch: "x86_64",
        archive_name: "llama-b12110-bin-ubuntu-x64.tar.gz",
        url: "https://github.com/ggml-org/llama.cpp/releases/download/b12110/llama-b12110-bin-ubuntu-x64.tar.gz",
        sha256: "368227edcc229be5f9bed16a2223d79b5e41d0ac2e19bcf5825e82291c060e55",
        size_bytes: 23_051_958,
    },
    LlamaServerRuntimeSpec {
        os: "windows",
        arch: "x86_64",
        archive_name: "llama-b12110-bin-win-cpu-x64.zip",
        url: "https://github.com/ggml-org/llama.cpp/releases/download/b12110/llama-b12110-bin-win-cpu-x64.zip",
        sha256: "98901e327c252875accaf2d1bf364e10e868843b0b63468baad1aa945cdc5da2",
        size_bytes: 24_708_445,
    },
];

/// Look up the pinned `llama-server` runtime archive for `(os, arch)`.
pub fn llama_runtime_spec_for(os: &str, arch: &str) -> Option<&'static LlamaServerRuntimeSpec> {
    LLAMA_SERVER_RUNTIMES
        .iter()
        .find(|r| r.os.eq_ignore_ascii_case(os) && r.arch.eq_ignore_ascii_case(arch))
}

/// Pinned `llama-server` runtime archive for the current compilation target.
pub fn current_platform_llama_runtime_spec() -> Option<&'static LlamaServerRuntimeSpec> {
    llama_runtime_spec_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// Look up a pinned local CPU helper model by id, context-tagged alias, or filename.
/// Context-tagged LFM3 aliases (`lfm3-1.7b-gsq`, `lfm3-1.7b-gsq-64k`,
/// `lfm3-1.7b-gsq-128k`, `lfm3-1.7b-gsq-256k`) resolve to the 1GB `IQ4_XS_RCO`
/// GGUF spec unless `slq` is in the alias. FunctionGemma aliases
/// (`functiongemma-270m-it`, `functiongemma-270m`, `functiongemma`) resolve to
/// the `boot-assistant` spec (`functiongemma-270m-it-UD-Q6_K_XL.gguf`).
pub fn find_boot_model_spec(id_or_file: &str) -> Option<&'static BootModelSpec> {
    let needle = id_or_file.trim();
    if let Some(exact) = BOOT_MODELS
        .iter()
        .find(|m| m.id.eq_ignore_ascii_case(needle) || m.file_name.eq_ignore_ascii_case(needle))
    {
        return Some(exact);
    }
    let lower = needle.to_ascii_lowercase();
    if lower.starts_with("lfm3-1.7b-gsq") {
        if lower.contains("slq") {
            return BOOT_MODELS.iter().find(|m| m.id == "lfm3-1.7b-gsq-slq");
        }
        return BOOT_MODELS.iter().find(|m| m.id == "lfm3-1.7b-gsq-1gb");
    }
    if lower.starts_with("functiongemma") || lower.ends_with("/functiongemma-270m-it") {
        return BOOT_MODELS.iter().find(|m| m.id == "boot-assistant");
    }
    None
}

/// Resolve the context window (`-c`) to pass to `llama-server` for `model_id`
/// and an optional config `context_length` override:
/// - Explicit `configured_ctx` (`> 0`) wins when provided
/// - Context-tagged aliases (`*-64k`, `*-128k`, `*-256k`) map to `65_536`,
///   `131_072`, and `262_144` respectively
/// - Other `lfm3-*` models default to `65_536` (64k)
/// - `functiongemma*` aliases default to `32_768` (32k native context)
/// - `boot-assistant` (`functiongemma-270m-it-UD-Q6_K_XL.gguf`) defaults to `8_192`
pub fn resolve_boot_model_context(model_id: Option<&str>, configured_ctx: Option<usize>) -> usize {
    let lower = model_id.unwrap_or("").trim().to_ascii_lowercase();
    if lower.ends_with("-256k") {
        return configured_ctx
            .filter(|&c| c >= LFM3_CTX_64K)
            .unwrap_or(LFM3_CTX_256K);
    }
    if lower.ends_with("-128k") {
        return configured_ctx
            .filter(|&c| c >= LFM3_CTX_64K)
            .unwrap_or(LFM3_CTX_128K);
    }
    if lower.ends_with("-64k") {
        return configured_ctx
            .filter(|&c| c >= LFM3_CTX_64K)
            .unwrap_or(LFM3_CTX_64K);
    }
    if let Some(ctx) = configured_ctx.filter(|&c| c > 0) {
        return ctx;
    }
    if lower.starts_with("lfm3") {
        LFM3_CTX_64K
    } else if lower.contains("functiongemma") {
        FUNCTIONGEMMA_CTX_32K
    } else {
        8_192
    }
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

/// Candidate locations for a pre-bundled GGUF model shipped inside a release
/// archive (`<exe_dir>/models/LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf`,
/// `<exe_dir>/models/functiongemma-270m-it-UD-Q6_K_XL.gguf`, etc.)
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

/// Ensure `llama-server` is available for the current platform: returns an
/// existing binary from [`find_llama_server`] when present, or downloads,
/// SHA256-verifies, and extracts the pinned `ggml-org/llama.cpp` `b12110`
/// archive for the host OS/arch (`macos-arm64`, `macos-x64`, `linux-x64`,
/// `windows-x64`) into `dir/bin/`.
pub async fn ensure_llama_server(dir: &Path) -> Result<PathBuf> {
    if let Some(existing) = find_llama_server() {
        return Ok(existing);
    }
    let Some(spec) = current_platform_llama_runtime_spec() else {
        bail!("{}", LLAMA_INSTALL_HINT);
    };
    let bin_dir = dir.join("bin");
    let expected_bin = bin_dir.join(LLAMA_SERVER_BIN_NAME);
    if expected_bin.is_file() {
        return Ok(expected_bin);
    }
    std::fs::create_dir_all(&bin_dir).with_context(|| format!("creating {}", bin_dir.display()))?;
    let archive_path = dir.join(spec.archive_name);
    download_verified_archive(dir, &archive_path, spec).await?;
    extract_llama_runtime_archive(&archive_path, &bin_dir, spec)?;
    let _ = std::fs::remove_file(&archive_path);
    if expected_bin.is_file() {
        return Ok(expected_bin);
    }
    bail!(
        "extracted {} into {} but {} was not found",
        spec.archive_name,
        bin_dir.display(),
        expected_bin.display()
    )
}

async fn download_verified_archive(
    dir: &Path,
    dest: &Path,
    spec: &LlamaServerRuntimeSpec,
) -> Result<()> {
    use futures::StreamExt;

    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    println!(
        "  Downloading llama-server runtime {} ({} MB)...",
        spec.archive_name,
        spec.size_bytes / 1_000_000
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()?;
    let resp = client
        .get(spec.url)
        .send()
        .await
        .context("starting llama-server runtime download")?;
    if !resp.status().is_success() {
        bail!(
            "llama-server runtime download returned HTTP {}",
            resp.status()
        );
    }
    let (partial, mut file) = crate::session::checkpoint::create_atomic_temp_file(dest, 0o600)
        .with_context(|| format!("creating temporary archive beside {}", dest.display()))?;
    let download_result = async {
        let mut stream = resp.bytes_stream();
        let mut downloaded: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("llama-server runtime download stream")?;
            downloaded = check_download_progress(downloaded, chunk.len(), spec.size_bytes)?;
            file.write_all(&chunk)?;
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
    if sha256_file(&partial)? != spec.sha256 {
        let _ = std::fs::remove_file(&partial);
        bail!(
            "downloaded llama-server archive {} failed sha256 verification (expected {})",
            spec.archive_name,
            spec.sha256
        );
    }
    if let Err(error) = crate::session::checkpoint::replace_atomically(&partial, dest) {
        let _ = std::fs::remove_file(&partial);
        return Err(error).with_context(|| format!("moving {} into place", dest.display()));
    }
    Ok(())
}

fn extract_llama_runtime_archive(
    archive_path: &Path,
    bin_dir: &Path,
    spec: &LlamaServerRuntimeSpec,
) -> Result<()> {
    use crate::safety::process_env::SanitizedEnvExt;

    let status = if spec.archive_name.ends_with(".tar.gz") {
        Command::new("tar")
            .sanitized_env()
            .arg("-xzf")
            .arg(archive_path)
            .arg("--strip-components=1")
            .arg("-C")
            .arg(bin_dir)
            .status()
            .context("extracting llama-server .tar.gz archive with tar")?
    } else {
        Command::new("tar")
            .sanitized_env()
            .arg("-xf")
            .arg(archive_path)
            .arg("-C")
            .arg(bin_dir)
            .status()
            .context("extracting llama-server .zip archive with tar")?
    };
    if !status.success() {
        bail!("extracting {} exited with {}", spec.archive_name, status);
    }
    Ok(())
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

/// Canonical model ID of the shipped support-system model
/// (`functiongemma-270m-it-UD-Q6_K_XL.gguf`, 273 MiB, ~476 MiB RSS).
pub const SHIPPED_BOOT_MODEL_ID: &str = "boot-assistant";

/// Canonical model ID of the primary local CPU coding upgrade model
/// (`LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf`, 954 MiB, 64k/128k/256k ctx).
pub const PRIMARY_LOCAL_CODING_MODEL_ID: &str = "lfm3-1.7b-gsq-1gb";

/// Return the [`BootModelSpec`] for the default support-system model (`boot-assistant` / `functiongemma-270m-it-UD-Q6_K_XL.gguf`).
pub fn shipped_boot_model_spec() -> &'static BootModelSpec {
    find_boot_model_spec(SHIPPED_BOOT_MODEL_ID).expect("boot-assistant spec exists")
}

/// Ensure the support-system model exists (either pre-bundled next to the
/// executable or under `dir`) and passes integrity verification.
/// Downloads `functiongemma-270m-it-UD-Q6_K_XL.gguf` (273 MiB) first so the user
/// has an instant `< 500 MiB` RAM support assistant to inspect hardware readiness
/// and choose their next model (`LFM3-1.7B-GSQ` 64k/128k/256k or remote endpoints).
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
    let lfm3_path = dir.join(LFM3_1GB_MODEL_FILE_NAME);
    if let Ok(meta) = std::fs::symlink_metadata(&lfm3_path) {
        if meta.file_type().is_file() && verify_model_file(&lfm3_path)? {
            return Ok(lfm3_path);
        }
        eprintln!(
            "  boot model at {} failed sha256 verification — deleting and re-downloading.",
            lfm3_path.display()
        );
        std::fs::remove_file(&lfm3_path)?;
    }
    if let Some(bundled) = find_bundled_model() {
        return Ok(bundled);
    }
    ensure_model_spec(dir, shipped_boot_model_spec()).await
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

/// Render a deterministic progress bar line with percentage, MiB transferred,
/// throughput (`MiB/s`), and estimated remaining seconds (`ETA`).
pub fn format_download_progress_bar(downloaded: u64, total: u64, elapsed_secs: f64) -> String {
    let pct = if total > 0 {
        ((downloaded as f64 / total as f64) * 100.0).clamp(0.0, 100.0)
    } else {
        100.0
    };
    let width: usize = 24;
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(width - filled));
    let dl_mib = downloaded as f64 / (1024.0 * 1024.0);
    let total_mib = total as f64 / (1024.0 * 1024.0);
    let speed_mibs = if elapsed_secs > 0.05 {
        dl_mib / elapsed_secs
    } else {
        0.0
    };
    let eta_str = if downloaded >= total {
        "done".to_string()
    } else if speed_mibs > 0.05 {
        let rem_secs = ((total_mib - dl_mib).max(0.0) / speed_mibs).ceil() as u64;
        if rem_secs >= 60 {
            format!("ETA {}m{:02}s", rem_secs / 60, rem_secs % 60)
        } else {
            format!("ETA {}s", rem_secs)
        }
    } else {
        "ETA --".to_string()
    };
    format!(
        "  [{bar}] {pct:>5.1}%  {dl_mib:>6.1} / {total_mib:.1} MiB  ({speed_mibs:>5.1} MiB/s, {eta_str})"
    )
}

/// Render a structured pre-flight card shown to the user before downloading a
/// local GGUF model, summarizing model specs, SHA256 pin, host RAM/disk checks,
/// and hardware-tailored optimization tips.
pub fn format_download_preflight_banner(
    spec: &BootModelSpec,
    dest: &Path,
    readiness: &ModelResourceReadiness,
) -> String {
    let size_mib = spec.size_bytes as f64 / (1024.0 * 1024.0);
    let short_sha = if spec.sha256.len() >= 16 {
        format!(
            "{}…{}",
            &spec.sha256[..8],
            &spec.sha256[spec.sha256.len() - 8..]
        )
    } else {
        spec.sha256.to_string()
    };
    let ram_badge = if readiness.ram_ok { "PASS" } else { "WARN" };
    let disk_badge = if readiness.disk_ok { "PASS" } else { "FAIL" };
    let mut lines = vec![
        "  ┌─ Selfware Local Model Download ──────────────────────────────────────────".to_string(),
        format!("  │ Model:       {} ({})", spec.id, spec.file_name),
        format!("  │ Description: {}", spec.description),
        format!(
            "  │ Size & Pin:  {:.1} MiB ({} bytes) | SHA256 {}",
            size_mib, spec.size_bytes, short_sha
        ),
        format!("  │ Destination: {}", dest.display()),
        format!(
            "  │ Host RAM:    [{}] {} MiB available (requires ~{} MiB RSS at {} ctx)",
            ram_badge,
            readiness.available_ram_mb,
            readiness.required_ram_mb,
            readiness.context_length
        ),
        format!(
            "  │ Host Disk:   [{}] {} MiB free (requires {} MiB incl. atomic buffer)",
            disk_badge, readiness.available_disk_mb, readiness.required_disk_mb
        ),
        format!(
            "  │ Optimal Fit: {} (ctx {})",
            readiness.recommended_model_id, readiness.recommended_context_length
        ),
    ];
    for tip in &readiness.tips {
        lines.push(format!("  │ Tip:         {}", tip));
    }
    if spec.id == "boot-assistant" {
        lines.push(
            "  │ Next Step:   After the support assistant starts, run `selfware boot` to choose:"
                .to_string(),
        );
        lines.push(
            "  │              • Local LFM3-1.7B-GSQ (64k/128k/256k ctx, ~1.53–1.98 GiB RAM)"
                .to_string(),
        );
        lines.push(
            "  │              • Cloud / Local Server (selfware-design, openrouter, ollama, vllm)"
                .to_string(),
        );
    }
    lines.push(
        "  └──────────────────────────────────────────────────────────────────────────".to_string(),
    );
    lines.join("\n")
}

async fn download_model_spec(dir: &Path, dest: &Path, spec: &BootModelSpec) -> Result<()> {
    use futures::StreamExt;
    use std::io::IsTerminal;

    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let default_ctx = resolve_boot_model_context(Some(spec.id), None);
    let avail_ram_mb = detect_available_memory_mb();
    let avail_disk_mb = detect_available_disk_mb(dir);
    let readiness =
        evaluate_model_resource_readiness(spec, default_ctx, false, avail_ram_mb, avail_disk_mb);
    println!(
        "{}",
        format_download_preflight_banner(spec, dest, &readiness)
    );
    if !readiness.disk_ok {
        bail!(
            "insufficient disk space in {} to download {} (need {} MiB including temporary buffer, available {} MiB) — free disk space or set SELFWARE_BOOT_MODEL to an existing GGUF path",
            dir.display(),
            spec.file_name,
            readiness.required_disk_mb,
            readiness.available_disk_mb
        );
    }
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
    let is_tty = std::io::stdout().is_terminal();
    let start_instant = std::time::Instant::now();
    let step_bytes = (spec.size_bytes / 10).max(16 * 1024 * 1024);
    let download_result = async {
        let mut stream = resp.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut next_report: u64 = step_bytes;
        let mut last_tty_update = std::time::Instant::now();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("model download stream")?;
            downloaded = check_download_progress(downloaded, chunk.len(), spec.size_bytes)?;
            file.write_all(&chunk)?;
            let elapsed = start_instant.elapsed().as_secs_f64();
            if is_tty {
                if last_tty_update.elapsed() >= Duration::from_millis(120)
                    || downloaded == spec.size_bytes
                {
                    print!(
                        "\r{}",
                        format_download_progress_bar(downloaded, spec.size_bytes, elapsed)
                    );
                    let _ = std::io::stdout().flush();
                    last_tty_update = std::time::Instant::now();
                }
            } else if downloaded >= next_report || downloaded == spec.size_bytes {
                println!(
                    "{}",
                    format_download_progress_bar(downloaded, spec.size_bytes, elapsed)
                );
                next_report = downloaded.saturating_add(step_bytes);
            }
        }
        if is_tty {
            println!();
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
    println!(
        "  [PASS] Downloaded {} ({:.1} MiB) — verifying SHA256 integrity...",
        spec.file_name,
        spec.size_bytes as f64 / (1024.0 * 1024.0)
    );
    Ok(())
}

/// Select the best pinned CPU helper model that fits comfortably inside
/// `available_ram_mb` (leaving headroom for OS + `llama-server` KV cache + `selfware`):
/// - `< 1500` MB: `boot-assistant` (`functiongemma-270m-it-UD-Q6_K_XL.gguf`, 273 MB weights, ~476 MB RSS)
/// - `1500..2600` MB (2 GB RAM class): `lfm3-1.7b-gsq-1gb` (`LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf`, 954 MiB weights, ~1.53 GiB RSS at 64k)
/// - `>= 2600` MB: `lfm3-1.7b-gsq-slq` (`LFM3-1.7B-GSQ-Q6_K_SLQ.gguf`, 1.51 GiB weights, ~2.15 GiB RSS at 64k)
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

/// Detect available disk space in MiB on the mount point containing `target_dir`
/// via `sysinfo::Disks` (picks the longest matching mount prefix).
pub fn detect_available_disk_mb(target_dir: &Path) -> u64 {
    use sysinfo::Disks;

    let canonical = target_dir
        .canonicalize()
        .or_else(|_| {
            target_dir
                .parent()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
                .and_then(Path::canonicalize)
        })
        .unwrap_or_else(|_| target_dir.to_path_buf());

    let disks = Disks::new_with_refreshed_list();
    let best_match = disks
        .list()
        .iter()
        .filter(|d| canonical.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len());

    let avail_bytes = best_match
        .map(|d| d.available_space())
        .or_else(|| disks.list().iter().map(|d| d.available_space()).max())
        .unwrap_or(0);
    avail_bytes / (1024 * 1024)
}

/// Pre-flight RAM and disk-space readiness report for a local bundled model
/// (`LFM3-1.7B-GSQ` 64k/128k/256k or `FunctionGemma 270M` 32k), including
/// actionable tips to help the user pick the optimal local configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResourceReadiness {
    pub model_id: &'static str,
    pub file_name: &'static str,
    pub context_length: usize,
    pub model_already_downloaded: bool,
    pub required_ram_mb: u64,
    pub available_ram_mb: u64,
    pub ram_ok: bool,
    pub required_disk_mb: u64,
    pub available_disk_mb: u64,
    pub disk_ok: bool,
    pub recommended_model_id: &'static str,
    pub recommended_context_length: usize,
    pub tips: Vec<String>,
}

impl ModelResourceReadiness {
    /// Whether both RAM and disk checks passed.
    pub fn is_ready(&self) -> bool {
        self.ram_ok && self.disk_ok
    }

    /// Concise one-line status summary for `boot --check` and `llm-doctor`.
    pub fn summary_line(&self) -> String {
        let disk_note = if self.model_already_downloaded {
            format!("{} MiB free (cached)", self.available_disk_mb)
        } else {
            format!(
                "{} / {} MiB free to download",
                self.available_disk_mb, self.required_disk_mb
            )
        };
        format!(
            "{} (ctx {}): RAM {} / {} MiB req [{}], Disk {} [{}] — optimal for host: {} (ctx {})",
            self.model_id,
            self.context_length,
            self.available_ram_mb,
            self.required_ram_mb,
            if self.ram_ok { "OK" } else { "LOW" },
            disk_note,
            if self.disk_ok { "OK" } else { "LOW" },
            self.recommended_model_id,
            self.recommended_context_length,
        )
    }
}

/// Measured combined RSS (`llama-server` + `selfware`) required for `spec` at `ctx_size`:
/// - `boot-assistant` (`functiongemma-270m-it-UD-Q6_K_XL.gguf`): `520 MiB` (measured `476 MiB` at 32k `q8_0` KV + `44 MiB` `selfware`)
/// - `lfm3-1.7b-gsq-1gb` (`LFM3-1.7B-GSQ-1GB-IQ4_XS_RCO.gguf`):
///   - `<= 64k` (`65536`, `q8_0` KV): `1,580 MiB` (measured `1,530 + 46 = 1,576 MiB`)
///   - `<= 128k` (`131072`, `q8_0` KV): `1,980 MiB` (measured `1,927 + 46 = 1,973 MiB`)
///   - `> 128k` (`262144`, `q4_0` KV): `2,040 MiB` (measured `1,984 + 46 = 2,030 MiB`)
/// - `lfm3-1.7b-gsq-slq` (`LFM3-1.7B-GSQ-Q6_K_SLQ.gguf`):
///   - `<= 64k`: `2,150 MiB`, `<= 128k`: `2,550 MiB`, `> 128k`: `2,610 MiB`
pub fn required_ram_mb_for_spec(spec: &BootModelSpec, ctx_size: usize) -> u64 {
    match spec.id {
        "boot-assistant" => 520,
        "lfm3-1.7b-gsq-slq" => {
            if ctx_size > LFM3_CTX_128K {
                2610
            } else if ctx_size > LFM3_CTX_64K {
                2550
            } else {
                2150
            }
        }
        _ => {
            if ctx_size > LFM3_CTX_128K {
                2040
            } else if ctx_size > LFM3_CTX_64K {
                1980
            } else {
                1580
            }
        }
    }
}

/// Pure, deterministic evaluation of RAM + disk readiness and optimization tips.
pub fn evaluate_model_resource_readiness(
    spec: &BootModelSpec,
    ctx_size: usize,
    model_already_downloaded: bool,
    available_ram_mb: u64,
    available_disk_mb: u64,
) -> ModelResourceReadiness {
    let required_ram_mb = required_ram_mb_for_spec(spec, ctx_size);
    let model_mb = spec.size_bytes.div_ceil(1024 * 1024);
    let required_disk_mb = if model_already_downloaded {
        64
    } else {
        model_mb.saturating_add(64)
    };
    let ram_ok = available_ram_mb >= required_ram_mb;
    let disk_ok = available_disk_mb == 0 || available_disk_mb >= required_disk_mb;

    let (recommended_model_id, recommended_context_length) = if available_ram_mb < 520 {
        ("openrouter-free", 1_000_000)
    } else if available_ram_mb < 1580 {
        ("boot-assistant", FUNCTIONGEMMA_CTX_32K)
    } else if available_ram_mb < 1980 {
        ("lfm3-1.7b-gsq-64k", LFM3_CTX_64K)
    } else if available_ram_mb < 2040 {
        ("lfm3-1.7b-gsq-128k", LFM3_CTX_128K)
    } else {
        ("lfm3-1.7b-gsq-256k", LFM3_CTX_256K)
    };

    let mut tips = Vec::new();
    if !ram_ok {
        if spec.id.starts_with("lfm3") && ctx_size > LFM3_CTX_64K && available_ram_mb >= 1580 {
            tips.push(format!(
                "RAM low for {} at {} ctx (need {} MiB, have {} MiB): lower `context_length = 65536` (recipe card `local-lfm3-64k`) to cut RSS to ~1,576 MiB.",
                spec.id, ctx_size, required_ram_mb, available_ram_mb
            ));
        } else if available_ram_mb >= 520 {
            tips.push(format!(
                "RAM low for {} (need {} MiB, have {} MiB): switch to base `boot-assistant` (`functiongemma-270m-it-UD-Q6_K_XL.gguf`, recipe card `local-bundled`) which uses only ~476 MiB RSS and 273 MiB disk with a 32k context window.",
                spec.id, required_ram_mb, available_ram_mb
            ));
        } else {
            tips.push(format!(
                "Host available RAM ({} MiB) is below 520 MiB: use a zero-local-RAM remote endpoint card (`selfware-design` or `openrouter-free`) via `selfware boot`.",
                available_ram_mb
            ));
        }
    }
    if !disk_ok {
        tips.push(format!(
            "Disk space low (need {} MiB to store `{}`, have {} MiB): free disk space, use `boot-assistant` (`functiongemma-270m-it`, 273 MiB), or point `SELFWARE_BOOT_MODEL` to an external volume.",
            required_disk_mb, spec.file_name, available_disk_mb
        ));
    }
    if ram_ok && disk_ok {
        if spec.id == "boot-assistant" && available_ram_mb >= 1580 {
            tips.push(format!(
                "Optimal tip: Your host has {} MiB available RAM — you can run the shipped `lfm3-1.7b-gsq-64k` (~1.53 GiB RSS) or `{}` (ctx {}) for higher multi-turn coding quality.",
                available_ram_mb, recommended_model_id, recommended_context_length
            ));
        } else if spec.id.starts_with("lfm3")
            && ctx_size == LFM3_CTX_64K
            && available_ram_mb >= 2040
        {
            tips.push(format!(
                "Optimal tip: 64k context (`q8_0` KV, ~1.53 GiB RSS) gives the fastest prefill/decode; your {} MiB free RAM also supports `local-lfm3-128k` (~1.93 GiB RSS) or `local-lfm3-256k` (`q4_0` KV, ~1.98 GiB RSS).",
                available_ram_mb
            ));
        } else if spec.id.starts_with("lfm3") && ctx_size >= LFM3_CTX_256K {
            tips.push(
                "Optimal tip: 256k context uses `q4_0` KV cache (~1.98 GiB RSS) to fit in <2 GiB RAM; switch to `local-lfm3-64k` (`q8_0` KV, ~1.53 GiB RSS) if you want maximum token/s speed and lower memory.".to_string(),
            );
        }
    }

    ModelResourceReadiness {
        model_id: spec.id,
        file_name: spec.file_name,
        context_length: ctx_size,
        model_already_downloaded,
        required_ram_mb,
        available_ram_mb,
        ram_ok,
        required_disk_mb,
        available_disk_mb,
        disk_ok,
        recommended_model_id,
        recommended_context_length,
        tips,
    }
}

/// Inspect live host RAM (including any already-resident `llama-server` RSS) and
/// free disk space in `dir` to verify whether `model_id` at `ctx_size` can run
/// safely on this machine.
pub fn check_model_resource_readiness(
    model_id: Option<&str>,
    ctx_size: Option<usize>,
    dir: &Path,
) -> ModelResourceReadiness {
    let spec = model_id
        .and_then(find_boot_model_spec)
        .unwrap_or_else(shipped_boot_model_spec);
    let effective_ctx = resolve_boot_model_context(model_id.or(Some(spec.id)), ctx_size);
    let model_path = dir.join(spec.file_name);
    let model_already_downloaded = verify_model_file_spec(&model_path, spec).unwrap_or(false)
        || find_bundled_model()
            .and_then(|p| verify_model_file_spec(&p, spec).ok())
            .unwrap_or(false);

    let mem = measure_memory_snapshot();
    // If our own llama-server child is already holding the model in RAM, credit its
    // current RSS back to available RAM so a running server isn't double-charged.
    let effective_avail_ram_mb = mem
        .system_available_mb
        .saturating_add(mem.llama_server_rss_mb.unwrap_or(0));
    let avail_disk_mb = detect_available_disk_mb(dir);

    evaluate_model_resource_readiness(
        spec,
        effective_ctx,
        model_already_downloaded,
        effective_avail_ram_mb,
        avail_disk_mb,
    )
}

/// Measured resident-set memory (RSS) snapshot for `selfware`, the active
/// local `llama-server` child (if running), and the host machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootMemorySnapshot {
    /// Resident memory (RSS) of the current `selfware` process in MiB.
    pub selfware_rss_mb: u64,
    /// OS PID of the background `llama-server` child, if one is running.
    pub llama_server_pid: Option<u32>,
    /// Resident memory (RSS) of the background `llama-server` child in MiB, if running.
    pub llama_server_rss_mb: Option<u64>,
    /// Active context window (`-c`) of the background `llama-server`, if running.
    pub llama_server_ctx: Option<usize>,
    /// Combined RSS (`selfware_rss_mb + llama_server_rss_mb.unwrap_or(0)`) in MiB.
    pub combined_rss_mb: u64,
    /// Available host RAM in MiB.
    pub system_available_mb: u64,
    /// Total host RAM in MiB.
    pub system_total_mb: u64,
}

impl BootMemorySnapshot {
    /// Human-readable one-line summary of the measured memory footprint.
    pub fn summary_line(&self) -> String {
        match (
            self.llama_server_rss_mb,
            self.llama_server_pid,
            self.llama_server_ctx,
        ) {
            (Some(srv_mb), Some(pid), Some(ctx)) => format!(
                "selfware {} MiB + llama-server {} MiB (PID {}, ctx {}) = {} MiB RSS (host avail: {} / {} MiB)",
                self.selfware_rss_mb,
                srv_mb,
                pid,
                ctx,
                self.combined_rss_mb,
                self.system_available_mb,
                self.system_total_mb
            ),
            _ => format!(
                "selfware {} MiB RSS (no local llama-server child; host avail: {} / {} MiB)",
                self.selfware_rss_mb, self.system_available_mb, self.system_total_mb
            ),
        }
    }
}

/// Measure the live resident memory (RSS) of `selfware`, its background
/// `llama-server` child (if active), and the host's available/total RAM.
/// Refreshes only the two specific PIDs (`selfware` and `llama-server`) so
/// measurement completes in `< 1 ms`.
pub fn measure_memory_snapshot() -> BootMemorySnapshot {
    use sysinfo::{
        MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System,
    };

    let mut sys = System::new_with_specifics(
        RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
    );
    let system_total_mb = sys.total_memory() / (1024 * 1024);
    let avail_bytes = sys.available_memory();
    let system_available_mb = if avail_bytes > 0 {
        avail_bytes / (1024 * 1024)
    } else {
        system_total_mb
    };

    let self_pid = sysinfo::get_current_pid().ok();
    let child_info = background_server_slot()
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|g| (g.child.id(), g.ctx_size)));

    let mut pids_to_refresh = Vec::with_capacity(2);
    if let Some(pid) = self_pid {
        pids_to_refresh.push(pid);
    }
    if let Some((cid, _)) = child_info {
        pids_to_refresh.push(Pid::from_u32(cid));
    }
    if !pids_to_refresh.is_empty() {
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids_to_refresh),
            true,
            ProcessRefreshKind::nothing().with_memory(),
        );
    }

    let selfware_rss_mb = self_pid
        .and_then(|pid| sys.process(pid))
        .map(|p| p.memory() / (1024 * 1024))
        .unwrap_or(0);

    let (llama_server_pid, llama_server_rss_mb, llama_server_ctx) =
        if let Some((cid, ctx)) = child_info {
            let rss_mb = sys
                .process(Pid::from_u32(cid))
                .map(|p| p.memory() / (1024 * 1024));
            (Some(cid), rss_mb, Some(ctx))
        } else {
            (None, None, None)
        };

    let combined_rss_mb = selfware_rss_mb + llama_server_rss_mb.unwrap_or(0);

    BootMemorySnapshot {
        selfware_rss_mb,
        llama_server_pid,
        llama_server_rss_mb,
        llama_server_ctx,
        combined_rss_mb,
        system_available_mb,
        system_total_mb,
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
    build_server_args_with_context(model, alias, port, 8192)
}

/// argv for `llama-server` with an explicit model alias and context window
/// (`8192`, `65536` / 64k, `131072` / 128k, or `262144` / 256k):
/// - `-np 1 --no-repack` avoids allocating 4 parallel slots or a duplicate
///   614 MiB `CPU_REPACK` tensor buffer, keeping RSS low across macOS, Linux,
///   and Windows.
/// - Context windows `<= 131072` (up to 128k) use `q8_0` KV-cache quantization
///   (`-ctk q8_0 -ctv q8_0`, measured 1.49 GiB RSS at 64k and 1.89 GiB RSS at 128k).
/// - Context windows `> 131072` (e.g. 256k = `262144`) use `q4_0` KV-cache
///   quantization (`-ctk q4_0 -ctv q4_0`, measured 1.95 GiB RSS at 256k) so
///   even a full 256k context window fits inside a 2 GiB RAM budget.
pub fn build_server_args_with_context(
    model: &Path,
    alias: &str,
    port: u16,
    ctx_size: usize,
) -> Vec<String> {
    let effective_ctx = ctx_size.max(2048);
    let kv_type = if effective_ctx > LFM3_CTX_128K {
        "q4_0"
    } else {
        "q8_0"
    };
    vec![
        "-m".into(),
        model.to_string_lossy().into_owned(),
        "--alias".into(),
        alias.into(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "-np".into(),
        "1".into(),
        "--no-repack".into(),
        "-fa".into(),
        "on".into(),
        "-c".into(),
        effective_ctx.to_string(),
        "-ctk".into(),
        kv_type.into(),
        "-ctv".into(),
        kv_type.into(),
        "-b".into(),
        "256".into(),
        "-ub".into(),
        "128".into(),
        "--jinja".into(),
        "--reasoning".into(),
        "off".into(),
        "--reasoning-budget".into(),
        "0".into(),
    ]
}

/// Default port range (`8787..=8797`) for the local bundled `llama-server`.
/// If port `8787` is occupied by an unrelated local service (e.g. a dev web
/// server), `selfware` transparently falls back to the next available loopback
/// port in this range.
pub const BOOT_PORT_RANGE: std::ops::RangeInclusive<u16> = 8787..=8797;

/// Whether `endpoint` targets the local bundled boot-assistant server port range (`8787..=8797`).
pub fn is_boot_server_endpoint(endpoint: &str) -> bool {
    let trimmed = endpoint.trim().trim_end_matches('/');
    let without_v1 = trimmed.strip_suffix("/v1").unwrap_or(trimmed);
    let Some(port_str) = without_v1
        .strip_prefix("http://127.0.0.1:")
        .or_else(|| without_v1.strip_prefix("http://localhost:"))
    else {
        return false;
    };
    port_str
        .parse::<u16>()
        .is_ok_and(|port| BOOT_PORT_RANGE.contains(&port))
}

/// Verify that a `/health` response body actually came from `llama-server`
/// (`{"status":"ok"}`) rather than an unrelated HTTP server (e.g. a local SPA
/// returning `200 OK` with `<!doctype html>`).
pub(crate) fn is_llama_health_json(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("status")
                .and_then(|s| s.as_str())
                .map(|s| s.eq_ignore_ascii_case("ok"))
        })
        .unwrap_or(false)
}

/// Find the first port in [`BOOT_PORT_RANGE`] (`8787..=8797`) that can be bound
/// on `127.0.0.1`.
pub fn find_available_boot_port() -> Option<u16> {
    BOOT_PORT_RANGE
        .into_iter()
        .find(|&port| std::net::TcpListener::bind(("127.0.0.1", port)).is_ok())
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
    let Ok(resp) = client.get(&health).send().await else {
        return false;
    };
    if !resp.status().is_success() {
        return false;
    }
    resp.text()
        .await
        .map(|body| is_llama_health_json(&body))
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
    ensure_background_boot_server_for_model_with_context(model_id, None).await
}

/// Ensure the bundled/boot `llama-server` is running for the requested
/// `model_id` and `ctx_size` (`65536` / 64k, `131072` / 128k, `262144` / 256k).
/// If a background server owned by this process is already running with a
/// different alias or context size, it is cleanly restarted with the requested
/// parameters. If port `8787` is occupied by an unrelated process, automatically
/// selects the next free port in [`BOOT_PORT_RANGE`] (`8787..=8797`).
pub async fn ensure_background_boot_server_for_model_with_context(
    model_id: Option<&str>,
    ctx_size: Option<usize>,
) -> Result<String> {
    let target_ctx = resolve_boot_model_context(model_id, ctx_size);
    let requested_alias = model_id.map(str::trim).filter(|s| !s.is_empty());

    let existing_guard_info = background_server_slot()
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|g| (g.port, g.alias.clone(), g.ctx_size)));

    if let Some((guard_port, guard_alias, guard_ctx)) = existing_guard_info {
        if is_boot_server_healthy(guard_port).await {
            let alias_mismatch =
                requested_alias.is_some_and(|req| !guard_alias.eq_ignore_ascii_case(req));
            let ctx_mismatch = guard_ctx != target_ctx;
            if !alias_mismatch && !ctx_mismatch {
                return Ok(format!("http://127.0.0.1:{}/v1", guard_port));
            }
        }
        shutdown_background_boot_server();
    } else if requested_alias.is_none()
        && ctx_size.is_none()
        && is_boot_server_healthy(SERVER_PORT).await
    {
        return Ok(format!("http://127.0.0.1:{}/v1", SERVER_PORT));
    }

    let server_bin = ensure_llama_server(&boot_dir()).await?;
    let (model_path, spec_id) = if let Some(spec) = model_id.and_then(find_boot_model_spec) {
        (ensure_model_spec(&boot_dir(), spec).await?, spec.id)
    } else if let Some(bundled) = find_bundled_model() {
        let id = verify_any_boot_model_file(&bundled)
            .ok()
            .flatten()
            .map(|s| s.id)
            .unwrap_or(SERVER_ALIAS);
        (bundled, id)
    } else {
        let spec = shipped_boot_model_spec();
        (ensure_model_spec(&boot_dir(), spec).await?, spec.id)
    };
    let alias = requested_alias.unwrap_or(spec_id);
    let effective_ctx = resolve_boot_model_context(Some(alias), ctx_size);
    let target_port = find_available_boot_port().unwrap_or(SERVER_PORT);
    let guard =
        spawn_server_with_context(&server_bin, &model_path, alias, target_port, effective_ctx)
            .await?;
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
    pub alias: String,
    pub ctx_size: usize,
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
    let ctx_size = resolve_boot_model_context(Some(alias), None);
    spawn_server_with_context(binary, model, alias, port, ctx_size).await
}

/// Spawn llama-server with an explicit model alias and context window (`64k`, `128k`, `256k`)
/// and wait until `/health` answers.
pub async fn spawn_server_with_context(
    binary: &Path,
    model: &Path,
    alias: &str,
    port: u16,
    ctx_size: usize,
) -> Result<LlamaServerGuard> {
    use crate::safety::process_env::SanitizedEnvExt;

    let actual_port = if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
        port
    } else {
        find_available_boot_port().unwrap_or(port)
    };

    let mut child = Command::new(binary)
        .sanitized_env()
        .args(build_server_args_with_context(
            model,
            alias,
            actual_port,
            ctx_size,
        ))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawning {}", binary.display()))?;

    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()?;
    let health = format!("http://127.0.0.1:{}/health", actual_port);
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
                if let Ok(body) = resp.text().await {
                    if is_llama_health_json(&body) {
                        return Ok(LlamaServerGuard {
                            child,
                            port: actual_port,
                            alias: alias.to_string(),
                            ctx_size,
                        });
                    }
                }
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
