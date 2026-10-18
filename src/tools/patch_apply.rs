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
fn push_unique_target(targets: &mut Vec<String>, path: &str) {
    let p = path.trim();
    if !p.is_empty() && p != "/dev/null" && !targets.iter().any(|t| t == p) {
        targets.push(p.to_string());
    }
}

/// Strip an optional git `a/` or `b/` diff prefix and trailing timestamp field.
fn clean_diff_header_path<'a>(raw: &'a str, prefix: &str) -> &'a str {
    let p = raw.split('\t').next().unwrap_or("").trim();
    p.strip_prefix(prefix).unwrap_or(p).trim()
}

/// Extract `(a_path, b_path)` from a `diff --git a/<old> b/<new>` header line.
///
/// Handles the standard `a/… b/…` form (splitting on the last `" b/"` occurrence)
/// and `--no-prefix` headers whose two paths contain no spaces.
fn parse_diff_git_header_paths(rest: &str) -> Option<(&str, &str)> {
    let rest = rest.trim();
    if let Some(a_rest) = rest.strip_prefix("a/") {
        let (a_path, b_path) = a_rest.rsplit_once(" b/")?;
        return Some((a_path.trim(), b_path.trim()));
    }
    let (a_path, b_path) = rest.split_once(' ')?;
    let b_path = b_path.trim();
    if b_path.contains(' ') {
        return None;
    }
    Some((
        a_path.strip_prefix("a/").unwrap_or(a_path),
        b_path.strip_prefix("b/").unwrap_or(b_path),
    ))
}

/// Reject diff header paths that use Git C-quoted syntax (e.g. `"b/\056git/config"`)
/// or contain control characters.
pub(crate) fn check_diff_header_safety(diff: &str) -> std::result::Result<(), String> {
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let trimmed = rest.trim();
            if trimmed.contains('"') || rest.chars().any(|c| c.is_control()) {
                return Err(format!(
                    "patch_apply rejects quoted or control-character diff header: {}",
                    line
                ));
            }
        } else if let Some(rest) = line
            .strip_prefix("rename from ")
            .or_else(|| line.strip_prefix("rename to "))
            .or_else(|| line.strip_prefix("copy from "))
            .or_else(|| line.strip_prefix("copy to "))
        {
            let p = rest.split('\t').next().unwrap_or("").trim();
            if p.contains('"')
                || rest.chars().any(|c| c.is_control() && c != '\t')
                || p.chars().any(|c| c.is_control())
            {
                return Err(format!(
                    "patch_apply rejects quoted or control-character rename/copy header: {}",
                    line
                ));
            }
        } else if let Some(rest) = line
            .strip_prefix("--- ")
            .or_else(|| line.strip_prefix("+++ "))
        {
            let p = rest.split('\t').next().unwrap_or("").trim();
            if p.contains('"')
                || rest.chars().any(|c| c.is_control() && c != '\t')
                || p.chars().any(|c| c.is_control())
            {
                return Err(format!(
                    "patch_apply rejects quoted or control-character file header: {}",
                    line
                ));
            }
        }
    }
    Ok(())
}

/// Parse a unified diff and return approximate stats and every path the patch
/// reads from or writes to.
///
/// Header lines carry paths in four places, and `git apply` acts on all of them:
/// - `--- a/<old>` / `+++ b/<new>` (hunk headers; deletions have `+++ /dev/null`)
/// - `rename from <old>` / `rename to <new>` (pure renames have no `---`/`+++`)
/// - `copy from <src>` / `copy to <dst>` (copies read `<src>` and write `<dst>`
///   with no `---`/`+++`)
/// - `diff --git a/<old> b/<new>` (mode-only or empty-file patches have no
///   other path header at all)
///
/// Every one of those paths — source and destination alike — must pass
/// `validate_tool_path`, or a `copy from .env` / `rename from .env` patch can
/// move a `denied_paths` secret into an allowed filename for `file_read`.
pub(crate) fn parse_diff_stats(diff: &str) -> (usize, usize, usize, Vec<String>) {
    let mut git_files = 0;
    let mut plus_files = 0;
    let mut insertions = 0;
    let mut deletions = 0;
    let mut targets = Vec::new();
    let mut in_hunk = false;

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            in_hunk = false;
            git_files += 1;
            if let Some((a_path, b_path)) = parse_diff_git_header_paths(rest) {
                push_unique_target(&mut targets, a_path);
                push_unique_target(&mut targets, b_path);
            }
        } else if let Some(rest) = line.strip_prefix("--- ") {
            in_hunk = false;
            push_unique_target(&mut targets, clean_diff_header_path(rest, "a/"));
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            in_hunk = false;
            plus_files += 1;
            push_unique_target(&mut targets, clean_diff_header_path(rest, "b/"));
        } else if line.starts_with("@@") {
            in_hunk = true;
        } else if !in_hunk {
            if let Some(rest) = line
                .strip_prefix("rename from ")
                .or_else(|| line.strip_prefix("rename to "))
                .or_else(|| line.strip_prefix("copy from "))
                .or_else(|| line.strip_prefix("copy to "))
            {
                push_unique_target(&mut targets, clean_diff_header_path(rest, ""));
            }
        } else if line.starts_with('+') && !line.starts_with("+++") {
            insertions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }

    let mut files = git_files.max(plus_files);
    if files == 0 && !targets.is_empty() {
        files = 1;
    }

    (files, insertions, deletions, targets)
}

/// Parse the old/new line counts of a hunk header `@@ -a[,b] +c[,d] @@`.
/// Per target file (the `+++ b/<path>` header): the removed/context text
/// and the added text of its hunks.
pub(crate) fn diff_sides_by_target(diff: &str) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    for line in diff.lines() {
        if let Some(target) = line.strip_prefix("+++ ") {
            let target = target.split('\t').next().unwrap_or("").trim();
            let target = target.strip_prefix("b/").unwrap_or(target).trim();
            if !target.is_empty() && target != "/dev/null" {
                out.push((target.to_string(), String::new(), String::new()));
            }
            continue;
        }
        if line.starts_with("--- ") || line.starts_with("@@") {
            continue;
        }
        let Some((_, removed, added)) = out.last_mut() else {
            continue;
        };
        if let Some(text) = line.strip_prefix('+') {
            added.push_str(text);
            added.push('\n');
        } else if let Some(text) = line.strip_prefix('-').or_else(|| line.strip_prefix(' ')) {
            removed.push_str(text);
            removed.push('\n');
        }
    }
    out
}

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

/// Cap on captured `git apply` output (256 KiB).
const MAX_PATCH_APPLY_OUTPUT_BYTES: usize = 256 * 1024;
/// Upper bound on each `git apply` invocation (60s).
const PATCH_APPLY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

async fn run_git_apply_bounded(
    args: &[&str],
) -> std::result::Result<
    crate::tools::process_guard::BoundedCommandOutput,
    crate::tools::process_guard::CommandRunError,
> {
    crate::tools::process_guard::run_command_bounded(
        git_apply_command(args),
        PATCH_APPLY_TIMEOUT,
        MAX_PATCH_APPLY_OUTPUT_BYTES,
    )
    .await
}

/// Every path `git apply` would read or write for the diff in `patch_file`,
/// taken from git's own parser rather than from a second scan of the text.
async fn git_patch_paths(patch_file: &str) -> Result<Vec<String>> {
    let mut paths: Vec<String> = Vec::new();
    for reverse in [false, true] {
        let mut args = vec!["apply", "--numstat", "-z"];
        if reverse {
            args.push("-R");
        }
        args.push(patch_file);
        let out = run_git_apply_bounded(&args)
            .await
            .map_err(|e| anyhow!("failed to run git apply --numstat: {}", e))?;
        if !out.status.success() {
            anyhow::bail!(
                "git could not parse the diff: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let stdout = std::str::from_utf8(&out.stdout)
            .map_err(|_| anyhow!("git apply --numstat reported a non-UTF-8 path"))?;
        for record in stdout.split('\0').filter(|r| !r.is_empty()) {
            let path = record
                .splitn(3, '\t')
                .nth(2)
                .ok_or_else(|| anyhow!("unexpected git apply --numstat record: {:?}", record))?;
            if !paths.iter().any(|p| p == path) {
                paths.push(path.to_string());
            }
        }
    }
    if paths.is_empty() {
        anyhow::bail!("git apply --numstat named no files for the diff");
    }
    Ok(paths)
}

/// Reject absolute paths and parent-directory escapes, then run the safety
/// validator on a path the diff touches.
fn validate_patch_path(path: &str, safety: &crate::config::SafetyConfig) -> Result<()> {
    if path.starts_with('/') {
        anyhow::bail!("patch_apply rejects absolute paths: {}", path);
    }
    if path.contains("..") {
        anyhow::bail!(
            "patch_apply rejects paths with parent-directory escapes: {}",
            path
        );
    }
    crate::tools::file::validate_tool_path(path, safety)
        .map_err(|e| anyhow!("patch_apply path validation failed for '{}': {}", path, e))
}

/// Build a sanitized, hardened `git apply` invocation: the tool applies
/// project-controlled diffs, so the child must not inherit host credentials
/// (see `safety::process_env`), and the repository's own config cannot run
/// a smudge filter or merge driver unless the repository is trusted
/// (`safety::git_exec`). Callers run it with `run_git_apply_bounded` (own
/// process group + kill-on-drop + timeout + output cap), so a dropped future
/// (or early return) kills its whole process group and cannot leave a child
/// holding repo locks.
fn git_apply_command(args: &[&str]) -> tokio::process::Command {
    let mut cmd = crate::safety::git_exec::git_command_async(
        crate::tools::workspace_root::current().path(),
        crate::safety::git_exec::GitScope::UserOperation,
    );
    cmd.in_workspace_root();
    cmd.args(args);
    cmd
}

/// Apply a unified diff using `git apply` with validation and optional 3-way fallback.
#[derive(Default)]
pub struct PatchApply {
    /// Per-instance safety config for path-policy enforcement.
    pub safety_config: Option<crate::config::SafetyConfig>,
}

impl PatchApply {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(cfg: crate::config::SafetyConfig) -> Self {
        Self {
            safety_config: Some(cfg),
        }
    }
}

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

        // Write diff to a temporary file
        let mut temp = NamedTempFile::new()?;
        temp.write_all(diff.as_bytes())?;
        let temp_path = temp.path().to_path_buf();
        let temp_path_str = temp_path.to_string_lossy().to_string();

        // Validate all target paths through the safety validator: the
        // `+++`/`---` names the text scan found, then — authoritatively —
        // every name git's own parser reports for the file that will be
        // applied (see `git_patch_paths`).
        let safety = crate::tools::file::resolve_safety_config(self.safety_config.as_ref());
        for path in &targets {
            validate_patch_path(path, &safety)?;
        }
        let git_paths = git_patch_paths(&temp_path_str).await?;
        for path in &git_paths {
            validate_patch_path(path, &safety)?;
        }

        if let Err(msg) = check_diff_header_safety(diff) {
            anyhow::bail!("{msg}");
        }

        // Added lines copied from a DISPLAY of the file (a `[REDACTED:…]`
        // secret marker, an entity-escaped character) would corrupt it.
        for (path, removed, added) in diff_sides_by_target(diff) {
            let resolved = crate::tools::workspace_root::anchor(&path);
            let original = crate::tools::file::read_to_string_capped(
                std::path::Path::new(&resolved),
                crate::tools::file::MAX_SOURCE_READ_SIZE,
            )
            .ok();
            crate::tools::file::refuse_display_artifacts(
                "patch_apply",
                &path,
                original.as_deref(),
                &removed,
                &added,
            )?;
        }

        // Try git apply --check first
        let check_output = run_git_apply_bounded(&["apply", "--check", &temp_path_str]).await;

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
                let stripped_check = run_git_apply_bounded(&["apply", "--check", &t_path]).await;
                if matches!(stripped_check, Ok(ref out) if out.status.success()) {
                    for path in git_patch_paths(&t_path).await? {
                        validate_patch_path(&path, &safety)?;
                    }
                    numbered_temp = Some(t);
                }
            }
        }
        let prefixes_stripped = numbered_temp.is_some();

        let applied = match check_output {
            Ok(ref out) if out.status.success() => {
                // Check passed — apply for real
                let apply_out = run_git_apply_bounded(&["apply", &temp_path_str]).await;
                matches!(apply_out, Ok(ref o) if o.status.success())
            }
            _ if prefixes_stripped => {
                let t_path = numbered_temp
                    .as_ref()
                    .map(|t| t.path().to_string_lossy().to_string())
                    .unwrap_or_default();
                let apply_out = run_git_apply_bounded(&["apply", &t_path]).await;
                matches!(apply_out, Ok(ref o) if o.status.success())
            }
            _ if allow_3way => {
                // Try 3-way merge fallback
                let apply3_out = run_git_apply_bounded(&["apply", "-3", &temp_path_str]).await;
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

        for path in &targets {
            crate::tools::file::clear_file_snapshot(path);
        }
        for path in &git_paths {
            crate::tools::file::clear_file_snapshot(path);
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
