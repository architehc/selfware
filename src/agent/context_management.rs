use serde_json::Value;

use super::*;

impl Agent {
    // =========================================================================
    // Context Management
    // =========================================================================

    fn is_critical_context_message(message: &Message) -> bool {
        if message
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty())
        {
            return true;
        }

        if message.role == "tool" {
            return true;
        }

        let text = message.content.text();
        message.role == "user"
            && (text.contains("<tool_result>")
                || text.contains("STUCK LOOP DETECTED")
                || text.contains("NO ACTION LOOP DETECTED")
                || text.contains("NO ACTION DETECTED AGAIN")
                || text.contains("selfware_system_directive")
                || text.contains("Use a tool NOW")
                || text.contains("FAILURE #")
                || text.contains("Attempt ")
                || text.contains("Your tool call was malformed")
                || text.contains("RETRY SUPPRESSED")
                || text.contains("TOOL INPUT RECOVERY")
                || text.contains("Repeated unchanged reread blocked"))
    }

    fn has_kept_tool_result_for_call(
        messages: &[Message],
        keep: &[bool],
        assistant_idx: usize,
        tool_call_id: &str,
    ) -> bool {
        messages
            .iter()
            .enumerate()
            .skip(assistant_idx + 1)
            .any(|(idx, message)| {
                keep[idx]
                    && message.role == "tool"
                    && message.tool_call_id.as_deref() == Some(tool_call_id)
            })
    }

    fn has_kept_prior_assistant_call(
        messages: &[Message],
        keep: &[bool],
        tool_idx: usize,
        tool_call_id: &str,
    ) -> bool {
        messages
            .iter()
            .enumerate()
            .take(tool_idx)
            .rev()
            .any(|(idx, message)| {
                keep[idx]
                    && message
                        .tool_calls
                        .as_ref()
                        .is_some_and(|calls| calls.iter().any(|call| call.id == tool_call_id))
            })
    }

    fn enforce_tool_call_pair_invariants(messages: &[Message], keep: &mut [bool]) {
        let original_keep = keep.to_vec();

        for (idx, message) in messages.iter().enumerate() {
            if !original_keep[idx] {
                continue;
            }

            let Some(calls) = &message.tool_calls else {
                continue;
            };
            if calls.is_empty() {
                continue;
            }

            let has_all_results = calls.iter().all(|call| {
                Self::has_kept_tool_result_for_call(messages, &original_keep, idx, &call.id)
            });
            if !has_all_results {
                keep[idx] = false;
            }
        }

        for (idx, message) in messages.iter().enumerate() {
            if !keep[idx] || message.role != "tool" {
                continue;
            }

            let Some(tool_call_id) = message.tool_call_id.as_deref() else {
                keep[idx] = false;
                continue;
            };

            if !Self::has_kept_prior_assistant_call(messages, keep, idx, tool_call_id) {
                keep[idx] = false;
            }
        }
    }

    /// Trim the message history so total estimated tokens stay within
    /// `max_context_tokens`. Removes the oldest non-system messages first.
    pub(super) fn trim_message_history(&mut self) {
        // Use the same estimator as the API (includes tool_calls tokens)
        // This ensures trim budget matches the actual API input_tokens calculation
        use crate::tokens::estimate_messages_tokens;
        let total: usize = estimate_messages_tokens(&self.messages);
        if total <= self.max_context_tokens {
            return;
        }
        let before_messages = self.messages.len();

        // Collect per-message token counts once (O(N)) instead of recomputing
        // every iteration. Use estimate_message_tokens for per-message breakdown.
        use super::context::estimate_message_tokens;
        let token_counts: Vec<usize> = self.messages.iter().map(estimate_message_tokens).collect();
        let mut pinned_critical: std::collections::HashSet<usize> = self
            .messages
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, message)| Self::is_critical_context_message(message))
            .take(20)
            .map(|(idx, _)| idx)
            .collect();

        // Always pin the FIRST user message — the original task. pinned_critical
        // above only keeps the 20 most-recent criticals, so on a long run the
        // original objective is neither system nor recent-critical and gets
        // evicted oldest-first, making the model lose the plot. Protect it.
        if let Some(first_user) = self.messages.iter().position(|m| m.role == "user") {
            pinned_critical.insert(first_user);
        }

        // Walk non-system messages oldest-first and mark them for removal until
        // the total fits within budget. Prefer removing non-critical messages
        // first so recent tool results and failure guidance survive trimming.
        let mut remaining = total;
        let mut keep = vec![true; self.messages.len()];
        for (i, tokens) in token_counts.iter().enumerate() {
            if remaining <= self.max_context_tokens {
                break;
            }
            if self.messages[i].role != "system" && !pinned_critical.contains(&i) {
                keep[i] = false;
                remaining -= tokens;
            }
        }

        for (i, tokens) in token_counts.iter().enumerate() {
            if remaining <= self.max_context_tokens {
                break;
            }
            if self.messages[i].role != "system" && keep[i] {
                keep[i] = false;
                remaining -= tokens;
            }
        }

        Self::enforce_tool_call_pair_invariants(&self.messages, &mut keep);

        // Retain only the messages we decided to keep (single O(N) pass).
        let mut idx = 0;
        self.messages.retain(|_| {
            let k = keep[idx];
            idx += 1;
            k
        });

        // Fallback: If we're still over budget, truncate individual messages
        // that exceed 50K tokens. This handles the edge case where a single
        // message is too large to fit, even after removing all other messages.
        const MAX_MESSAGE_TOKENS: usize = 50_000;
        let mut remaining = self.estimate_messages_tokens();
        if remaining > self.max_context_tokens {
            // Pre-compute which messages need truncation to avoid borrow issues
            let truncate_indices: Vec<usize> = self
                .messages
                .iter()
                .enumerate()
                .filter(|(_, msg)| {
                    msg.role != "system" && estimate_message_tokens(msg) > MAX_MESSAGE_TOKENS
                })
                .map(|(i, _)| i)
                .collect();

            for idx in truncate_indices {
                if remaining <= self.max_context_tokens {
                    break;
                }
                let msg_tokens = estimate_message_tokens(&self.messages[idx]);
                if msg_tokens > MAX_MESSAGE_TOKENS {
                    // Truncate this message to MAX_MESSAGE_TOKENS
                    let current_text = self.messages[idx].content.text().to_string();
                    let current_chars: Vec<char> = current_text.chars().collect();
                    let target_chars = (current_chars.len() as f64
                        * (MAX_MESSAGE_TOKENS as f64 / msg_tokens as f64))
                        as usize;
                    let truncated: String = current_chars
                        .into_iter()
                        .take(target_chars)
                        .collect::<String>()
                        + "\n...[truncated to fit context budget]";
                    self.messages[idx].content = crate::api::types::MessageContent::Text(truncated);
                    remaining = self.estimate_messages_tokens();
                }
            }
        }

        let after_messages = self.messages.len();
        let after_tokens = self.estimate_messages_tokens();
        let removed_messages = before_messages.saturating_sub(after_messages);
        if removed_messages > 0 {
            self.log_context_trim_event(
                before_messages,
                after_messages,
                total,
                after_tokens,
                removed_messages,
            );
        }
    }

    /// Walk the project directory and register all files at L1 in the context map.
    pub(super) async fn build_l1_project_tree(&mut self) {
        use walkdir::WalkDir;

        let root = super::current_project_root();
        let entries = tokio::task::spawn_blocking(move || {
            let mut entries = Vec::new();
            for entry in WalkDir::new(&root)
                .max_depth(10)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    // Skip hidden dirs, build artifacts, and other non-source directories.
                    if name.starts_with('.') {
                        return false;
                    }
                    !matches!(
                        name.as_ref(),
                        "target"
                            | "node_modules"
                            | ".venv"
                            | "__pycache__"
                            | ".mypy_cache"
                            | "vendor"
                            | "dist"
                            | "build"
                            | "out"
                            | "pkg"
                            // ML/data directories that pollute agent context
                            | "Hunyuan3D-2"
                            | "TRELLIS.2"
                            | "instantmesh_repo"
                            | "models"
                            | "data"
                            | ".cache"
                            | "gen3d_outputs"
                            | "hunyuan3d_outputs"
                            | "trellis2_outputs"
                            | "calibration"
                    )
                })
            {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry
                    .path()
                    .strip_prefix(&root)
                    .unwrap_or(entry.path())
                    .to_path_buf();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                entries.push((path, size));
            }
            entries
        })
        .await
        .unwrap_or_default();

        let mut count = 0usize;
        for (path, size) in entries {
            self.context_map.register_tree_entry(path, size);
            count += 1;
        }
        tracing::info!(
            "L1 project tree: {} files registered, {} tokens",
            count,
            self.context_map.total_tokens()
        );
    }

    /// For Review modality: auto-load L2 skeletons for all source files
    /// so the model can see the codebase structure without reading every file.
    pub(super) async fn auto_load_skeletons_for_review(&mut self) {
        use super::context_map::extract_rust_skeleton;
        use crate::evolve::ContextMode;

        let root = super::current_project_root();
        let files_at_tree: Vec<std::path::PathBuf> = self
            .context_map
            .files_at_level(ContextMode::Map)
            .iter()
            .filter(|p| p.to_string_lossy().ends_with(".rs"))
            .map(|p| p.to_path_buf())
            .collect();

        let mut loaded = 0usize;
        // Cap skeleton loading to avoid bloating the system prompt.
        // 30 files gives a good overview without overwhelming the context.
        const MAX_SKELETON_FILES: usize = 30;
        // Also cap total skeleton tokens to ~15K (reasonable for any context size).
        const MAX_SKELETON_TOKENS: usize = 15_000;
        let mut total_skeleton_tokens = 0usize;
        for path in files_at_tree {
            if loaded >= MAX_SKELETON_FILES || total_skeleton_tokens >= MAX_SKELETON_TOKENS {
                tracing::info!(
                    "Skeleton cap reached: {} files, {} tokens",
                    loaded,
                    total_skeleton_tokens
                );
                break;
            }

            // Check budget before loading.
            let estimate = self
                .context_map
                .can_load(&path, ContextMode::Lite)
                .await;
            if !estimate.fits {
                tracing::info!(
                    "Skeleton budget exhausted after {} files ({:.0}% used)",
                    loaded,
                    estimate.usage_pct * 100.0
                );
                break;
            }

            let full_path = root.join(&path);
            let content = match tokio::fs::read_to_string(&full_path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let skeleton = extract_rust_skeleton(&path, &content);
            if skeleton.items.is_empty() {
                continue;
            }
            total_skeleton_tokens += skeleton.token_count;
            self.context_map.load_skeleton(&path, skeleton);
            loaded += 1;
        }

        // Inject skeletons into the system prompt so the model has the codebase
        // overview at the START of context (RoPE high-attention zone).
        // This avoids consecutive user messages and ensures the model sees it.
        if loaded > 0 {
            let mut skeleton_text = format!(
                "\n\n## Codebase Overview ({} Rust files, function/struct signatures)\n\
                 You already have the full project structure below. \
                 Use `file_read` only for files you need to see in full detail.\n\n",
                loaded
            );
            let skeleton_paths: Vec<std::path::PathBuf> = self
                .context_map
                .files_at_level(ContextMode::Lite)
                .iter()
                .map(|p| p.to_path_buf())
                .collect();
            for path in &skeleton_paths {
                if let Some(skel) = self.context_map.skeleton(path) {
                    skeleton_text.push_str(&skel.render());
                    skeleton_text.push('\n');
                }
            }
            // Append to the system message (first message).
            if let Some(first) = self.messages.first_mut() {
                if first.role == "system" {
                    first.content = format!("{}\n{}", first.content, skeleton_text).into();
                }
            }
        }

        let stats = self.context_map.stats();
        tracing::info!(
            "Review mode: loaded {} skeletons ({} tokens, {:.0}% of budget)",
            stats.l2_count,
            stats.l2_tokens,
            self.context_map.usage_fraction() * 100.0
        );
    }

    /// Track a file_read in the context map at L3 (full content).
    pub(super) async fn track_file_read_in_context_map(&mut self, path: &str, content: &str) {
        use std::path::Path;
        let p = Path::new(path);
        // Estimate before loading.
        let estimate = self
            .context_map
            .can_load(p, crate::evolve::ContextMode::Full)
            .await;
        if !estimate.fits {
            // Auto-compress to make room.
            let needed = estimate
                .estimated_tokens
                .saturating_sub(self.context_map.remaining());
            let freed = self.context_map.compress_to_fit(needed);
            tracing::debug!(
                "Auto-compressed {} tokens to fit {} (needed {})",
                freed,
                path,
                needed
            );
        }
        self.context_map.load_full(p, content.to_string());
    }

    /// Bulk-read multiple files in parallel using tokio tasks.
    /// Loads files into the context map at L3, respecting budget limits.
    /// Returns (loaded_count, skipped_count, total_tokens_added).
    pub(super) async fn parallel_bulk_read(
        &mut self,
        paths: Vec<std::path::PathBuf>,
    ) -> (usize, usize, usize) {
        use crate::evolve::ContextMode;
        use tokio::task::JoinSet;

        let root = super::current_project_root();
        let mut join_set = JoinSet::new();

        // Spawn parallel file reads (just IO, no LLM calls).
        for path in &paths {
            let full_path = root.join(path);
            let p = path.clone();
            join_set.spawn(async move {
                match tokio::fs::read_to_string(&full_path).await {
                    Ok(content) => Some((p, content)),
                    Err(_) => None,
                }
            });
        }

        // Collect results and load into context map (sequential — context_map is not Send).
        let mut loaded = 0usize;
        let mut skipped = 0usize;
        let mut tokens_added = 0usize;

        while let Some(result) = join_set.join_next().await {
            if let Ok(Some((path, content))) = result {
                // Skip if already at L3.
                if self.context_map.level_of(&path) == Some(ContextMode::Full) {
                    skipped += 1;
                    continue;
                }

                // Check budget.
                let estimate = self.context_map.can_load(&path, ContextMode::Full).await;
                if !estimate.fits {
                    // Try to compress existing content to make room.
                    let needed = estimate
                        .estimated_tokens
                        .saturating_sub(self.context_map.remaining());
                    let freed = self.context_map.compress_to_fit(needed);
                    if freed < needed {
                        skipped += 1;
                        continue; // Can't fit even after compression.
                    }
                }

                let token_count = crate::token_count::estimate_content_tokens(&content);
                self.context_map.load_full(&path, content);
                tokens_added += token_count;
                loaded += 1;
            }
        }

        tracing::info!(
            "Parallel bulk read: {} loaded, {} skipped, {} tokens ({:.0}% of budget)",
            loaded,
            skipped,
            tokens_added,
            self.context_map.usage_fraction() * 100.0
        );

        (loaded, skipped, tokens_added)
    }

    /// Generate a structured per-module summary of the codebase.
    /// Groups files by directory and produces a compact summary for each module
    /// that fits in the available context window.
    pub(super) fn generate_structured_summary(&self) -> String {
        use crate::evolve::ContextMode;
        use std::collections::BTreeMap;

        // Group files by top-level module directory.
        let mut modules: BTreeMap<String, Vec<(&std::path::Path, ContextMode, usize)>> =
            BTreeMap::new();

        let stats = self.context_map.stats();
        for level in [
            ContextMode::Full,
            ContextMode::Lite,
            ContextMode::Map,
        ] {
            for path in self.context_map.files_at_level(level.clone()) {
                let module = path
                    .components()
                    .take(2) // e.g., "src/agent" or "src/tools"
                    .collect::<std::path::PathBuf>()
                    .to_string_lossy()
                    .to_string();

                modules.entry(module).or_default().push((path, level.clone(), 0));
            }
        }

        let mut summary = format!(
            "# Codebase Summary ({} files, {}/{} tokens)\n\n",
            stats.l1_count + stats.l2_count + stats.l3_count,
            stats.total_tokens,
            stats.budget,
        );

        for (module, files) in &modules {
            let file_count = files.len();
            let l3_count = files
                .iter()
                .filter(|(_, l, _)| *l == ContextMode::Full)
                .count();
            let l2_count = files
                .iter()
                .filter(|(_, l, _)| *l == ContextMode::Lite)
                .count();

            summary.push_str(&format!(
                "## {} ({} files, {} full, {} skeleton)\n",
                module, file_count, l3_count, l2_count,
            ));

            // List key items from skeletons.
            for (path, level, _) in files {
                if *level == ContextMode::Lite {
                    if let Some(skel) = self.context_map.skeleton(path) {
                        let fn_count = skel
                            .items
                            .iter()
                            .filter(|i| {
                                matches!(i, super::context_map::SkeletonItem::Function { .. })
                            })
                            .count();
                        let struct_count = skel
                            .items
                            .iter()
                            .filter(|i| {
                                matches!(i, super::context_map::SkeletonItem::Struct { .. })
                            })
                            .count();
                        if fn_count > 0 || struct_count > 0 {
                            summary.push_str(&format!(
                                "  - {} ({} fn, {} struct, ~{} tok)\n",
                                path.display(),
                                fn_count,
                                struct_count,
                                skel.token_count,
                            ));
                        }
                    }
                } else if *level == ContextMode::Full {
                    summary.push_str(&format!("  - {} [FULL]\n", path.display()));
                }
            }
            summary.push('\n');
        }

        summary
    }

    /// Compress the context to fit within `target_tokens` by generating
    /// a structured summary and replacing old messages.
    /// Unlike the flat LLM-based compression, this is deterministic and fast.
    pub fn compress_to_structured_summary(&mut self, target_tokens: usize) {
        let current = self.estimate_messages_tokens();
        if current <= target_tokens {
            return;
        }

        let summary = self.generate_structured_summary();
        let summary_tokens = crate::token_count::estimate_content_tokens(&summary);

        tracing::info!(
            "Structured compression: {} tokens → ~{} token summary (target: {})",
            current,
            summary_tokens,
            target_tokens,
        );

        // Downgrade all L3 files to L2 to free context map space.
        let l3_files: Vec<std::path::PathBuf> = self
            .context_map
            .files_at_level(crate::evolve::ContextMode::Full)
            .iter()
            .map(|p| p.to_path_buf())
            .collect();
        for path in &l3_files {
            self.context_map.downgrade_to_skeleton(path);
        }

        // Replace old messages with the structured summary.
        // Keep: system message + last 4 messages.
        let keep_recent = 4;
        if self.messages.len() > keep_recent + 1 {
            // Preserve the ACTUAL system message by role, not just the first
            // message. If the first message isn't the system prompt, taking
            // first() kept the wrong message as "system" and silently discarded
            // the real system prompt during compression (found by GLM-5.2
            // reviewing context_management.rs; verified + fixed by Claude).
            let system_msg = self
                .messages
                .iter()
                .find(|m| m.role == "system")
                .cloned()
                .or_else(|| self.messages.first().cloned());
            let messages_before = self.messages.len();
            let recent: Vec<_> = self
                .messages
                .iter()
                .rev()
                .take(keep_recent)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let compressed_count = messages_before
                .saturating_sub(recent.len())
                .saturating_sub(usize::from(system_msg.is_some()));

            // Preserve the ORIGINAL TASK so compression down to "system + last 4 +
            // summary" doesn't drop the objective and make the model lose the plot
            // on long runs. The task is the first USER message AFTER the system
            // prompt (a user message before the system prompt is bootstrap noise).
            // Skip if it's already within the recent window.
            let system_idx = self.messages.iter().position(|m| m.role == "system");
            let original_task = self
                .messages
                .iter()
                .enumerate()
                .find(|(i, m)| m.role == "user" && system_idx.is_none_or(|s| *i > s))
                .map(|(_, m)| m.clone());
            self.messages.clear();
            if let Some(sys) = system_msg {
                self.messages.push(sys);
            }
            if let Some(task) = original_task {
                if !recent
                    .iter()
                    .any(|r| r.content.text() == task.content.text())
                {
                    self.messages.push(crate::api::types::Message::user(format!(
                        "[ORIGINAL TASK]:\n{}",
                        task.content.text()
                    )));
                }
            }
            self.messages.push(crate::api::types::Message::user(format!(
                "[STRUCTURED SUMMARY — {} earlier messages compressed]\n{}",
                compressed_count, summary
            )));
            self.messages
                .push(crate::api::types::Message::user("[RECENT CONTEXT]:"));
            self.messages.extend(recent);
        }
    }

    /// Estimate total tokens from accumulated messages (the actual context sent to API)
    pub(super) fn estimate_messages_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| {
                let text_tokens =
                    crate::token_count::estimate_tokens_with_overhead(&m.content.text_all(), 4);
                let image_tokens =
                    m.content.image_count() * crate::tokens::DEFAULT_IMAGE_TOKEN_ESTIMATE;
                text_tokens + image_tokens
            })
            .sum()
    }

    /// Best estimate of the CURRENT context size (tokens in the next request).
    ///
    /// This must reflect the assembled message set, not lifetime usage.
    /// `output::get_total_tokens()` is a cumulative process-global counter
    /// (`fetch_add` per API turn), so — because every turn re-sends the whole
    /// conversation — using it here made the reported size grow without bound and
    /// exceed the model window (observed 1332k against a 1049k window, latching
    /// compaction and the status bar at 100%). Estimate from the actual messages
    /// and memory instead (CTX-CUMULATIVE-TOKENS).
    pub(super) fn total_tokens_used(&self) -> usize {
        let msg_tokens = self.estimate_messages_tokens();
        let mem_tokens = self.memory.total_tokens();
        msg_tokens.max(mem_tokens)
    }

    pub(super) fn context_usage_pct(&self) -> f64 {
        let tokens = self.total_tokens_used();
        let window = self.memory.context_window();
        if window == 0 {
            return 0.0;
        }
        (tokens as f64 / window as f64 * 100.0).min(100.0)
    }

    /// Enhance cargo check/clippy errors with analyzer suggestions
    pub(super) fn enhance_cargo_errors(&self, result_str: &str) -> String {
        // Try to parse the result and extract errors
        if let Ok(result) = serde_json::from_str::<Value>(result_str) {
            if let Some(errors) = result.get("errors").and_then(|e| e.as_array()) {
                let raw_errors: Vec<_> = errors
                    .iter()
                    .filter_map(|e| {
                        let code = e.get("code").and_then(|c| c.as_str());
                        let message = e.get("message").and_then(|m| m.as_str())?;
                        let file = e.get("file").and_then(|f| f.as_str()).unwrap_or("unknown");
                        let line = e.get("line").and_then(|l| l.as_u64()).map(|l| l as u32);
                        let column = e.get("column").and_then(|c| c.as_u64()).map(|c| c as u32);
                        Some((code, message, file, line, column))
                    })
                    .collect();

                if !raw_errors.is_empty() {
                    let analyzed = self.error_analyzer.analyze_batch(&raw_errors);
                    let summary = self.error_analyzer.summary(&analyzed);

                    tracing::info!(
                        "Enhanced {} errors with analyzer suggestions",
                        analyzed.len()
                    );

                    return format!(
                        "{}\n\n<error_analysis>\n{}\n</error_analysis>",
                        result_str, summary
                    );
                }
            }
        }
        result_str.to_string()
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
#[path = "../../tests/unit/agent/context_management/context_management_test.rs"]
mod tests;
