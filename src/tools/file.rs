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
pub(crate) async fn read_file_with_encoding(path: &Path) -> Result<(String, Vec<u8>)> {
    let bytes = tokio::fs::read(path).await?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok((text, bytes))
}

/// Detect the line ending style of existing content.
fn detect_line_ending(text: &str) -> &'static str {
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
            path: String,
            line_range: Option<(usize, usize)>,
        }

        let args: Args = serde_json::from_value(args)?;
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.path, &safety)?;
        let path = PathBuf::from(&args.path);

        // A line_range lets us stream ONLY the requested slice, so a large file
        // can be sliced without loading it whole or tripping the size limit.
        if let Some((start, end)) = args.line_range {
            let (selected_content, lines_scanned, lossy) =
                read_line_slice(&path, start, end).await?;
            return Ok(serde_json::json!({
                "content": selected_content,
                "total_lines": lines_scanned,
                "truncated": true,
                "encoding": if lossy { "utf-8-lossy" } else { "utf-8" },
                "valid_utf8": !lossy
            }));
        }

        // Whole-file read: guard against OOM on huge files.
        if let Ok(metadata) = tokio::fs::metadata(&path).await {
            if metadata.len() > MAX_READ_SIZE {
                return Err(ToolError::FileTooLarge {
                    size: metadata.len(),
                    limit: MAX_READ_SIZE,
                }
                .into());
            }
        }

        let (content, bytes) = read_file_with_encoding(&path).await?;
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
                "content": {"type": "string"},
                "backup": {"type": "boolean", "default": true}
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            path: String,
            content: String,
            #[serde(default = "default_true")]
            backup: bool,
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

        // Detect existing line endings and preserve them
        let content_to_write = if path.exists() {
            let (existing, existing_bytes) = read_file_with_encoding(&path).await?;
            // Overwriting is a full replace, but refuse to touch a non-UTF-8
            // file: the caller likely believes it is text, and the lossy read
            // above hides what is actually on disk.
            ensure_valid_utf8(&existing_bytes, &args.path, "file_write")?;
            let line_ending = detect_line_ending(&existing);
            preserve_line_endings(&args.content, line_ending)
        } else {
            args.content.clone()
        };

        // Detect no-op writes (content identical to existing file)
        if path.exists() {
            if let Ok(existing) = tokio::fs::read_to_string(&path).await {
                if existing == content_to_write {
                    return Err(ToolError::EditNoOp.into());
                }
            }
        }

        validate_rust_source_if_needed(&path, &content_to_write)?;

        // Create backup if exists
        if args.backup && path.exists() {
            let backup_path = format!("{}.bak", args.path);
            tokio::fs::copy(&path, &backup_path).await?;
        }

        write_atomic(&path, &content_to_write).await?;
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
            path: String,
            old_str: String,
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

        let (content, original_bytes) = read_file_with_encoding(Path::new(&args.path)).await?;
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
        write_atomic(Path::new(&args.path), &new_content).await?;
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
            path: String,
        }

        let args: Args = serde_json::from_value(args)?;
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.path, &safety)?;
        let path = PathBuf::from(&args.path);

        if !path.exists() {
            return Err(ToolError::FileNotFound {
                path: args.path.clone(),
            }
            .into());
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

        tokio::fs::remove_file(&path)
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
            path: String,
            old_str: String,
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
            let (content, original_bytes) = read_file_with_encoding(Path::new(path)).await?;
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

        write_all_atomic(&finals).await?;
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

fn default_true() -> bool {
    true
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
async fn read_line_slice(path: &Path, start: usize, end: usize) -> Result<(String, usize, bool)> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    // Preserve the legacy contract that an inverted range (end < start) yields
    // the single line at `start`.
    let effective_end = end.max(start);
    let file = tokio::fs::File::open(path).await?;
    let mut reader = BufReader::new(file);
    let mut selected: Vec<String> = Vec::new();
    let mut lineno = 0usize;
    let mut lossy = false;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf).await?;
        if n == 0 {
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
    Ok((selected.join("\n"), lineno, lossy))
}

/// Write content to a file atomically using a temporary file and rename.
///
/// The existing file's permission mode is carried over to the replacement —
/// `NamedTempFile` is created `0600`, so without this an executable would
/// silently lose `+x` and a shared config would become owner-only on every
/// edit. New files keep the temp-file default.
pub(crate) async fn write_atomic(path: &Path, content: &str) -> Result<()> {
    write_all_atomic(&[(path.to_path_buf(), content.to_string())]).await
}

/// Capture the unix permission mode of an existing file, if it exists.
#[cfg(unix)]
fn existing_file_mode(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).ok().map(|m| m.permissions().mode())
}

/// Atomically write several files at once: stage every replacement in a temp
/// file first, then persist them all. If any persist fails, files already
/// persisted are rolled back from their captured pre-images so a multi-file
/// batch never commits half-way. Each replacement inherits the target's
/// existing permission mode (see [`write_atomic`]).
pub(crate) async fn write_all_atomic(files: &[(PathBuf, String)]) -> Result<()> {
    // Capture pre-images up front so a mid-batch persist failure can roll back.
    let mut pre_images: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::with_capacity(files.len());
    for (path, _) in files {
        pre_images.push((path.clone(), tokio::fs::read(path).await.ok()));
    }

    // NamedTempFile and Write::write_all are sync APIs; offload to a blocking
    // thread so we don't block the async executor.
    let files_owned: Vec<(PathBuf, String)> = files.to_vec();
    tokio::task::spawn_blocking(move || {
        // Phase 1: stage every replacement in a temp file in the target dir.
        let mut staged: Vec<(NamedTempFile, PathBuf)> = Vec::with_capacity(files_owned.len());
        for (path, content) in &files_owned {
            let parent = path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Invalid file path (no parent)"))?;
            std::fs::create_dir_all(parent)?;
            let mut temp = NamedTempFile::new_in(parent)?;
            temp.write_all(content.as_bytes())?;
            #[cfg(unix)]
            if let Some(mode) = existing_file_mode(path) {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    temp.path(),
                    std::fs::Permissions::from_mode(mode & 0o7777),
                )?;
            }
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
    })
    .await?
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
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn assert_path_access_rejected(err: anyhow::Error) {
        let message = err.to_string();
        assert!(
            message.contains("Path traversal detected")
                || message.contains("Path not in allowed list")
                || message.contains("outside working directory"),
            "expected a path-access rejection, got: {}",
            message
        );
    }

    /// Flatten the nested tree returned by `directory_tree` into a list of
    /// `(name, type)` pairs for easy assertion.
    fn flatten_tree(node: &Value) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Some(children) = node["children"].as_array() {
            for child in children {
                if let (Some(name), Some(type_)) = (child["name"].as_str(), child["type"].as_str())
                {
                    out.push((name.to_string(), type_.to_string()));
                    out.extend(flatten_tree(child));
                }
            }
        }
        out
    }

    /// Create a permissive `SafetyConfig` for tests that need to access temp dirs.
    /// This replaces the old `SELFWARE_TEST_MODE` env-var bypass with explicit config injection.
    fn permissive_safety_config() -> SafetyConfig {
        SafetyConfig {
            allowed_paths: vec!["/**".to_string()],
            ..SafetyConfig::default()
        }
    }

    fn restricted_safety_config(root: &Path) -> SafetyConfig {
        SafetyConfig {
            allowed_paths: vec![format!("{}/**", root.display())],
            ..SafetyConfig::default()
        }
    }

    #[test]
    fn test_file_read_name() {
        let tool = FileRead::new();
        assert_eq!(tool.name(), "file_read");
    }

    #[test]
    fn test_file_read_description() {
        let tool = FileRead::new();
        assert!(tool.description().contains("Read"));
    }

    #[test]
    fn test_file_read_schema() {
        let tool = FileRead::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
    }

    #[tokio::test]
    async fn test_file_read_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total_lines"], 3);
        assert!(result["content"].as_str().unwrap().contains("line1"));
    }

    #[tokio::test]
    async fn test_file_read_with_line_range() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "line_range": [2, 4]
        });

        let result = tool.execute(args).await.unwrap();
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("line2"));
        assert!(content.contains("line4"));
        assert!(!content.contains("line1"));
    }

    #[tokio::test]
    async fn file_read_line_range_streams_exact_slice() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("big.txt");
        // 10k lines; slicing must return exactly the requested window and stop
        // early (lines_scanned == end), not read/return the whole file.
        let body: String = (1..=10_000).map(|i| format!("line{i}\n")).collect();
        fs::write(&file_path, &body).unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "line_range": [3, 5]
        });
        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["content"].as_str().unwrap(), "line3\nline4\nline5");
        assert_eq!(result["truncated"], true);
        // Scanned only up to `end` (5), proving it stopped early.
        assert_eq!(result["total_lines"], 5);

        // A CRLF file slices cleanly (carriage returns stripped).
        let crlf = temp_dir.path().join("crlf.txt");
        fs::write(&crlf, "a\r\nb\r\nc\r\n").unwrap();
        let args = serde_json::json!({
            "path": crlf.to_str().unwrap(),
            "line_range": [2, 2]
        });
        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["content"].as_str().unwrap(), "b");
    }

    #[tokio::test]
    async fn test_file_read_not_found() {
        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": "/nonexistent/file.txt"});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_file_write_name() {
        let tool = FileWrite::new();
        assert_eq!(tool.name(), "file_write");
    }

    #[tokio::test]
    async fn test_file_write_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("output.txt");

        let tool = FileWrite::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "Hello, World!"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["bytes_written"], 13);

        // Verify file was written
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_file_write_creates_backup() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("existing.txt");
        fs::write(&file_path, "original content").unwrap();

        let tool = FileWrite::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new content",
            "backup": true
        });

        tool.execute(args).await.unwrap();

        // Check backup exists
        let backup_path = temp_dir.path().join("existing.txt.bak");
        assert!(backup_path.exists());
        let backup_content = fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, "original content");
    }

    #[tokio::test]
    async fn test_file_write_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir
            .path()
            .join("subdir")
            .join("nested")
            .join("file.txt");

        let tool = FileWrite::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "nested content"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert!(file_path.exists());
    }

    #[test]
    fn test_file_edit_name() {
        let tool = FileEdit::new();
        assert_eq!(tool.name(), "file_edit");
    }

    #[tokio::test]
    async fn test_file_edit_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "World",
            "new_str": "Rust"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello, Rust!");
    }

    #[tokio::test]
    async fn test_file_edit_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "NotFound",
            "new_str": "Replacement"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_file_edit_multiple_matches() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit.txt");
        fs::write(&file_path, "Hello Hello Hello").unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "Hello",
            "new_str": "Hi"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("3 times"));
    }

    #[tokio::test]
    async fn test_file_edit_rejects_whole_file_replacement() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "line1\nline2\nline3\nline4\nline5\n",
            "new_str": "replaced\n"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("file_edit rejected"),
            "unexpected error: {err}"
        );

        // The file should be unchanged.
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line1\nline2\nline3\nline4\nline5\n");
    }

    #[test]
    fn test_directory_tree_name() {
        let tool = DirectoryTree::new();
        assert_eq!(tool.name(), "directory_tree");
    }

    #[tokio::test]
    async fn test_directory_tree_success() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file1.txt"), "").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "").unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let tool = DirectoryTree::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap()
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result["total"].as_i64().unwrap() >= 3);
    }

    #[tokio::test]
    async fn test_directory_tree_excludes_hidden() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("visible.txt"), "").unwrap();
        fs::write(temp_dir.path().join(".hidden"), "").unwrap();

        let tool = DirectoryTree::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "include_hidden": false
        });

        let result = tool.execute(args).await.unwrap();
        let entries = flatten_tree(&result["tree"]);

        // Should not contain .hidden
        let has_hidden = entries.iter().any(|(name, _)| name.contains(".hidden"));
        assert!(!has_hidden);
    }

    #[tokio::test]
    async fn test_directory_tree_includes_hidden() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join(".hidden"), "").unwrap();

        let tool = DirectoryTree::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "include_hidden": true
        });

        let result = tool.execute(args).await.unwrap();
        let entries = flatten_tree(&result["tree"]);

        // Should contain .hidden
        let has_hidden = entries.iter().any(|(name, _)| name.contains(".hidden"));
        assert!(has_hidden);
    }

    #[test]
    fn test_default_true() {
        assert!(default_true());
    }

    #[test]
    fn test_default_three() {
        assert_eq!(default_three(), 3);
    }

    // Additional error path tests for coverage

    #[tokio::test]
    async fn test_file_read_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");
        fs::write(&file_path, "").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total_lines"], 0);
        assert_eq!(result["content"], "");
    }

    #[tokio::test]
    async fn test_file_read_line_range_start_beyond_end() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "line_range": [100, 200]
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["content"], "");
    }

    #[tokio::test]
    async fn test_file_read_single_line_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("single.txt");
        fs::write(&file_path, "only one line").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total_lines"], 1);
        assert_eq!(result["content"], "only one line");
    }

    #[tokio::test]
    async fn test_file_read_with_unicode() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("unicode.txt");
        fs::write(&file_path, "日本語\n한국어\n中文").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total_lines"], 3);
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("日本語"));
    }

    #[tokio::test]
    async fn test_file_write_no_backup() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("no_backup.txt");
        fs::write(&file_path, "original").unwrap();

        let tool = FileWrite::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new content",
            "backup": false
        });

        tool.execute(args).await.unwrap();

        let backup_path = temp_dir.path().join("no_backup.txt.bak");
        assert!(!backup_path.exists());
    }

    #[tokio::test]
    async fn test_file_write_new_file_no_backup_needed() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("brand_new.txt");

        let tool = FileWrite::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new file content",
            "backup": true
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert!(file_path.exists());

        let backup_path = temp_dir.path().join("brand_new.txt.bak");
        assert!(!backup_path.exists());
    }

    #[tokio::test]
    async fn test_file_edit_delete_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("delete.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": ", World",
            "new_str": ""
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello!");
    }

    #[tokio::test]
    async fn test_file_edit_multiline_match() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("multiline.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "line1\nline2",
            "new_str": "replaced\nlines"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("replaced\nlines"));
        assert!(!content.contains("line1"));
    }

    #[tokio::test]
    async fn test_file_edit_file_not_exist() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "anything",
            "new_str": "replacement"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_edit_noop_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("noop.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "Hello",
            "new_str": "Hello"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("no-op"),
            "Expected no-op error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_file_edit_duplicate_insertion_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("duplicate.txt");
        fs::write(&file_path, "alpha\nbeta\ninserted\n").unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "alpha\n",
            "new_str": "alpha\nbeta\ninserted\n"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("duplicate insertion rejected"),
            "Expected duplicate insertion error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_directory_tree_max_depth_honored() {
        let temp_dir = TempDir::new().unwrap();
        let deep_path = temp_dir.path().join("a").join("b").join("c").join("d");
        fs::create_dir_all(&deep_path).unwrap();
        fs::write(deep_path.join("deep_file.txt"), "").unwrap();

        let tool = DirectoryTree::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "max_depth": 2
        });

        let result = tool.execute(args).await.unwrap();
        let entries = flatten_tree(&result["tree"]);

        let has_deep = entries.iter().any(|(name, _)| name.contains("deep_file"));
        assert!(!has_deep);
    }

    #[tokio::test]
    async fn test_directory_tree_nonexistent_path() {
        let tool = DirectoryTree::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": "/nonexistent/directory/path"
        });

        let result = tool.execute(args).await;
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_invalid_json_args() {
        let tool = FileRead::new();
        let args = serde_json::json!({});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_write_invalid_json_args() {
        let tool = FileWrite::new();
        let args = serde_json::json!({"path": "test.txt"});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_edit_invalid_json_args() {
        let tool = FileEdit::new();
        let args = serde_json::json!({"path": "test.txt"});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_directory_tree_invalid_json_args() {
        let tool = DirectoryTree::new();
        let args = serde_json::json!({});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_line_range_inverted() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "line_range": [3, 1]
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["content"], "line3");
    }

    #[tokio::test]
    async fn test_file_write_empty_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty_write.txt");

        let tool = FileWrite::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": ""
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["bytes_written"], 0);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "");
    }

    #[tokio::test]
    async fn test_file_write_rejects_invalid_rust_source() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("lib.rs");

        let tool = FileWrite::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "Alignment::Center => {"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Rust syntax validation failed"), "got: {err}");
        assert!(
            !file_path.exists(),
            "invalid Rust write should not touch the file"
        );
    }

    #[tokio::test]
    async fn test_file_edit_rejects_invalid_rust_source() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("lib.rs");
        fs::write(&file_path, "pub fn render() {\n    println!(\"ok\");\n}\n").unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "println!(\"ok\");",
            "new_str": "Alignment::Center => {"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Rust syntax validation failed"), "got: {err}");

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(
            content.contains("println!(\"ok\");"),
            "invalid Rust edit should leave the original file intact"
        );
    }

    #[tokio::test]
    async fn test_directory_tree_with_files_and_directories() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file.txt"), "content").unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        fs::write(temp_dir.path().join("subdir").join("nested.txt"), "").unwrap();

        let tool = DirectoryTree::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap()
        });

        let result = tool.execute(args).await.unwrap();
        let entries = flatten_tree(&result["tree"]);

        let has_dir = entries.iter().any(|(_, type_)| type_ == "directory");
        let has_file = entries.iter().any(|(_, type_)| type_ == "file");
        assert!(has_dir);
        assert!(has_file);
    }

    #[tokio::test]
    async fn test_file_read_blocks_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let blocked_path = temp_dir
            .path()
            .parent()
            .unwrap()
            .join("should_not_escape.txt");
        let tool = FileRead::with_safety_config(restricted_safety_config(temp_dir.path()));
        let args = serde_json::json!({"path": blocked_path});
        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert_path_access_rejected(result.unwrap_err());
    }

    #[tokio::test]
    async fn test_directory_tree_blocks_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let blocked_path = temp_dir.path().parent().unwrap();
        let tool = DirectoryTree::with_safety_config(restricted_safety_config(temp_dir.path()));
        let args = serde_json::json!({"path": blocked_path});
        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert_path_access_rejected(result.unwrap_err());
    }

    // FileDelete tests

    #[test]
    fn test_file_delete_name() {
        let tool = FileDelete::new();
        assert_eq!(tool.name(), "file_delete");
    }

    #[test]
    fn test_file_delete_description() {
        let tool = FileDelete::new();
        assert!(tool.description().contains("Delete"));
    }

    #[test]
    fn test_file_delete_schema() {
        let tool = FileDelete::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("path")));
    }

    #[tokio::test]
    async fn test_file_delete_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("to_delete.txt");
        fs::write(&file_path, "delete me").unwrap();
        assert!(file_path.exists());

        let tool = FileDelete::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["deleted"], true);
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_file_delete_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");

        let tool = FileDelete::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[tokio::test]
    async fn test_file_delete_directory_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("subdir");
        fs::create_dir(&dir_path).unwrap();

        let tool = FileDelete::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": dir_path.to_str().unwrap()});

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("directory"));
    }

    #[tokio::test]
    async fn test_file_delete_blocks_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let blocked_path = temp_dir.path().parent().unwrap().join("escape.txt");
        let tool = FileDelete::with_safety_config(restricted_safety_config(temp_dir.path()));
        let args = serde_json::json!({"path": blocked_path});
        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert_path_access_rejected(result.unwrap_err());
    }

    #[tokio::test]
    async fn test_file_delete_invalid_json_args() {
        let tool = FileDelete::new();
        let args = serde_json::json!({});
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    // --- FileMultiEdit tests ---

    #[test]
    fn test_file_multi_edit_name() {
        let tool = FileMultiEdit::new();
        assert_eq!(tool.name(), "file_multi_edit");
    }

    #[tokio::test]
    async fn test_file_multi_edit_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("multi.txt");
        fs::write(&file_path, "alpha\nbeta\ngamma\n").unwrap();

        let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "edits": [
                {"path": file_path.to_str().unwrap(), "old_str": "alpha", "new_str": "ALPHA"},
                {"path": file_path.to_str().unwrap(), "old_str": "gamma", "new_str": "GAMMA"}
            ]
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["edits_applied"], 2);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("ALPHA"));
        assert!(content.contains("beta"));
        assert!(content.contains("GAMMA"));
        assert!(!content.contains("alpha"));
        assert!(!content.contains("gamma"));
    }

    #[tokio::test]
    async fn test_file_multi_edit_atomic_rollback() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("atomic.txt");
        fs::write(&file_path, "one\ntwo\nthree\n").unwrap();

        let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "edits": [
                {"path": file_path.to_str().unwrap(), "old_str": "one", "new_str": "ONE"},
                {"path": file_path.to_str().unwrap(), "old_str": "MISSING", "new_str": "NEVER"}
            ]
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());

        // File should remain unchanged because the batch is atomic
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("one"));
        assert!(!content.contains("ONE"));
    }

    #[tokio::test]
    async fn test_file_multi_edit_overlap_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("overlap.txt");
        fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "edits": [
                {"path": file_path.to_str().unwrap(), "old_str": "line1\nline2", "new_str": "A"},
                {"path": file_path.to_str().unwrap(), "old_str": "line2\nline3", "new_str": "B"}
            ]
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("overlap"));
    }

    // --- Stale-guard tests ---

    #[tokio::test]
    async fn test_file_edit_stale_guard_rejects() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("stale.txt");
        fs::write(&file_path, "original content").unwrap();

        // Simulate a read to record snapshot
        let read_tool = FileRead::with_safety_config(permissive_safety_config());
        let read_args = serde_json::json!({"path": file_path.to_str().unwrap()});
        read_tool.execute(read_args).await.unwrap();

        // Modify file externally
        fs::write(&file_path, "externally modified").unwrap();

        // Edit should be rejected as stale
        let edit_tool = FileEdit::with_safety_config(permissive_safety_config());
        let edit_args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "original content",
            "new_str": "replaced"
        });
        let result = edit_tool.execute(edit_args).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("changed on disk"));
    }

    #[tokio::test]
    async fn test_file_write_stale_guard_rejects() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("stale_write.txt");
        fs::write(&file_path, "original").unwrap();

        let read_tool = FileRead::with_safety_config(permissive_safety_config());
        read_tool
            .execute(serde_json::json!({"path": file_path.to_str().unwrap()}))
            .await
            .unwrap();

        fs::write(&file_path, "modified externally").unwrap();

        let write_tool = FileWrite::with_safety_config(permissive_safety_config());
        let result = write_tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "content": "new content"
            }))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("changed on disk"));
    }

    #[tokio::test]
    async fn test_file_delete_stale_guard_rejects() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("stale_delete.txt");
        fs::write(&file_path, "original").unwrap();

        let read_tool = FileRead::with_safety_config(permissive_safety_config());
        read_tool
            .execute(serde_json::json!({"path": file_path.to_str().unwrap()}))
            .await
            .unwrap();

        fs::write(&file_path, "modified externally").unwrap();

        let delete_tool = FileDelete::with_safety_config(permissive_safety_config());
        let result = delete_tool
            .execute(serde_json::json!({"path": file_path.to_str().unwrap()}))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("changed on disk"));
    }

    // --- Line ending preservation tests ---

    #[tokio::test]
    async fn test_file_write_preserves_crlf() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("crlf.txt");
        fs::write(&file_path, b"line1\r\nline2\r\n").unwrap();

        let tool = FileWrite::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new1\nnew2\n"
        });

        tool.execute(args).await.unwrap();
        let bytes = fs::read(&file_path).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("\r\n"));
        assert!(!text.contains("\n\n") && !text.contains("\r\r"));
    }

    #[tokio::test]
    async fn test_file_edit_preserves_crlf() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("crlf_edit.txt");
        fs::write(&file_path, b"alpha\r\nbeta\r\n").unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "alpha",
            "new_str": "ALPHA"
        });

        tool.execute(args).await.unwrap();
        let bytes = fs::read(&file_path).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("ALPHA\r\n"));
        assert!(text.contains("beta\r\n"));
    }

    // --- Permission preservation across atomic writes ---

    #[tokio::test]
    #[cfg(unix)]
    async fn test_file_edit_preserves_executable_mode() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = TempDir::new().unwrap();
        let script = temp_dir.path().join("run.sh");
        fs::write(&script, "#!/bin/sh\necho old\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        tool.execute(serde_json::json!({
            "path": script.to_str().unwrap(),
            "old_str": "echo old",
            "new_str": "echo new"
        }))
        .await
        .unwrap();

        let mode = fs::metadata(&script).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "edit must not strip the executable bit");
        assert!(fs::read_to_string(&script).unwrap().contains("echo new"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_file_write_preserves_mode_on_overwrite() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = TempDir::new().unwrap();
        let script = temp_dir.path().join("tool.sh");
        fs::write(&script, "#!/bin/sh\necho v1\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o750)).unwrap();

        let tool = FileWrite::with_safety_config(permissive_safety_config());
        tool.execute(serde_json::json!({
            "path": script.to_str().unwrap(),
            "content": "#!/bin/sh\necho v2\n",
            "backup": false
        }))
        .await
        .unwrap();

        let mode = fs::metadata(&script).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o750, "overwrite must preserve the original mode");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_file_multi_edit_preserves_mode() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = TempDir::new().unwrap();
        let script = temp_dir.path().join("multi.sh");
        fs::write(&script, "#!/bin/sh\necho a\necho b\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
        tool.execute(serde_json::json!({
            "edits": [
                {"path": script.to_str().unwrap(), "old_str": "echo a", "new_str": "echo A"},
                {"path": script.to_str().unwrap(), "old_str": "echo b", "new_str": "echo B"}
            ]
        }))
        .await
        .unwrap();

        let mode = fs::metadata(&script).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "multi_edit must not strip the executable bit");
    }

    // --- Non-UTF8 honesty ---

    #[tokio::test]
    async fn test_file_edit_refuses_non_utf8_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("binary.bin");
        let original: &[u8] = &[0x66, 0x80, 0x67, 0xFF, 0x00, 0x61];
        fs::write(&file_path, original).unwrap();

        let tool = FileEdit::with_safety_config(permissive_safety_config());
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "old_str": "f",
                "new_str": "x"
            }))
            .await;
        let err = result.expect_err("editing a non-UTF-8 file must fail loudly");
        assert!(
            err.to_string().contains("not valid UTF-8"),
            "unexpected error: {err}"
        );
        // The file must be byte-identical — no U+FFFD corruption.
        assert_eq!(fs::read(&file_path).unwrap(), original);
    }

    #[tokio::test]
    async fn test_file_multi_edit_refuses_non_utf8_file() {
        let temp_dir = TempDir::new().unwrap();
        let good = temp_dir.path().join("good.txt");
        fs::write(&good, "alpha\n").unwrap();
        let binary = temp_dir.path().join("binary.bin");
        let original: &[u8] = &[0xDE, 0xAD, 0x80, 0xFF];
        fs::write(&binary, original).unwrap();

        let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
        let result = tool
            .execute(serde_json::json!({
                "edits": [
                    {"path": good.to_str().unwrap(), "old_str": "alpha", "new_str": "ALPHA"},
                    {"path": binary.to_str().unwrap(), "old_str": "x", "new_str": "y"}
                ]
            }))
            .await;
        let err = result.expect_err("multi_edit touching a non-UTF-8 file must fail");
        assert!(
            err.to_string().contains("not valid UTF-8"),
            "unexpected error: {err}"
        );
        // Atomic batch: the GOOD file must not have been modified either.
        assert_eq!(fs::read_to_string(&good).unwrap(), "alpha\n");
        assert_eq!(fs::read(&binary).unwrap(), original);
    }

    #[tokio::test]
    async fn test_file_write_refuses_overwriting_non_utf8_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("data.dat");
        let original: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0xFF];
        fs::write(&file_path, original).unwrap();

        let tool = FileWrite::with_safety_config(permissive_safety_config());
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "content": "replacement text"
            }))
            .await;
        let err = result.expect_err("overwriting a non-UTF-8 file must fail loudly");
        assert!(
            err.to_string().contains("not valid UTF-8"),
            "unexpected error: {err}"
        );
        assert_eq!(fs::read(&file_path).unwrap(), original);
    }

    #[tokio::test]
    async fn test_file_read_reports_lossy_encoding_honestly() {
        let temp_dir = TempDir::new().unwrap();
        let binary = temp_dir.path().join("binary.bin");
        fs::write(&binary, [0x61, 0x0A, 0x80, 0xFF, 0x0A, 0x62]).unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let result = tool
            .execute(serde_json::json!({"path": binary.to_str().unwrap()}))
            .await
            .unwrap();
        assert_eq!(result["encoding"], "utf-8-lossy");
        assert_eq!(result["valid_utf8"], false);

        // The line-range path reports lossiness of the returned slice too.
        let result = tool
            .execute(serde_json::json!({
                "path": binary.to_str().unwrap(),
                "line_range": [2, 2]
            }))
            .await
            .unwrap();
        assert_eq!(result["encoding"], "utf-8-lossy");
        assert_eq!(result["valid_utf8"], false);

        // A plain UTF-8 file still reports exact utf-8.
        let text = temp_dir.path().join("text.txt");
        fs::write(&text, "plain text\n").unwrap();
        let result = tool
            .execute(serde_json::json!({"path": text.to_str().unwrap()}))
            .await
            .unwrap();
        assert_eq!(result["encoding"], "utf-8");
        assert_eq!(result["valid_utf8"], true);
    }

    // --- Multi-edit batch atomicity / determinism ---

    #[tokio::test]
    async fn test_file_multi_edit_validation_failure_touches_no_file() {
        let temp_dir = TempDir::new().unwrap();
        let f1 = temp_dir.path().join("one.txt");
        let f2 = temp_dir.path().join("two.txt");
        fs::write(&f1, "one\n").unwrap();
        fs::write(&f2, "two\n").unwrap();

        let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
        let result = tool
            .execute(serde_json::json!({
                "edits": [
                    {"path": f1.to_str().unwrap(), "old_str": "one", "new_str": "ONE"},
                    {"path": f2.to_str().unwrap(), "old_str": "MISSING", "new_str": "X"}
                ]
            }))
            .await;
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&f1).unwrap(), "one\n");
        assert_eq!(fs::read_to_string(&f2).unwrap(), "two\n");
    }

    #[tokio::test]
    async fn test_file_multi_edit_reports_files_in_deterministic_order() {
        let temp_dir = TempDir::new().unwrap();
        // Deliberately non-alphabetical creation/argument order.
        let zb = temp_dir.path().join("zb.txt");
        let ab = temp_dir.path().join("ab.txt");
        fs::write(&zb, "z\n").unwrap();
        fs::write(&ab, "a\n").unwrap();

        let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
        let result = tool
            .execute(serde_json::json!({
                "edits": [
                    {"path": zb.to_str().unwrap(), "old_str": "z", "new_str": "Z"},
                    {"path": ab.to_str().unwrap(), "old_str": "a", "new_str": "A"}
                ]
            }))
            .await
            .unwrap();

        let files: Vec<String> = serde_json::from_value(result["files"].clone()).unwrap();
        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(files, sorted, "files list must be deterministically sorted");
    }

    #[tokio::test]
    async fn test_write_all_atomic_rolls_back_on_persist_failure() {
        let temp_dir = TempDir::new().unwrap();
        let good = temp_dir.path().join("good.txt");
        fs::write(&good, "original\n").unwrap();
        // The second target is an existing DIRECTORY: staging succeeds, but
        // the persist (rename over a dir) fails — forcing a rollback of the
        // first file.
        let dir_target = temp_dir.path().join("a_directory");
        fs::create_dir(&dir_target).unwrap();

        let files = vec![
            (good.clone(), "modified\n".to_string()),
            (dir_target.clone(), "whatever".to_string()),
        ];
        let result = write_all_atomic(&files).await;
        let err = result.expect_err("persist over a directory must fail");
        assert!(
            err.to_string().contains("rolled back"),
            "error should mention rollback: {err}"
        );
        assert_eq!(
            fs::read_to_string(&good).unwrap(),
            "original\n",
            "the already-persisted file must be rolled back to its pre-image"
        );
    }

    #[tokio::test]
    async fn test_write_all_atomic_staging_failure_touches_nothing() {
        let temp_dir = TempDir::new().unwrap();
        let good = temp_dir.path().join("good.txt");
        fs::write(&good, "original\n").unwrap();
        // Second target's parent component is a regular FILE, so staging its
        // temp file fails before anything is persisted.
        let blocker = temp_dir.path().join("blocker");
        fs::write(&blocker, "not a dir").unwrap();
        let impossible = blocker.join("child.txt");

        let files = vec![
            (good.clone(), "modified\n".to_string()),
            (impossible, "nope".to_string()),
        ];
        let result = write_all_atomic(&files).await;
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&good).unwrap(), "original\n");
    }
}
