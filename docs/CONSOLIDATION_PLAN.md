# Consolidation Plan — Duplicated Subsystems

Status: decision record. Each item is marked `[done]` / `[open]` / `[stale]`
with the date its status last changed.

Status 2026-07-17: executed the `devops/container.rs` deletion (item 2) plus
removal of verified-caller-less modules: safety autonomy/sandbox/threat_modeling,
ui swarm_viz/input_handler/diff_viewer/selections, agent subagent/worktree,
observability dashboard trim, TUI swarm cluster, bin/swarm_folder_analyzer, and
the dead lib.rs shims container/sandbox/threat_modeling.

Status 2026-07-28 (cleanup waves 1-3, −5,934 lines): item 4 fully executed —
token accounting was unified into `tokens.rs`/`token_count.rs`, then the whole
`tokens.rs` subsystem was deleted in wave 3, leaving `token_count.rs` as the
single accounting backend. Item 6 partially executed (the `redact`, `analysis`,
`ui::demo`, and `ui::tui` flat shims are gone). The item-1 note about swl's
`#![allow(dead_code)]` was stale: the attribute is now
`#![allow(dead_code, unused_imports, unused_variables)]`.

## 1. Workflow engines: `swl/` vs `orchestration/workflows.rs` — [open]

- **Winner:** `orchestration/workflows.rs` (production-ready, powers the SWL
  runtime path in the CLI). `swl/` is self-described EXPERIMENTAL with a
  module-wide `#![allow(dead_code, unused_imports, unused_variables)]`.
- **Merge path:** keep `swl` parsing/validation as a front-end that lowers to
  the orchestration executor; delete `swl/`'s own runtime/guardrails duplicate
  logic; then remove the `#![allow(...)]` blanket.

## 2. Container/process: `devops/` vs `tools/container`, `tools/process` — [done 2026-07-17]

- **Winner:** `tools/container` + `tools/process` (what the agent actually calls).
- **Merge path:** `devops/process_manager.rs` is wired (process tool) — keep;
  `devops/container.rs` `ContainerManager` has zero callers — port any unique
  behaviors into `tools/container/`, then delete `devops/container.rs`.

## 3. Self-improvement: `evolution/` vs `cognitive/self_improvement` — [open]

- **Winner:** both survive, but need one front door. `evolution/` is the
  daemon/fitness/tournament engine; `cognitive/self_improvement` is in-session
  learning. Document the boundary: in-session learning feeds candidates to the
  evolution daemon; no third path may appear.

## 4. Token accounting (4+ places) — [done 2026-07-28]

- Sites (historical): `src/tokens.rs`, `src/token_count.rs`, `src/memory/mod.rs`,
  `src/agent/{compression,context,context_management}.rs`.
- **Winner:** `token_count.rs` as the single accounting backend. Executed in
  two steps: accounting was first unified into `tokens.rs` (budgets/costs) +
  `token_count.rs` (tokenizer backend), then the `tokens.rs` subsystem was
  deleted outright in cleanup wave 3 (2026-07-28). `memory/` and `agent/*`
  now go through `crate::token_count`.

## 5. SWE-bench locations (3) — [open]

- Sites: `src/bench_harness/swebench_pro/` (Rust runner),
  `scripts/swebench_pro/` (Python reference/spec), `system_tests/swe_bench_pro/` (fixtures/e2e).
- **Winner:** `src/bench_harness/swebench_pro/`. Keep `scripts/swebench_pro/`
  as the documented reference implementation; `system_tests/swe_bench_pro/`
  holds fixtures only. No new code in the system_tests copy.

## 6. `lib.rs` flat re-export shims — [open, partially done 2026-07-28]

- `src/lib.rs:109-139` re-exports `pub(crate)` domain modules under legacy flat
  paths (`crate::checkpoint`, `crate::telemetry`, …), so everything has two
  import spellings.
- **Winner:** the module paths (`crate::session::checkpoint`, …).
- **Merge path:** migrate `agent/` (the main legacy consumer) to module paths,
  then delete the shims. Mechanical, one module at a time. Partially executed:
  the `redact`, `analysis`, `ui::demo`, and `ui::tui` shims were removed in the
  2026-07 cleanup waves.

## 7. God files (follow-up refactors, no winner needed) — [open]

- `src/agent/tool_dispatch/mod.rs` (2.7k after the wave-2 split),
  `src/agent/execution.rs` (4.8k), `src/testing/verification.rs` (4.0k),
  `src/cli/mod.rs` (4.5k), `src/agent/interactive.rs` (3.9k),
  `src/analysis/vector_store.rs` (3.4k), `src/devops/process_manager.rs` (3.3k).
  (`src/tokens.rs`, previously on this list, was deleted in wave 3.)
- Split by responsibility when next touched for a feature; no bulk rewrite.

## 8. ContextGuard pruned — [done 2026-07-24]

`src/safety/context_guard.rs` (620 lines, heuristic substring scanner) had zero
production call sites — only its own tests. Deleted along with the
`safety::context_guard` re-exports. If injection defense is needed later
(roadmap Rec 2), rebuild it against a real ingestion path (web search / MCP
payloads entering the agent loop) rather than resurrecting the heuristic scanner.

## 9. ContextEnvelope — [done 2026-07-25]

Evidence paths now ship tier-projected content (Map=cards, Lite=skeletons,
Compact=reduced source) from a content-hashed `ContextEnvelope`; preview and
outbound responses share `content_hash`, and pinned over-budget tiers are
rejected with typed 422. The composer manifest / chat system prompt / expand
remain on the old path — candidates for the fast-follow unification.
Follow-up: /api/assistant/task evidence is not yet tier-projected (it selects
via task-aware neighborhoods with its own compact flag; extending the envelope
there is a separate design).

## Open items carried over from the 2026-07-16 cleanup plan

The 2026-07-16 repo-cleanup program (plan/spec under `docs/superpowers/`) is
executed and its plan documents are retired. Still unresolved:
- The merge work behind decision records 1, 3, 5, 6, and 7 above remains
  follow-up; records 2, 4, 8, and 9 are done (see the per-item markers).
- Local disk reclaim (`target/` ~78G, `scripts/node_modules/` ~18M) was
  deliberately left to the user, per the design's "Out of scope" section.
