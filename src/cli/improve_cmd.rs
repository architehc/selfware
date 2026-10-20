use anyhow::Result;

use crate::agent::Agent;
use crate::config::Config;
use crate::ui::components::{render_header, WorkshopContext};
use crate::ui::style::{Glyphs, SelfwareStyle};

use super::ONE_SHOT_CONFIRM_TIMEOUT;

/// Decode a path as git renders it in porcelain / `--name-status` output:
/// paths containing special characters are C-style quoted with backslash
/// escapes (`"a b.txt"`, octal `\ooo` for control bytes and non-ASCII).
/// Unquoted paths pass through unchanged. The improve sweep must decode,
/// or a protected path containing a space (e.g. an instruction file with a
/// space) silently dodges it (2026-09-21 follow-up review finding).
pub(crate) fn decode_git_path(text: &str) -> String {
    let Some(inner) = text.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return text.to_string();
    };
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b != b'\\' {
            out.push(b);
            i += 1;
            continue;
        }
        i += 1; // consume the backslash
        if i >= bytes.len() {
            break;
        }
        match bytes[i] {
            b'\\' => out.push(b'\\'),
            b'"' => out.push(b'"'),
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'v' => out.push(0x0b),
            b'?' => out.push(b'?'),
            oct @ b'0'..=b'7' => {
                // Up to three octal digits, the way git renders non-ASCII
                // and control bytes.
                let mut val = u32::from(oct - b'0');
                let mut consumed = 1;
                while consumed < 3 && i + 1 < bytes.len() && (b'0'..=b'7').contains(&bytes[i + 1]) {
                    i += 1;
                    val = val * 8 + u32::from(bytes[i] - b'0');
                    consumed += 1;
                }
                out.push(val as u8);
            }
            other => {
                out.push(b'\\');
                out.push(other);
            }
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse `git status --porcelain` stdout into the working-tree paths it
/// reports (tracked modifications AND untracked files). Rename entries
/// (`R  orig -> new`) yield BOTH the source and the destination: renaming a
/// protected path to an unprotected name is still a modification of the
/// protected path, so the source must be swept too (2026-09-21 follow-up
/// review finding). Paths are decoded from git's C-style quoting. Kept pure
/// so the improve gate's protected-path sweep is unit-testable without a
/// real repository.
pub(crate) fn porcelain_paths(stdout: &[u8]) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for line in String::from_utf8_lossy(stdout).lines() {
        // Porcelain lines are `XY <path>` (path starts at byte 3).
        let Some(p) = line.get(3..) else { continue };
        if p.is_empty() {
            continue;
        }
        match p.split_once(" -> ") {
            Some((src, dest)) => {
                out.push(std::path::PathBuf::from(decode_git_path(src)));
                out.push(std::path::PathBuf::from(decode_git_path(dest)));
            }
            None => out.push(std::path::PathBuf::from(decode_git_path(p))),
        }
    }
    out
}

/// Parse `git diff --name-status --no-renames` stdout into the paths it
/// touches. Every status line is `<status>\t<path>` and deletions are listed
/// like any other change; `--no-renames` renders a rename as a delete + add
/// pair so BOTH the original and the new path appear in the sweep. Paths are
/// decoded from git's C-style quoting.
pub(crate) fn diff_name_status_paths(stdout: &[u8]) -> Vec<std::path::PathBuf> {
    String::from_utf8_lossy(stdout)
        .lines()
        .filter_map(|line| {
            let path = line.rsplit_once('\t').map(|(_, p)| p).unwrap_or(line);
            let path = path.trim();
            if path.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(decode_git_path(path)))
            }
        })
        .collect()
}

/// The repository's current HEAD sha, or `None` when the checkout is not a
/// git repo or has an unborn HEAD. Used by the improve gate to sweep
/// changes the agent COMMITTED (they vanish from `git status --porcelain`).
pub(crate) async fn current_head_sha(project_root: &std::path::Path) -> Option<String> {
    let mut cmd = crate::safety::git_exec::git_command_async(
        project_root,
        crate::safety::git_exec::GitScope::Internal,
    );
    cmd.args(["rev-parse", "HEAD"]);
    let output = cmd.output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout);
    let sha = sha.trim();
    if sha.is_empty() {
        None
    } else {
        Some(sha.to_string())
    }
}

/// Tail of a failed gate's output (the full `cargo clippy` stderr on a big
/// tree is hundreds of lines; the CLI message only needs the end).
pub(crate) fn gate_failure_tail(stderr: &[u8], max_lines: usize) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text.lines().collect();
    let tail = lines.len().saturating_sub(max_lines);
    lines[tail..].join("\n")
}

/// Spawn a gate command with a SANITIZED environment and process-group
/// isolation, and wait for it with a deadline.
///
/// Sanitization: gate children run project-controlled code and must never
/// inherit the operator's credentials (`SELFWARE_API_KEY`, exported tokens),
/// exactly like the evolution daemon's cargo commands. The sanitizer is
/// applied BEFORE any task-specific env so the allowlist survives.
///
/// Timeout: the child is spawned in its own process group (`process_group(0)`)
/// and the WHOLE group — cargo plus its rustc/test grandchildren — is
/// SIGKILLed when the deadline passes; `kill_on_drop` kills the direct child
/// and the tokio runtime's SIGCHLD handler reaps the zombie, so a 600s gate
/// cannot leak subprocesses (2026-09-21 follow-up review finding).
pub(crate) async fn run_gate_command(
    program: &str,
    args: &[&str],
    project_root: &std::path::Path,
    within: std::time::Duration,
) -> anyhow::Result<std::process::Output> {
    // git runs after the agent had the repository to itself: its
    // `.git/config` may now name an fsmonitor, filter or textconv program.
    // The hardened spawn (`safety::git_exec`) neutralises them.
    let mut cmd = if program == "git" {
        crate::safety::git_exec::git_command_async(
            project_root,
            crate::safety::git_exec::GitScope::Internal,
        )
    } else {
        let mut cmd = tokio::process::Command::new(program);
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        crate::safety::process_env::pin_rustup_toolchain(&mut cmd);
        cmd
    };
    cmd.args(args)
        .current_dir(project_root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("'{program} {}' failed to spawn: {e}", args.join(" ")))?;
    let child_pid = child.id();
    // A dropped future (Ctrl-C of the gate) also kills the whole group.
    let mut pg_guard = crate::tools::process_guard::ProcessGroupGuard::new(child_pid);

    let wait_fut = tokio::time::timeout(
        within,
        crate::tools::process_guard::wait_with_output_without_reaping(&mut child),
    );
    tokio::pin!(wait_fut);

    match wait_fut.await {
        Ok(Ok(output)) => {
            pg_guard.disarm();
            Ok(output)
        }
        Ok(Err(e)) => anyhow::bail!("'{program} {}' failed to run: {e}", args.join(" ")),
        Err(_) => {
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
            anyhow::bail!(
                "'{program} {}' timed out after {}s — the process group was terminated",
                args.join(" "),
                within.as_secs()
            )
        }
    }
}

/// Pre-commit-style gate for `selfware improve`: the same checks the repo's
/// pre-commit hook enforces — `cargo fmt --check`, `cargo clippy --all-targets
/// -- -D warnings`, `cargo test --lib` — plus a protected-path sweep of the
/// working tree that covers UNCOMMITTED/untracked changes (`git status
/// --porcelain`) AND changes the agent COMMITTED since `pre_run_head` (`git
/// diff` against that tree — committed edits no longer appear in porcelain).
/// `improve` runs the agent on the LIVE checkout with no sandbox (2026-09-21
/// review, critical), so an Ok from the agent only means "task finished"; a
/// success claim is only printed after every check actually ran and passed
/// here, mirroring what the evolution daemon's candidate evaluation enforces
/// in its shadow worktree.
///
/// Returns Ok(()) only when every check ran and passed; otherwise the failing
/// gate's output.
pub(crate) async fn run_improvement_gates(
    project_root: &std::path::Path,
    pre_run_head: Option<&str>,
) -> anyhow::Result<()> {
    use std::time::Duration;

    async fn run_cargo_gate(
        project_root: &std::path::Path,
        args: &[&str],
        within: Duration,
    ) -> anyhow::Result<()> {
        let output = run_gate_command("cargo", args, project_root, within).await?;
        if !output.status.success() {
            let tail = gate_failure_tail(&output.stderr, 40);
            let tail = if tail.trim().is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            } else {
                tail
            };
            anyhow::bail!(
                "'cargo {}' failed ({}):\n{}",
                args.join(" "),
                output.status,
                tail.trim()
            );
        }
        Ok(())
    }

    run_cargo_gate(
        project_root,
        &["fmt", "--", "--check"],
        Duration::from_secs(120),
    )
    .await?;
    run_cargo_gate(
        project_root,
        &["clippy", "--all-targets", "--", "-D", "warnings"],
        Duration::from_secs(300),
    )
    .await?;
    run_cargo_gate(project_root, &["test", "--lib"], Duration::from_secs(600)).await?;

    // Protected-path sweep #1: uncommitted / untracked working-tree changes.
    // `--untracked-files=all` enumerates untracked FILES; the default would
    // collapse an untracked directory to a single `?? newdir/` line, hiding
    // e.g. `newdir/AGENTS.md` from the sweep (2026-09-21 follow-up finding).
    let status = run_gate_command(
        "git",
        &["status", "--porcelain", "--untracked-files=all"],
        project_root,
        Duration::from_secs(30),
    )
    .await?;
    if !status.status.success() {
        anyhow::bail!("git status failed ({})", status.status);
    }
    let mut dirtied = porcelain_paths(&status.stdout);

    // Protected-path sweep #2: changes the agent COMMITTED during the run —
    // those no longer appear in `git status --porcelain`. A diff against the
    // pre-run HEAD tree captures them; an unborn/absent HEAD falls back to
    // the porcelain sweep alone.
    if let Some(head) = pre_run_head {
        let diff = run_gate_command(
            "git",
            &["diff", "--name-status", "--no-renames", head],
            project_root,
            Duration::from_secs(30),
        )
        .await?;
        if !diff.status.success() {
            anyhow::bail!("git diff against the pre-run HEAD failed ({})", diff.status);
        }
        dirtied.extend(diff_name_status_paths(&diff.stdout));
    }

    let touched: Vec<_> = dirtied
        .into_iter()
        .filter(|p| crate::evolution::is_protected(p))
        .collect();
    if let Some(p) = touched.first() {
        anyhow::bail!(
            "the agent modified protected path '{}' — the change is refused",
            p.display()
        );
    }
    Ok(())
}

/// Parse `git ls-files --others --exclude-standard -z` stdout: the NUL-
/// separated list of non-ignored untracked files, relative to the repo root.
/// Pure so the rollback's untracked bookkeeping is unit-testable.
pub(crate) fn parse_untracked_nul_list(stdout: &[u8]) -> Vec<std::path::PathBuf> {
    String::from_utf8_lossy(stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .collect()
}

/// Untracked files present AFTER a run but not BEFORE it — what the agent
/// created. The rollback removes exactly these (and only these).
pub(crate) fn untracked_created_during_run(
    before: &[std::path::PathBuf],
    after: &[std::path::PathBuf],
) -> Vec<std::path::PathBuf> {
    let before: std::collections::BTreeSet<&std::path::Path> =
        before.iter().map(|p| p.as_path()).collect();
    let mut created: Vec<std::path::PathBuf> = after
        .iter()
        .filter(|p| !before.contains(p.as_path()))
        .cloned()
        .collect();
    created.sort();
    created.dedup();
    created
}

/// Pre-existing untracked files the agent deleted during the run. Their
/// content was never captured, so the rollback can only report them.
pub(crate) fn untracked_removed_during_run(
    before: &[std::path::PathBuf],
    after: &[std::path::PathBuf],
) -> Vec<std::path::PathBuf> {
    let after: std::collections::BTreeSet<&std::path::Path> =
        after.iter().map(|p| p.as_path()).collect();
    let mut removed: Vec<std::path::PathBuf> = before
        .iter()
        .filter(|p| !after.contains(p.as_path()))
        .cloned()
        .collect();
    removed.sort();
    removed.dedup();
    removed
}

/// Pre-run capture of the live checkout, so a failed `improve` cycle can
/// restore exactly the state the agent started from. `improve` runs on the
/// live checkout (no daemon-style worktree isolation — see the follow-up
/// note), so without this a gate failure or agent error left broken /
/// policy-violating edits dirty in the working tree with no rollback
/// (2026-09-21 follow-up review, W1a).
pub(crate) struct ImproveTreeSnapshot {
    /// Result of `git stash create`: a DANGLING commit (never written to a
    /// ref, so history is not polluted) whose tree IS the pre-run
    /// index+worktree state. `None` when nothing tracked was dirty.
    pub(crate) tracked_stash: Option<String>,
    /// HEAD at snapshot time: the fallback restore source when no stash was
    /// created (a clean pre-run tree), and the baseline for counting commits
    /// the agent left behind.
    pub(crate) pre_run_head: Option<String>,
    /// Non-ignored untracked files present before the run.
    pub(crate) untracked_before: Vec<std::path::PathBuf>,
    /// Every working-tree path porcelain reported at snapshot time, so the
    /// post-rollback protected-path sweep can ignore the operator's own
    /// pre-existing dirty files (restored faithfully, not agent damage).
    pub(crate) pre_run_dirty: Vec<std::path::PathBuf>,
}

pub(crate) async fn snapshot_improve_tree(
    project_root: &std::path::Path,
    pre_run_head: Option<&str>,
) -> ImproveTreeSnapshot {
    use std::time::Duration;
    let pre_run_head = pre_run_head.map(str::to_string);
    // `git stash create` outputs the stash commit sha, or nothing on a clean
    // tree; on an unborn repo it exits non-zero. Either way: None, safe.
    let tracked_stash = run_gate_command(
        "git",
        &["stash", "create", "selfware-improve-pre-run"],
        project_root,
        Duration::from_secs(30),
    )
    .await
    .ok()
    .filter(|o| o.status.success())
    .and_then(|o| {
        let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    });
    let untracked_before = run_gate_command(
        "git",
        &["ls-files", "--others", "--exclude-standard", "-z"],
        project_root,
        Duration::from_secs(30),
    )
    .await
    .ok()
    .filter(|o| o.status.success())
    .map(|o| parse_untracked_nul_list(&o.stdout))
    .unwrap_or_default();
    let pre_run_dirty = run_gate_command(
        "git",
        &["status", "--porcelain", "--untracked-files=all"],
        project_root,
        Duration::from_secs(30),
    )
    .await
    .ok()
    .filter(|o| o.status.success())
    .map(|o| porcelain_paths(&o.stdout))
    .unwrap_or_default();
    ImproveTreeSnapshot {
        tracked_stash,
        pre_run_head,
        untracked_before,
        pre_run_dirty,
    }
}

/// What a rollback did, for the CLI failure report.
#[derive(Debug, Default)]
pub(crate) struct ImproveRollback {
    pub(crate) restored_tracked: bool,
    pub(crate) removed_untracked: Vec<std::path::PathBuf>,
    pub(crate) deleted_pre_existing_untracked: Vec<std::path::PathBuf>,
    pub(crate) committed_since_head: usize,
    pub(crate) notes: Vec<String>,
}

impl ImproveRollback {
    pub(crate) fn render(&self) -> String {
        let mut bits: Vec<String> = Vec::new();
        bits.push(if self.restored_tracked {
            "the tracked tree was restored to its pre-run state".to_string()
        } else {
            "the tracked tree was NOT restored".to_string()
        });
        if !self.removed_untracked.is_empty() {
            let listed: Vec<String> = self
                .removed_untracked
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            bits.push(format!(
                "{} new untracked file(s) the agent created were removed: {}",
                self.removed_untracked.len(),
                listed.join(", ")
            ));
        }
        if !self.deleted_pre_existing_untracked.is_empty() {
            let listed: Vec<String> = self
                .deleted_pre_existing_untracked
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            bits.push(format!(
                "{} pre-existing untracked file(s) the agent deleted could not be restored \
                 (their content was not captured): {}",
                self.deleted_pre_existing_untracked.len(),
                listed.join(", ")
            ));
        }
        if self.committed_since_head > 0 {
            bits.push(format!(
                "the agent left {} commit(s) since the pre-run HEAD — they are still in \
                 history (dropping them would rewrite it); review with `git log <pre-run-head>..HEAD`",
                self.committed_since_head
            ));
        }
        bits.extend(self.notes.iter().cloned());
        bits.join("; ")
    }
}

/// Restore the live checkout to the pre-run snapshot captured by
/// [`snapshot_improve_tree`]:
///
/// 1. Tracked tree (index + worktree) is restored from the stash commit when
///    one exists, else from the pre-run HEAD tree — `git restore` never moves
///    a branch, so history is untouched. The operator's OWN pre-run dirty
///    files are part of the snapshot and come back exactly as they were.
/// 2. Untracked files the agent created (present after, absent before) are
///    deleted; pre-existing untracked files the agent deleted are reported
///    (their content was not captured).
/// 3. A post-rollback protected-path re-sweep runs; any protected path still
///    touched that was NOT already dirty pre-run is reported.
///
/// This is an in-place rollback for the LIVE checkout. Deeper daemon-style
/// worktree isolation (run the agent in a shadow worktree so the live tree
/// is never touched at all) is the documented follow-up.
pub(crate) async fn rollback_improve_tree(
    project_root: &std::path::Path,
    snapshot: &ImproveTreeSnapshot,
) -> ImproveRollback {
    use std::time::Duration;

    let mut rollback = ImproveRollback::default();

    // 1. Tracked restore.
    let restore_source = snapshot
        .tracked_stash
        .clone()
        .or_else(|| snapshot.pre_run_head.clone());
    match restore_source {
        Some(source) => {
            let ok = run_gate_command(
                "git",
                &[
                    "restore",
                    "--source",
                    &source,
                    "--staged",
                    "--worktree",
                    "--",
                    ".",
                ],
                project_root,
                Duration::from_secs(60),
            )
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);
            rollback.restored_tracked = ok;
            if !ok {
                rollback.notes.push(
                    "`git restore` of the tracked tree failed — the tree may still contain \
                     the agent's edits"
                        .to_string(),
                );
            }
        }
        None => {
            rollback.notes.push(
                "no git snapshot and no pre-run HEAD — the tracked tree was left as-is".to_string(),
            );
        }
    }

    // 2. Untracked bookkeeping.
    let untracked_after = run_gate_command(
        "git",
        &["ls-files", "--others", "--exclude-standard", "-z"],
        project_root,
        Duration::from_secs(30),
    )
    .await
    .ok()
    .filter(|o| o.status.success())
    .map(|o| parse_untracked_nul_list(&o.stdout))
    .unwrap_or_default();
    rollback.removed_untracked =
        untracked_created_during_run(&snapshot.untracked_before, &untracked_after);
    for created in &rollback.removed_untracked {
        let full = project_root.join(created);
        let is_real_dir = std::fs::symlink_metadata(&full)
            .map(|m| m.file_type().is_dir())
            .unwrap_or(false);
        if is_real_dir {
            let _ = std::fs::remove_dir_all(&full);
        } else {
            let _ = std::fs::remove_file(&full);
        }
        // Prune directories the removed file populated: an agent that wrote
        // `deep/dir/file.py` leaves an empty `deep/dir/` skeleton behind.
        // Walking up, only directories that are genuinely empty are removed,
        // so a pre-existing directory that happens to contain a new file keeps
        // its other contents (it will not be empty) and is left alone.
        let mut parent = full.parent();
        while let Some(dir) = parent {
            if dir == project_root {
                break;
            }
            let empty = std::fs::read_dir(dir)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if !empty {
                break;
            }
            let _ = std::fs::remove_dir(dir);
            parent = dir.parent();
        }
    }
    rollback.deleted_pre_existing_untracked =
        untracked_removed_during_run(&snapshot.untracked_before, &untracked_after);

    // 3. Commits the agent left on the branch (reported, not dropped: doing
    //    so would rewrite history the operator may have touched).
    if let Some(head) = &snapshot.pre_run_head {
        // `<head>..HEAD`: commits made during the run. The bare
        // `rev-list --count <head> HEAD` form counted the UNION of both
        // histories (the whole repository), so a run that committed nothing
        // still reported "the agent left N commit(s)".
        let range = format!("{head}..HEAD");
        if let Ok(o) = run_gate_command(
            "git",
            &["rev-list", "--count", &range],
            project_root,
            Duration::from_secs(30),
        )
        .await
        {
            if o.status.success() {
                if let Ok(n) = String::from_utf8_lossy(&o.stdout).trim().parse::<usize>() {
                    rollback.committed_since_head = n;
                }
            }
        }
    }

    // 4. Post-rollback protected-path re-sweep: report anything still
    //    touched that was not already dirty pre-run (the operator's own
    //    pre-run dirty files were restored faithfully and are not damage).
    let mut still_dirty = run_gate_command(
        "git",
        &["status", "--porcelain", "--untracked-files=all"],
        project_root,
        Duration::from_secs(30),
    )
    .await
    .ok()
    .filter(|o| o.status.success())
    .map(|o| porcelain_paths(&o.stdout))
    .unwrap_or_default();
    if let Some(head) = &snapshot.pre_run_head {
        if let Ok(o) = run_gate_command(
            "git",
            &["diff", "--name-status", "--no-renames", head],
            project_root,
            Duration::from_secs(30),
        )
        .await
        {
            if o.status.success() {
                still_dirty.extend(diff_name_status_paths(&o.stdout));
            }
        }
    }
    let pre_run_dirty: std::collections::BTreeSet<&std::path::Path> =
        snapshot.pre_run_dirty.iter().map(|p| p.as_path()).collect();
    let remaining_protected: Vec<&std::path::Path> = still_dirty
        .iter()
        .map(|p| p.as_path())
        .filter(|p| !pre_run_dirty.contains(p) && crate::evolution::is_protected(p))
        .collect();
    if !remaining_protected.is_empty() {
        let listed: Vec<String> = remaining_protected
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        rollback.notes.push(format!(
            "protected path(s) still modified after rollback: {}",
            listed.join(", ")
        ));
    }

    rollback
}

/// Write a report or policy artifact under `<repo_root>/.selfware/<rel_path>`,
/// refusing symlinks on `.selfware`, any intermediate directory, or the target file.
pub(crate) fn write_selfware_artifact_checked(
    repo_root: &std::path::Path,
    rel_path: &std::path::Path,
    contents: impl AsRef<[u8]>,
) -> Result<std::path::PathBuf> {
    let selfware_dir = repo_root.join(".selfware");
    let mut current = repo_root.to_path_buf();
    for component in std::path::Path::new(".selfware")
        .components()
        .chain(rel_path.parent().into_iter().flat_map(|p| p.components()))
    {
        let std::path::Component::Normal(part) = component else {
            anyhow::bail!(
                "Refusing relative/non-normal artifact component in {}",
                rel_path.display()
            );
        };
        current.push(part);
        if let Ok(meta) = std::fs::symlink_metadata(&current) {
            if meta.file_type().is_symlink() {
                anyhow::bail!(
                    "Refusing to write artifact through symlinked directory: {}",
                    current.display()
                );
            }
        }
    }
    std::fs::create_dir_all(&current)?;
    let target = selfware_dir.join(rel_path);
    if let Ok(meta) = std::fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            anyhow::bail!(
                "Refusing to overwrite symlinked artifact file: {}",
                target.display()
            );
        }
    }
    crate::session::checkpoint::write_bytes_atomically(&target, contents.as_ref(), 0o600)?;
    Ok(target)
}

pub(crate) struct EvolveArmsArgs {
    pub(crate) repo_root: std::path::PathBuf,
    pub(crate) arms_file: Option<std::path::PathBuf>,
    pub(crate) consensus: usize,
    pub(crate) replicates: usize,
    pub(crate) parallel: usize,
    pub(crate) bench_cmd: Option<String>,
    pub(crate) arm_sandbox: bool,
    pub(crate) keep_arms: bool,
    pub(crate) dry_run: bool,
}

/// `selfware evolve --workflow arms`: build, test and benchmark candidate
/// arms against the baseline in quarantined snapshots and report the
/// verdict. Nothing is applied to the repository.
pub(crate) async fn run_evolve_arms(a: EvolveArmsArgs) -> Result<()> {
    use crate::evolve::multi_arm_runner::{
        MultiArmConfig, MultiArmEvolutionRunner, OptimizationArm,
    };
    use crate::safety::quarantine::{HostToolchain, QuarantineOptions, RegistryMode};
    use anyhow::Context as _;

    let arms_file = a
        .arms_file
        .ok_or_else(|| anyhow::anyhow!("--workflow arms needs --arms-file <arms.json>"))?;
    let arms_meta = std::fs::symlink_metadata(&arms_file)
        .with_context(|| format!("inspecting {}", arms_file.display()))?;
    if arms_meta.file_type().is_symlink() {
        anyhow::bail!(
            "Refusing to read symlinked arms file: {}",
            arms_file.display()
        );
    }
    const MAX_ARMS_FILE_BYTES: u64 = 16 * 1024 * 1024;
    if arms_meta.len() > MAX_ARMS_FILE_BYTES {
        anyhow::bail!(
            "Arms file '{}' is too large ({} bytes, max {} bytes)",
            arms_file.display(),
            arms_meta.len(),
            MAX_ARMS_FILE_BYTES
        );
    }
    let arms: Vec<OptimizationArm> = serde_json::from_str(
        &crate::tools::file::read_to_string_capped(&arms_file, MAX_ARMS_FILE_BYTES)
            .with_context(|| format!("reading {}", arms_file.display()))?,
    )
    .with_context(|| format!("parsing {}", arms_file.display()))?;
    let bench_command = match a.bench_cmd {
        Some(cmd) => Some(
            shlex::split(&cmd)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("--bench-cmd `{cmd}` is not a valid command line")
                })?,
        ),
        None => None,
    };
    let config = MultiArmConfig {
        consensus_threshold: a.consensus,
        replicates: a.replicates,
        bench_command,
        parallel: a.parallel.max(1),
        quarantine: QuarantineOptions {
            registry: RegistryMode::CloneOrEmpty,
            sandbox: a.arm_sandbox,
        },
        keep_arms: a.keep_arms,
        ..Default::default()
    };
    let scratch = std::env::temp_dir().join("selfware-evolve-arms");
    println!(
        "   {} arm(s) from {}; consensus {}; {} benchmark replicate(s); bench: {}; sandbox: {}",
        arms.len(),
        arms_file.display(),
        config.consensus_threshold,
        config.replicates,
        config.bench_command.as_ref().map_or(
            "none (no fitness is measured; no arm can pass)".to_string(),
            |c| c.join(" ")
        ),
        if a.arm_sandbox {
            "sandbox-exec"
        } else {
            "off (no filesystem sandbox)"
        },
    );
    if a.dry_run {
        println!(
            "   Dry-run mode: nothing built. Arm directories would go under {}",
            scratch.display()
        );
        return Ok(());
    }
    let host = HostToolchain::detect(&a.repo_root)?;
    let report = MultiArmEvolutionRunner::new(config.consensus_threshold, &scratch)
        .run_quarantined(&a.repo_root, "HEAD", &arms, &host, &config)
        .await?;
    println!("{}", report.to_pr_markdown());
    let rel_report = std::path::PathBuf::from("evolve-arms").join(format!(
        "{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    let path = write_selfware_artifact_checked(
        &a.repo_root,
        &rel_report,
        serde_json::to_string_pretty(&report)?,
    )?;
    println!(
        "   Report: {} (nothing was applied to the repository; the winning diff is in the report)",
        path.display()
    );
    Ok(())
}

pub(crate) async fn handle_improve_command(
    config: Config,
    ctx: &WorkshopContext,
    quiet: bool,
    dry_run: bool,
    continuous: bool,
    max_cycles: usize,
) -> Result<()> {
    use crate::cognitive::self_edit::SelfEditOrchestrator;

    if !quiet {
        println!("{}", render_header(ctx));
        println!(
            "\n{} {}\n",
            Glyphs::gear(),
            "Self-Improvement Analysis".workshop_title()
        );
    }

    let project_root = std::env::current_dir()?;
    if let Err(e) = crate::safety::killswitch::check_killswitch(Some(&project_root)) {
        eprintln!("Killswitch active: autonomous improvement disabled ({e})");
        return Err(anyhow::anyhow!("Killswitch active: {e}"));
    }
    let orchestrator = SelfEditOrchestrator::new(project_root.clone());
    let targets = orchestrator.analyze_self();

    if targets.is_empty() {
        println!(
            "   {} No improvement targets found. The codebase looks good!",
            Glyphs::bloom()
        );
        return Ok(());
    }

    println!(
        "   {} Found {} improvement targets:\n",
        Glyphs::magnifier(),
        targets.len().to_string().emphasis()
    );

    for (i, target) in targets.iter().take(10).enumerate() {
        let file_info = target.file.as_deref().unwrap_or("(no specific file)");
        println!(
            "   {}. [{}] {} (priority: {:.2})",
            i + 1,
            target.category,
            target.description,
            target.priority
        );
        println!(
            "      File: {} | Source: {:?}",
            file_info.path_local(),
            target.source
        );
    }

    if dry_run {
        println!("\n   {} Dry-run mode: no changes applied.", Glyphs::leaf());
        return Ok(());
    }

    let cycles = if continuous { max_cycles } else { 1 };
    let mut agent = Agent::new(config)
        .await?
        .with_confirmation_timeout(ONE_SHOT_CONFIRM_TIMEOUT);

    for cycle in 0..cycles {
        if let Err(e) = crate::safety::killswitch::check_killswitch(Some(&project_root)) {
            eprintln!("Killswitch tripped during improvement loop: {e}");
            return Err(anyhow::anyhow!("Killswitch active: {e}"));
        }
        let targets = orchestrator.analyze_self();
        // Protected-path gate, shared with the evolution daemon
        // (evolution::is_protected): filter out protected targets BEFORE
        // calling select_target so a high-priority protected target does not
        // cause an infinite skip loop across cycles.
        let targets: Vec<_> = targets
            .into_iter()
            .filter(|t| {
                let is_protected = t
                    .file
                    .as_deref()
                    .is_some_and(|f| crate::evolution::is_protected(std::path::Path::new(f)));
                if is_protected {
                    println!(
                        "   {} Skipping target: '{}' is a protected path (self-modification \
                         safety policy) — the improvement agent must not edit it.",
                        Glyphs::frost(),
                        t.file.as_deref().unwrap_or("(no file)")
                    );
                }
                !is_protected
            })
            .collect();
        let Some(target) = orchestrator.select_target(&targets) else {
            println!(
                "\n   {} No more improvement targets. Done!",
                Glyphs::bloom()
            );
            break;
        };

        println!(
            "\n   {} Cycle {}/{}: applying '{}'",
            Glyphs::gear(),
            cycle + 1,
            cycles,
            target.description
        );

        // Capture the pre-run HEAD so the post-run gate can also see
        // changes the agent COMMITTED during the run (those vanish
        // from `git status --porcelain`; follow-up review finding),
        // and snapshot the whole pre-run tree so a failed cycle can
        // roll the live checkout back (follow-up review W1a: broken /
        // policy-violating edits used to stay dirty with no rollback).
        let pre_run_head = current_head_sha(&project_root).await;
        let snapshot = snapshot_improve_tree(&project_root, pre_run_head.as_deref()).await;

        let prompt = orchestrator.build_improvement_prompt(target);
        match agent.run_task(&prompt).await {
            Ok(()) => {
                // Ok from the agent means "task finished", not "the
                // codebase still passes the gates". `improve` runs on
                // the live checkout, so a pre-commit-style gate runs
                // HERE (fmt/clippy/test + protected-path sweep) before
                // any success is printed; a run whose tree fails the
                // gate is reported as failed, never as applied.
                match run_improvement_gates(&project_root, pre_run_head.as_deref()).await {
                    Ok(()) => {
                        println!("   {} Improvement applied successfully.", Glyphs::bloom());
                    }
                    Err(gate_err) => {
                        let rollback = rollback_improve_tree(&project_root, &snapshot).await;
                        return Err(anyhow::anyhow!(
                            "improvement gate failed — the attempt was rolled back. \
                             {}; Gate error: {gate_err:#}",
                            rollback.render()
                        ));
                    }
                }
            }
            Err(e) => {
                println!("   {} Improvement failed: {}", Glyphs::frost(), e);
                let rollback = rollback_improve_tree(&project_root, &snapshot).await;
                return Err(anyhow::anyhow!(
                    "improvement agent failed — the run did not complete and its \
                     changes were rolled back. {}; Agent error: {e:#}",
                    rollback.render()
                ));
            }
        }
    }

    Ok(())
}

pub(crate) struct EvolveCommandArgs {
    pub(crate) generations: usize,
    pub(crate) population: usize,
    pub(crate) parallel: usize,
    pub(crate) dry_run: bool,
    pub(crate) workflow: String,
    pub(crate) arms_file: Option<std::path::PathBuf>,
    pub(crate) consensus: usize,
    pub(crate) replicates: usize,
    pub(crate) bench_cmd: Option<String>,
    pub(crate) arm_sandbox: bool,
    pub(crate) keep_arms: bool,
}

pub(crate) async fn handle_evolve_command(
    config: &Config,
    ctx: &WorkshopContext,
    quiet: bool,
    args: EvolveCommandArgs,
) -> Result<()> {
    let EvolveCommandArgs {
        generations,
        population,
        parallel,
        dry_run,
        workflow,
        arms_file,
        consensus,
        replicates,
        bench_cmd,
        arm_sandbox,
        keep_arms,
    } = args;

    if !quiet {
        println!("{}", render_header(ctx));
    }

    let repo_root = std::env::current_dir()?;
    if let Err(e) = crate::safety::killswitch::check_killswitch(Some(&repo_root)) {
        eprintln!("Killswitch active: evolution disabled ({e})");
        return Err(anyhow::anyhow!("Killswitch active: {e}"));
    }

    if workflow == "rsi" {
        // RSI Orchestrator workflow: recursive self-improvement with
        // circuit breaker, fitness measurement, meta-learning, and
        // state persistence across restarts.
        use crate::cognitive::rsi_orchestrator::RSIOrchestrator;

        if !quiet {
            println!(
                "\n{} {}\n",
                Glyphs::gear(),
                "RSI Orchestrator".workshop_title()
            );
        }

        if dry_run {
            println!("   RSI project root: {}", repo_root.display());
            println!("\n   {} Dry-run mode: no RSI loop started.", Glyphs::leaf());
            return Ok(());
        }

        let mut orchestrator =
            RSIOrchestrator::new(repo_root).with_max_iterations_per_run(generations);
        if !quiet {
            // `-g/--generations` bounds the RSI workflow too: its
            // per-run iteration ceiling is what actually stops a run,
            // so ignoring the flag made `-g 1` a silent no-op here.
            println!(
                "   RSI per-run ceiling: {} iteration(s) (from -g/--generations); \
                 --population/--parallel do not apply to this workflow.",
                generations
            );
        }
        match orchestrator.run_loop().await {
            Ok(()) => {
                println!("\n   {} RSI loop completed successfully.", Glyphs::bloom());
            }
            // Propagate instead of printing: returning Ok on a stop
            // made a circuit-breaker halt indistinguishable from
            // success to whatever scheduled the command.
            Err(e) => {
                eprintln!("\n   {} RSI loop stopped: {}", Glyphs::frost(), e);
                return Err(anyhow::anyhow!("RSI loop stopped: {e}"));
            }
        }
    } else if workflow == "replay" {
        use crate::evolution::policy::{
            BreadthFirstPolicy, FixedPopulationPolicy, ParetoAdaptivePolicy, RefineTop1Policy,
        };
        use crate::evolution::replay::{PromotionReadiness, ReplaySimulator, SearchPolicyFactory};
        use crate::evolution::tree_log::AttemptTree;

        if !quiet {
            println!(
                "\n{} {}\n",
                Glyphs::gear(),
                "Dream-RSI Offline Policy Replay & Held-Out Validation".workshop_title()
            );
        }

        let attempts_dir = repo_root.join(".selfware").join("attempts");
        if !attempts_dir.exists() {
            println!(
                "   {} No attempt history found at .selfware/attempts/",
                Glyphs::leaf()
            );
            return Ok(());
        }

        let mut log_files: Vec<std::path::PathBuf> = std::fs::read_dir(&attempts_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
            .collect();
        log_files.sort();

        if log_files.is_empty() {
            println!(
                "   {} No JSONL attempt trees found in .selfware/attempts/",
                Glyphs::leaf()
            );
            return Ok(());
        }

        let mut loaded_trees = Vec::new();
        for f in &log_files {
            match AttemptTree::load_from_jsonl(f) {
                Ok(t) if !t.is_empty() => loaded_trees.push((f.clone(), t)),
                Ok(_) => {}
                Err(e) => {
                    eprintln!(
                        "   Warning: skipping corrupted attempt tree {}: {}",
                        f.display(),
                        e
                    );
                }
            }
        }

        if loaded_trees.is_empty() {
            println!(
                "   {} All JSONL attempt trees in .selfware/attempts/ were empty or corrupt",
                Glyphs::leaf()
            );
            return Ok(());
        }

        let total_attempts: usize = loaded_trees.iter().map(|(_, t)| t.len()).sum();
        println!(
            "   Loaded {} attempt tree(s) with {} total attempts across {}\n",
            loaded_trees.len(),
            total_attempts,
            attempts_dir.display()
        );

        let (discovery_trees, validation_trees) = if loaded_trees.len() > 1 {
            let val_count = ((loaded_trees.len() as f64 * 0.30).round() as usize)
                .clamp(1, loaded_trees.len() - 1);
            let split_idx = loaded_trees.len() - val_count;
            let disc: Vec<AttemptTree> = loaded_trees[..split_idx]
                .iter()
                .map(|(_, t)| t.clone())
                .collect();
            let val: Vec<AttemptTree> = loaded_trees[split_idx..]
                .iter()
                .map(|(_, t)| t.clone())
                .collect();
            (disc, val)
        } else {
            let (path, single_tree) = &loaded_trees[0];
            match single_tree.split_held_out(0.40) {
                Ok((disc, val)) => (vec![disc], vec![val]),
                Err(e) => {
                    println!(
                        "   {} Attempt history in {} cannot be partitioned for held-out validation: {}. Replay requires at least 2 independent branches or multiple trees.",
                        Glyphs::leaf(),
                        path.display(),
                        e
                    );
                    return Ok(());
                }
            }
        };

        let disc_nodes: usize = discovery_trees.iter().map(|t| t.len()).sum();
        let val_nodes: usize = validation_trees.iter().map(|t| t.len()).sum();
        println!(
            "   Split: {} discovery attempts ({} trees) | {} held-out validation attempts ({} trees)\n",
            disc_nodes,
            discovery_trees.len(),
            val_nodes,
            validation_trees.len()
        );

        let pop_size = population.max(2);
        let candidate_factories: Vec<(&'static str, SearchPolicyFactory)> = vec![
            (
                "FixedPopulation (Incumbent)",
                Box::new(move || Box::new(FixedPopulationPolicy::new(pop_size))),
            ),
            (
                "BreadthFirstPolicy",
                Box::new(|| Box::new(BreadthFirstPolicy::new(3))),
            ),
            (
                "RefineTop1Policy",
                Box::new(|| Box::new(RefineTop1Policy::new(3))),
            ),
            (
                "ParetoAdaptivePolicy",
                Box::new(|| Box::new(ParetoAdaptivePolicy::new())),
            ),
        ];

        let outcome = ReplaySimulator::evaluate_candidates_full(
            &discovery_trees,
            &validation_trees,
            0.0,
            parallel,
            &candidate_factories,
            0.2,
        )?;
        let summaries = &outcome.summaries;

        println!("┌─────────────────────────────┬──────────────┬──────────────┬──────────────┬──────────────┬─────────────┐");
        println!("│ Policy                      │ Disc Probes  │ Disc Tokens  │ Val Score    │ Val Obj J    │ Incumbent?  │");
        println!("├─────────────────────────────┼──────────────┼──────────────┼──────────────┼──────────────┼─────────────┤");
        for s in summaries {
            let status = if s.policy_name.contains("Incumbent") {
                "Baseline"
            } else if s.beats_incumbent {
                "★ Beats Inc"
            } else {
                "Sub-optimal"
            };

            println!(
                "│ {:<27} │ {:>12} │ {:>12} │ {:>12.4} │ {:>12.4} │ {:<11} │",
                s.policy_name,
                s.discovery_probes,
                s.discovery_tokens,
                s.validation_terminal_score,
                s.validation_objective,
                status
            );
        }
        println!("└─────────────────────────────┴──────────────┴──────────────┴──────────────┴──────────────┴─────────────┘");

        if let Some(ref stab) = outcome.discovery_stability {
            let status_str = if stab.is_stable {
                "STABLE (W >= 0.70)"
            } else {
                "VOLATILE (W < 0.70)"
            };
            println!(
                "\n   {} Ranking Concordance across {} discovery tree(s): Kendall's W = {:.4} [{}]",
                if stab.is_stable {
                    Glyphs::bloom()
                } else {
                    Glyphs::frost()
                },
                stab.tree_count,
                stab.kendall_w,
                status_str
            );
        }
        if let Some(ref stab) = outcome.validation_stability {
            let status_str = if stab.is_stable {
                "STABLE (W >= 0.70)"
            } else {
                "VOLATILE (W < 0.70)"
            };
            println!(
                "   {} Ranking Concordance across {} validation tree(s): Kendall's W = {:.4} [{}]",
                if stab.is_stable {
                    Glyphs::bloom()
                } else {
                    Glyphs::frost()
                },
                stab.tree_count,
                stab.kendall_w,
                status_str
            );
        }

        match outcome.promotion_readiness() {
            PromotionReadiness::Ready {
                winner_name,
                validation_objective,
                incumbent_objective,
                kendall_w,
                beta,
            } => {
                println!(
                    "\n   {} Policy '{}' won on held-out validation (Val Obj: {:.4} vs Incumbent {:.4}, W = {:.4}, Beta = {:.2}). Ready for promotion.",
                    Glyphs::bloom(),
                    winner_name,
                    validation_objective,
                    incumbent_objective,
                    kendall_w,
                    beta,
                );
                let mut tree_digests = Vec::new();
                let mut tree_files = Vec::new();
                for (p, _) in &loaded_trees {
                    match crate::tools::file::read_to_string_capped(p, 16 * 1024 * 1024) {
                        Ok(content) => {
                            tree_digests.push(crate::evolution::tree_log::compute_sha256(
                                content.as_bytes(),
                            ));
                            tree_files.push(p.to_string_lossy().to_string());
                        }
                        Err(e) => {
                            anyhow::bail!(
                                "Promotion aborted: cannot read tree file {}: {}",
                                p.display(),
                                e
                            );
                        }
                    }
                }
                let report_digest = outcome.report_digest();
                let evidence_hash = crate::evolution::replay::compute_policy_evidence_hash(
                    &winner_name,
                    validation_objective,
                    incumbent_objective,
                    kendall_w,
                    beta,
                    &tree_digests,
                    &report_digest,
                );
                let payload = serde_json::json!({
                    "policy_name": winner_name,
                    "promoted_at": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    "validation_objective": validation_objective,
                    "incumbent_objective": incumbent_objective,
                    "kendall_w": kendall_w,
                    "beta": beta,
                    "tree_files": tree_files,
                    "tree_digests": tree_digests,
                    "report_digest": report_digest,
                    "evidence_hash": evidence_hash,
                });
                if dry_run {
                    println!("   [dry-run] Skipping writing active policy to disk.");
                } else if let Ok(json_str) = serde_json::to_string_pretty(&payload) {
                    match write_selfware_artifact_checked(
                        &repo_root,
                        std::path::Path::new("active_policy.json"),
                        json_str,
                    ) {
                        Ok(active_policy_path) => {
                            println!(
                                "   {} Persisted promoted policy to {}",
                                Glyphs::leaf(),
                                active_policy_path.display()
                            );
                        }
                        Err(e) => {
                            anyhow::bail!("Failed to persist promoted active policy: {}", e);
                        }
                    }
                }
            }
            PromotionReadiness::BlockedByInstability {
                winner_name,
                validation_objective,
                incumbent_objective,
                kendall_w,
            } => {
                println!(
                    "\n   {} Promotion BLOCKED by ranking instability: Candidate '{}' has higher mean score ({:.4} vs {:.4}) but volatile rankings across validation trees (Kendall's W = {:.4} < 0.70). Retaining incumbent.",
                    Glyphs::frost(),
                    winner_name,
                    validation_objective,
                    incumbent_objective,
                    kendall_w,
                );
            }
            PromotionReadiness::RetainIncumbent {
                incumbent_objective,
                best_candidate_name,
                best_candidate_objective,
            } => {
                if let (Some(name), Some(score)) = (best_candidate_name, best_candidate_objective) {
                    println!(
                        "\n   {} Top candidate '{}' ({:.4}) did not beat incumbent baseline ({:.4}) on held-out validation. Retaining incumbent.",
                        Glyphs::leaf(),
                        name,
                        score,
                        incumbent_objective
                    );
                } else {
                    println!(
                        "\n   {} Incumbent policy remains optimal on held-out validation (Val Obj: {:.4}). Retaining incumbent.",
                        Glyphs::bloom(),
                        incumbent_objective
                    );
                }
            }
            PromotionReadiness::InsufficientEvidence { reason } => {
                println!(
                    "\n   {} Insufficient evidence for promotion: {reason}. Retaining incumbent.",
                    Glyphs::leaf()
                );
            }
        }

        // Persist evaluation outcome to .selfware/replay_validation_latest.json
        if dry_run {
            println!("   [dry-run] Skipping writing replay validation report to disk.");
        } else {
            let report_json = serde_json::to_string_pretty(&outcome).unwrap_or_default();
            if let Err(e) = write_selfware_artifact_checked(
                &repo_root,
                std::path::Path::new("replay_validation_latest.json"),
                report_json,
            ) {
                eprintln!(
                    "   Warning: failed to write replay validation report: {}",
                    e
                );
            }
        }

        return Ok(());
    } else if workflow == "arms" {
        return run_evolve_arms(EvolveArmsArgs {
            repo_root,
            arms_file,
            consensus,
            replicates,
            parallel,
            bench_cmd,
            arm_sandbox,
            keep_arms,
            dry_run,
        })
        .await;
    } else if workflow == "investigate" {
        use crate::evolution::investigate::{export_markdown, investigate_attempts_file};

        if !quiet {
            println!(
                "\n{} {}\n",
                Glyphs::gear(),
                "RSI Active Investigative Search & Review Interface".workshop_title()
            );
        }

        let attempts_dir = repo_root.join(".selfware").join("attempts");
        if !attempts_dir.exists() {
            println!(
                "   {} No attempt history found at .selfware/attempts/",
                Glyphs::leaf()
            );
            return Ok(());
        }

        let mut log_files: Vec<std::path::PathBuf> = std::fs::read_dir(&attempts_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
            .collect();
        log_files.sort();

        if log_files.is_empty() {
            println!(
                "   {} No JSONL attempt trees found in .selfware/attempts/",
                Glyphs::leaf()
            );
            return Ok(());
        }

        let latest_file = log_files.last().unwrap();
        if log_files.len() > 1 {
            println!(
                "   Found {} attempt log(s) across {}; analyzing latest: {}\n",
                log_files.len(),
                attempts_dir.display(),
                latest_file.display()
            );
        } else {
            println!("   Investigating latest log: {}\n", latest_file.display());
        }
        let dossiers = investigate_attempts_file(latest_file, &repo_root)?;

        println!("   Analyzed {} attempts in latest run.\n", dossiers.len());

        for dossier in &dossiers {
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("Attempt ID:  {}", dossier.attempt_id);
            println!(
                "Hypothesis:  {}",
                dossier.degrees.degree_1_intent.description
            );
            println!(
                "Files:       {:?}",
                dossier.degrees.degree_2_syntax.files_touched
            );
            println!(
                "Status:      {:?} (Score: {:.4})",
                dossier.degrees.degree_4_empirical.status,
                dossier
                    .degrees
                    .degree_4_empirical
                    .composite_score
                    .unwrap_or(0.0)
            );
            println!(
                "Consensus:   {} ({:.1}% approval [4-check static governance model (compile, safety, complexity, provenance)])",
                dossier.consensus.decision,
                dossier.consensus.consensus_score * 100.0
            );
            if !dossier.findings.is_empty() {
                println!(
                    "Findings:    {} opaque structure(s) flagged:",
                    dossier.findings.len()
                );
                for f in &dossier.findings {
                    println!("  - [{}] {}: {}", f.severity, f.category, f.title);
                    if f.citation.line_range == (0, 0) {
                        println!(
                            "    Citation: {} [line range unmeasured/unavailable in recorded tree]",
                            f.citation.file_path
                        );
                    } else {
                        println!(
                            "    Citation: {} (L{}-L{})",
                            f.citation.file_path, f.citation.line_range.0, f.citation.line_range.1
                        );
                    }
                }
            } else {
                println!("Findings:    None (Transparent contracts)");
            }
        }

        // Write full investigative report to .selfware/investigation_report_latest.md
        if dry_run {
            println!("\n   [dry-run] Skipping writing investigative report to disk.");
        } else if let Some(last_dossier) = dossiers.last() {
            let md = export_markdown(last_dossier);
            match write_selfware_artifact_checked(
                &repo_root,
                std::path::Path::new("investigation_report_latest.md"),
                md,
            ) {
                Ok(report_path) => {
                    println!(
                        "\n   {} Full investigative dossier exported to {}",
                        Glyphs::bloom(),
                        report_path.display()
                    );
                }
                Err(e) => {
                    eprintln!("   Failed to export investigative dossier: {e}");
                }
            }
        }

        return Ok(());
    } else {
        // Default evolution daemon workflow
        use crate::evolution::daemon;
        use crate::evolution::{
            EvolutionConfig, FitnessWeights, LlmConfig, MutationTargets, SafetyConfig,
        };

        if !quiet {
            println!(
                "\n{} {}\n",
                Glyphs::gear(),
                "Evolution Daemon".workshop_title()
            );
        }

        let evo_config = EvolutionConfig {
            generations,
            population_size: population,
            parallel_eval: parallel,
            checkpoint_interval: 5,
            fitness_weights: FitnessWeights::default(),
            mutation_targets: MutationTargets {
                config_keys: config.evolution.config_keys.clone(),
                prompt_logic: config
                    .evolution
                    .prompt_logic
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect(),
                tool_code: config
                    .evolution
                    .tool_code
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect(),
                cognitive: config
                    .evolution
                    .cognitive
                    .iter()
                    .map(std::path::PathBuf::from)
                    .collect(),
            },
            safety: SafetyConfig::default(),
            llm: {
                // Use the hypothesis_model profile if configured, else fall back to default
                let hypothesis_profile = config
                    .evolution
                    .hypothesis_model
                    .as_deref()
                    .and_then(|name| config.resolve_model(Some(name)));
                if let Some(profile) = hypothesis_profile {
                    tracing::info!(
                        "Evolution using '{}' model profile for hypothesis generation: {}",
                        config
                            .evolution
                            .hypothesis_model
                            .as_deref()
                            .unwrap_or("default"),
                        profile.model
                    );
                    LlmConfig {
                        endpoint: profile.endpoint.clone(),
                        model: profile.model.clone(),
                        api_key: profile.api_key.as_ref().map(|k| k.expose().to_string()),
                        max_tokens: profile.max_tokens,
                        temperature: profile.temperature,
                    }
                } else {
                    LlmConfig {
                        endpoint: config.endpoint.clone(),
                        model: config.model.clone(),
                        api_key: config.api_key.as_ref().map(|k| k.expose().to_string()),
                        max_tokens: config.max_tokens,
                        temperature: config.temperature,
                    }
                }
            },
        };

        if dry_run {
            println!("   Evolution config: {:?}", evo_config);
            println!(
                "\n   {} Dry-run mode: no evolution started.",
                Glyphs::leaf()
            );
            return Ok(());
        }

        // Catch panics from the daemon so an evolve bug (e.g. a
        // string-slice panic) surfaces as a clean error instead of
        // unwinding through the whole CLI. Worktree cleanup itself is
        // handled inside the daemon by drop guards, so no shadow
        // worktrees leak even when this fires.
        let result = futures::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(
            daemon::evolve(evo_config, &repo_root),
        ))
        .await;
        let result = match result {
            Ok(r) => r,
            Err(payload) => {
                let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic payload".to_string()
                };
                eprintln!(
                    "\n   {} Evolution daemon panicked: {}",
                    Glyphs::frost(),
                    msg
                );
                anyhow::bail!("Evolution daemon panicked: {}", msg);
            }
        };

        if result.outcome == "killed" {
            eprintln!(
                "\n   {} Evolution interrupted: shutdown requested ({} generations, {} improvements, duration {:.1}s)",
                Glyphs::frost(),
                result.generations_run,
                result.improvements.len(),
                result.total_duration.as_secs_f64(),
            );
            // SIGTERM -> Terminated (143); SIGINT/ESC -> Cancelled (130).
            return Err(crate::errors::AgentError::for_current_shutdown().into());
        }

        if let Some(ref abort_reason) = result.aborted {
            eprintln!(
                "\n   {} Evolution aborted: {}",
                Glyphs::frost(),
                abort_reason
            );
            anyhow::bail!("Evolution aborted: {}", abort_reason);
        }

        if result.outcome == "policy_stopped" {
            let reason = result
                .stop_reason
                .as_deref()
                .unwrap_or("search policy converged");
            println!(
                "\n   {} Evolution converged (policy stopped): {} ({} generations, {} improvements)",
                Glyphs::bloom(),
                reason,
                result.generations_run,
                result.improvements.len()
            );
        } else {
            println!(
                "\n   {} Evolution complete: {} generations, {} improvements",
                Glyphs::bloom(),
                result.generations_run,
                result.improvements.len()
            );
        }
        println!(
            "   SAB: {:.0} → {:.0} ({:+.1})",
            result.initial_sab_score,
            result.final_sab_score,
            result.final_sab_score - result.initial_sab_score
        );
        println!("   Duration: {:.0}s", result.total_duration.as_secs_f64());
    }

    Ok(())
}
