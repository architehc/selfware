use super::Tool;
use crate::config::SafetyConfig;
use crate::errors::ToolError;
use crate::safety::path_validator::PathValidator;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};
#[cfg(not(unix))]
use tempfile::NamedTempFile;

/// Global safety configuration set at startup from the user-loaded config.
///
/// `validate_tool_path` no longer reads this directly — `resolve_safety_config()`
/// does. The global exists only as the last-resort fallback when a tool lacks
/// a per-instance config. New tools should always use `with_safety_config()`.
pub(super) static SAFETY_CONFIG: OnceLock<RwLock<SafetyConfig>> = OnceLock::new();

/// Register the runtime-loaded safety configuration for tool path validation.
///
/// This should be called once during agent initialization (before any file tools
/// execute) so that `validate_tool_path` honours user settings.
///
/// DEPRECATED: For multi-agent scenarios where each agent needs a different
/// safety config, use the per-instance `with_safety_config()` constructor on
/// each tool struct instead. This global initializer will be removed once all
/// call sites are migrated to per-instance configs.
pub fn init_safety_config(config: &SafetyConfig) {
    let lock = SAFETY_CONFIG.get_or_init(|| RwLock::new(config.clone()));
    if let Ok(mut guard) = lock.write() {
        *guard = config.clone();
    }
}

/// Reset the process-global safety config to the default for tests.
///
/// This prevents tests that run after agent-initialization tests from
/// inheriting a permissive config left behind in `SAFETY_CONFIG`.
#[cfg(test)]
pub(crate) fn reset_safety_config_for_tests() {
    let lock = SAFETY_CONFIG.get_or_init(|| RwLock::new(SafetyConfig::default()));
    if let Ok(mut guard) = lock.write() {
        *guard = SafetyConfig::default();
    }
}

/// Maximum file size for reads (50 MB) to prevent OOM from accidentally reading huge files.
const MAX_READ_SIZE: u64 = 50 * 1024 * 1024;
/// Maximum file size for writes (10 MB) to prevent accidentally writing huge files.
const MAX_WRITE_SIZE: usize = 10 * 1024 * 1024;

// ---------------------------------------------------------------------------
// File snapshot tracking for stale-guard protection
// ---------------------------------------------------------------------------

/// Snapshot of a file at the time it was last read by `file_read`.
#[derive(Debug, Clone)]
struct FileSnapshot {
    content_hash: u64,
    last_modified: u64,
}

static FILE_SNAPSHOTS: OnceLock<Mutex<HashMap<String, FileSnapshot>>> = OnceLock::new();

fn get_snapshots() -> &'static Mutex<HashMap<String, FileSnapshot>> {
    FILE_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record a snapshot of a file after reading it.
pub(crate) fn record_file_snapshot(path: &str, content: &str) {
    let last_modified = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let content_hash = hasher.finish();

    if let Ok(mut guard) = get_snapshots().lock() {
        guard.insert(
            path.to_string(),
            FileSnapshot {
                content_hash,
                last_modified,
            },
        );
    }
}

/// Remove a file snapshot (e.g. after deletion).
pub(crate) fn clear_file_snapshot(path: &str) {
    if let Ok(mut guard) = get_snapshots().lock() {
        guard.remove(path);
    }
}

/// Check whether a file on disk has changed since the last recorded snapshot.
/// Returns `Some(true)` if stale, `Some(false)` if unchanged, `None` if no snapshot exists.
pub(crate) fn is_file_stale(path: &str) -> Option<bool> {
    let guard = get_snapshots().lock().ok()?;
    let snapshot = guard.get(path)?;

    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        // A FIFO/device swapped in after the read would hang or stream
        // forever below; a changed file type is a change.
        return Some(true);
    }
    let current_mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())?;

    if snapshot.last_modified != current_mtime {
        return Some(true);
    }

    // Secondary hash check for same-mtime modifications
    let current_bytes = std::fs::read(path).ok()?;
    let current_text = String::from_utf8_lossy(&current_bytes);
    let mut hasher = DefaultHasher::new();
    current_text.hash(&mut hasher);
    let current_hash = hasher.finish();

    Some(snapshot.content_hash != current_hash)
}

/// Read a file as raw bytes and convert to String, handling non-UTF8 gracefully.
///
/// Path-based: kept for callers outside this module that have not moved to
/// descriptor-checked reads. File tools here use [`read_file_checked`].
pub(crate) async fn read_file_with_encoding(path: &Path) -> Result<(String, Vec<u8>)> {
    let bytes = tokio::fs::read(path).await?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok((text, bytes))
}

/// The validator file tools use: the tool's resolved config anchored at the
/// process working directory (the same anchor `validate_tool_path` uses).
fn tool_path_validator(config: &SafetyConfig) -> PathValidator {
    let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
    PathValidator::new(config, working_dir)
}

/// Open `path` as a regular file whose DESCRIPTOR passed path policy
/// (see [`PathValidator::open_regular_file`]). All reads must go through the
/// returned handle — re-opening by path would reintroduce the
/// validate-then-reopen race. FIFOs, sockets, devices and directories are
/// refused without blocking.
pub(crate) fn open_checked_regular(path: &str, config: &SafetyConfig) -> Result<std::fs::File> {
    tool_path_validator(config)
        .open_regular_file(path)
        .map(|v| v.file)
        .map_err(|e| anyhow::anyhow!(e))
}

/// Read all bytes from an already-checked handle, refusing anything larger
/// than `limit` (checked against the descriptor's own size, not the path's).
fn read_checked_handle(mut file: std::fs::File, limit: Option<u64>) -> Result<(String, Vec<u8>)> {
    use std::io::Read;
    if let Some(limit) = limit {
        let size = file.metadata()?.len();
        if size > limit {
            return Err(ToolError::FileTooLarge { size, limit }.into());
        }
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok((text, bytes))
}

/// Descriptor-checked counterpart of [`read_file_with_encoding`]: validate,
/// open once, re-validate the descriptor's real path, read from that same
/// descriptor.
pub(crate) async fn read_file_checked(
    path: &str,
    config: &SafetyConfig,
    limit: Option<u64>,
) -> Result<(String, Vec<u8>)> {
    let path = path.to_string();
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        let file = open_checked_regular(&path, &config)?;
        read_checked_handle(file, limit)
    })
    .await?
}

/// Detect the line ending style of existing content.
pub(crate) fn detect_line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Normalize content to use the given line ending style.
pub(crate) fn preserve_line_endings(content: &str, line_ending: &str) -> String {
    let normalized = content.replace("\r\n", "\n");
    if line_ending == "\r\n" {
        normalized.replace('\n', "\r\n")
    } else {
        normalized
    }
}

/// Read file contents. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`FileRead::with_safety_config`].
#[derive(Default)]
pub struct FileRead {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    /// When `None`, falls back to the global or default config (backward compatible).
    pub safety_config: Option<SafetyConfig>,
}

/// Write or overwrite entire file. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`FileWrite::with_safety_config`].
#[derive(Default)]
pub struct FileWrite {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    pub safety_config: Option<SafetyConfig>,
}

/// Apply surgical edit to file. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`FileEdit::with_safety_config`].
#[derive(Default)]
pub struct FileEdit {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    pub safety_config: Option<SafetyConfig>,
}

/// Delete a file. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`FileDelete::with_safety_config`].
#[derive(Default)]
pub struct FileDelete {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    pub safety_config: Option<SafetyConfig>,
}

/// Apply multiple surgical edits atomically. Supports optional per-instance
/// safety configuration via [`FileMultiEdit::with_safety_config`].
#[derive(Default)]
pub struct FileMultiEdit {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    pub safety_config: Option<SafetyConfig>,
}

/// List directory structure. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`DirectoryTree::with_safety_config`].
#[derive(Default)]
pub struct DirectoryTree {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    pub safety_config: Option<SafetyConfig>,
}

// ---------------------------------------------------------------------------
// Constructors for dependency-injected safety configuration.
//
// Each file tool can be created with either:
// - `Tool::new()` / `Tool::default()` -- no per-instance config; uses the global or default
// - `Tool::with_safety_config(config)` -- uses the given config, ignoring the global
// ---------------------------------------------------------------------------

impl FileRead {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl FileWrite {
    pub fn new() -> Self {
        Self {
            safety_config: None,
        }
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl FileEdit {
    pub fn new() -> Self {
        Self {
            safety_config: None,
        }
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl FileDelete {
    pub fn new() -> Self {
        Self {
            safety_config: None,
        }
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl FileMultiEdit {
    pub fn new() -> Self {
        Self {
            safety_config: None,
        }
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl DirectoryTree {
    pub fn new() -> Self {
        Self {
            safety_config: None,
        }
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

#[async_trait]
impl Tool for FileRead {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read file contents. Use for examining code, configs, or any text file."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the file"
                },
                "line_range": {
                    "type": "array",
                    "items": {"type": "integer"},
                    "minItems": 2,
                    "maxItems": 2,
                    "description": "Optional [start, end] line range (1-indexed, inclusive)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            /// `path` is the canonical field; aliases absorb the variations
            /// models commonly emit for file paths.
            #[serde(alias = "file_path", alias = "file", alias = "filepath")]
            path: String,
            line_range: Option<(usize, usize)>,
        }

        let args: Args = serde_json::from_value(args)?;
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.path, &safety)?;

        // A line_range lets us stream ONLY the requested slice, so a large file
        // can be sliced without loading it whole or tripping the size limit.
        if let Some((start, end)) = args.line_range {
            let file = {
                let p = args.path.clone();
                let cfg = safety.clone();
                tokio::task::spawn_blocking(move || open_checked_regular(&p, &cfg)).await??
            };
            let (selected_content, lines_scanned, lossy, reached_eof) =
                read_line_slice(file, start, end).await?;
            let lines_returned = selected_content.lines().count();
            if reached_eof {
                // The scan consumed the whole file, so lines_scanned is the
                // true total line count — safe to report honestly.
                return Ok(serde_json::json!({
                    "content": selected_content,
                    "lines_returned": lines_returned,
                    "total_lines": lines_scanned,
                    "truncated": false,
                    "encoding": if lossy { "utf-8-lossy" } else { "utf-8" },
                    "valid_utf8": !lossy
                }));
            } else {
                // The slice ended before EOF — we do NOT know the true total.
                // Report has_more instead of a misleading total_lines.
                return Ok(serde_json::json!({
                    "content": selected_content,
                    "lines_returned": lines_returned,
                    "total_lines": null,
                    "has_more": true,
                    "truncated": true,
                    "encoding": if lossy { "utf-8-lossy" } else { "utf-8" },
                    "valid_utf8": !lossy
                }));
            }
        }

        // Whole-file read: guard against OOM on huge files (checked against
        // the opened descriptor's size inside read_file_checked).
        let (content, bytes) = read_file_checked(&args.path, &safety, Some(MAX_READ_SIZE)).await?;
        let valid_utf8 = std::str::from_utf8(&bytes).is_ok();

        // Record snapshot for stale-guard detection
        record_file_snapshot(&args.path, &content);

        let total_lines = content.lines().count();

        Ok(serde_json::json!({
            "content": content,
            "total_lines": total_lines,
            "truncated": false,
            "encoding": if valid_utf8 { "utf-8" } else { "utf-8-lossy" },
            "valid_utf8": valid_utf8
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

#[async_trait]
impl Tool for FileWrite {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write or overwrite entire file. Creates parent directories if needed."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(alias = "file_path", alias = "file", alias = "filepath")]
            path: String,
            #[serde(alias = "text", alias = "body")]
            content: String,
            /// Accepted for compatibility with callers that still send it,
            /// and ignored: on-disk `<file>.bak` siblings littered users'
            /// workspaces (src/lib.rs.bak, README.md.bak). Undo comes from
            /// the in-memory edit history; crash safety comes from the
            /// atomic temp+rename write.
            #[serde(default)]
            #[allow(dead_code)]
            backup: Option<bool>,
        }

        let args: Args = serde_json::from_value(args)?;
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.path, &safety)?;
        let path = PathBuf::from(&args.path);

        // Check write size limit to prevent accidentally writing huge files
        if args.content.len() > MAX_WRITE_SIZE {
            return Err(ToolError::WriteTooLarge {
                size: args.content.len(),
                limit: MAX_WRITE_SIZE,
            }
            .into());
        }

        // Stale-guard: reject if file changed since last read
        if path.exists() {
            if let Some(true) = is_file_stale(&args.path) {
                return Err(ToolError::FileStale {
                    path: args.path.clone(),
                }
                .into());
            }
        }

        // Detect existing line endings and preserve them. `exists()` follows
        // symlinks and returns false for a missing target; anything that
        // does exist is read through the descriptor-checked path, which
        // also refuses FIFOs/devices/directories instead of blocking on them.
        let existing = if path.exists() {
            Some(read_file_checked(&args.path, &safety, None).await?)
        } else {
            None
        };
        let content_to_write = if let Some((existing_text, existing_bytes)) = &existing {
            // Overwriting is a full replace, but refuse to touch a non-UTF-8
            // file: the caller likely believes it is text, and the lossy read
            // above hides what is actually on disk.
            ensure_valid_utf8(existing_bytes, &args.path, "file_write")?;
            let line_ending = detect_line_ending(existing_text);
            let content_to_write = preserve_line_endings(&args.content, line_ending);
            // Detect no-op writes (content identical to existing file).
            // Valid UTF-8 was just proven, so the lossy text is exact.
            if *existing_text == content_to_write {
                return Err(ToolError::EditNoOp.into());
            }
            content_to_write
        } else {
            args.content.clone()
        };

        validate_rust_source_if_needed(&path, &content_to_write)?;

        write_atomic_checked(&path, &content_to_write, &safety).await?;
        clear_file_snapshot(&args.path);

        Ok(serde_json::json!({
            "success": true,
            "bytes_written": content_to_write.len(),
            "path": args.path
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::file_write()
    }
}

#[async_trait]
impl Tool for FileEdit {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn description(&self) -> &str {
        "Apply surgical edit to file. The old_str must match EXACTLY once. Include enough context to ensure unique match."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "old_str": {"type": "string", "description": "Exact string to find (must be unique)"},
                "new_str": {"type": "string", "description": "Replacement string (empty to delete)"}
            },
            "required": ["path", "old_str", "new_str"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(alias = "file_path", alias = "file", alias = "filepath")]
            path: String,
            /// `old_string` / `new_string` are accepted as aliases: several
            /// system-injected file_edit templates historically showed those
            /// names, and models mirroring them failed with
            /// "missing field 'old_str'". Canonical is old_str/new_str.
            #[serde(alias = "old_string")]
            old_str: String,
            #[serde(alias = "new_string")]
            new_str: String,
        }

        let args: Args = serde_json::from_value(args)?;
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.path, &safety)?;

        // Stale-guard: reject if file changed since last read
        if let Some(true) = is_file_stale(&args.path) {
            return Err(ToolError::FileStale {
                path: args.path.clone(),
            }
            .into());
        }

        let (content, original_bytes) = read_file_checked(&args.path, &safety, None).await?;
        ensure_valid_utf8(&original_bytes, &args.path, "file_edit")?;
        let line_ending = detect_line_ending(&content);

        // Check for exactly one match
        let matches = content.matches(&args.old_str).count();
        if matches == 0 {
            return Err(ToolError::EditStringNotFound.into());
        }
        if matches > 1 {
            return Err(ToolError::EditStringMultiple { count: matches }.into());
        }
        if args.old_str == args.new_str {
            return Err(ToolError::EditNoOp.into());
        }
        if args.new_str.contains(&args.old_str) && content.contains(&args.new_str) {
            bail!(
                "file_edit duplicate insertion rejected: the requested replacement block is already present in {}. Re-read the file and make a different targeted edit.",
                args.path
            );
        }

        // Catastrophic whole-file replacement guard. Replacing the vast majority
        // of a file is almost always an accidental loss of context; prefer
        // smaller, targeted edits. Use file_write if a full rewrite is intended.
        if !content.is_empty() {
            let ratio = args.old_str.len() as f64 / content.len() as f64;
            if ratio > 0.85 {
                bail!(
                    "file_edit rejected: old_str matches {:.0}% of {}. \
                     Use a smaller, targeted edit with surrounding context, \
                     or use file_write if you truly intend to replace the entire file.",
                    ratio * 100.0,
                    args.path
                );
            }
        }

        let new_content = content.replace(&args.old_str, &args.new_str);
        let new_content = preserve_line_endings(&new_content, line_ending);
        validate_rust_source_if_needed(Path::new(&args.path), &new_content)?;
        write_atomic_checked(Path::new(&args.path), &new_content, &safety).await?;
        clear_file_snapshot(&args.path);

        Ok(serde_json::json!({
            "success": true,
            "matches_found": 1,
            "path": args.path
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::file_write()
    }
}

#[async_trait]
impl Tool for FileDelete {
    fn name(&self) -> &str {
        "file_delete"
    }

    fn description(&self) -> &str {
        "Delete a file. Use with caution -- this is irreversible without version control."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the file to delete"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(alias = "file_path", alias = "file", alias = "filepath")]
            path: String,
        }

        let args: Args = serde_json::from_value(args)?;
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.path, &safety)?;
        let path = PathBuf::from(&args.path);

        if !path.exists() {
            clear_file_snapshot(&args.path);
            return Ok(serde_json::json!({
                "deleted": true,
                "already_absent": true,
                "path": args.path,
                "message": format!("File already absent: {}", args.path)
            }));
        }
        if path.is_dir() {
            return Err(ToolError::PathIsDirectory {
                path: args.path.clone(),
            }
            .into());
        }

        // Stale-guard: reject if file changed since last read
        if let Some(true) = is_file_stale(&args.path) {
            return Err(ToolError::FileStale {
                path: args.path.clone(),
            }
            .into());
        }

        remove_file_checked(&args.path, &safety)
            .await
            .with_context(|| format!("Failed to delete file: {}", args.path))?;

        clear_file_snapshot(&args.path);

        Ok(serde_json::json!({
            "deleted": true,
            "path": args.path
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::file_destructive()
    }
}

#[async_trait]
impl Tool for FileMultiEdit {
    fn name(&self) -> &str {
        "file_multi_edit"
    }

    fn description(&self) -> &str {
        "Apply multiple surgical edits atomically. If any edit fails validation, NONE are applied."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "edits": {
                    "type": "array",
                    "description": "Ordered list of edits to apply",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "old_str": {"type": "string", "description": "Exact string to find (must be unique)"},
                            "new_str": {"type": "string", "description": "Replacement string"}
                        },
                        "required": ["path", "old_str", "new_str"]
                    }
                }
            },
            "required": ["edits"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct EditItem {
            #[serde(alias = "file_path", alias = "file", alias = "filepath")]
            path: String,
            #[serde(alias = "old_string")]
            old_str: String,
            #[serde(alias = "new_string")]
            new_str: String,
        }

        #[derive(Deserialize)]
        struct Args {
            edits: Vec<EditItem>,
        }

        let args: Args = serde_json::from_value(args)?;
        let safety = resolve_safety_config(self.safety_config.as_ref());

        if args.edits.is_empty() {
            return Err(ToolError::InvalidToolCall {
                name: "file_multi_edit".to_string(),
                message: "No edits provided".to_string(),
            }
            .into());
        }

        // Validate paths and stale-guard first
        for edit in &args.edits {
            validate_tool_path(&edit.path, &safety)?;
            if let Some(true) = is_file_stale(&edit.path) {
                return Err(ToolError::FileStale {
                    path: edit.path.clone(),
                }
                .into());
            }
        }

        // Group edits by file path
        let mut edits_by_file: HashMap<String, Vec<(usize, &EditItem)>> = HashMap::new();
        for (idx, edit) in args.edits.iter().enumerate() {
            edits_by_file
                .entry(edit.path.clone())
                .or_default()
                .push((idx, edit));
        }

        // Phase 1: read all files and validate edits (find line ranges, check overlaps, unique matches)
        let mut file_contents: HashMap<String, String> = HashMap::new();
        let mut file_line_endings: HashMap<String, &'static str> = HashMap::new();

        for (path, edits) in &edits_by_file {
            let (content, original_bytes) = read_file_checked(path, &safety, None).await?;
            ensure_valid_utf8(&original_bytes, path, "file_multi_edit")?;
            file_line_endings.insert(path.clone(), detect_line_ending(&content));

            // Validate each edit: exactly one match
            for (idx, edit) in edits {
                let matches = content.matches(&edit.old_str).count();
                if matches == 0 {
                    return Err(ToolError::Execution {
                        name: "file_multi_edit".to_string(),
                        message: format!("Edit {}: old_str not found in {}", idx, edit.path),
                    }
                    .into());
                }
                if matches > 1 {
                    return Err(ToolError::Execution {
                        name: "file_multi_edit".to_string(),
                        message: format!(
                            "Edit {}: old_str matches {} times in {} (expected exactly 1)",
                            idx, matches, edit.path
                        ),
                    }
                    .into());
                }
                if edit.old_str == edit.new_str {
                    return Err(ToolError::Execution {
                        name: "file_multi_edit".to_string(),
                        message: format!(
                            "Edit {}: old_str and new_str are identical in {} — no-op edit",
                            idx, edit.path
                        ),
                    }
                    .into());
                }
            }

            // Check for overlapping edits in the same file
            if edits.len() > 1 {
                let mut ranges = Vec::new();
                for (_idx, edit) in edits {
                    let byte_pos =
                        content
                            .find(&edit.old_str)
                            .ok_or_else(|| ToolError::Execution {
                                name: "file_multi_edit".to_string(),
                                message: format!("old_str not found in {}", edit.path),
                            })?;
                    let before = &content[..byte_pos];
                    let start_line = before.lines().count() + 1;
                    let end_line = start_line + edit.old_str.lines().count().saturating_sub(1);
                    ranges.push((start_line, end_line, edit));
                }

                for i in 0..ranges.len() {
                    for j in (i + 1)..ranges.len() {
                        let (s1, e1, edit1) = &ranges[i];
                        let (s2, e2, _edit2) = &ranges[j];
                        if s1 <= e2 && s2 <= e1 {
                            return Err(ToolError::Execution {
                                name: "file_multi_edit".to_string(),
                                message: format!(
                                    "Edits overlap in {}: lines {}-{} and {}-{}",
                                    edit1.path, s1, e1, s2, e2
                                ),
                            }
                            .into());
                        }
                    }
                }
            }

            file_contents.insert(path.clone(), content);
        }

        // Phase 2: compute every file's final content up front (including Rust
        // syntax validation), THEN write them all in one batch. If any write
        // fails, `write_all_atomic` rolls back the files already persisted from
        // their pre-images — a failed batch never leaves 1..N-1 files modified.
        // Paths are processed in sorted order so the batch is deterministic
        // (HashMap iteration order is not).
        let mut sorted_paths: Vec<&String> = edits_by_file.keys().collect();
        sorted_paths.sort();

        let mut finals: Vec<(PathBuf, String)> = Vec::with_capacity(sorted_paths.len());
        for path in sorted_paths {
            let edits = &edits_by_file[path];
            let content = file_contents.get_mut(path).unwrap();
            let line_ending = file_line_endings.get(path).copied().unwrap_or("\n");

            // Build (byte_pos, old_len, new_str) for each edit
            let mut replacements: Vec<(usize, usize, String)> = Vec::new();
            for (_idx, edit) in edits {
                let pos = content.find(&edit.old_str).unwrap();
                replacements.push((pos, edit.old_str.len(), edit.new_str.clone()));
            }

            // Sort by position descending so earlier replacements don't shift later ones
            replacements.sort_by_key(|r| r.0);
            replacements.reverse();

            for (pos, len, new_str) in replacements {
                content.replace_range(pos..pos + len, &new_str);
            }

            let final_content = preserve_line_endings(content, line_ending);
            validate_rust_source_if_needed(Path::new(path), &final_content)?;
            finals.push((PathBuf::from(path), final_content));
        }

        write_all_atomic_checked(&finals, &safety).await?;
        for (path, _) in &finals {
            clear_file_snapshot(&path.to_string_lossy());
        }

        let files_changed: Vec<String> = finals
            .iter()
            .map(|(path, _)| path.to_string_lossy().into_owned())
            .collect();
        Ok(serde_json::json!({
            "success": true,
            "edits_applied": args.edits.len(),
            "files_changed": files_changed.len(),
            "files": files_changed
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::file_write()
    }
}

/// Nested node returned by the `directory_tree` tool.
#[derive(Serialize)]
struct TreeNode {
    name: String,
    #[serde(rename = "type")]
    type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    children: Vec<TreeNode>,
}

/// Insert a walked entry into the nested tree based on its relative path.
fn insert_tree_entry(root: &mut TreeNode, relative: &Path, type_: &str, size: u64) {
    let mut components: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    if components.is_empty() {
        return;
    }
    let file_name = components.pop().unwrap();

    let mut current = root;
    for component in components {
        let child_idx = current
            .children
            .iter()
            .position(|c| c.name == component && c.type_ == "directory");
        let idx = match child_idx {
            Some(idx) => idx,
            None => {
                current.children.push(TreeNode {
                    name: component,
                    type_: "directory".to_string(),
                    size: None,
                    children: Vec::new(),
                });
                current.children.len() - 1
            }
        };
        current = &mut current.children[idx];
    }

    // If this entry is a directory, it may already exist as a parent placeholder
    // from a previously-inserted child. Reuse that node and just mark it.
    if type_ == "directory" {
        if let Some(existing) = current
            .children
            .iter_mut()
            .find(|c| c.name == file_name && c.type_ == "directory")
        {
            existing.size = Some(size);
            return;
        }
    }

    current.children.push(TreeNode {
        name: file_name,
        type_: type_.to_string(),
        size: Some(size),
        children: Vec::new(),
    });
}

/// Recursively sort a tree node: directories first, then files alphabetically.
fn sort_tree_node(node: &mut TreeNode) {
    node.children.sort_by(|a, b| {
        let a_dir = a.type_ == "directory";
        let b_dir = b.type_ == "directory";
        match (a_dir, b_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });
    for child in &mut node.children {
        sort_tree_node(child);
    }
}

/// Count all nodes in the tree (including the root).
fn count_tree_nodes(node: &TreeNode) -> usize {
    1 + node.children.iter().map(count_tree_nodes).sum::<usize>()
}

#[async_trait]
impl Tool for DirectoryTree {
    fn name(&self) -> &str {
        "directory_tree"
    }

    fn description(&self) -> &str {
        "Return a nested directory tree. Use to understand project layout and parent/child relationships."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "max_depth": {"type": "integer", "default": 3},
                "include_hidden": {"type": "boolean", "default": false}
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            #[serde(alias = "file_path", alias = "file", alias = "filepath")]
            path: String,
            #[serde(default = "default_three")]
            max_depth: usize,
            #[serde(default)]
            include_hidden: bool,
        }

        let args: Args = serde_json::from_value(args)?;
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.path, &safety)?;

        let walk_path = args.path.clone();
        let max_depth = args.max_depth;
        let include_hidden = args.include_hidden;

        let tree: TreeNode = tokio::task::spawn_blocking(move || {
            // Use filter_entry (not filter_map) so hidden directories are not descended into.
            // filter_map would skip the hidden entry from output but still walk its children.
            /// Directories to never descend into — build artifacts, caches, VCS internals.
            const SKIP_DIRS: &[&str] = &[
                "target",
                "node_modules",
                "dist",
                "build",
                "__pycache__",
                ".worktrees",
                "vendor",
                "pkg",
                "out",
                "cmake-build-debug",
            ];

            let walker = walkdir::WalkDir::new(&walk_path)
                .max_depth(max_depth)
                .into_iter()
                .filter_entry(|e| {
                    if include_hidden {
                        return true;
                    }
                    if e.depth() == 0 {
                        return true;
                    }
                    let name = e.file_name().to_str().unwrap_or("");
                    // Skip hidden entries and known-large directories
                    !name.starts_with('.') && !SKIP_DIRS.contains(&name)
                });

            #[derive(Serialize)]
            struct EntryInfo {
                path: PathBuf,
                type_: &'static str,
                size: u64,
            }

            let mut entries: Vec<EntryInfo> = Vec::new();
            for entry in walker.filter_map(|e| e.ok()) {
                let path = entry.path();
                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                entries.push(EntryInfo {
                    path: path.to_path_buf(),
                    type_: if metadata.is_dir() {
                        "directory"
                    } else {
                        "file"
                    },
                    size: metadata.len(),
                });
            }

            let root_name = Path::new(&walk_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| walk_path.clone());

            let mut root = TreeNode {
                name: root_name,
                type_: "directory".to_string(),
                size: None,
                children: Vec::new(),
            };

            let walk_path_buf = PathBuf::from(&walk_path);
            for entry in entries {
                let relative = match entry.path.strip_prefix(&walk_path_buf) {
                    Ok(r) if !r.as_os_str().is_empty() => r,
                    _ => continue,
                };
                insert_tree_entry(&mut root, relative, entry.type_, entry.size);
            }

            sort_tree_node(&mut root);
            root
        })
        .await?;

        let total = count_tree_nodes(&tree);

        Ok(serde_json::json!({
            "root": args.path,
            "tree": tree,
            "total": total
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

fn default_three() -> usize {
    3
}

/// Resolve which `SafetyConfig` to use for path validation.
///
/// Priority: per-instance config > process-global > default.
/// Call this at the tool level, then pass the result to `validate_tool_path`.
/// This keeps the global state lookup at the tool boundary rather than
/// buried inside the validation function.
pub(crate) fn resolve_safety_config(instance_config: Option<&SafetyConfig>) -> SafetyConfig {
    if let Some(cfg) = instance_config {
        return cfg.clone();
    }
    SAFETY_CONFIG
        .get()
        .and_then(|lock| lock.read().ok().map(|guard| guard.clone()))
        .unwrap_or_default()
}

/// Validate that a tool path is safe to access.
///
/// Takes a resolved `&SafetyConfig` — callers should use
/// `resolve_safety_config()` to pick the right config before calling this.
pub(crate) fn validate_tool_path(path: &str, config: &SafetyConfig) -> Result<()> {
    #[cfg(test)]
    {
        if std::env::var("SELFWARE_TEST_MODE").is_ok() {
            if !path.starts_with("tests/e2e-projects/") && !path.starts_with("/tmp/selfware-test-")
            {
                anyhow::bail!("Test mode only valid for test fixtures, got: {}", path);
            }
            return Ok(());
        }
    }
    let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
    PathValidator::new(config, working_dir)
        .validate(path)
        .map_err(|e| anyhow::anyhow!(e))
}

fn validate_rust_source_if_needed(path: &Path, content: &str) -> Result<()> {
    let is_rust_source = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"));
    if !is_rust_source {
        return Ok(());
    }

    syn::parse_file(content).map(|_| ()).map_err(|err| {
        ToolError::InvalidRustSyntax {
            path: path.display().to_string(),
            message: err.to_string(),
        }
        .into()
    })
}

/// Read only lines `start..=end` (1-based, inclusive) by streaming the file, so
/// a large file can be sliced without loading it all into memory. Bytes are
/// decoded as UTF-8 lossily (adequate for a line slice of source/text files).
/// Returns the joined slice, the number of lines scanned (== the true total
/// only when the scan reached EOF), and whether any returned line contained
/// invalid UTF-8 (i.e. the returned text is lossy rather than exact).
///
/// Takes the descriptor-checked handle from [`open_checked_regular`] — never
/// a path — so the slice is read from exactly the object that was validated.
async fn read_line_slice(
    file: std::fs::File,
    start: usize,
    end: usize,
) -> Result<(String, usize, bool, bool)> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    // Preserve the legacy contract that an inverted range (end < start) yields
    // the single line at `start`.
    let effective_end = end.max(start);
    let file = tokio::fs::File::from_std(file);
    let mut reader = BufReader::new(file);
    let mut selected: Vec<String> = Vec::new();
    let mut lineno = 0usize;
    let mut lossy = false;
    let mut reached_eof = false;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf).await?;
        if n == 0 {
            reached_eof = true;
            break;
        }
        lineno += 1;
        if lineno >= start && lineno <= effective_end {
            if std::str::from_utf8(&buf).is_err() {
                lossy = true;
            }
            let mut line = String::from_utf8_lossy(&buf).into_owned();
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            selected.push(line);
        }
        if lineno >= effective_end {
            break; // stop early — don't read the rest of a huge file
        }
    }
    Ok((selected.join("\n"), lineno, lossy, reached_eof))
}

/// Write content to a file atomically using a temporary file and rename.
///
/// The existing file's permission mode is carried over to the replacement —
/// the temp file is created `0600`, so without this an executable would
/// silently lose `+x` and a shared config would become owner-only on every
/// edit. New files keep the temp-file default.
///
/// No policy re-check: for callers outside this module that validated with
/// their own config. File tools use [`write_atomic_checked`].
pub(crate) async fn write_atomic(path: &Path, content: &str) -> Result<()> {
    write_all_atomic(&[(path.to_path_buf(), content.to_string())]).await
}

/// [`write_atomic`] with the pinned parent directory's real location
/// re-validated against `config` (see [`write_all_atomic_checked`]).
pub(crate) async fn write_atomic_checked(
    path: &Path,
    content: &str,
    config: &SafetyConfig,
) -> Result<()> {
    write_all_atomic_checked(&[(path.to_path_buf(), content.to_string())], config).await
}

/// Atomically write several files at once: stage every replacement in a temp
/// file first, then persist them all. If any persist fails, files already
/// persisted are rolled back from their captured pre-images so a multi-file
/// batch never commits half-way. Each replacement inherits the target's
/// existing permission mode (see [`write_atomic`]).
///
/// On Unix all I/O is fd-relative to the pinned parent directory (see
/// [`write_all_atomic_checked`]); without a config, the directory's location
/// is not re-validated.
pub(crate) async fn write_all_atomic(files: &[(PathBuf, String)]) -> Result<()> {
    let files_owned: Vec<(PathBuf, String)> = files.to_vec();
    tokio::task::spawn_blocking(move || write_all_blocking(&files_owned, None)).await?
}

/// [`write_all_atomic`] for file tools: closes the validate-then-write race.
///
/// On Unix each target's parent directory is opened ONCE as a descriptor
/// (missing directories created with `mkdirat` + `openat(O_NOFOLLOW)`), the
/// descriptor's real path is re-validated against `config`, and every
/// subsequent operation — pre-image read, temp create
/// (`openat(O_CREAT|O_EXCL|O_NOFOLLOW)`), `fsync`, `renameat`, rollback — is
/// relative to that same descriptor. An intermediate directory swapped for a
/// symlink after validation is refused (its real path is outside policy) or,
/// if swapped after the directory was pinned, cannot redirect the write.
/// Non-regular targets (FIFO, device, directory) are refused before anything
/// is touched. Non-Unix platforms keep the path-based behaviour.
pub(crate) async fn write_all_atomic_checked(
    files: &[(PathBuf, String)],
    config: &SafetyConfig,
) -> Result<()> {
    let files_owned: Vec<(PathBuf, String)> = files.to_vec();
    let config = config.clone();
    tokio::task::spawn_blocking(move || write_all_blocking(&files_owned, Some(&config))).await?
}

/// Delete a file through its pinned, re-validated parent directory
/// (`unlinkat`), so a directory swapped after validation cannot redirect the
/// unlink outside the workspace. Non-Unix: path-based `remove_file`.
async fn remove_file_checked(path: &str, config: &SafetyConfig) -> Result<()> {
    let path = path.to_string();
    let config = config.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            use std::os::unix::io::AsRawFd;
            let (dir, name) = tool_path_validator(&config)
                .open_parent_dir(&path, false)
                .map_err(|e| anyhow::anyhow!(e))?;
            let c_name = std::ffi::CString::new(name.as_bytes())?;
            // SAFETY: single NUL-terminated component, live directory fd.
            // Flags 0 = never removes a directory.
            if unsafe { libc::unlinkat(dir.as_raw_fd(), c_name.as_ptr(), 0) } != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = &config;
            std::fs::remove_file(&path)?;
            Ok(())
        }
    })
    .await?
}

#[cfg(all(test, unix))]
static FAIL_RENAME_FOR: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();

/// Test hook: make the commit-phase `renameat` for `path` fail, to exercise
/// the rollback path deterministically.
#[cfg(all(test, unix))]
pub(crate) fn inject_rename_failure_for_tests(path: &Path) {
    FAIL_RENAME_FOR
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .push(path.to_path_buf());
}

#[cfg(unix)]
fn rename_should_fail_for_tests(_path: &Path) -> bool {
    #[cfg(test)]
    {
        if let Some(lock) = FAIL_RENAME_FOR.get() {
            if let Ok(guard) = lock.lock() {
                return guard.iter().any(|p| p == _path);
            }
        }
    }
    false
}

/// What was at a target before the batch, for rollback.
#[cfg(unix)]
enum PreImage {
    Missing,
    Bytes(Vec<u8>),
    /// The target was a symlink (renameat replaces the link itself).
    Symlink(std::ffi::CString),
}

#[cfg(unix)]
struct PinnedTarget {
    dir: std::fs::File,
    name: std::ffi::CString,
    display: PathBuf,
    pre_image: PreImage,
    mode: Option<u32>,
}

#[cfg(unix)]
fn temp_name() -> std::ffi::CString {
    std::ffi::CString::new(format!(".sw-{}.tmp", uuid::Uuid::new_v4().simple()))
        .expect("generated name has no NUL")
}

/// Capture a target's pre-image relative to its pinned directory. The final
/// component is opened `O_NOFOLLOW|O_NONBLOCK`: a symlink is recorded as a
/// symlink (never read through), and a FIFO/device/directory is refused.
#[cfg(unix)]
fn capture_pre_image(
    dir: &std::fs::File,
    name: &std::ffi::CStr,
    display: &Path,
) -> Result<(PreImage, Option<u32>)> {
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::io::{AsRawFd, FromRawFd};
    // SAFETY: NUL-terminated single component, live directory descriptor.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::NotFound {
            return Ok((PreImage::Missing, None));
        }
        if err.raw_os_error() == Some(libc::ELOOP) {
            let mut buf = vec![0u8; libc::PATH_MAX as usize];
            // SAFETY: buffer length passed matches the allocation.
            let n = unsafe {
                libc::readlinkat(
                    dir.as_raw_fd(),
                    name.as_ptr(),
                    buf.as_mut_ptr() as *mut libc::c_char,
                    buf.len(),
                )
            };
            if n < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            buf.truncate(n as usize);
            // Carry the link target's mode over, as the path-based write did.
            let mode = std::fs::metadata(display)
                .ok()
                .map(|m| m.permissions().mode());
            return Ok((PreImage::Symlink(std::ffi::CString::new(buf)?), mode));
        }
        return Err(err.into());
    }
    // SAFETY: the new descriptor is owned exactly once by the File.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(anyhow::anyhow!(
            crate::safety::path_validator::not_regular_error(display, &meta)
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok((PreImage::Bytes(bytes), Some(meta.permissions().mode())))
}

/// Create `tmp` in the pinned directory (`O_CREAT|O_EXCL|O_NOFOLLOW`), write,
/// apply `mode`, fsync.
#[cfg(unix)]
fn stage_temp(
    dir: &std::fs::File,
    tmp: &std::ffi::CStr,
    bytes: &[u8],
    mode: Option<u32>,
) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::io::{AsRawFd, FromRawFd};
    // SAFETY: NUL-terminated single component, live directory descriptor.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            tmp.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the new descriptor is owned exactly once by the File.
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let result = (|| {
        file.write_all(bytes)?;
        if let Some(mode) = mode {
            file.set_permissions(std::fs::Permissions::from_mode(mode & 0o7777))?;
        }
        file.sync_all()
    })();
    if result.is_err() {
        unlink_at(dir, tmp);
    }
    result
}

#[cfg(unix)]
fn unlink_at(dir: &std::fs::File, name: &std::ffi::CStr) {
    use std::os::unix::io::AsRawFd;
    // SAFETY: NUL-terminated single component, live directory descriptor.
    unsafe {
        libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0);
    }
}

#[cfg(unix)]
fn rename_at(
    dir: &std::fs::File,
    from: &std::ffi::CStr,
    to: &std::ffi::CStr,
) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;
    // SAFETY: both names are single components resolved against the same
    // pinned directory; renameat replaces a destination symlink itself,
    // never its target.
    if unsafe { libc::renameat(dir.as_raw_fd(), from.as_ptr(), dir.as_raw_fd(), to.as_ptr()) } != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Best-effort restore of one target's pre-image, relative to its pinned dir.
#[cfg(unix)]
fn restore_pre_image(target: &PinnedTarget) {
    use std::os::unix::io::AsRawFd;
    match &target.pre_image {
        PreImage::Missing => unlink_at(&target.dir, &target.name),
        PreImage::Bytes(bytes) => {
            let tmp = temp_name();
            if stage_temp(&target.dir, &tmp, bytes, target.mode).is_ok()
                && rename_at(&target.dir, &tmp, &target.name).is_err()
            {
                unlink_at(&target.dir, &tmp);
            }
        }
        PreImage::Symlink(link) => {
            let tmp = temp_name();
            // SAFETY: NUL-terminated strings, live directory descriptor.
            let ok =
                unsafe { libc::symlinkat(link.as_ptr(), target.dir.as_raw_fd(), tmp.as_ptr()) }
                    == 0;
            if ok && rename_at(&target.dir, &tmp, &target.name).is_err() {
                unlink_at(&target.dir, &tmp);
            }
        }
    }
}

#[cfg(unix)]
fn write_all_blocking(files: &[(PathBuf, String)], config: Option<&SafetyConfig>) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let validator = config.map(tool_path_validator);

    // Phase 0: pin every parent directory (creating missing ones) and
    // capture pre-images. Nothing is modified except creating directories
    // that the policy check already approved.
    let mut targets: Vec<PinnedTarget> = Vec::with_capacity(files.len());
    for (path, _) in files {
        let (dir, name) = match &validator {
            Some(v) => v
                .open_parent_dir(&path.to_string_lossy(), true)
                .map_err(|e| anyhow::anyhow!(e))?,
            None => crate::safety::path_validator::open_parent_dir_fd(path, true, &|_| Ok(()))
                .map_err(|e| anyhow::anyhow!(e))?,
        };
        let name = std::ffi::CString::new(name.as_bytes())?;
        let (pre_image, mode) = capture_pre_image(&dir, &name, path)?;
        targets.push(PinnedTarget {
            dir,
            name,
            display: path.clone(),
            pre_image,
            mode,
        });
    }

    // Phase 1: stage every replacement in a temp file in its pinned dir.
    let mut staged: Vec<std::ffi::CString> = Vec::with_capacity(files.len());
    for (target, (_, content)) in targets.iter().zip(files) {
        let tmp = temp_name();
        if let Err(e) = stage_temp(&target.dir, &tmp, content.as_bytes(), target.mode) {
            for (t, s) in targets.iter().zip(&staged) {
                unlink_at(&t.dir, s);
            }
            return Err(anyhow::anyhow!(
                "Failed to stage atomic write to {}: {}",
                target.display.display(),
                e
            ));
        }
        staged.push(tmp);
    }

    // Phase 2: renameat them all; roll back earlier renames on failure.
    for (idx, (target, tmp)) in targets.iter().zip(&staged).enumerate() {
        let result = if rename_should_fail_for_tests(&target.display) {
            Err(std::io::Error::other("injected rename failure"))
        } else {
            rename_at(&target.dir, tmp, &target.name)
        };
        if let Err(e) = result {
            for (t, s) in targets.iter().zip(&staged).skip(idx) {
                unlink_at(&t.dir, s);
            }
            for t in targets.iter().take(idx) {
                restore_pre_image(t);
            }
            return Err(anyhow::anyhow!(
                "Failed to persist atomic write to {}: {} (rolled back {} earlier file(s))",
                target.display.display(),
                e,
                idx
            ));
        }
    }
    // Make the renames durable; best effort (not every fs supports it).
    for t in &targets {
        let _ = t.dir.sync_all();
    }
    Ok(())
}

#[cfg(not(unix))]
fn write_all_blocking(files: &[(PathBuf, String)], _config: Option<&SafetyConfig>) -> Result<()> {
    // Capture pre-images up front so a mid-batch persist failure can roll back.
    let pre_images: Vec<(PathBuf, Option<Vec<u8>>)> = files
        .iter()
        .map(|(path, _)| (path.clone(), std::fs::read(path).ok()))
        .collect();

    // Phase 1: stage every replacement in a temp file in the target dir.
    let mut staged: Vec<(NamedTempFile, PathBuf)> = Vec::with_capacity(files.len());
    for (path, content) in files {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid file path (no parent)"))?;
        std::fs::create_dir_all(parent)?;
        let mut temp = NamedTempFile::new_in(parent)?;
        temp.write_all(content.as_bytes())?;
        staged.push((temp, path.clone()));
    }

    // Phase 2: persist them all; roll back earlier persists on failure.
    for (idx, (temp, path)) in staged.into_iter().enumerate() {
        if let Err(e) = temp.persist(&path) {
            for (rb_path, pre_image) in pre_images.iter().take(idx) {
                match pre_image {
                    Some(bytes) => {
                        let _ = std::fs::write(rb_path, bytes);
                    }
                    None => {
                        let _ = std::fs::remove_file(rb_path);
                    }
                }
            }
            return Err(anyhow::anyhow!(
                "Failed to persist atomic write to {}: {} (rolled back {} earlier file(s))",
                path.display(),
                e,
                idx
            ));
        }
    }
    Ok(())
}

/// Refuse to rewrite a file whose bytes are not valid UTF-8.
///
/// The edit pipeline decodes via `String::from_utf8_lossy`; writing the lossy
/// view back would replace every invalid byte with U+FFFD and silently corrupt
/// binary (or otherwise-encoded) files. Better to fail loudly.
fn ensure_valid_utf8(bytes: &[u8], path: &str, tool: &str) -> Result<()> {
    if std::str::from_utf8(bytes).is_err() {
        return Err(ToolError::Execution {
            name: tool.to_string(),
            message: format!(
                "Refusing to modify {}: the file is not valid UTF-8 (binary or another \
                 encoding). Editing it through a lossy decode would corrupt its contents. \
                 If a full overwrite of a non-text file is truly intended, delete it first.",
                path
            ),
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/tools/file/file_test.rs"]
mod tests;
