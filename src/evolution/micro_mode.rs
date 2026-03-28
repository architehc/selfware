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

use super::ast_tools;
use super::tournament::Hypothesis;
use std::path::{Path, PathBuf};

/// Detect if we should use micro mode based on model name
pub fn is_micro_model(model_name: &str) -> bool {
    let name_lower = model_name.to_lowercase();
    // Check for micro model sizes - use negative checks to avoid matching "32b" as "2b" or "3b"
    name_lower.contains("0.8b")
        || name_lower.contains("1.5b")
        || (name_lower.contains("1b")
            && !name_lower.contains("10b")
            && !name_lower.contains("11b")
            && !name_lower.contains("12b")
            && !name_lower.contains("13b")
            && !name_lower.contains("14b")
            && !name_lower.contains("15b")
            && !name_lower.contains("16b")
            && !name_lower.contains("17b")
            && !name_lower.contains("18b")
            && !name_lower.contains("19b"))
        || (name_lower.contains("2b")
            && !name_lower.contains("20b")
            && !name_lower.contains("21b")
            && !name_lower.contains("22b")
            && !name_lower.contains("23b")
            && !name_lower.contains("24b")
            && !name_lower.contains("25b")
            && !name_lower.contains("26b")
            && !name_lower.contains("27b")
            && !name_lower.contains("28b")
            && !name_lower.contains("29b")
            && !name_lower.contains("12b")
            && !name_lower.contains("32b"))
        || (name_lower.contains("3b")
            && !name_lower.contains("30b")
            && !name_lower.contains("31b")
            && !name_lower.contains("32b")
            && !name_lower.contains("33b")
            && !name_lower.contains("34b")
            && !name_lower.contains("35b")
            && !name_lower.contains("36b")
            && !name_lower.contains("37b")
            && !name_lower.contains("38b")
            && !name_lower.contains("39b")
            && !name_lower.contains("13b")
            && !name_lower.contains("23b"))
        || name_lower.contains("tiny")
        || name_lower.contains("small")
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

/// Get the patch length for validation
fn patch_len(hypothesis: &Hypothesis) -> usize {
    hypothesis.patch.len()
}

/// Check if a file content is small enough for micro mode
pub fn is_micro_safe_file(content: &str) -> bool {
    content.len() <= MICRO_MAX_CONTEXT_CHARS / MICRO_MAX_FILES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_micro_model() {
        assert!(is_micro_model("qwen3.5-0.8b"));
        assert!(is_micro_model("Qwen3.5-1.5B-Instruct"));
        assert!(is_micro_model("Llama-3.2-1B"));
        assert!(!is_micro_model("qwen3.5-32b"));
        assert!(!is_micro_model("gpt-4"));
    }

    #[test]
    fn test_build_micro_system_prompt() {
        let prompt = build_micro_system_prompt(5);
        assert!(prompt.contains("Generate 3")); // Clamped to 3
        assert!(prompt.contains("JSON array"));
        assert!(prompt.contains("/no_think"));
    }

    #[test]
    fn test_validate_micro_hypothesis_too_many_edits() {
        // This would need a real Hypothesis with JSON patch
        // Skipping for now - would need mock data
    }
}
