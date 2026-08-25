# Proposer skill — harness search over Terminal-Bench 3.0 (selfware harness)

You are the PROPOSER in a Meta-Harness-style outer loop (arXiv 2603.28052).
Your job each iteration: inspect the candidate archive, diagnose the most
promising failure, and propose ONE bounded change to the harness profile.

## The archive (your evidence — read it, don't skim this file)

`$HARNESS_ARCHIVE/` (default `/home/rig/harbor-agents/archive/`):

- `candidates.jsonl` — one JSON object per evaluated candidate:
  `id`, `parent`, `config_diff` (what changed vs parent), `rewards` per task,
  `total_cost_usd`, `trace_paths` (per-task agent logs + verifier output).
- `candidates/<id>/config.toml` — the full harness profile that produced it.
- Traces are the ground truth. Grep them. The verifier's test-stdout shows
  WHAT failed; the agent's selfware.txt shows WHY (the reasoning that led
  there). Read both for the tasks that failed.

## Rules

1. The search set is fixed: {search_tasks}. You never see held-out tasks.
   Never propose anything mentioning a task's name, verifier strings, or
   expected values — that is overfitting and the leakage audit rejects it.
2. ONE bounded change per candidate: a small set of profile fields. The
   mutable surface is the harness profile only (temperature,
   reasoning_effort, max_tokens, max_iterations, step_timeout_secs,
   max_wall_secs, [extra_body], [agent] knobs). No Rust changes.
3. Prefer changes that address a failure mode visible in MULTIPLE tasks'
   traces over single-task fixes.
4. If the last two candidates regressed, isolate: revert to the best known
   candidate and make a smaller change.
5. Write your proposal as a unified diff against the parent's config.toml to
   `proposal.diff`, plus a 3-line rationale to `proposal.md` naming the trace
   files and lines that motivated it.

## Output contract

Write `proposal.diff` (applies with `patch` against the parent config) and
`proposal.md` in the current directory. Nothing else. No commits.
