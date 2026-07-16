# Selfware Repo Cleanup & Operationalization — Design

Date: 2026-07-16
Status: Approved by user (2026-07-16)

## Goal

Make the selfware repository fully operational (green CI gates, non-flaky tests),
clean (~650K of stale tracked content removed, dead modules gone), and surface
advanced subsystems that are implemented but unreachable. Execution is phased,
one reviewable commit stream per phase, test suite green after each phase.

## Current state (from 5-agent audit)

- `cargo check --all-targets`: 0 errors / 0 warnings (default + `extras`); clippy clean.
- ~8,740 tests; suite is flaky, not broken: 1–2 rotating failures per run from
  parallel-test races over process-global state (`HOME` mutation in
  `src/cognitive/memory_system.rs:418-666`, shared state in `agent::execution`,
  `agent::task_runner`, `mcp::transport` child reaping, credentials env test).
- `test_e2e_browser_pdf_round_trip` (`tests/e2e_tools_test.rs:560`) fails instead
  of skipping when no headless Chrome exists.
- CI `lint` job red: 11 files unformatted. `semver` job structurally broken
  (crate unpublished, git-only dep). `deny.toml` dormant (no CI job). RUSTSEC
  ignore lists drifted (6 in workflows vs 7 in deny.toml).
- Uncommitted working-tree work: `src/memory/swebench*.rs` deleted,
  `bench_harness/swebench_pro/runner.rs` + `memory/mod.rs` modified. Compiles clean.
- Dead code: `src/concurrent_queue/` and `src/kv_store/` have zero callers
  (kv_store is gated on the misnamed `tokens` feature); `interview.rs` reachable
  only via caller-less `templates::scaffold_from_context`; `observability/log_analysis`
  never invoked; `swl/codegen` never invoked; `system_tests/agent_regression.rs`
  has no `[[bin]]` target; `fuzz/fuzz_targets/fuzz_target_1` is an empty stub.
- Broken/stale scripts: `scripts/verify_122b_workflows.sh` (missing config),
  `scripts/fuzzy_apply.py` (zero refs), dev-runner cluster (`dev_runner.py`,
  `dev-test-runner.sh`, `advanced-dev-runner.sh`, `lightweight_dev_test.sh`,
  `parallel-runner.sh`, `visual_feedback_loop.py`; one provably broken).
- Stale tracked content: `IMPROVE_01..10_*.md` (~188K), `CODEBASE_REVIEW.md`,
  `BACKLOG.md`, 13 historical docs reviews, `experiments/interactive-ux-overhaul/`,
  `experiments/selfware-longrun-mega/`, tracked `benchmark_results/2026-06-24/`,
  `demos/` (unreferenced, needs external solana toolchain).
- Advanced but unsurfaced: `consolidation` (coded+tested, not in default features),
  `skills` (TUI-only), `log_analysis` (no callers), `interview`→`init` (coded,
  never invoked), `swl codegen` (no subcommand), swebench eval paths hardcode
  `/home/ivo/...`.
- CHANGELOG.md says 0.4.0-beta.1; Cargo.toml says 0.3.0-beta.1.
- Local disk reclaim available: `target/` 78G, `scripts/node_modules` 18M.

## Decisions (locked with user)

1. Scope: full program (health + deletions + feature surfacing + dedup decisions).
2. Dedup depth: dead-only now. Live duplicates (swl vs YAML workflows, devops vs
   tools/container, evolution vs cognitive self-improvement, token accounting ×4,
   3 SWE-bench locations) get a documented consolidation plan, merged in a follow-up.
3. Stale content: `git rm` outright (recoverable via git history), no in-tree archive.
4. Execution: phased sequential, selective parallelism only on disjoint file sets.

## Phase 0 — Unblock

- Commit the in-flight swebench_pro/memory work as its own commit.
- `cargo fmt` as a separate commit. Lint gate green immediately.

## Phase 1 — Test & CI health

- Serialize the 5 `HOME`-mutating tests in `src/cognitive/memory_system.rs:418-666`
  with an env lock (check `src/test_support.rs` for an existing helper first).
- Fix shared-state races: `agent::execution` task-state tracker,
  `agent::task_runner::test_list_tasks_multiple`, `mcp::transport` child-reaping
  timing, `agent` credentials env test.
- Widen the skip filter in `test_e2e_browser_pdf_round_trip` so missing headless
  Chrome skips instead of panics.
- CI: remove or gate the `semver` job; add `cargo deny check` job; sync RUSTSEC
  ignore lists between workflows and deny.toml.
- `.gitignore`: make the benchmark-results pattern cover `benchmark_results/`;
  add `/models/`.
- Gate: 3 consecutive green `cargo test` runs + clippy + fmt.

## Phase 2 — Deletions

Each `git rm` preceded by a reference grep. Verified untouchables:
`templates/`, `workflows/*.swl`, `scripts/playwright-bridge.js`,
`selfware-qa-schema.yaml`, `selfware.example.toml`, `scripts/swebench_pro/`,
`scripts/qa-orchestrator.py`, `scripts/report-aggregator.js`.

- Root: `IMPROVE_01..10_*.md`, `CODEBASE_REVIEW.md`, `BACKLOG.md`.
- `docs/`: COMPREHENSIVE_REVIEW.md, DEEP_DIVE_REVIEW.md, UX_RECOMMENDATIONS.md,
  CLAUDE_UX_RECOMMENDATIONS_SYNTHESIS.md, COMPARATIVE_ANALYSIS_AND_ACTION_PLAN.md,
  QWEN_CODE_CLI_UI.md, agent_swarm_ui_guide.md, AGENT_SWARM_UI_SUMMARY.md,
  LONG_RUNNING_TEST_PLAN.md, MEGA_TEST_PLAN_SUMMARY.md, HERMES_SETUP_*.md (3).
  README-linked living docs stay.
- `experiments/interactive-ux-overhaul/`, `experiments/selfware-longrun-mega/`,
  tracked `benchmark_results/`, `demos/`.
- Broken scripts: `verify_122b_workflows.sh`, `fuzzy_apply.py`, dev-runner cluster
  (6 files), `system_tests/agent_regression.rs`, `fuzz/fuzz_targets/fuzz_target_1`.
- Dead modules: `src/concurrent_queue/` (+ lib.rs export), `src/kv_store/`
  (+ drop from `tokens` feature gate). `interview.rs` is KEPT (wired in Phase 3).
- Makefile: remove no-op targets (`qa-python`, `qa-nodejs`, `install`).

## Phase 3 — Surface advanced functions (TDD, each with tests)

1. `consolidation` → default features (memory sleep-cycle ships by default).
2. `skills` available in classic REPL/headless, not just TUI — reuse
   `SkillRegistry::discover()` in `agent/interactive.rs` slash handling.
3. `log_analysis` wired into `doctor`/`llm-doctor`.
4. `interview` → `selfware init`: connect `InterviewContext` →
   `templates::scaffold_from_context` in `cli/init_wizard.rs`.
5. `selfware workflow codegen` subcommand for `swl::codegen::generate_rust_stub`.
6. De-hardcode `/home/ivo/...` swebench official-eval paths
   (`cli/args.rs:645-658`) → config option with sensible default.

## Phase 4 — Docs & dedup decisions

- `docs/CONSOLIDATION_PLAN.md`: decision records for live duplicates — swl vs
  orchestration YAML workflows; devops/container vs tools/container; evolution vs
  cognitive/self_improvement; token accounting (tokens.rs, token_count.rs,
  memory/, agent context mgmt); 3 SWE-bench locations; lib.rs flat re-export
  shim policy. Follow-up roadmap, not executed in this pass.
- Fix `CHANGELOG.md` version mismatch; update stale `ORCHESTRATION_STATUS.md`
  (coordinator no longer a stub); fix dead script refs in
  `docs/SWE_BENCH_EVALUATION_STRATEGY.md`.
- Final gate: full suite ×3, clippy `--all-targets --features extras`, fmt,
  `cargo doc --features extras`.

## Safety & rollback

- One or more commits per phase; suite green before starting the next phase.
- A phase that goes red and isn't quickly fixable gets reverted, not patched forward.
- Git mutations (commits, git rm) are confirmed with the user before each phase's
  first commit.

## Out of scope

- Merging live duplicates (plan doc only).
- God-file refactors (`agent/tool_dispatch.rs` 5.9k LOC, `cli/mod.rs` 3.9k, etc.)
  — noted in CONSOLIDATION_PLAN.md as follow-up.
- Editor extensions, k8s, release.yml.
- Local disk cleanup (`cargo clean` for 78G is the user's call).
- macOS/Windows verification (delegated to the CI matrix).
