//! Micro Evolution Mode — For 0.8B-3B Parameter Models
//!
//! Small models struggle with:
//! - Large context windows (>4K tokens)
//! - Complex multi-file reasoning
//! - Precise JSON array formatting
//! - Multi-step search-and-replace
//!
//! This module provides simplified mutation strategies that trade
//! sophistication for reliability on resource-constrained hardware.

use super::tournament::Hypothesis;
use std::path::{Path, PathBuf};

/// Detect if we should use micro mode based on model name.
/// Matches small parameter counts as word-bounded tokens to avoid
/// false positives (e.g. "32b" matching "2b", "13b" matching "3b").
pub fn is_micro_model(model_name: &str) -> bool {
    let name_lower = model_name.to_lowercase();
    if name_lower.contains("tiny") || name_lower.contains("small") {
        return true;
    }
    let micro_sizes = ["0.8b", "1.5b", "1b", "2b", "3b", "4b"];
    for size in &micro_sizes {
        let mut search_from = 0;
        while search_from < name_lower.len() {
            if let Some(pos) = name_lower[search_from..].find(size) {
                let abs_pos = search_from + pos;
                let end_pos = abs_pos + size.len();
                // Check word boundaries: preceding char must not be alphanumeric
                let prev_ok =
                    abs_pos == 0 || !name_lower.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
                // Following char must not be alphanumeric
                let next_ok = end_pos >= name_lower.len()
                    || !name_lower.as_bytes()[end_pos].is_ascii_alphanumeric();
                if prev_ok && next_ok {
                    return true;
                }
                search_from = abs_pos + 1;
            } else {
                break;
            }
        }
    }
    false
}

/// Maximum context size for micro mode (in characters)
/// ~4K tokens ≈ 16K chars for code
pub const MICRO_MAX_CONTEXT_CHARS: usize = 12_000;

/// Maximum files to include in micro mode context
pub const MICRO_MAX_FILES: usize = 3;

/// Maximum edits per hypothesis in micro mode
/// Small models do better with single, focused changes
pub const MICRO_MAX_EDITS: usize = 2;

/// Simplified system prompt for micro models
/// - Shorter instructions
/// - More examples
/// - Explicit JSON schema
pub fn build_micro_system_prompt(population_size: usize) -> String {
    // Clamp population for micro mode - sequential evaluation
    let n = population_size.min(3);

    format!(
        r#"You are a code improvement engine. Generate {n} code mutations.

FORMAT:
Return a JSON array like this:
[{{"description": "fix off-by-one error", "edits": [{{"file": "src/foo.rs", "search": "for i in 0..n", "replace": "for i in 0..=n"}}], "target_files": ["src/foo.rs"]}}]

RULES:
1. "search" must be EXACT text from the file (copy-paste)
2. Only modify files shown in the context
3. Never modify src/evolution/, src/safety/, or benches/
4. Keep edits small - change only what's needed

/no_think"#
    )
}

/// Filter and limit mutation targets for micro mode
/// Returns the smallest files first, limited by MICRO_MAX_FILES
pub fn select_micro_targets(all_targets: &[PathBuf], repo_root: &Path) -> Vec<(PathBuf, String)> {
    let files_with_content: Vec<(PathBuf, usize, String)> = all_targets
        .iter()
        .filter_map(|p| {
            let full_path = repo_root.join(p);
            let size = std::fs::metadata(&full_path)
                .map(|m| m.len() as usize)
                .unwrap_or(usize::MAX);
            let content = std::fs::read_to_string(&full_path).ok()?;
            Some((p.clone(), size, content))
        })
        .collect();

    // Sort by size (smallest first)
    let mut files_sorted = files_with_content;
    files_sorted.sort_by_key(|(_, size, _)| *size);

    // Take top N smallest files that fit in budget
    let mut selected = Vec::new();
    let mut total_chars = 0usize;

    for (path, _size, content) in files_sorted.into_iter().take(MICRO_MAX_FILES) {
        if total_chars + content.len() > MICRO_MAX_CONTEXT_CHARS {
            break;
        }
        total_chars += content.len();
        selected.push((path, content));
    }

    selected
}

/// Build micro-mode context with aggressive truncation
pub fn build_micro_context(targets: &[(PathBuf, String)]) -> String {
    let mut context = String::with_capacity(MICRO_MAX_CONTEXT_CHARS);

    for (path, content) in targets {
        // Add line numbers for reference
        let numbered: String = content
            .lines()
            .enumerate()
            .map(|(i, line)| format!("{:3}| {}\n", i + 1, line))
            .collect();

        context.push_str(&format!("\n=== {} ===\n{}", path.display(), numbered));
    }

    context
}

/// Validate that a hypothesis is "micro-safe"
/// - No more than MICRO_MAX_EDITS
/// - Each edit is reasonably sized
pub fn validate_micro_hypothesis(hypothesis: &Hypothesis) -> Result<(), String> {
    // Parse the edits from the patch JSON
    let edits: Vec<serde_json::Value> = match serde_json::from_str(&hypothesis.patch) {
        Ok(v) => v,
        Err(_) => {
            // Legacy patch format - allow but warn
            return Ok(());
        }
    };

    if edits.len() > MICRO_MAX_EDITS {
        return Err(format!(
            "Micro mode: hypothesis has {} edits, max is {}",
            edits.len(),
            MICRO_MAX_EDITS
        ));
    }

    for (i, edit) in edits.iter().enumerate() {
        let search = edit["search"].as_str().unwrap_or("");
        let replace = edit["replace"].as_str().unwrap_or("");

        // Limit search/replace size to prevent ambiguous matches
        if search.len() > 500 {
            return Err(format!(
                "Micro mode: edit {} search too long ({} chars, max 500)",
                i,
                search.len()
            ));
        }

        if replace.len() > 1000 {
            return Err(format!(
                "Micro mode: edit {} replace too long ({} chars, max 1000)",
                i,
                replace.len()
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/micro_mode/micro_mode_test.rs"]
mod tests;
