use anyhow::Result;
use colored::*;
use regex::Regex;
use sha2::{Digest, Sha256};

use super::*;

/// Stable snapshot of the workspace used by one context operation. The
/// workspace-root handle can be changed when an agent enters or leaves a
/// worktree, so resolve every path against the root that was active when the
/// operation began.
struct ContextWorkspace {
    path: std::path::PathBuf,
    explicit: bool,
}

impl ContextWorkspace {
    fn capture() -> Self {
        let root = crate::tools::workspace_root::current();
        Self {
            path: root.path(),
            explicit: root.is_explicit(),
        }
    }

    /// Preserve legacy relative labels when the root follows the process
    /// cwd, but pin relative paths to an explicit base/worktree before any
    /// async or blocking I/O can lose the task-local root.
    fn anchor(&self, path: &std::path::Path) -> std::path::PathBuf {
        if path.is_absolute() || !self.explicit {
            path.to_path_buf()
        } else {
            self.path.join(path)
        }
    }

    fn walk_root(&self) -> std::path::PathBuf {
        if self.explicit {
            self.path.clone()
        } else {
            std::path::PathBuf::from(".")
        }
    }
}

/// Prune directories that are never project source before a repository-wide
/// walk descends into them. In particular, private coding-agent state may
/// contain transcripts, caches, credentials, and nested worktree clones.
fn retain_public_context_entry(entry: &walkdir::DirEntry) -> bool {
    crate::evolve::graph::retain_repository_entry(entry)
}

fn context_path_label(path: &std::path::Path) -> String {
    crate::safety::source_context::quote_untrusted_label(&path.to_string_lossy())
}

pub(super) fn context_file_header(path: &std::path::Path) -> String {
    format!(
        "\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n",
        context_path_label(path)
    )
}

fn context_file_message_name(path: &std::path::Path) -> String {
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    let short_hash = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("selfware_ctx_{short_hash}")
}

fn context_file_message(path: &std::path::Path, content: String) -> Message {
    let mut message = Message::user(content);
    message.name = Some(context_file_message_name(path));
    message
}

pub(super) fn is_context_file_message(message: &Message, path: &std::path::Path) -> bool {
    let expected_name = context_file_message_name(path);
    message.role == "user"
        && (message.name.as_deref() == Some(expected_name.as_str())
            // Compatibility for context messages created before they carried
            // an internal identity. A successful refresh migrates them.
            || (message.name.is_none()
                && message.content.text().starts_with(&context_file_header(path))))
}

fn context_file_message_indices(messages: &[Message], path: &std::path::Path) -> Vec<usize> {
    messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| is_context_file_message(message, path).then_some(index))
        .collect()
}

fn context_file_message_indices_for_paths(
    messages: &[Message],
    paths: &[&std::path::Path],
) -> Vec<usize> {
    messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            paths
                .iter()
                .any(|path| is_context_file_message(message, path))
                .then_some(index)
        })
        .collect()
}

async fn collect_context_paths(
    walk_root: std::path::PathBuf,
    extensions: Vec<String>,
) -> Vec<std::path::PathBuf> {
    crate::tools::workspace_root::spawn_blocking(move || {
        let mut out = Vec::new();
        for entry in walkdir::WalkDir::new(walk_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(retain_public_context_entry)
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path().to_path_buf();
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if extensions.iter().any(|expected| expected == extension) {
                out.push(path);
            }
        }
        out
    })
    .await
    .unwrap_or_default()
}

fn replace_context_file_messages(
    messages: &mut Vec<Message>,
    existing_indices: &[usize],
    replacement: Message,
) {
    if let Some((&first, duplicates)) = existing_indices.split_first() {
        messages[first] = replacement;
        for &index in duplicates.iter().rev() {
            messages.remove(index);
        }
    } else {
        messages.push(replacement);
    }
}

fn messages_at_indices_tokens(messages: &[Message], indices: &[usize]) -> usize {
    indices
        .iter()
        .map(|&index| {
            crate::token_count::estimate_messages_tokens(std::slice::from_ref(&messages[index]))
        })
        .sum()
}

impl Agent {
    fn migrate_context_file_path(&mut self, previous: &str, resolved: &std::path::Path) {
        let resolved = resolved.to_string_lossy().into_owned();
        if previous == resolved {
            return;
        }

        for tracked in &mut self.file_tracker.context_files {
            if tracked == previous {
                *tracked = resolved.clone();
            }
        }

        // A checkpoint may contain both the old relative spelling and the
        // resolved spelling. Keep one tracker entry after migrating it.
        let mut kept_resolved = false;
        self.file_tracker.context_files.retain(|tracked| {
            if tracked != &resolved {
                return true;
            }
            if kept_resolved {
                false
            } else {
                kept_resolved = true;
                true
            }
        });
    }

    /// Remove every generated context message and tracker entry for one
    /// canonical file identity. This keeps a successful delete from leaving
    /// old source text in the model conversation.
    pub(super) fn remove_context_file(&mut self, path: &str) {
        let key = self.file_tracker.key(path);
        let tracked_paths = self
            .file_tracker
            .context_files
            .iter()
            .filter(|tracked| self.file_tracker.key(tracked) == key)
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>();
        self.messages.retain(|message| {
            !tracked_paths
                .iter()
                .any(|tracked| is_context_file_message(message, tracked))
        });
        self.file_tracker.remove_deleted(path);
    }

    fn remove_resolved_context_file(&mut self, tracked: &str, resolved: &std::path::Path) {
        let tracked_key = self.file_tracker.key(tracked);
        let resolved_string = resolved.to_string_lossy();
        let resolved_key = self.file_tracker.key(resolved_string.as_ref());
        let mut identities = self
            .file_tracker
            .context_files
            .iter()
            .filter(|candidate| {
                let key = self.file_tracker.key(candidate);
                key == tracked_key || key == resolved_key
            })
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>();
        identities.push(std::path::PathBuf::from(tracked));
        identities.push(resolved.to_path_buf());
        self.messages.retain(|message| {
            !identities
                .iter()
                .any(|identity| is_context_file_message(message, identity))
        });
        self.file_tracker.remove_deleted(tracked);
        if resolved_string.as_ref() != tracked {
            self.file_tracker.remove_deleted(resolved_string.as_ref());
        }
    }
}

impl Agent {
    /// Refresh any stale files that are in context
    /// Returns the number of files refreshed
    pub(super) async fn refresh_stale_context_files(&mut self) -> usize {
        if self.file_tracker.stale_files.is_empty() {
            return 0;
        }

        // Find which stale files are in our context
        let stale_in_context: Vec<String> = self
            .file_tracker
            .context_files
            .iter()
            .filter(|f| self.file_tracker.is_stale(f))
            .cloned()
            .collect();

        if stale_in_context.is_empty() {
            self.file_tracker.stale_files.clear();
            return 0;
        }

        let workspace = ContextWorkspace::capture();
        let mut refreshed = 0;
        let mut refreshed_paths = Vec::new();
        for path_str in &stale_in_context {
            let tracked_path = std::path::Path::new(path_str);
            let path = workspace.anchor(tracked_path);
            if let Err(error) = self.validate_context_path(&path) {
                warn!("Skipping unsafe context file {path_str}: {error}");
                continue;
            }
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => {
                    let raw = format!("{}{}", context_file_header(&path), content);
                    let new_content = self.sanitize_context_data(&path, &raw);
                    let new_message = context_file_message(&path, new_content);

                    // Find and replace the existing message for this file.
                    let existing_indices = context_file_message_indices_for_paths(
                        &self.messages,
                        &[tracked_path, &path],
                    );
                    if !existing_indices.is_empty() {
                        let budget = self.max_context_tokens;
                        let current = crate::token_count::estimate_messages_tokens(&self.messages);
                        let old = messages_at_indices_tokens(&self.messages, &existing_indices);
                        let replacement = crate::token_count::estimate_messages_tokens(
                            std::slice::from_ref(&new_message),
                        );
                        let projected = current.saturating_sub(old).saturating_add(replacement);
                        if budget > 0 && projected > budget && projected > current {
                            warn!(
                                path = %path.display(),
                                projected,
                                budget,
                                "Skipping stale context refresh that would exceed measured budget"
                            );
                            continue;
                        }
                        replace_context_file_messages(
                            &mut self.messages,
                            &existing_indices,
                            new_message,
                        );
                        self.migrate_context_file_path(path_str, &path);
                        refreshed += 1;
                        refreshed_paths
                            .push((path_str.clone(), path.to_string_lossy().into_owned()));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.remove_resolved_context_file(path_str, &path);
                }
                Err(error) => {
                    warn!(path = %path.display(), %error, "Could not refresh context file")
                }
            }
        }

        // Clear the stale set for refreshed files
        for (tracked, resolved) in &refreshed_paths {
            self.file_tracker.clear_stale(tracked);
            self.file_tracker.clear_stale(resolved);
        }

        refreshed
    }

    /// Clear all context (messages and memory)
    pub(super) fn clear_context(&mut self) {
        self.messages.retain(|m| m.role == "system");
        // Same bug class as /clear: retaining by role keeps the system
        // messages but leaves the previous task's focus overlay on
        // messages[0] plus current_task_context / last_assistant_response
        // / failure-mode counters, which would leak the old task into the
        // next answer. Reset that per-task state explicitly.
        if let Some(first) = self.messages.iter_mut().find(|m| m.role == "system") {
            let clean = super::task_runner::strip_focus_overlay(first.content.text());
            first.content = crate::api::types::MessageContent::from_text(clean);
        }
        self.current_task_context.clear();
        self.last_assistant_response.clear();
        self.reset_failure_mode_counters();
        self.memory.clear();
        self.file_tracker.context_files.clear();
        self.file_tracker.stale_files.clear();
        self.clear_task_state_memory();
    }

    /// Load files matching pattern into context
    pub(super) async fn load_files_to_context(&mut self, pattern: &str) -> Result<usize> {
        let mut loaded = 0;
        let mut loaded_tokens = 0usize;
        let extensions: Vec<&str> = if pattern == "." || pattern == "*" {
            vec!["rs", "toml", "md", "ts", "tsx", "js", "jsx", "py", "go"]
        } else {
            pattern
                .split(',')
                .map(|s| s.trim().trim_start_matches('.'))
                .collect()
        };

        println!();
        println!(
            "{} Loading files with extensions: {}",
            "📂".bright_cyan(),
            extensions.join(", ").bright_yellow()
        );
        println!();

        // Capture the active root before spawning: Tokio task-locals do not
        // automatically cross a blocking-task boundary.
        let workspace = ContextWorkspace::capture();
        let extensions_owned = extensions.iter().map(|value| value.to_string()).collect();
        let paths = collect_context_paths(workspace.walk_root(), extensions_owned).await;

        // Count the payload that is already in the conversation, then admit
        // each file using the shared tokenizer.  File-size fractions are not
        // a safe projection for code and previously let /ctx load exceed the
        // configured model window.
        let budget = self.max_context_tokens;
        let mut current_tokens = crate::token_count::estimate_messages_tokens(&self.messages);
        let mut skipped_for_budget = 0usize;
        let mut skipped_for_limit = 0usize;

        for path in paths {
            let path_str = path.display().to_string();
            let already_tracked = self.file_tracker.context_files.contains(&path_str);
            if !already_tracked
                && self.file_tracker.context_files.len() >= MAX_TRACKED_CONTEXT_FILES
            {
                skipped_for_limit += 1;
                tracing::warn!(
                    path = %path.display(),
                    limit = MAX_TRACKED_CONTEXT_FILES,
                    "/ctx load skipped file because the tracked-file limit was reached"
                );
                continue;
            }
            if let Err(error) = self.validate_context_path(&path) {
                warn!("Skipping unsafe context file {}: {error}", path.display());
                continue;
            }
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let raw = format!("{}{}", context_file_header(&path), content);
                let full_content = self.sanitize_context_data(&path, &raw);
                let message = context_file_message(&path, full_content);
                let file_tokens =
                    crate::token_count::estimate_messages_tokens(std::slice::from_ref(&message));
                let existing_indices = context_file_message_indices(&self.messages, &path);
                let old_tokens = messages_at_indices_tokens(&self.messages, &existing_indices);
                let projected = current_tokens
                    .saturating_sub(old_tokens)
                    .saturating_add(file_tokens);
                if budget > 0 && projected > budget && projected > current_tokens {
                    skipped_for_budget += 1;
                    tracing::warn!(
                        path = %path.display(),
                        file_tokens,
                        projected,
                        budget,
                        "/ctx load skipped file that would exceed the measured context budget"
                    );
                    continue;
                }
                current_tokens = projected;
                loaded_tokens = loaded_tokens.saturating_add(file_tokens);

                // Add to context files tracking (bounded to prevent memory exhaustion)
                if !already_tracked {
                    self.file_tracker.context_files.push(path_str.clone());
                }

                // Install one tracked message for this path, migrating and
                // deduplicating messages created by older /ctx loads.
                replace_context_file_messages(&mut self.messages, &existing_indices, message);

                let k_tokens = file_tokens as f64 / 1000.0;
                println!(
                    "  {} {} ({:.1}k tokens)",
                    "✓".bright_green(),
                    path_str.bright_white(),
                    k_tokens
                );
                loaded += 1;
            }
        }

        let window = self.memory.context_window();
        let pct = if window > 0 {
            current_tokens as f64 / window as f64 * 100.0
        } else {
            0.0
        };
        let total_k = loaded_tokens as f64 / 1000.0;
        let window_k = window as f64 / 1000.0;
        println!();
        println!(
            "  {} Loaded {} files, {:.0}k measured tokens ({:.1}% of {:.0}k context; {} skipped for budget, {} for file limit)",
            "📊".bright_cyan(),
            loaded,
            total_k,
            pct,
            window_k,
            skipped_for_budget,
            skipped_for_limit
        );
        println!();
        Ok(loaded)
    }

    /// Reload previously loaded context files
    pub(super) async fn reload_context(&mut self) -> Result<usize> {
        let files = self.file_tracker.context_files.clone();
        if files.is_empty() {
            println!(
                "{} No files previously loaded. Use '/ctx load <pattern>' first.",
                "⚠️".bright_yellow()
            );
            return Ok(0);
        }

        let workspace = ContextWorkspace::capture();
        let mut loaded = 0;
        let budget = self.max_context_tokens;
        let mut refreshed_paths = Vec::new();
        for path_str in &files {
            let tracked_path = std::path::Path::new(path_str);
            let path = workspace.anchor(tracked_path);
            if let Err(error) = self.validate_context_path(&path) {
                warn!("Skipping unsafe context file {path_str}: {error}");
                continue;
            }
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => {
                    let raw = format!("{}{}", context_file_header(&path), content);
                    let full_content = self.sanitize_context_data(&path, &raw);
                    let new_message = context_file_message(&path, full_content);
                    let file_tokens = crate::token_count::estimate_messages_tokens(
                        std::slice::from_ref(&new_message),
                    );
                    let existing_indices = context_file_message_indices_for_paths(
                        &self.messages,
                        &[tracked_path, &path],
                    );
                    let current = crate::token_count::estimate_messages_tokens(&self.messages);
                    let old_tokens = messages_at_indices_tokens(&self.messages, &existing_indices);
                    let projected = current
                        .saturating_sub(old_tokens)
                        .saturating_add(file_tokens);
                    if budget > 0 && projected > budget && projected > current {
                        warn!(
                            path = %path.display(),
                            projected,
                            budget,
                            "Skipping context reload that would exceed measured budget"
                        );
                        continue;
                    }
                    replace_context_file_messages(
                        &mut self.messages,
                        &existing_indices,
                        new_message,
                    );
                    self.migrate_context_file_path(path_str, &path);
                    refreshed_paths.push((path_str.clone(), path.to_string_lossy().into_owned()));
                    println!(
                        "  {} {}",
                        "✓".bright_green(),
                        path.display().to_string().bright_white()
                    );
                    loaded += 1;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.remove_resolved_context_file(path_str, &path);
                }
                Err(error) => {
                    warn!(path = %path.display(), %error, "Could not reload context file")
                }
            }
        }

        // Failed, unsafe, or over-budget reads remain stale so a later reload
        // can retry them without claiming that their old content was refreshed.
        for (tracked, resolved) in &refreshed_paths {
            self.file_tracker.clear_stale(tracked);
            self.file_tracker.clear_stale(resolved);
        }

        Ok(loaded)
    }

    /// Copy all source files to clipboard
    pub(super) async fn copy_sources_to_clipboard(&self) -> Result<usize> {
        let output = self.source_context_for_copy().await;
        let size = output.len();

        crate::util::copy_to_clipboard(&output).await?;

        Ok(size)
    }

    /// Build the exact payload used by `/ctx copy`. Kept separate from the
    /// clipboard side effect so root selection and source filtering can be
    /// regression-tested without depending on a desktop clipboard service.
    pub(super) async fn source_context_for_copy(&self) -> String {
        let workspace = ContextWorkspace::capture();
        let extensions = vec!["rs".to_string(), "toml".to_string()];
        let paths = collect_context_paths(workspace.walk_root(), extensions).await;
        let mut output = String::new();

        for path in paths {
            if let Err(error) = self.validate_context_path(&path) {
                warn!("Skipping unsafe context file {}: {error}", path.display());
                continue;
            }
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let raw = format!("{}{}\n", context_file_header(&path), content);
                output.push_str(&self.sanitize_context_data(&path, &raw));
            }
        }

        output
    }

    /// Expand @file references in input (e.g., "@src/main.rs" becomes file content)
    /// Also supports @directory/ to include a directory tree (max depth 3)
    /// Returns the expanded input and the list of files that were included
    pub(super) async fn expand_file_references(&self, input: &str) -> (String, Vec<String>) {
        use std::sync::LazyLock;

        static FILE_REF_RE: LazyLock<Regex> = LazyLock::new(|| {
            // Allow backslash, colon, and tilde so Windows paths like C:\Users\...\file.txt are matched
            Regex::new(r"@([a-zA-Z0-9_./\\\:\~\-]+(?:\.[a-zA-Z0-9]+)?/?)")
                .expect("Invalid file reference regex")
        });

        let mut expanded = input.to_string();
        let mut included_files = Vec::new();
        let workspace = ContextWorkspace::capture();

        for caps in FILE_REF_RE.captures_iter(input) {
            let Some(full_match) = caps.get(0).map(|m| m.as_str()) else {
                continue;
            };
            let Some(file_path) = caps.get(1).map(|m| m.as_str()) else {
                continue;
            };
            let path = workspace.anchor(std::path::Path::new(file_path));

            // Directory references are reads too.  Validate the root before
            // metadata or traversal so @/etc/ cannot bypass the same policy
            // enforced for an individual @file.
            if self.validate_context_path(&path).is_err() {
                continue;
            }

            let is_dir = tokio::fs::metadata(&path)
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false);
            if is_dir {
                // Directory reference: include a bounded tree listing (max depth 3).
                let walk_path = path.clone();
                let entries = crate::tools::workspace_root::spawn_blocking(move || {
                    let mut entries = Vec::new();
                    for entry in walkdir::WalkDir::new(walk_path)
                        .max_depth(3)
                        .follow_links(false)
                        .into_iter()
                        .filter_entry(retain_public_context_entry)
                        .filter_map(|e| e.ok())
                    {
                        let entry_path = entry.path();
                        if entry.file_type().is_file() {
                            entries.push(entry_path.to_path_buf());
                            if entries.len() >= 1_000 {
                                break;
                            }
                        }
                    }
                    entries
                })
                .await
                .unwrap_or_default();
                let mut dir_content =
                    format!("Directory tree for {}:\n", context_path_label(&path));
                let mut file_count = 0usize;
                for entry_path in entries {
                    if self.validate_context_path(&entry_path).is_err() {
                        continue;
                    }
                    dir_content.push_str("  ");
                    dir_content.push_str(&context_path_label(&entry_path));
                    dir_content.push('\n');
                    file_count += 1;
                }
                let dir_content = self.sanitize_context_data(&path, &dir_content);
                expanded = expanded.replacen(full_match, &dir_content, 1);
                included_files.push(format!(
                    "{}/ ({} files)",
                    file_path.trim_end_matches('/'),
                    file_count
                ));
            } else {
                let Ok(content) = tokio::fs::read_to_string(&path).await else {
                    continue;
                };
                let raw = format!(
                    "\nFile: {} ({})\n<file_content>\n{}\n</file_content>\n",
                    context_path_label(&path),
                    Self::format_file_size(content.len()),
                    content.trim()
                );
                let file_block = self.sanitize_context_data(&path, &raw);
                expanded = expanded.replacen(full_match, &file_block, 1);
                included_files.push(file_path.to_string());
            }
        }

        (expanded, included_files)
    }

    /// Format file size for display
    pub(super) fn format_file_size(bytes: usize) -> String {
        if bytes >= 1024 * 1024 {
            format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
        } else if bytes >= 1024 {
            format!("{:.1}KB", bytes as f64 / 1024.0)
        } else {
            format!("{}B", bytes)
        }
    }

    /// Compress context to reduce token usage
    pub(super) async fn compress_context(&mut self) -> Result<usize> {
        let before = self.compressor.estimate_tokens(&self.messages);

        if !self.compressor.should_compress(&self.messages) {
            println!(
                "{} Context is within limits, no compression needed",
                "ℹ️".bright_cyan()
            );
            return Ok(0);
        }

        println!("{} Compressing context...", "🗜️".bright_cyan());

        let before_messages = self.messages.len();
        let (compressed, _usage) = self
            .compressor
            .compress_with_task(&self.client, &self.messages, self.current_task_text())
            .await?;
        self.messages = compressed;
        self.ensure_task_anchor_present();
        // Account the summarizer LLM call against the budget.
        // Delta-add (never total = input + output): after a resume, `total`
        // carries the restored prior-run budget whose input/output split was
        // not persisted.
        self.sync_api_usage();

        let after = self.compressor.estimate_tokens(&self.messages);
        self.log_context_compression_event(super::session_log::ContextCompressionLogDetails {
            strategy: "summary",
            success: after < before,
            before_messages,
            after_messages: self.messages.len(),
            before_tokens: before,
            after_tokens: after,
            threshold: self.compressor.compression_threshold(),
            error: None,
        });
        let saved = before.saturating_sub(after);
        let pct = if before > 0 {
            saved as f64 / before as f64 * 100.0
        } else {
            0.0
        };

        println!(
            "{} Compressed: {} → {} tokens ({:.1}% reduction)",
            "✓".bright_green(),
            before.to_string().bright_yellow(),
            after.to_string().bright_green(),
            pct
        );

        Ok(saved)
    }
}
