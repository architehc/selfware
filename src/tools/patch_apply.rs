use super::Tool;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::io::Write;
use tempfile::NamedTempFile;

/// Parse a unified diff and return approximate stats and target paths.
///
/// File deletions (`+++ /dev/null`) target the OLD file path (`--- a/<path>`):
/// that is the path the operation actually affects, so it is what must be
/// validated and checkpointed — never `/dev/null`.
fn parse_diff_stats(diff: &str) -> (usize, usize, usize, Vec<String>) {
    let mut files = 0;
    let mut insertions = 0;
    let mut deletions = 0;
    let mut targets = Vec::new();
    let mut old_path: Option<String> = None;

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("--- ") {
            // Remember the old-file path; a following `+++ /dev/null` means
            // this file is being deleted.
            let p = rest.split('\t').next().unwrap_or("").trim();
            let p = p.strip_prefix("a/").unwrap_or(p);
            old_path = if p.is_empty() || p == "/dev/null" {
                None
            } else {
                Some(p.to_string())
            };
        } else if line.starts_with("+++ ") {
            files += 1;
            if line.starts_with("+++ /dev/null") {
                // File deletion: the old path is the operation's target.
                if let Some(old) = old_path.take() {
                    targets.push(old);
                }
            } else {
                // Extract path after "+++ b/" or "+++ "
                let path = line
                    .strip_prefix("+++ b/")
                    .or_else(|| line.strip_prefix("+++ "))
                    .unwrap_or("")
                    .split('\t')
                    .next()
                    .unwrap_or("")
                    .trim();
                if !path.is_empty() {
                    targets.push(path.to_string());
                }
            }
            old_path = None;
        } else if line.starts_with('+') && !line.starts_with("+++") && !line.starts_with("@@") {
            insertions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") && !line.starts_with("@@") {
            deletions += 1;
        }
    }

    (files, insertions, deletions, targets)
}

/// Apply a unified diff using `git apply` with validation and optional 3-way fallback.
pub struct PatchApply;

#[async_trait]
impl Tool for PatchApply {
    fn name(&self) -> &str {
        "patch_apply"
    }

    fn description(&self) -> &str {
        "Apply a unified diff patch to the working directory. Validates with git apply --check first, then tries 3-way merge on failure."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "diff": {
                    "type": "string",
                    "description": "Unified diff text to apply"
                },
                "allow_3way": {
                    "type": "boolean",
                    "default": true,
                    "description": "Try 3-way merge if direct apply fails"
                }
            },
            "required": ["diff"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let diff = args["diff"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing diff"))?;
        let allow_3way = args
            .get("allow_3way")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if diff.trim().is_empty() {
            return Err(anyhow!("diff is empty"));
        }

        let (files, insertions, deletions, targets) = parse_diff_stats(diff);

        // Validate all target paths through the safety validator
        let safety = crate::tools::file::resolve_safety_config(None);
        for path in &targets {
            // Reject absolute paths and parent-directory escapes
            if path.starts_with('/') {
                anyhow::bail!("patch_apply rejects absolute paths: {}", path);
            }
            if path.contains("..") {
                anyhow::bail!(
                    "patch_apply rejects paths with parent-directory escapes: {}",
                    path
                );
            }
            crate::tools::file::validate_tool_path(path, &safety)
                .map_err(|e| anyhow!("patch_apply path validation failed for '{}': {}", path, e))?;
        }

        // Write diff to a temporary file
        let mut temp = NamedTempFile::new()?;
        temp.write_all(diff.as_bytes())?;
        let temp_path = temp.path().to_path_buf();
        let temp_path_str = temp_path.to_string_lossy().to_string();

        // Try git apply --check first
        let check_output = tokio::process::Command::new("git")
            .args(["apply", "--check", &temp_path_str])
            .output()
            .await;

        let applied = match check_output {
            Ok(ref out) if out.status.success() => {
                // Check passed — apply for real
                let apply_out = tokio::process::Command::new("git")
                    .args(["apply", &temp_path_str])
                    .output()
                    .await;
                matches!(apply_out, Ok(ref o) if o.status.success())
            }
            _ if allow_3way => {
                // Try 3-way merge fallback
                let apply3_out = tokio::process::Command::new("git")
                    .args(["apply", "-3", &temp_path_str])
                    .output()
                    .await;
                matches!(apply3_out, Ok(ref o) if o.status.success())
            }
            _ => false,
        };

        if !applied {
            let stderr = match check_output {
                Ok(out) => String::from_utf8_lossy(&out.stderr).to_string(),
                Err(e) => format!("failed to run git apply --check: {}", e),
            };
            anyhow::bail!("git apply failed: {}", stderr);
        }

        Ok(serde_json::json!({
            "success": true,
            "files_changed": files,
            "insertions": insertions,
            "deletions": deletions
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::file_write()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/patch_apply/patch_apply_test.rs"]
mod tests;
