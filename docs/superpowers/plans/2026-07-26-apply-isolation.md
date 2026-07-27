# Apply Isolation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** `/api/actions/apply` stages agent runs in an isolated shadow worktree with a bounded, verified diff and a one-use merge token — nothing touches the live checkout until `POST /api/actions/apply/commit`.

**Architecture:** `apply::spawn` creates a shadow worktree (existing `create_shadow_worktree` in src/evolution/ast_tools.rs:115) pinned to HEAD, runs the agent there under a global apply lock, then verifies the diff (scope + non-empty + base unmoved). A new commit endpoint merges only on exact `diff_digest` match and unchanged HEAD.

Spec: `docs/superpowers/specs/2026-07-26-apply-isolation-design.md` (read first).

## Global Constraints

- No new dependencies (`git2`, `sha2`, `uuid`, `tokio` are already deps).
- The agent invocation stays `run --yolo` — only its cwd changes.
- Honest status: empty diff is a typed 422, never "success with nothing".
- Existing apply tests must keep passing (adapted to staged semantics where behavior intentionally changed — call each out in the report).
- `cargo fmt` + `cargo clippy --all-targets -- -D warnings` clean (pre-commit gate).

---

### Task 1: Shadow worktree staging + apply lock

**Files:**
- Modify: `src/evolve/apply.rs` (spawn ~:52-110, ApplyRun struct, registry)
- Modify: `src/evolve/server.rs` (apply_action_handler call site ~:652-720)
- Test: `tests/evolve/` (new `apply_isolation_test.rs`, register in mod.rs)

**Interfaces:**
- Consumes: `crate::evolution::ast_tools::{create_shadow_worktree, cleanup_worktree}` (check exact re-export path), `git2::Repository`
- Produces:
  - `ApplyRun` gains `shadow_path: Option<PathBuf>`, `base_revision: Option<String>`, `diff: Option<StagedDiff>`
  - `pub struct StagedDiff { pub digest: String, pub files_changed: usize, pub insertions: usize, pub deletions: usize, pub preview: String }`
  - `pub static APPLY_LOCK: LazyLock<tokio::sync::Mutex<()>>` in apply.rs

- [ ] **Step 1: failing test** — apply run's writes appear in the shadow worktree, NOT the live checkout (fixture: temp repo with one committed file; stage a run that appends a line; assert live file unchanged while status is Running/Staged).
- [ ] **Step 2: implement** — spawn takes APPLY_LOCK for the whole stage; creates shadow at HEAD via git2 (record `head.oid`), runs agent with `current_dir(shadow)`, records `shadow_path` + `base_revision` in ApplyRun. On spawn failure: cleanup shadow, typed error.
- [ ] **Step 3: green + commit** — `feat(evolve): stage apply runs in isolated shadow worktrees`

### Task 2: Bounded diff verification

**Files:**
- Modify: `src/evolve/apply.rs` (run-completion path)

- [ ] **Step 1: failing tests** — (a) diff touching a path outside `src/`+`docs/` → run status `Rejected("diff_out_of_scope")` with the path named; (b) agent producing no changes → `Rejected("empty_diff")`.
- [ ] **Step 2: implement** — on child exit: compute `git2` diff `base..worktree` (patch text), sha256 digest, stats, capped preview (8KB). Scope rule: every changed path must start with `src/` or `docs/`. Store `StagedDiff`; status `Staged`. On violation: status `Rejected(reason)`, keep shadow for inspection, cleanup after 1h or on next apply (simple: cleanup on read).
- [ ] **Step 3: green + commit** — `feat(evolve): bounded diff verification for staged apply runs`

### Task 3: One-use commit endpoint

**Files:**
- Modify: `src/evolve/apply.rs` (new `commit_staged`), `src/evolve/server.rs` (route + handler)
- Test: `tests/evolve/apply_isolation_test.rs`

- [ ] **Step 1: failing tests** — (a) commit with correct digest merges into live checkout; (b) second call → 404 `unknown_run`; (c) wrong digest → 404; (d) HEAD moved between stage and commit → 409 `base_moved`.
- [ ] **Step 2: implement** — `POST /api/actions/apply/commit {run_id, diff_digest}`: registry lookup; digest must match `StagedDiff.digest` exactly; HEAD must equal `base_revision`; merge via git2 (merge the run branch; ff if possible, else merge commit); consume the run (one-use); cleanup shadow; return `{merged: true, new_head}`.
- [ ] **Step 3: green + commit** — `feat(evolve): one-use merge token for staged apply runs`

### Task 4: UI two-step flow

**Files:**
- Modify: `src/evolve/web/app.js` (actions panel)

- [ ] **Step 1: implement** — "Run action" now shows staged diff preview (files/insertions/deletions) with an "Apply" button that POSTs commit with the digest; rejected runs show the typed reason. `node --check` passes; visual verification by parent.
- [ ] **Step 2: commit** — `feat(evolve-ui): two-step apply flow with diff preview`

---

## Self-Review Notes
- Spec coverage: §2.1 → T1; §2.2 → T2; §2.3 → T3+T4; §2.4 → T1 (lock); §3 taxonomy → T2/T3; §5 tests → T1-T3.
- Type consistency: `StagedDiff`/`APPLY_LOCK`/`ApplyRun` fields named identically across tasks.
