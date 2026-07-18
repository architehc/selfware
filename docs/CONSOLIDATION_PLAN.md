# Consolidation Plan — Duplicated Subsystems

Status: decision records only; merging is follow-up work, out of scope for the
2026-07-16 cleanup. Each entry picks a winner and sketches the merge path.

Status 2026-07-17: executed the `devops/container.rs` deletion (item 2) plus
removal of verified-caller-less modules: safety autonomy/sandbox/threat_modeling,
ui swarm_viz/input_handler/diff_viewer/selections, agent subagent/worktree,
observability dashboard trim, TUI swarm cluster, bin/swarm_folder_analyzer, and
the dead lib.rs shims container/sandbox/threat_modeling.

## 1. Workflow engines: `swl/` vs `orchestration/workflows.rs`

- **Winner:** `orchestration/workflows.rs` (production-ready, powers the SWL
  runtime path in the CLI). `swl/` is self-described EXPERIMENTAL with a
  module-wide `#![allow(dead_code)]`.
- **Merge path:** keep `swl` parsing/validation as a front-end that lowers to
  the orchestration executor; delete `swl/`'s own runtime/guardrails duplicate
  logic; then remove the `#[allow(dead_code)]`.

## 2. Container/process: `devops/` vs `tools/container`, `tools/process`

- **Winner:** `tools/container` + `tools/process` (what the agent actually calls).
- **Merge path:** `devops/process_manager.rs` is wired (process tool) — keep;
  `devops/container.rs` `ContainerManager` has zero callers — port any unique
  behaviors into `tools/container/`, then delete `devops/container.rs`.

## 3. Self-improvement: `evolution/` vs `cognitive/self_improvement`

- **Winner:** both survive, but need one front door. `evolution/` is the
  daemon/fitness/tournament engine; `cognitive/self_improvement` is in-session
  learning. Document the boundary: in-session learning feeds candidates to the
  evolution daemon; no third path may appear.

## 4. Token accounting (4+ places)

- Sites: `src/tokens.rs`, `src/token_count.rs`, `src/memory/mod.rs`,
  `src/agent/{compression,context,context_management}.rs`.
- **Winner:** `tokens.rs` for budgets/costs, `token_count.rs` as the single
  tokenizer backend (tiktoken/HF/heuristic). `memory/` and `agent/*` must call
  into those two instead of re-estimating.

## 5. SWE-bench locations (3)

- Sites: `src/bench_harness/swebench_pro/` (Rust runner),
  `scripts/swebench_pro/` (Python reference/spec), `system_tests/swe_bench_pro/` (fixtures/e2e).
- **Winner:** `src/bench_harness/swebench_pro/`. Keep `scripts/swebench_pro/`
  as the documented reference implementation; `system_tests/swe_bench_pro/`
  holds fixtures only. No new code in the system_tests copy.

## 6. `lib.rs` flat re-export shims

- `src/lib.rs:109-139` re-exports `pub(crate)` domain modules under legacy flat
  paths (`crate::checkpoint`, `crate::telemetry`, …), so everything has two
  import spellings.
- **Winner:** the module paths (`crate::session::checkpoint`, …).
- **Merge path:** migrate `agent/` (the main legacy consumer) to module paths,
  then delete the shims. Mechanical, one module at a time.

## 7. God files (follow-up refactors, no winner needed)

- `src/agent/tool_dispatch.rs` (5.9k), `src/agent/execution.rs` (4.8k),
  `src/testing/verification.rs` (4.0k), `src/cli/mod.rs` (3.9k),
  `src/agent/interactive.rs` (3.9k), `src/analysis/vector_store.rs` (3.4k),
  `src/devops/process_manager.rs` (3.3k), `src/tokens.rs` (3.1k).
- Split by responsibility when next touched for a feature; no bulk rewrite.

## Open items carried over from the 2026-07-16 cleanup plan

The 2026-07-16 repo-cleanup program (plan/spec under `docs/superpowers/`) is
executed and its plan documents are retired. Still unresolved:
- The merge work behind decision records 1, 3, 4, 5, 6, and 7 above remains
  follow-up; record 2 is only partially executed (see status note at top).
- Local disk reclaim (`target/` ~78G, `scripts/node_modules/` ~18M) was
  deliberately left to the user, per the design's "Out of scope" section.
