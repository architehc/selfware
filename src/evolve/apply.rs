//! Applying evolve actions by driving the agent (`selfware run`) as a subprocess.
//!
//! A node-action like `merge_duplicate` describes *what* to change; applying it
//! means actually editing the code. Rather than reimplement editing, this spawns
//! `selfware run "<task>" --yolo` (the same agent, headless + auto-approve),
//! streams its output into a run registry, and reports status — so the UI can
//! kick off a consolidation and watch it land.
//!
//! Safety: the caller must ensure a clean working tree first (checked at the
//! endpoint), so the resulting diff is exactly the agent's work and reviewable.
//!
//! Isolation: each run is staged in a shadow git worktree created at the live
//! checkout's HEAD (`crate::evolution::ast_tools::create_shadow_worktree`), and
//! the agent subprocess runs with that worktree as its cwd — writes never touch
//! the live checkout. Runs are serialized through [`APPLY_LOCK`], held from
//! staging until the agent process exits.
//!
//! Verification: when a run exits successfully, its staged diff is verified
//! (`verify_staged_diff`) — base tree vs. the shadow's workdir, computed via
//! the shadow's index (no commit is made, so the run branch stays untouched
//! for the later merge step). The diff must be non-empty and stay inside
//! `src/` + `docs/`, otherwise the run is `Rejected` with a typed reason
//! and the shadow is kept for inspection. Verified runs become `Staged` and
//! carry a [`StagedDiff`] (digest + stats + capped preview) for the commit
//! step to bind against.
//!
//! Commit: [`commit_staged`] is the one-use merge endpoint's core. The caller
//! must present the run id plus the exact [`StagedDiff::digest`]; the live
//! checkout's HEAD must still equal the run's `base_revision`. The merge
//! itself commits the shadow's staged state (the same index
//! `verify_staged_diff` wrote, re-staged to capture the full workdir) and
//! fast-forwards the live branch to it — the worktrees share one object store
//! and `parent(new) == base == live HEAD`, so this is exactly
//! `git merge --ff-only` plus a safe checkout that refuses to clobber local
//! edits. A merged run is consumed (removed, shadow cleaned), so a second
//! commit with the same token is an `unknown_run` 404.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock};

use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Mutex, MutexGuard};

use crate::evolution::ast_tools::{cleanup_worktree, create_shadow_worktree_named};

/// Serializes apply runs: two concurrent applies must never race the same
/// checkout. The guard is taken at staging time and held (inside the spawned
/// exit-watcher task) until the agent process exits.
pub static APPLY_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    Running,
    /// Agent exited 0 and the staged diff passed verification; awaiting the
    /// deliberate commit step.
    Staged,
    /// Agent exited 0 but the staged diff failed verification (out of scope or
    /// empty); the payload names the typed reason.
    Rejected(String),
    /// Merged into the live checkout by the commit step.
    Succeeded,
    Failed,
}

/// Summary of the diff an apply run staged in its shadow worktree (base..run).
/// Populated when the run is verified; `None` while the run is in flight.
#[derive(Debug, Clone, Serialize)]
pub struct StagedDiff {
    pub digest: String,
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub preview: String,
}

/// One agent-driven apply run.
#[derive(Debug, Clone, Serialize)]
pub struct ApplyRun {
    pub id: String,
    pub prompt: String,
    pub status: ApplyStatus,
    pub output: String,
    pub exit_code: Option<i32>,
    /// Shadow worktree the agent runs in (isolated from the live checkout).
    pub shadow_path: Option<PathBuf>,
    /// Live checkout HEAD oid the shadow was created at (revision-lock base).
    pub base_revision: Option<String>,
    /// Verified staged diff, computed on run completion.
    pub diff: Option<StagedDiff>,
}

/// Shared registry of in-flight / finished apply runs.
pub type ApplyRegistry = Arc<Mutex<HashMap<String, ApplyRun>>>;

pub fn new_registry() -> ApplyRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

/// The maximum output kept per run (bytes), so a chatty agent can't grow memory
/// unbounded; older output is dropped from the front.
const MAX_OUTPUT: usize = 200_000;

/// Cap on the diff preview kept per run (bytes), so the status endpoint stays
/// cheap even for large staged diffs.
const MAX_PREVIEW: usize = 8 * 1024;

/// Cap on retained shadow worktrees: when a new apply starts and finds more
/// than this many old shadows under `.worktrees/`, the oldest are pruned.
/// Staged and Rejected runs keep their shadows (for merge / inspection) until
/// they are removed from the registry or fall off this cap.
const MAX_KEPT_SHADOWS: usize = 3;

/// Why a staged diff was rejected (typed reasons per the isolation spec §3).
/// `Display` renders the exact reason string stored in
/// `ApplyStatus::Rejected`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    /// The diff touched a path outside `src/` + `docs/`; carries the first
    /// offending path.
    OutOfScope(String),
    /// The agent produced no changes.
    Empty,
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RejectReason::OutOfScope(path) => write!(f, "diff_out_of_scope: {path}"),
            RejectReason::Empty => write!(f, "empty_diff"),
        }
    }
}

/// Verify the diff a run staged in its shadow worktree: base tree vs. the
/// shadow's workdir. The workdir is first staged into the shadow's index
/// (`git add -A` equivalent, no commit — the run branch stays untouched for
/// the merge step) because libgit2 produces no patch content or line stats for
/// untracked files in a raw tree→workdir diff; tree→index covers new files,
/// modifications, and deletions uniformly (respecting .gitignore).
///
/// Returns `Ok(Ok(StagedDiff))` when the diff is non-empty and fully inside
/// `src/` + `docs/`; `Ok(Err(RejectReason))` for a typed rejection; `Err` for
/// infra failures (git2), which the caller reports as a Failed run.
pub fn verify_staged_diff(
    shadow_path: &Path,
    base_revision: &str,
) -> std::result::Result<std::result::Result<StagedDiff, RejectReason>, git2::Error> {
    let repo = git2::Repository::open(shadow_path)?;
    let base = repo
        .find_commit(git2::Oid::from_str(base_revision)?)?
        .tree()?;

    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;
    let diff = repo.diff_tree_to_index(Some(&base), None, None)?;

    if diff.deltas().len() == 0 {
        return Ok(Err(RejectReason::Empty));
    }

    // Scope rule: every changed path must stay inside src/ or docs/.
    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !(path.starts_with("src/") || path.starts_with("docs/")) {
            return Ok(Err(RejectReason::OutOfScope(path)));
        }
    }

    let stats = diff.stats()?;
    let mut patch = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        // Include the origin marker so distinct patches can't share a digest —
        // content() alone strips the leading +/-/_space.
        patch.push(line.origin() as u8);
        patch.extend_from_slice(line.content());
        true
    })?;

    let digest = format!("{:x}", Sha256::digest(&patch));
    let preview = String::from_utf8_lossy(&patch)
        .chars()
        .take(MAX_PREVIEW)
        .collect();

    Ok(Ok(StagedDiff {
        digest,
        files_changed: stats.files_changed(),
        insertions: stats.insertions(),
        deletions: stats.deletions(),
        preview,
    }))
}

/// Prune old shadow worktrees down to [`MAX_KEPT_SHADOWS`] (oldest first, by
/// mtime), skipping `protected` paths (shadows of runs still in the registry).
/// Best-effort: individual cleanup failures are ignored so staging never fails
/// over housekeeping.
fn prune_old_shadows(project_root: &Path, protected: &[PathBuf]) {
    let worktrees_dir = project_root.join(".worktrees");
    let Ok(entries) = std::fs::read_dir(&worktrees_dir) else {
        return;
    };
    let mut shadows: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && !protected.contains(p)
                // Only prune OUR namespace — the mutation-testing daemon's
                // `evolution-*` worktrees are protected by WorktreeGuard
                // invisible to us and must never be reaped.
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("evolve-apply-"))
        })
        .filter_map(|p| {
            p.metadata()
                .and_then(|m| m.modified())
                .ok()
                .map(|mtime| (mtime, p))
        })
        .collect();
    if shadows.len() <= MAX_KEPT_SHADOWS {
        return;
    }
    shadows.sort_by_key(|(mtime, _)| *mtime);
    let excess = shadows.len() - MAX_KEPT_SHADOWS;
    for (_, path) in shadows.into_iter().take(excess) {
        let _ = cleanup_worktree(project_root, &path);
    }
}

/// Typed staging/spawn failures for apply runs (per the isolation spec's error
/// taxonomy; the HTTP layer maps these to 500s today).
#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("failed to create shadow worktree: {0}")]
    ShadowWorktree(#[from] crate::evolution::ast_tools::WorktreeError),
    #[error("failed to read live checkout HEAD: {0}")]
    BaseRevision(String),
    #[error("failed to spawn agent process: {0}")]
    Spawn(std::io::Error),
}

/// A staged apply run: shadow worktree created at the live HEAD, run
/// registered, [`APPLY_LOCK`] held. The guard must be kept alive until the
/// agent process exits (the exit-watcher task owns it in [`spawn`]).
#[derive(Debug)]
pub struct StagedRun {
    pub id: String,
    pub shadow_path: PathBuf,
    pub base_revision: String,
    pub guard: MutexGuard<'static, ()>,
}

/// Stage an apply run without launching the agent: take the apply lock, create
/// a shadow worktree at the live checkout's HEAD, record the base revision, and
/// register the run as Running. On any failure the shadow is cleaned up, the
/// run is registered as Failed, and a typed error is returned.
pub async fn stage_run(
    prompt: String,
    project_root: PathBuf,
    registry: ApplyRegistry,
) -> std::result::Result<StagedRun, ApplyError> {
    let guard = APPLY_LOCK.lock().await;
    let id = format!("apply-{}", uuid::Uuid::new_v4().simple());

    // Simple shadow cap: a new apply prunes old shadows beyond the cap, keeping
    // the ones still referenced by registered runs.
    {
        let registry_guard = registry.lock().await;
        let protected: Vec<PathBuf> = registry_guard
            .values()
            .filter_map(|run| run.shadow_path.clone())
            .collect();
        drop(registry_guard);
        prune_old_shadows(&project_root, &protected);
    }

    // Read HEAD FIRST: an external commit between worktree creation and the
    // oid read would pin a base that doesn't match the shadow's actual base.
    let base_revision = match read_head_oid(&project_root) {
        Ok(oid) => oid,
        Err(e) => {
            register_failed(&registry, &id, &prompt).await;
            return Err(ApplyError::BaseRevision(e.to_string()));
        }
    };

    // Apply shadows live in the `evolve-apply-` namespace so lifecycle pruning
    // never reaps the mutation-testing daemon's `evolution-*` worktrees.
    let shadow_path = match create_shadow_worktree_named(
        &project_root,
        &format!("evolve-apply-{}", id.trim_start_matches("apply-")),
    ) {
        Ok(path) => path,
        Err(e) => {
            register_failed(&registry, &id, &prompt).await;
            return Err(e.into());
        }
    };

    registry.lock().await.insert(
        id.clone(),
        ApplyRun {
            id: id.clone(),
            prompt,
            status: ApplyStatus::Running,
            output: String::new(),
            exit_code: None,
            shadow_path: Some(shadow_path.clone()),
            base_revision: Some(base_revision.clone()),
            diff: None,
        },
    );

    Ok(StagedRun {
        id,
        shadow_path,
        base_revision,
        guard,
    })
}

/// HEAD oid of the live checkout, recorded as the run's revision-lock base.
fn read_head_oid(project_root: &std::path::Path) -> std::result::Result<String, git2::Error> {
    let repo = git2::Repository::open(project_root)?;
    let oid = repo.head()?.peel_to_commit()?.id().to_string();
    Ok(oid)
}

/// Register a run that never got off the ground as Failed (honest status —
/// AGENTS.md §3), so callers can inspect it via the status endpoint.
async fn register_failed(registry: &ApplyRegistry, id: &str, prompt: &str) {
    registry.lock().await.insert(
        id.to_string(),
        ApplyRun {
            id: id.to_string(),
            prompt: prompt.to_string(),
            status: ApplyStatus::Failed,
            output: String::new(),
            exit_code: None,
            shadow_path: None,
            base_revision: None,
            diff: None,
        },
    );
}

/// Spawn `selfware run "<prompt>" --yolo` inside a staged shadow worktree of
/// `project_root`, streaming stdout + stderr into the run's output buffer.
/// Returns the run id immediately; the run continues in the background with
/// the apply lock held until the agent process exits.
pub async fn spawn(
    prompt: String,
    project_root: PathBuf,
    registry: ApplyRegistry,
) -> Result<String> {
    let exe = std::env::current_exe()?;
    let staged = stage_run(prompt.clone(), project_root.clone(), registry.clone()).await?;
    let id = staged.id;

    let mut child = match Command::new(exe)
        .arg("run")
        .arg(&prompt)
        .arg("--yolo")
        .current_dir(&staged.shadow_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            let _ = cleanup_worktree(&project_root, &staged.shadow_path);
            if let Some(run) = registry.lock().await.get_mut(&id) {
                run.status = ApplyStatus::Failed;
            }
            return Err(ApplyError::Spawn(e).into());
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let reg = registry.clone();
    let rid = id.clone();
    let shadow_for_verify = staged.shadow_path.clone();
    let base_for_verify = staged.base_revision.clone();
    tokio::spawn(async move {
        // Hold the apply lock across the whole staged run (spawn → exit).
        let _guard = staged.guard;
        tokio::join!(
            pump_pipe(stdout, reg.clone(), rid.clone()),
            pump_pipe(stderr, reg.clone(), rid.clone()),
        );
        let code = child.wait().await.ok().and_then(|s| s.code());
        // Verify the staged diff on successful exit; failures (non-zero exit or
        // git2 infra errors) stay honest Failed runs, typed rejections become
        // Rejected with the reason, and clean diffs become Staged.
        let verified = if code == Some(0) {
            Some(verify_staged_diff(&shadow_for_verify, &base_for_verify))
        } else {
            None
        };
        if let Some(run) = reg.lock().await.get_mut(&rid) {
            run.exit_code = code;
            match verified {
                Some(Ok(Ok(diff))) => {
                    run.status = ApplyStatus::Staged;
                    run.diff = Some(diff);
                }
                Some(Ok(Err(reason))) => {
                    run.status = ApplyStatus::Rejected(reason.to_string());
                }
                Some(Err(e)) => {
                    run.status = ApplyStatus::Failed;
                    run.output
                        .push_str(&format!("\ndiff verification failed: {e}\n"));
                    if run.output.len() > MAX_OUTPUT {
                        let cut = run.output.len() - MAX_OUTPUT;
                        run.output.drain(..cut);
                    }
                }
                None => {
                    run.status = ApplyStatus::Failed;
                }
            }
        }
    });

    Ok(id)
}

/// Stream one pipe's lines into the run's output buffer (bounded). Generic over
/// stdout/stderr, which are distinct concrete types.
async fn pump_pipe<R>(pipe: Option<R>, reg: ApplyRegistry, rid: String)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let Some(pipe) = pipe else { return };
    let mut lines = BufReader::new(pipe).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(run) = reg.lock().await.get_mut(&rid) {
            run.output.push_str(&line);
            run.output.push('\n');
            if run.output.len() > MAX_OUTPUT {
                let cut = run.output.len() - MAX_OUTPUT;
                run.output.drain(..cut);
            }
        }
    }
}

pub async fn get(registry: &ApplyRegistry, id: &str) -> Option<ApplyRun> {
    registry.lock().await.get(id).cloned()
}

/// Remove a run from the registry, cleaning up its shadow worktree if it still
/// exists. (The commit step consumes runs one-use this way.)
pub async fn remove(registry: &ApplyRegistry, id: &str, project_root: &Path) -> Option<ApplyRun> {
    let run = registry.lock().await.remove(id);
    if let Some(shadow) = run.as_ref().and_then(|r| r.shadow_path.as_ref()) {
        let _ = cleanup_worktree(project_root, shadow);
    }
    run
}

/// Outcome of a successful one-use merge ([`commit_staged`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeOutcome {
    /// OID of the new live-checkout HEAD after the fast-forward.
    pub new_head: String,
    /// Files changed by the merged diff (from the verified [`StagedDiff`]).
    pub files_changed: usize,
}

/// Typed commit-step failures (isolation spec §3). The HTTP layer maps these:
/// `UnknownRun` → 404 `unknown_run` (bad/used id *or* wrong digest — a wrong
/// digest is deliberately indistinguishable from a bad token, and does not
/// consume the run), `NotStaged` → 409 `not_staged` (a state conflict, the
/// same class as `base_moved`), `BaseMoved` → 409 `base_moved`, `Git` → 500.
#[derive(Debug, thiserror::Error)]
pub enum CommitError {
    #[error("unknown_run: {0}")]
    UnknownRun(String),
    #[error("not_staged: run is {0:?}; only Staged runs can be committed")]
    NotStaged(ApplyStatus),
    #[error("base_moved: staged at {base} but live HEAD is {head}; rebase required")]
    BaseMoved { base: String, head: String },
    #[error("merge failed: {0}")]
    Git(String),
}

/// Merge a staged run into the live checkout and consume it (one-use).
///
/// Takes [`APPLY_LOCK`] for the whole merge (commit is cheap; spec §2.4), then
/// checks, in order: run exists, run is `Staged`, `diff_digest` matches the
/// verified [`StagedDiff::digest`] exactly, and the live HEAD still equals the
/// run's `base_revision`. On success the shadow's staged state is committed
/// (message references the run id) and the live branch fast-forwarded to it;
/// the run is then removed via [`remove`], cleaning up the shadow worktree.
/// On any check failure the run stays registered so the caller can retry with
/// the right token or rebase.
pub async fn commit_staged(
    registry: &ApplyRegistry,
    run_id: &str,
    diff_digest: &str,
    project_root: &Path,
) -> std::result::Result<MergeOutcome, CommitError> {
    let _guard = APPLY_LOCK.lock().await;

    let run = get(registry, run_id)
        .await
        .ok_or_else(|| CommitError::UnknownRun(run_id.to_string()))?;

    if run.status != ApplyStatus::Staged {
        return Err(CommitError::NotStaged(run.status));
    }

    let (diff, base, shadow) = match (
        run.diff.as_ref(),
        run.base_revision.as_ref(),
        run.shadow_path.as_ref(),
    ) {
        (Some(diff), Some(base), Some(shadow)) => (diff, base, shadow),
        // A Staged run always carries these; absence is an internal invariant
        // violation, reported honestly as an infra error (AGENTS.md §3).
        _ => {
            return Err(CommitError::Git(format!(
                "staged run {run_id} is missing its diff, base revision, or shadow path"
            )))
        }
    };

    // One-use token: the digest binds this exact diff. A wrong digest is
    // indistinguishable from a bad/used run id (spec §3) and does NOT consume
    // the run.
    if diff.digest != diff_digest {
        return Err(CommitError::UnknownRun(run_id.to_string()));
    }

    // Revision lock: the live checkout must not have moved since staging.
    let head = read_head_oid(project_root).map_err(|e| CommitError::Git(e.to_string()))?;
    if head != *base {
        return Err(CommitError::BaseMoved {
            base: base.clone(),
            head,
        });
    }

    let files_changed = diff.files_changed;
    let new_head = merge_shadow(shadow, project_root, base, run_id, &run.prompt)?;

    // Consume one-use: removal also cleans up the shadow worktree. The merge
    // commit lives on in the shared object store.
    remove(registry, run_id, project_root).await;

    Ok(MergeOutcome {
        new_head,
        files_changed,
    })
}

/// The merge itself: commit the shadow's staged state and fast-forward the
/// live branch to it. Both worktrees share one object store, and the caller
/// verified `parent(new) == base == live HEAD`, so this is exactly
/// `git merge --ff-only`: the checkout is done first (safe mode — refuses to
/// clobber local edits), and only then is the branch ref moved, so a failed
/// checkout leaves the live ref untouched.
fn merge_shadow(
    shadow_path: &Path,
    project_root: &Path,
    base_revision: &str,
    run_id: &str,
    prompt: &str,
) -> std::result::Result<String, CommitError> {
    fn git_err(e: git2::Error) -> CommitError {
        CommitError::Git(e.to_string())
    }

    let shadow_repo = git2::Repository::open(shadow_path).map_err(git_err)?;
    // Guard against retry-after-failed-checkout: the shadow HEAD must still be
    // the run's base, otherwise a second attempt would stack an empty commit
    // on the previous one (breaking the parent == base invariant).
    {
        let shadow_head = shadow_repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(git_err)?;
        if shadow_head.id().to_string() != base_revision {
            return Err(CommitError::BaseMoved {
                base: base_revision.to_string(),
                head: format!("shadow at {}", shadow_head.id()),
            });
        }
    }
    let mut index = shadow_repo.index().map_err(git_err)?;
    // Re-stage the workdir so the commit captures exactly what
    // verify_staged_diff indexed (add_all is idempotent).
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .map_err(git_err)?;
    index.write().map_err(git_err)?;
    let tree_id = index.write_tree().map_err(git_err)?;
    let tree = shadow_repo.find_tree(tree_id).map_err(git_err)?;
    let base_commit = shadow_repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(git_err)?;
    let signature = shadow_repo
        .signature()
        .or_else(|_| git2::Signature::now("selfware-evolve", "evolve@selfware.local"))
        .map_err(git_err)?;
    let summary: String = prompt
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(72)
        .collect();
    let message = format!("evolve apply {run_id}: {summary}");
    let new_oid = shadow_repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            &message,
            &tree,
            &[&base_commit],
        )
        .map_err(git_err)?;

    let live = git2::Repository::open(project_root).map_err(git_err)?;
    let new_commit = live.find_commit(new_oid).map_err(git_err)?;
    live.checkout_tree(
        new_commit.as_object(),
        Some(git2::build::CheckoutBuilder::new().safe()),
    )
    .map_err(git_err)?;
    live.head()
        .map_err(git_err)?
        .set_target(new_oid, "evolve apply: fast-forward staged run")
        .map_err(git_err)?;

    Ok(new_oid.to_string())
}
