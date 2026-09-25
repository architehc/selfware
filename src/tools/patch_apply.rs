use super::workspace_root::CommandRootExt;
use super::Tool;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::io::Write;
use tempfile::NamedTempFile;

/// True when the diff creates, converts, or retargets a symbolic link (git
/// file mode 120000).
///
/// Unified diffs encode symlinks as file entries with mode `120000`; without
/// this check `git apply` happily materialises an in-repo symlink whose
/// target can be any host path (`link:/etc/shadow`). Path validation of the
/// leaf filename never sees the target, so the entry itself must be refused.
///
/// The default is to deny symlink-touching diffs ENTIRELY (fail-closed): a
/// newly created symlink and a retargeted existing symlink are equally able
/// to point at sensitive host files, and every later tool that reads through
/// the project root would then follow it. There is deliberately no opt-out —
/// if a legitimate symlink workflow ever appears, an explicit allowlist for
/// *known* in-repo targets should be designed, not added as a blanket flag.
fn diff_touches_symlink_mode(diff: &str) -> bool {
    diff.lines().any(|line| {
        (line.starts_with("new file mode ")
            || line.starts_with("old mode ")
            || line.starts_with("new mode "))
            && line.ends_with("120000")
    })
}

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

/// Parse the old/new line counts of a hunk header `@@ -a[,b] +c[,d] @@`.
fn hunk_counts(header: &str) -> Option<(usize, usize)> {
    let rest = header.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let new = rest.split_once(" @@")?.0;
    let count = |range: &str| -> Option<usize> {
        match range.split_once(',') {
            Some((_, n)) => n.parse().ok(),
            None => Some(1),
        }
    };
    Some((count(old)?, count(new)?))
}

/// A diff whose hunk lines carry file_read's line-number prefixes
/// (` <pad>42<TAB>code`, `-<pad>42<TAB>code`), with those prefixes removed.
///
/// Returns `None` unless EVERY context and removed line of every hunk has
/// the prefix (a genuine diff of numbered-looking content is left alone);
/// added lines lose a prefix only where they carry one, since lines the
/// model wrote fresh usually have none. Hunk bodies are delimited by the
/// header's line counts, so header-like content lines (`--- x`) inside a
/// hunk are handled as content.
fn strip_numbered_diff(diff: &str) -> Option<String> {
    use super::line_numbers::numbered_prefix_len;
    let mut out = String::with_capacity(diff.len());
    let mut old_left = 0usize;
    let mut new_left = 0usize;
    let mut changed = false;
    for segment in diff.split_inclusive('\n') {
        if old_left == 0 && new_left == 0 {
            if segment.starts_with("@@ ") {
                let (o, n) = hunk_counts(segment.trim_end())?;
                old_left = o;
                new_left = n;
            }
            out.push_str(segment);
            continue;
        }
        let marker = segment.chars().next()?;
        let body = &segment[marker.len_utf8()..];
        match marker {
            ' ' | '-' => {
                let n = numbered_prefix_len(body)?;
                if marker == ' ' {
                    old_left = old_left.checked_sub(1)?;
                    new_left = new_left.checked_sub(1)?;
                } else {
                    old_left = old_left.checked_sub(1)?;
                }
                out.push(marker);
                out.push_str(&body[n..]);
                changed = true;
            }
            '+' => {
                new_left = new_left.checked_sub(1)?;
                out.push('+');
                match numbered_prefix_len(body) {
                    Some(n) => out.push_str(&body[n..]),
                    None => out.push_str(body),
                }
            }
            '\\' => out.push_str(segment),
            _ => return None,
        }
    }
    changed.then_some(out)
}

/// Build a sanitized `git apply` invocation: the tool applies
/// project-controlled diffs, so the child must not inherit host credentials
/// (see `safety::process_env`). `kill_on_drop` ensures a dropped future (or
/// early return) cannot leave a cached child holding repo locks.
fn git_apply_command(args: &[&str]) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("git");
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    cmd.in_workspace_root();
    cmd.kill_on_drop(true);
    cmd.args(args);
    cmd
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

        // Fail-closed symlink guard: refuse ANY hunk that creates, converts,
        // or retargets a symlink (mode 120000) before `git apply` can
        // materialise it — an in-repo symlink can point at /etc/shadow or any
        // other host file, and leaf-filename validation cannot see the target.
        if diff_touches_symlink_mode(diff) {
            anyhow::bail!(
                "patch_apply refuses diffs that create or retarget symlinks (mode 120000): \
                 an in-repo symlink can point at sensitive host files"
            );
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
        let check_output = git_apply_command(&["apply", "--check", &temp_path_str])
            .output()
            .await;

        // A diff built from file_read output may carry its line-number
        // prefixes. Only when the diff does not apply as written, and every
        // context/removed line is prefixed, is the de-numbered diff tried.
        let check_passed = matches!(check_output, Ok(ref out) if out.status.success());
        let mut numbered_temp: Option<NamedTempFile> = None;
        if !check_passed {
            if let Some(stripped) = strip_numbered_diff(diff) {
                let mut t = NamedTempFile::new()?;
                t.write_all(stripped.as_bytes())?;
                let t_path = t.path().to_string_lossy().to_string();
                let stripped_check = git_apply_command(&["apply", "--check", &t_path])
                    .output()
                    .await;
                if matches!(stripped_check, Ok(ref out) if out.status.success()) {
                    numbered_temp = Some(t);
                }
            }
        }
        let prefixes_stripped = numbered_temp.is_some();

        let applied = match check_output {
            Ok(ref out) if out.status.success() => {
                // Check passed — apply for real
                let apply_out = git_apply_command(&["apply", &temp_path_str]).output().await;
                matches!(apply_out, Ok(ref o) if o.status.success())
            }
            _ if prefixes_stripped => {
                let t_path = numbered_temp
                    .as_ref()
                    .map(|t| t.path().to_string_lossy().to_string())
                    .unwrap_or_default();
                let apply_out = git_apply_command(&["apply", &t_path]).output().await;
                matches!(apply_out, Ok(ref o) if o.status.success())
            }
            _ if allow_3way => {
                // Try 3-way merge fallback
                let apply3_out = git_apply_command(&["apply", "-3", &temp_path_str])
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

        let mut result = serde_json::json!({
            "success": true,
            "files_changed": files,
            "insertions": insertions,
            "deletions": deletions
        });
        if prefixes_stripped {
            result["line_number_prefixes_stripped"] = Value::Bool(true);
            result["note"] = Value::String(
                "line-number prefixes stripped: the diff's context/removed lines carried \
                 file_read's `N<TAB>` line-number metadata; it was applied without it"
                    .to_string(),
            );
        }
        Ok(result)
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::file_write()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/patch_apply/patch_apply_test.rs"]
mod tests;
