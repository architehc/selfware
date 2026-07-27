use anyhow::Result;
use colored::*;
use regex::Regex;

use super::*;

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
            .filter(|f| self.file_tracker.stale_files.contains(f.as_str()))
            .cloned()
            .collect();

        if stale_in_context.is_empty() {
            self.file_tracker.stale_files.clear();
            return 0;
        }

        let mut refreshed = 0;
        for path_str in &stale_in_context {
            let file_marker = format!("// FILE: {}", path_str);
            if let Ok(content) = tokio::fs::read_to_string(path_str).await {
                let file_header = format!(
                    "\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n",
                    path_str
                );
                let new_content = format!("{}{}", file_header, content);

                // Find and replace the existing message for this file
                if let Some(msg) = self
                    .messages
                    .iter_mut()
                    .find(|m| m.role == "user" && m.content.contains(&file_marker))
                {
                    msg.content = crate::api::types::MessageContent::Text(new_content);
                    refreshed += 1;
                }
            }
        }

        // Clear the stale set for refreshed files
        for path_str in &stale_in_context {
            self.file_tracker.stale_files.remove(path_str);
        }

        refreshed
    }

    /// Clear all context (messages and memory)
    pub(super) fn clear_context(&mut self) {
        self.messages.retain(|m| m.role == "system");
        self.memory.clear();
        self.file_tracker.context_files.clear();
        self.file_tracker.stale_files.clear();
        self.clear_task_state_memory();
    }

    /// Load files matching pattern into context
    pub(super) async fn load_files_to_context(&mut self, pattern: &str) -> Result<usize> {
        use walkdir::WalkDir;

        let mut loaded = 0;
        let mut total_tokens = 0usize;
        let extensions: Vec<&str> = if pattern == "." || pattern == "*" {
            vec!["rs", "toml", "md", "ts", "tsx", "js", "jsx", "py", "go"]
        } else {
            pattern
                .split(',')
                .map(|s| s.trim().trim_start_matches('.'))
                .collect()
        };

        // Pre-scan: estimate total tokens from file sizes before loading.
        // walkdir performs blocking I/O, so run it on the blocking pool.
        let extensions_owned: Vec<String> = extensions.iter().map(|s| s.to_string()).collect();
        let (estimated_tokens, file_count) = tokio::task::spawn_blocking(move || {
            let mut estimated_tokens: usize = 0;
            let mut file_count: usize = 0;
            for entry in WalkDir::new(".").into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let p = entry.path().display().to_string();
                    if p.contains("/target/")
                        || p.contains("/node_modules/")
                        || p.contains("/.git/")
                        || p.contains("/.worktrees/")
                        || p.contains("/__pycache__/")
                    {
                        continue;
                    }
                    let ext = entry
                        .path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    if extensions_owned.iter().any(|e| e == ext) {
                        if let Ok(meta) = entry.metadata() {
                            // Rough estimate: ~4 chars per token
                            estimated_tokens += meta.len() as usize / 4;
                            file_count += 1;
                        }
                    }
                }
            }
            (estimated_tokens, file_count)
        })
        .await
        .unwrap_or((0, 0));

        let budget = self.memory.context_window();
        if budget > 0 && estimated_tokens > budget {
            println!(
                "{} Estimated {} tokens from {} files exceeds context budget of {}. \
                 Use '/ctx load <specific-dir>' to load a subset.",
                "❌".bright_red(),
                estimated_tokens,
                file_count,
                budget
            );
            return Ok(0);
        }
        if let Some(pct) = (estimated_tokens * 100).checked_div(budget) {
            if pct > 75 {
                tracing::warn!(
                    "/ctx load: estimated {} tokens from {} files (~{}% of context budget). \
                     Consider loading specific subdirectories instead.",
                    estimated_tokens,
                    file_count,
                    pct
                );
                println!(
                    "{} Loading {} files (~{} tokens, ~{}% of budget). Large context may degrade performance.",
                    "⚠️".bright_yellow(),
                    file_count,
                    estimated_tokens,
                    pct
                );
            }
        }

        println!();
        println!(
            "{} Loading files with extensions: {}",
            "📂".bright_cyan(),
            extensions.join(", ").bright_yellow()
        );
        println!();

        // Collect matching paths on the blocking pool, then read them asynchronously.
        let extensions_owned: Vec<String> = extensions.iter().map(|s| s.to_string()).collect();
        let paths = tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            for entry in WalkDir::new(".")
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let path = entry.path().to_path_buf();
                let path_str = path.display().to_string();

                // Skip build artifacts and hidden dirs
                if path_str.contains("/target/")
                    || path_str.contains("/node_modules/")
                    || path_str.contains("/.git/")
                    || path_str.contains("/.worktrees/")
                    || path_str.contains("/__pycache__/")
                {
                    continue;
                }

                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if extensions_owned.iter().any(|e| e == ext) {
                    out.push(path);
                }
            }
            out
        })
        .await
        .unwrap_or_default();

        for path in paths {
            let path_str = path.display().to_string();
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let file_header = format!("\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n", path_str);
                let full_content = format!("{}{}", file_header, content);
                let file_tokens =
                    crate::token_count::estimate_tokens_with_overhead(&full_content, 4);
                total_tokens += file_tokens;

                // Add to context files tracking (bounded to prevent memory exhaustion)
                const MAX_CONTEXT_FILES: usize = 10_000;
                if !self.file_tracker.context_files.contains(&path_str)
                    && self.file_tracker.context_files.len() < MAX_CONTEXT_FILES
                {
                    self.file_tracker.context_files.push(path_str.clone());
                }

                // Add as user message with file content
                self.messages.push(Message::user(full_content));

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
            total_tokens as f64 / window as f64 * 100.0
        } else {
            0.0
        };
        let total_k = total_tokens as f64 / 1000.0;
        let window_k = window as f64 / 1000.0;
        println!();
        println!(
            "  {} Loaded {} files, ~{:.0}k tokens ({:.1}% of {:.0}k context)",
            "📊".bright_cyan(),
            loaded,
            total_k,
            pct,
            window_k
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

        // Remove only messages that contain file content (// FILE: headers)
        // Keep all conversation messages intact
        self.messages
            .retain(|m| !(m.role == "user" && m.content.contains("// FILE: ")));

        let mut loaded = 0;
        for path_str in &files {
            if let Ok(content) = tokio::fs::read_to_string(path_str).await {
                let file_header = format!("\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n", path_str);
                self.messages
                    .push(Message::user(format!("{}{}", file_header, content)));
                println!("  {} {}", "✓".bright_green(), path_str.bright_white());
                loaded += 1;
            }
        }

        // Clear stale tracking since we just refreshed everything
        self.file_tracker.stale_files.clear();

        Ok(loaded)
    }

    /// Copy all source files to clipboard
    pub(super) async fn copy_sources_to_clipboard(&self) -> Result<usize> {
        use std::process::Stdio;
        use walkdir::WalkDir;

        let mut output = String::new();
        let extensions = ["rs", "toml"];

        // Directory traversal is blocking I/O; collect matching paths on the blocking pool
        // and then read their contents asynchronously.
        let paths = tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            for entry in WalkDir::new(".")
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let path = entry.path().to_path_buf();
                let path_str = path.display().to_string();

                if path_str.contains("/target/")
                    || path_str.contains("/.git/")
                    || path_str.contains("/.worktrees/")
                {
                    continue;
                }

                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if extensions.contains(&ext) {
                    out.push(path);
                }
            }
            out
        })
        .await
        .unwrap_or_default();

        for path in paths {
            let path_str = path.display().to_string();
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                output.push_str(&format!("\n// ═══════════════════════════════════════════\n// FILE: {}\n// ═══════════════════════════════════════════\n{}\n", path_str, content));
            }
        }

        let size = output.len();

        // Try xclip first, then xsel, then wl-copy (Wayland)
        let clipboard_cmd = if tokio::process::Command::new("which")
            .arg("xclip")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(("xclip", vec!["-selection", "clipboard"]))
        } else if tokio::process::Command::new("which")
            .arg("xsel")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(("xsel", vec!["--clipboard", "--input"]))
        } else if tokio::process::Command::new("which")
            .arg("wl-copy")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Some(("wl-copy", vec![]))
        } else {
            None
        };

        if let Some((cmd, args)) = clipboard_cmd {
            let mut child = tokio::process::Command::new(cmd)
                .args(&args)
                .stdin(Stdio::piped())
                .spawn()?;

            if let Some(stdin) = child.stdin.as_mut() {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(output.as_bytes()).await?;
            }
            child.wait().await?;
        } else {
            return Err(anyhow::anyhow!(
                "No clipboard tool found (xclip, xsel, or wl-copy)"
            ));
        }

        Ok(size)
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

        for caps in FILE_REF_RE.captures_iter(input) {
            let Some(full_match) = caps.get(0).map(|m| m.as_str()) else {
                continue;
            };
            let Some(file_path) = caps.get(1).map(|m| m.as_str()) else {
                continue;
            };
            let path = std::path::Path::new(file_path);

            let is_dir = tokio::fs::metadata(path)
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false);
            if is_dir {
                // Directory reference: include tree listing + file contents (max depth 3)
                let file_path_owned = file_path.to_string();
                let (dir_content, file_count) = tokio::task::spawn_blocking(move || {
                    let mut dir_content = format!("Directory tree for {}:\n```\n", file_path_owned);
                    let mut file_count = 0;
                    for entry in walkdir::WalkDir::new(&file_path_owned)
                        .max_depth(3)
                        .into_iter()
                        .filter_map(|e| e.ok())
                    {
                        let entry_path = entry.path();
                        let display = entry_path.display().to_string();
                        if display.contains("/target/")
                            || display.contains("\\target\\")
                            || display.contains("/.git/")
                            || display.contains("\\.git\\")
                            || display.contains("/node_modules/")
                            || display.contains("\\node_modules\\")
                        {
                            continue;
                        }
                        if entry.file_type().is_file() {
                            dir_content.push_str(&format!("  {}\n", display));
                            file_count += 1;
                        }
                    }
                    dir_content.push_str("```\n");
                    (dir_content, file_count)
                })
                .await
                .unwrap_or_default();
                expanded = expanded.replacen(full_match, &dir_content, 1);
                included_files.push(format!(
                    "{}/ ({} files)",
                    file_path.trim_end_matches('/'),
                    file_count
                ));
            } else if let Ok(content) = tokio::fs::read_to_string(file_path).await {
                let file_block = format!(
                    "\n```{} ({})\n{}\n```\n",
                    file_path,
                    Self::format_file_size(content.len()),
                    content.trim()
                );
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

        let (compressed, usage) = self
            .compressor
            .compress(&self.client, &self.messages)
            .await?;
        self.messages = compressed;
        // Account the summarizer LLM call against the budget.
        // Delta-add (never total = input + output): after a resume, `total`
        // carries the restored prior-run budget whose input/output split was
        // not persisted.
        self.cumulative_token_usage.input += usage.prompt_tokens;
        self.cumulative_token_usage.output += usage.completion_tokens;
        self.cumulative_token_usage.total += usage.prompt_tokens + usage.completion_tokens;
        if let Some(cost) = usage.cost {
            self.cumulative_cost_usd += cost;
        }

        let after = self.compressor.estimate_tokens(&self.messages);
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
