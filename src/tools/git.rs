use super::file::{resolve_safety_config, validate_tool_path};
use super::process_guard::GroupedOutputExt;
use super::workspace_root::CommandRootExt;
use super::Tool;
use crate::config::SafetyConfig;
use anyhow::{Context, Result};
use async_trait::async_trait;
use git2::{Repository, StatusOptions};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn};

/// Validate a git tag name to prevent shell injection.
///
/// Only allows alphanumeric characters plus `-`, `.`, `_`, and `/`.
/// Rejects spaces, shell metacharacters, control characters, and empty names.
fn validate_tag_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Tag name must not be empty");
    }
    if name.len() > 256 {
        anyhow::bail!("Tag name too long (max 256 characters)");
    }
    for c in name.chars() {
        if !(c.is_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '/') {
            anyhow::bail!(
                "Invalid character '{}' in tag name '{}'. Only alphanumeric, '-', '.', '_', '/' are allowed.",
                c,
                name
            );
        }
    }
    // Reject names starting with '-' (could be interpreted as a flag)
    if name.starts_with('-') {
        anyhow::bail!("Tag name must not start with '-'");
    }
    Ok(())
}

/// Validate and normalise a model-supplied `git_push` `branch`.
///
/// The branch reaches git after `--`, so it cannot become a flag, but git
/// still reads it as a *refspec*: `+HEAD:main` force-pushes HEAD onto `main`
/// (bypassing both the `force` block and the protected-branch compare, which
/// only saw the literal string), `feature:main` writes `main`, and `:main`
/// deletes it. A branch must therefore be a plain branch name: a leading
/// `refs/heads/` is stripped, then `+`, `:` and everything
/// `git check-ref-format --branch` rejects are refused. The caller pushes the
/// explicit refspec `refs/heads/<b>:refs/heads/<b>`, so the protected-branch
/// check sees the real destination (review, 0.9.2).
pub(crate) fn normalize_push_branch(raw: &str) -> Result<String> {
    let b = raw.strip_prefix("refs/heads/").unwrap_or(raw);
    if b.is_empty() {
        anyhow::bail!("git_push `branch` must not be empty");
    }
    if b.starts_with('-') {
        anyhow::bail!("git_push `branch` must be a name, not an option: {raw:?}");
    }
    if b.contains('+') || b.contains(':') {
        anyhow::bail!(
            "git_push `branch` must be a plain branch name, not a refspec: {raw:?} \
             ('+' forces and ':' picks the destination; use `force`, which is blocked, \
             or check out the branch you mean to push)"
        );
    }
    let bad_char =
        |c: char| c.is_control() || matches!(c, ' ' | '~' | '^' | '?' | '*' | '[' | '\\');
    if b.chars().any(bad_char)
        || b.contains("..")
        || b.contains("@{")
        || b == "@"
        || b.contains("//")
        || b.ends_with('/')
        || b.ends_with('.')
        || b.split('/')
            .any(|c| c.starts_with('.') || c.ends_with(".lock"))
        || b.starts_with("refs/")
        || b == "HEAD"
    {
        anyhow::bail!("git_push `branch` is not a valid branch name: {raw:?}");
    }
    Ok(b.to_string())
}

/// Syntactic check on a model-supplied `git_push` `remote`.
///
/// Only a *named* remote (`origin`, `upstream`) may be pushed to. Anything
/// git would treat as a location instead — a URL (`https://`, `ssh://`,
/// `git://`, `file://`), scp-style `user@host:repo`, or a local path
/// (`/tmp/x`, `./x`, `../x`, `~/x`) — ships the workspace to an arbitrary
/// destination and is refused. The tool additionally requires the name to be
/// one listed by `git remote` ([`ensure_configured_remote`]), because git
/// falls back to treating an unknown bare name as a path.
pub(crate) fn validate_push_remote_syntax(remote: &str) -> Result<()> {
    let bad = remote.is_empty()
        || remote.starts_with('-')
        || remote.starts_with('/')
        || remote.starts_with('.')
        || remote.starts_with('~')
        || remote.contains("://")
        || remote.contains(':')
        || remote.contains('@')
        || remote.contains('\\')
        || remote.chars().any(|c| c.is_control() || c.is_whitespace());
    if bad {
        anyhow::bail!(
            "git_push `remote` must be the name of a configured remote (e.g. origin), \
             not a URL, host or path: {remote:?}"
        );
    }
    Ok(())
}

/// Require `remote` to be one of the remotes `git remote` lists.
pub(crate) fn ensure_configured_remote(remote: &str, configured: &[String]) -> Result<()> {
    if configured.iter().any(|r| r == remote) {
        return Ok(());
    }
    anyhow::bail!(
        "git_push `remote` {remote:?} is not a configured remote (configured: {configured:?}); \
         add it with `git remote add` first"
    )
}

/// Counter for unique temp file names within the same process.
static COMMIT_MSG_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The directory whose git configuration governs a `git -C <repo_path>`
/// call: `repo_path` under the workspace root when it is a directory, the
/// workspace root otherwise (a file path is diffed from the root).
fn git_config_dir(repo_path: &str) -> std::path::PathBuf {
    let root = crate::tools::workspace_root::current().path();
    let joined = root.join(repo_path);
    if joined.is_dir() {
        joined
    } else {
        root
    }
}

/// Whether the repository's own hooks ran for a commit in `dir`: only in a
/// trusted repository (`crate::safety::git_exec`, `GitScope::UserOperation`).
fn repository_hooks_note(dir: &std::path::Path) -> &'static str {
    if crate::safety::git_exec::repo_is_trusted(dir) {
        "ran (repository trusted)"
    } else {
        "not run: repository not trusted (`selfware trust` its selfware.toml to run hooks)"
    }
}

/// Write a commit message to a temp file for use with `git commit --file`.
///
/// Returns `Some(path)` on success, or `None` if writing fails (caller should
/// fall back to `-m`).
fn write_commit_message_file(message: &str) -> Option<PathBuf> {
    let seq = COMMIT_MSG_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_dir = std::env::temp_dir();
    let msg_file = temp_dir.join(format!(
        "selfware_commit_msg_{}_{}.txt",
        std::process::id(),
        seq
    ));
    match std::fs::write(&msg_file, message) {
        Ok(()) => Some(msg_file),
        Err(e) => {
            warn!(
                "Failed to write commit message to temp file {}: {}. Falling back to -m.",
                msg_file.display(),
                e
            );
            None
        }
    }
}

/// Validate that a repo/working-directory path is within the allowed paths.
fn validate_git_path(path: &str, safety_config: Option<&SafetyConfig>) -> Result<()> {
    let safety = resolve_safety_config(safety_config);
    validate_tool_path(path, &safety)
}

/// Cap on captured `git push` output.
const MAX_PUSH_OUTPUT_BYTES: usize = 256 * 1024;
/// Upper bound on the follow-up remote check after a push timed out (also
/// never longer than the push's own timeout).
const PUSH_REMOTE_CHECK_MAX_SECS: u64 = 15;

/// What a push that timed out left on the remote, as checked afterwards with
/// `git ls-remote` against the local branch commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PushRemoteState {
    /// The remote branch points at the local commit: the push landed.
    Pushed,
    /// The remote branch is absent or at another commit when checked.
    NotPushed,
    /// The check itself failed or timed out: nothing is known.
    Unknown,
}

/// Result of [`check_remote_after_push`].
#[derive(Debug, Clone)]
pub(crate) struct PushRemoteCheck {
    pub state: PushRemoteState,
    pub local_commit: Option<String>,
    pub remote_commit: Option<String>,
    /// Why the state is `Unknown` (empty otherwise).
    pub detail: String,
}

/// What a kill after a push timeout reached, per platform.
#[cfg(unix)]
const PUSH_KILL_SCOPE: &str = "git and its remote helpers were killed";
#[cfg(not(unix))]
const PUSH_KILL_SCOPE: &str =
    "git was killed (on this platform its remote helpers may still be running)";

/// After a timed-out push, compare `refs/heads/<branch>` on `remote`
/// (`git ls-remote`) with the local branch commit, bounded by `check_timeout`
/// per command. Both commands run in their own process group.
pub(crate) async fn check_remote_after_push(
    remote: &str,
    branch: &str,
    check_timeout: std::time::Duration,
) -> PushRemoteCheck {
    use crate::tools::process_guard::{run_command_bounded, CommandRunError};
    let refname = format!("refs/heads/{branch}");
    let unknown = |local: Option<String>, detail: String| PushRemoteCheck {
        state: PushRemoteState::Unknown,
        local_commit: local,
        remote_commit: None,
        detail,
    };
    let describe = |what: &str, e: CommandRunError| match e {
        CommandRunError::Timeout(d) => format!("{what} timed out after {}s", d.as_secs()),
        other => format!("{what} failed: {other}"),
    };

    let mut local_cmd = crate::safety::git_exec::git_command_async(
        crate::tools::workspace_root::current().path(),
        crate::safety::git_exec::GitScope::UserOperation,
    );
    local_cmd.in_workspace_root();
    local_cmd
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("{refname}^{{commit}}"));
    let local = match run_command_bounded(local_cmd, check_timeout, 4096).await {
        Ok(out) if out.status.success() => {
            let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (!sha.is_empty()).then_some(sha)
        }
        Ok(_) => None,
        Err(e) => return unknown(None, describe("git rev-parse", e)),
    };
    let Some(local) = local else {
        return unknown(None, format!("local {refname} could not be resolved"));
    };

    let mut ls_cmd = crate::safety::git_exec::git_command_async(
        crate::tools::workspace_root::current().path(),
        crate::safety::git_exec::GitScope::UserOperation,
    );
    ls_cmd.in_workspace_root();
    if let Ok(v) = std::env::var("SSH_AUTH_SOCK") {
        ls_cmd.env("SSH_AUTH_SOCK", v);
    }
    ls_cmd.args(["ls-remote", "--"]).arg(remote).arg(&refname);
    let out = match run_command_bounded(ls_cmd, check_timeout, 64 * 1024).await {
        Ok(out) => out,
        Err(e) => return unknown(Some(local), describe("git ls-remote", e)),
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return unknown(
            Some(local),
            format!("git ls-remote failed: {}", stderr.trim()),
        );
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let remote_commit = stdout.lines().find_map(|line| {
        let (sha, name) = line.split_once('\t')?;
        (name.trim() == refname).then(|| sha.trim().to_string())
    });
    let state = if remote_commit.as_deref() == Some(local.as_str()) {
        PushRemoteState::Pushed
    } else {
        PushRemoteState::NotPushed
    };
    PushRemoteCheck {
        state,
        local_commit: Some(local),
        remote_commit,
        detail: String::new(),
    }
}

/// The tool result for a push that hit its timeout. Never claims the push
/// did not happen: it reports what the remote check saw, or that nothing is
/// known.
pub(crate) fn push_timeout_result(
    remote: &str,
    branch: &str,
    timeout_secs: u64,
    check: &PushRemoteCheck,
) -> Value {
    let head = format!("git push timed out after {timeout_secs}s; {PUSH_KILL_SCOPE}.");
    let recheck = format!("git ls-remote {remote} refs/heads/{branch}");
    let local = check.local_commit.as_deref().unwrap_or("?");
    let message = match check.state {
        PushRemoteState::Pushed => format!(
            "{head} The push landed anyway: `{recheck}` shows the remote branch at {local}, \
             the local commit."
        ),
        PushRemoteState::NotPushed => format!(
            "{head} When checked, `{recheck}` showed the remote branch {} instead of the \
             local commit {local}, so the push had not landed. A server that had already \
             received the data could still apply it: re-run `{recheck}` before retrying.",
            match check.remote_commit.as_deref() {
                Some(sha) => format!("at {sha}"),
                None => "absent".to_string(),
            }
        ),
        PushRemoteState::Unknown => format!(
            "{head} The remote may already have received the push; the follow-up check \
             could not tell ({}). Check `{recheck}` before retrying.",
            check.detail
        ),
    };
    serde_json::json!({
        "success": check.state == PushRemoteState::Pushed,
        "timed_out": true,
        "remote_state": check.state,
        "verified_by": if check.state == PushRemoteState::Unknown { Value::Null } else { Value::from("git ls-remote") },
        "local_commit": check.local_commit,
        "remote_commit": check.remote_commit,
        "remote": remote,
        "branch": branch,
        "force": false,
        "output": message,
    })
}

#[derive(Default)]
pub struct GitStatus {
    pub safety_config: Option<SafetyConfig>,
}

#[derive(Default)]
pub struct GitDiff {
    pub safety_config: Option<SafetyConfig>,
}

#[derive(Default)]
pub struct GitCommit {
    pub safety_config: Option<SafetyConfig>,
}

#[derive(Default)]
pub struct GitPush {
    pub safety_config: Option<SafetyConfig>,
}

#[derive(Default)]
pub struct GitCheckpoint {
    pub safety_config: Option<SafetyConfig>,
}

impl GitStatus {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl GitDiff {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl GitCommit {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl GitPush {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl GitCheckpoint {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

#[async_trait]
impl Tool for GitCheckpoint {
    fn name(&self) -> &str {
        "git_checkpoint"
    }

    fn description(&self) -> &str {
        "Create a git checkpoint (commit) before dangerous operations. Returns commit hash for rollback. \
         Use this before any batch of changes that might break the build."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {"type": "string", "description": "Checkpoint description"},
                "tag": {"type": "string", "description": "Optional tag for easy rollback (e.g., 'before-refactor')"},
                "auto_branch": {"type": "boolean", "default": true, "description": "Create auto-incrementing agent branch if on main"}
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let msg = args
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?;
        let tag = args.get("tag").and_then(|v| v.as_str());
        let auto_branch = args
            .get("auto_branch")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Validate cwd is within allowed paths
        validate_git_path(".", self.safety_config.as_ref())?;

        // Check current branch
        let mut cmd = crate::safety::git_exec::git_command_async(
            crate::tools::workspace_root::current().path(),
            crate::safety::git_exec::GitScope::UserOperation,
        );
        cmd.in_workspace_root();
        let branch_output = cmd
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output_grouped()
            .await?;
        let current_branch = String::from_utf8_lossy(&branch_output.stdout)
            .trim()
            .to_string();

        // Auto-create agent working branch if on main/master
        let target_branch =
            if auto_branch && (current_branch == "main" || current_branch == "master") {
                let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                let agent_branch = format!("agent-{}", timestamp);

                let mut cmd = crate::safety::git_exec::git_command_async(
                    crate::tools::workspace_root::current().path(),
                    crate::safety::git_exec::GitScope::UserOperation,
                );
                cmd.in_workspace_root();
                cmd.args(["checkout", "-b", &agent_branch])
                    .output_grouped()
                    .await?;

                info!("Created agent branch: {}", agent_branch);
                agent_branch
            } else {
                current_branch
            };

        // Stage all changes
        let mut cmd = crate::safety::git_exec::git_command_async(
            crate::tools::workspace_root::current().path(),
            crate::safety::git_exec::GitScope::UserOperation,
        );
        cmd.in_workspace_root();
        cmd.args(["add", "-A"])
            .output_grouped()
            .await
            .context("Failed to stage changes")?;

        // Commit with checkpoint marker
        let full_msg = format!("[AGENT CHECKPOINT] {}", msg);
        let msg_file = write_commit_message_file(&full_msg);
        let mut cmd = crate::safety::git_exec::git_command_async(
            crate::tools::workspace_root::current().path(),
            crate::safety::git_exec::GitScope::UserOperation,
        );
        cmd.in_workspace_root();
        let commit_output = if let Some(ref path) = msg_file {
            cmd.arg("commit")
                .arg("--file")
                .arg(path)
                .arg("--allow-empty")
                .output_grouped()
                .await
                .context("Failed to create checkpoint commit")?
        } else {
            cmd.args(["commit", "-m", &full_msg, "--allow-empty"])
                .output_grouped()
                .await
                .context("Failed to create checkpoint commit")?
        };
        if let Some(path) = msg_file {
            let _ = std::fs::remove_file(path);
        }

        // Get hash
        let mut cmd = crate::safety::git_exec::git_command_async(
            crate::tools::workspace_root::current().path(),
            crate::safety::git_exec::GitScope::UserOperation,
        );
        cmd.in_workspace_root();
        let hash_output = cmd.args(["rev-parse", "HEAD"]).output_grouped().await?;
        let hash = String::from_utf8_lossy(&hash_output.stdout)
            .trim()
            .to_string();

        // Create or move tag
        if let Some(tag_name) = tag {
            validate_tag_name(tag_name)?;
            let mut cmd = crate::safety::git_exec::git_command_async(
                crate::tools::workspace_root::current().path(),
                crate::safety::git_exec::GitScope::UserOperation,
            );
            cmd.in_workspace_root();
            cmd.args(["tag", "-f", tag_name, &hash])
                .output_grouped()
                .await?;
        }

        // Get status summary
        let mut cmd = crate::safety::git_exec::git_command_async(
            crate::tools::workspace_root::current().path(),
            crate::safety::git_exec::GitScope::UserOperation,
        );
        cmd.in_workspace_root();
        let status_output = cmd.args(["status", "--short"]).output_grouped().await?;
        let status = String::from_utf8_lossy(&status_output.stdout);

        Ok(serde_json::json!({
            "repository_hooks": repository_hooks_note(&crate::tools::workspace_root::current().path()),
            "hash": hash,
            "branch": target_branch,
            "message": full_msg,
            "success": commit_output.status.success(),
            "files_changed": !status.is_empty(),
            "tag": tag
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::git()
    }
}

#[async_trait]
impl Tool for GitStatus {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "Get current git status including branch, staged/unstaged changes."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "repo_path": {"type": "string", "description": "Repository path (default: current)"}
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let args = super::workspace_root::anchor_json(args, &["repo_path"]);
        let repo_path = args
            .get("repo_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        validate_git_path(repo_path, self.safety_config.as_ref())?;

        let repo = Repository::open(repo_path)?;
        let head = repo.head()?;
        let branch = head.shorthand().unwrap_or("HEAD");

        let mut status_opts = StatusOptions::new();
        let statuses = repo.statuses(Some(&mut status_opts))?;

        let mut staged = vec![];
        let mut unstaged = vec![];
        let mut untracked = vec![];

        for status in statuses.iter() {
            let path = status.path().unwrap_or("??");
            let status_bits = status.status();

            if status_bits.is_index_new()
                || status_bits.is_index_modified()
                || status_bits.is_index_deleted()
            {
                staged.push(path.to_string());
            }
            if status_bits.is_wt_modified() || status_bits.is_wt_deleted() {
                unstaged.push(path.to_string());
            }
            if status_bits.is_wt_new() {
                untracked.push(path.to_string());
            }
        }

        Ok(serde_json::json!({
            "branch": branch,
            "staged": staged,
            "unstaged": unstaged,
            "untracked": untracked
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

#[async_trait]
impl Tool for GitDiff {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Show diff of changes. Can diff working tree, staged, or between commits."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Specific file or directory"},
                "staged": {"type": "boolean", "description": "Diff staged changes", "default": false},
                "base": {"type": "string", "description": "Compare against specific commit"}
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let args = super::workspace_root::anchor_json(args, &["path"]);
        let repo_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let staged = args
            .get("staged")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let base = args.get("base").and_then(|v| v.as_str());

        validate_git_path(repo_path, self.safety_config.as_ref())?;

        // `base` is passed as a single argv entry; reject leading dashes so it
        // can't smuggle in a flag (e.g. `--output=/path`).
        if let Some(base) = base {
            if base.starts_with('-') {
                anyhow::bail!("Invalid base for git diff: {}", base);
            }
        }

        let mut cmd = crate::safety::git_exec::git_command_async(
            git_config_dir(repo_path),
            crate::safety::git_exec::GitScope::UserOperation,
        );
        cmd.in_workspace_root();

        let path_obj = std::path::Path::new(repo_path);
        if path_obj.is_dir() {
            cmd.arg("-C").arg(repo_path).arg("diff");
            if staged {
                cmd.arg("--cached");
            }
            if let Some(base) = base {
                cmd.arg(base);
            }
        } else {
            let (work_dir, file_spec) = if let Some(parent) = path_obj
                .parent()
                .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
            {
                (parent, path_obj.file_name().unwrap_or(path_obj.as_os_str()))
            } else {
                (std::path::Path::new("."), path_obj.as_os_str())
            };
            cmd.arg("-C").arg(work_dir).arg("diff");
            if staged {
                cmd.arg("--cached");
            }
            if let Some(base) = base {
                cmd.arg(base);
            }
            cmd.arg("--").arg(file_spec);
        }

        let output = cmd.output_grouped().await?;
        // A non-zero exit (bad revision, not a repo, ...) used to be reported
        // as `has_changes: false` with the error text silently dropped.
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git diff failed: {}", stderr.trim());
        }
        let diff = String::from_utf8_lossy(&output.stdout);

        Ok(serde_json::json!({
            "diff": diff.to_string(),
            "has_changes": !diff.is_empty()
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

#[async_trait]
impl Tool for GitCommit {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        "Stage files and create a commit. Use conventional commit format."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "files": {"type": "array", "items": {"type": "string"}, "description": "Files to stage. Empty stages tracked modifications only (git add -u); list new/untracked files explicitly to include them."},
                "message": {"type": "string", "description": "Commit message"},
                "commit_type": {"type": "string", "enum": ["feat", "fix", "refactor", "docs", "test", "chore"]}
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let repo_path = ".";
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?;
        let files = args
            .get("files")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        validate_git_path(repo_path, self.safety_config.as_ref())?;

        // Validate individual file paths
        for file in &files {
            if let Some(f) = file.as_str() {
                validate_git_path(f, self.safety_config.as_ref())?;
            }
        }

        // Stage files
        if files.is_empty() {
            // Empty list stages tracked modifications/deletions only (`-u`), NOT
            // new untracked files. `git add -A` would sweep in whatever happens
            // to be untracked in the tree — a stray .env, a build artifact, a
            // spilled secret — and (with push allowed by default) publish it.
            // To commit a NEW file, pass it explicitly in `files`.
            let mut cmd = crate::safety::git_exec::git_command_async(
                git_config_dir(repo_path),
                crate::safety::git_exec::GitScope::UserOperation,
            );
            cmd.in_workspace_root();
            let add_output = cmd
                .arg("-C")
                .arg(repo_path)
                .arg("add")
                .arg("-u")
                .output_grouped()
                .await?;
            // A failed add must not silently become a partial commit below.
            if !add_output.status.success() {
                let stderr = String::from_utf8_lossy(&add_output.stderr);
                anyhow::bail!("git add -u failed: {}", stderr.trim());
            }
        } else {
            for file in files {
                if let Some(f) = file.as_str() {
                    if f.contains("..") || f.starts_with('/') {
                        anyhow::bail!("Invalid file path for git commit: {}", f);
                    }
                    let mut cmd = crate::safety::git_exec::git_command_async(
                        git_config_dir(repo_path),
                        crate::safety::git_exec::GitScope::UserOperation,
                    );
                    cmd.in_workspace_root();
                    let add_output = cmd
                        .arg("-C")
                        .arg(repo_path)
                        .arg("add")
                        .arg("--")
                        .arg(f)
                        .output_grouped()
                        .await?;
                    // A typo'd/unmatched path makes `git add` exit non-zero;
                    // ignoring it would produce a partial commit reported as
                    // success. Surface the real error instead.
                    if !add_output.status.success() {
                        let stderr = String::from_utf8_lossy(&add_output.stderr);
                        anyhow::bail!("git add failed for '{}': {}", f, stderr.trim());
                    }
                }
            }
        }

        // Commit — write message to temp file for defense-in-depth against
        // shell metacharacters, falling back to -m if the write fails.
        let msg_file = write_commit_message_file(message);
        let mut cmd = crate::safety::git_exec::git_command_async(
            git_config_dir(repo_path),
            crate::safety::git_exec::GitScope::UserOperation,
        );
        cmd.in_workspace_root();
        let output = if let Some(ref path) = msg_file {
            cmd.arg("-C")
                .arg(repo_path)
                .arg("commit")
                .arg("--file")
                .arg(path)
                .output_grouped()
                .await?
        } else {
            cmd.arg("-C")
                .arg(repo_path)
                .arg("commit")
                .arg("-m")
                .arg(message)
                .output_grouped()
                .await?
        };
        if let Some(path) = msg_file {
            let _ = std::fs::remove_file(path);
        }

        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        // On failure git explains itself on stderr (hooks, identity, empty
        // staging area) — surface it instead of reporting a bare failure.
        let combined = if success {
            stdout.to_string()
        } else {
            format!("{}{}", stdout, stderr)
        };

        Ok(serde_json::json!({
            "success": success,
            "output": combined,
            "repository_hooks": repository_hooks_note(&git_config_dir(repo_path)),
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::git()
    }
}

#[async_trait]
impl Tool for GitPush {
    fn name(&self) -> &str {
        "git_push"
    }

    fn description(&self) -> &str {
        "Push commits to a remote repository. Force push is blocked by the safety checker."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "remote": {
                    "type": "string",
                    "description": "Remote name (default: origin)",
                    "default": "origin"
                },
                "branch": {
                    "type": "string",
                    "description": "Branch to push (default: current branch)"
                },
                "force": {
                    "type": "boolean",
                    "description": "Force push (blocked by safety checker)",
                    "default": false
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds for the network push (default: 120)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let remote = args
            .get("remote")
            .and_then(|v| v.as_str())
            .unwrap_or("origin");
        let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

        if force {
            anyhow::bail!("Force push is blocked by the safety checker.");
        }

        // Refuse refspec/flag/URL-shaped operands before anything is spawned.
        validate_push_remote_syntax(remote)?;
        let explicit_branch = args
            .get("branch")
            .and_then(|v| v.as_str())
            .map(normalize_push_branch)
            .transpose()?;

        // Validate cwd is within allowed paths
        validate_git_path(".", self.safety_config.as_ref())?;

        // Determine branch
        let branch = if let Some(b) = explicit_branch {
            b
        } else {
            let mut cmd = crate::safety::git_exec::git_command_async(
                crate::tools::workspace_root::current().path(),
                crate::safety::git_exec::GitScope::UserOperation,
            );
            cmd.in_workspace_root();
            let output = cmd
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output_grouped()
                .await
                .context("Failed to get current branch")?;
            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to detect current branch: {}", err.trim());
            }
            let current = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // A detached HEAD reports "HEAD"; that is not a branch to push.
            normalize_push_branch(&current)
                .with_context(|| format!("current checkout {current:?} is not a pushable branch"))?
        };

        if let Some(ref safety_config) = self.safety_config {
            if safety_config
                .protected_branches
                .iter()
                .any(|b| b == &branch)
            {
                anyhow::bail!(
                    "Push to protected branch '{}' is blocked by the safety checker \
                     (protected_branches: {:?}).",
                    branch,
                    safety_config.protected_branches
                );
            }
        }

        // Only push to a remote the user configured: git treats an unknown
        // bare name as a local path.
        let mut remotes_cmd = crate::safety::git_exec::git_command_async(
            crate::tools::workspace_root::current().path(),
            crate::safety::git_exec::GitScope::UserOperation,
        );
        remotes_cmd.in_workspace_root();
        let remotes_out = remotes_cmd
            .arg("remote")
            .output_grouped()
            .await
            .context("Failed to list git remotes")?;
        if !remotes_out.status.success() {
            let err = String::from_utf8_lossy(&remotes_out.stderr);
            anyhow::bail!("Failed to list git remotes: {}", err.trim());
        }
        let configured: Vec<String> = String::from_utf8_lossy(&remotes_out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        ensure_configured_remote(remote, &configured)?;

        let mut cmd = crate::safety::git_exec::git_command_async(
            crate::tools::workspace_root::current().path(),
            crate::safety::git_exec::GitScope::UserOperation,
        );
        cmd.in_workspace_root();
        // Push goes over the network and may need the user's SSH agent;
        // re-add it explicitly after the env clear.
        if let Ok(v) = std::env::var("SSH_AUTH_SOCK") {
            cmd.env("SSH_AUTH_SOCK", v);
        }
        // Explicit refspec: the destination is exactly the validated branch
        // the protected-branch check above compared against.
        cmd.arg("push")
            .arg("--")
            .arg(remote)
            .arg(format!("refs/heads/{branch}:refs/heads/{branch}"));
        // Own process group + guard (`run_command_bounded`): a timeout, a
        // cancel, or a dropped future kills git AND its remote helpers
        // (`git-remote-https`, `ssh`). `kill_on_drop` alone signalled only
        // `git`, and the orphaned helper kept pushing after we had reported
        // the timeout.
        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120)
            .max(1);
        let output = match crate::tools::process_guard::run_command_bounded(
            cmd,
            std::time::Duration::from_secs(timeout_secs),
            MAX_PUSH_OUTPUT_BYTES,
        )
        .await
        {
            Ok(output) => output,
            Err(crate::tools::process_guard::CommandRunError::Timeout(_)) => {
                // The helpers are dead, but the remote may already have the
                // update: check instead of claiming it did not happen.
                let check_timeout =
                    std::time::Duration::from_secs(timeout_secs.min(PUSH_REMOTE_CHECK_MAX_SECS));
                let check = check_remote_after_push(remote, &branch, check_timeout).await;
                return Ok(push_timeout_result(remote, &branch, timeout_secs, &check));
            }
            Err(e) => anyhow::bail!("Failed to execute git push: {e}"),
        };
        // git's own exit status decides whether the push happened; a helper
        // that lingered past git's exit (and was killed) does not undo it.
        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(serde_json::json!({
            "success": success,
            "remote": remote,
            "branch": branch,
            "force": force,
            "lingering_helpers_killed": output.killed_descendants,
            "output": format!("{}{}", stdout, stderr)
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        // Git push is high risk due to potential for remote changes
        crate::safety::ToolMetadata::custom(
            false,
            false,
            crate::safety::RiskLevel::High,
            true,
            false,
        )
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/git/git_test.rs"]
mod tests;
