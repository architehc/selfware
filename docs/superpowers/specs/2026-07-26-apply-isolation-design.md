# Evolve Apply Isolation — Design

Date: 2026-07-26
Status: Draft (awaiting user review)
Origin: External review round 4, finding #2 (verified TRUE): `/api/actions/apply`
accepts model-generated instructions and spawns `selfware run --yolo` in the LIVE
working directory — no isolated worktree, no bounded diff, no rollback, no revision
lock — and two concurrent applies can race the same checkout (swarm agent-43 #2).

## 1. Goal

An apply run must never be able to (a) silently modify the live checkout outside
its declared scope, (b) interleave with a concurrent apply or user edit, or (c)
leave unrecoverable state. Every apply is: **staged in isolation → verified →
merged deliberately**, with the exact diff bound to a one-use token.

## 2. Design

### 2.1 Stage in an isolated shadow worktree

- `apply::spawn` (src/evolve/apply.rs:52) creates a per-run shadow worktree
  (machinery exists: `src/evolution/ast_tools.rs` shadow worktree + RAII
  `WorktreeGuard`) at the current HEAD, branch `evolve-apply/<run_id>`.
- The agent process runs with cwd = the shadow worktree (NOT the live checkout).
  Its `run --yolo` autonomy is unchanged — but confined.
- The shadow is pinned to a recorded `base_revision` (HEAD oid + worktree digest).

### 2.2 Bounded, verified diff

- On run completion, compute `git diff base..run-branch`. Reject (and discard the
  shadow) when:
  - the diff touches paths outside the action's declared scope (`scope_paths`
    from the action manifest; default: reject anything outside `src/` + `docs/`),
  - the diff is empty (report honestly: "agent produced no changes" — never
    report success per AGENTS.md §3),
  - the live checkout's HEAD moved during the run (revision lock violation —
    the merge would silently rebase the agent's work onto a moved base).
- `git2` (already a dependency) for the diff; no shelling to git needed.

### 2.3 One-use apply token + deliberate merge

- `POST /api/actions/apply` (existing) becomes **stage**: runs the agent, verifies
  the diff, and returns `{ run_id, diff_digest, files_changed, insertions,
  deletions, preview }` with HTTP 200 — nothing merged yet.
- New `POST /api/actions/apply/commit { run_id, diff_digest }`: merges the staged
  branch into the live checkout — only if `diff_digest` matches the staged diff
  exactly (one-use; a second call 404s) and HEAD still equals `base_revision`
  (otherwise 409 with "rebase required").
- The web UI gets a two-step flow: "Run action" → diff preview → "Apply".
- Rollback is structural: discard the worktree (nothing happened), or
  `git revert` the merge commit if already merged.

### 2.4 Serialization

- A single `apply_lock: tokio::sync::Mutex<()>` on the server serializes stage
  runs (concurrent applies were a verified race). Commit is cheap and takes the
  same lock.

## 3. Error taxonomy (typed, per the protocol spec)

- 409 `base_moved` — live HEAD changed during the run
- 422 `diff_out_of_scope` — lists the offending paths
- 422 `empty_diff` — agent produced no changes
- 404 `unknown_run` — bad/used token
- 500 — process/infra failures (existing mapping)

## 4. Non-goals

- Changing the agent's behavior inside the run (still `run --yolo`).
- PR/remote flows (local-only, same as today).
- Auto-merging without the commit step — human or caller confirmation is the point.

## 5. Testing

- Shadow isolation: apply run's writes do NOT appear in the live checkout until commit.
- Bounded diff: out-of-scope path → 422 with the path listed; empty diff → 422.
- Revision lock: move HEAD mid-run (test commits) → commit step 409s.
- Token: second commit call with same digest → 404; wrong digest → 404.
- Serialization: two concurrent applies complete without interleaved state.
- Rollback: discard shadow → live checkout byte-identical to base.
