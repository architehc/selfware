# Claude Code vs Selfware: SWE-bench Pro Roadmap for Qwen3.6 Quants

Date: 2026-04-29
Status: implementation roadmap

Scope: comparison of local `/home/ivo/claude-code` and `/home/ivo/selfware`, plus targeted lessons from SWE-agent, mini-SWE-agent, OpenHands, Aider, Augment SWE-bench Agent, AutoCodeRover, MASAI, official SWE-bench, official SWE-bench Pro, and Qwen3.6 docs.

Goal: maximize valid SWE-bench Pro performance for Qwen3.6 27B local quants on the 2x4090 rig. The score that matters is official Docker pass/fail, not "non-empty patch".

## Executive Verdict

Selfware already has the harder-to-build local benchmark skeleton: Qwen3.6 quant catalog, llama-server boot, cloned workdirs, patch capture, per-run artifacts, trials, and aggregation. Claude Code is stronger where selfware is currently losing points: strict loop discipline, edit safety, headless protocol, real subagents/worktrees, and production-grade recovery.

The bottleneck is not raw model capability. Current selfware failures are dominated by harness and loop behavior:

- `READ_LOOP`: model reads 7-20 steps, guard blocks reads, model retries blocked reads, then exits or hits max iterations.
- `EDIT_FAILURE_LOOP`: validator/tool feedback traps the model in repeated bad edits.
- `NONTERM_PROSE`: Qwen thinks/plans without tool calls.
- `FAKE_COMPLETE`: task exits with 0-byte patch or summary-only completion.
- `PROXY_SCORING`: current aggregate treats `exit_code == 0 && patch_bytes > 0` as success.

Qwen's own model card reports Qwen3.6-27B at 53.5 on SWE-bench Pro using an internal bash/file-edit scaffold, temp/top-p tuned, and 200K context. That means the model is plausibly capable. Selfware needs a stricter, benchmark-aware agent-computer interface to unlock it.

## P0: Correctness And Score Validity

These are blockers. Do these before claiming scores.

### 1. Restore Build Health

Current dirty `main` has compile blockers reported by agents: missing `message_has_tool_calls`, missing `text_fallback_tool_calls` initialization, bench-harness struct mismatches, and deleted observability/browser modules still referenced.

Implementation targets:

- Fix build under `cargo check --all-targets --all-features`.
- Add a CI job that includes integration target/features, not only default/extras.
- Do not merge any benchmark changes until compile is green.

Acceptance:

- `cargo check --all-targets --all-features` passes.
- `cargo test --features bench-harness` passes or has explicitly quarantined known platform tests.

### 2. Add Official SWE-bench Pro Eval

Current `src/bench_harness/swebench_pro/runner.rs` reports proxy success at lines around `successes = exit_code == 0 && patch_bytes > 0` and chooses best trial by most patch lines. That is not an official score.

Implementation targets:

- Add `selfware bench swebench-pro --official-eval`.
- Emit official Pro patch JSON: `instance_id`, `patch`, `prefix`.
- Invoke `swe_bench_pro_eval.py` from `SWE-bench_Pro-os`.
- Parse eval output into `aggregate.json`.
- Add `resolved`, `eval_completed`, `patch_applied`, `eval_error`, `f2p_p2p_passed`, `official_resolution_rate`.
- Keep `attempted_patch` as a diagnostic, not a score.

Acceptance:

- A run can produce per-quant official eval directories like `eval/Q4_K_P/trial_001/`.
- `aggregate.json` separates `attempted_patch_rate` from `official_resolution_rate`.
- No headline table calls non-empty patch a pass.

### 3. Make Official Runs Leakage-Safe

Current SWE-bench Pro prompt injects `fail_to_pass` and selected test files. That is useful for local diagnostics but not valid for official-style scoring.

Implementation targets:

- Add `--prompt-mode diagnostic|official`.
- `official` includes issue statement and repo context only.
- `diagnostic` may include fail-to-pass tests but marks output as `leaky_oracle_prompt: true`.
- Add immutable instance manifests: `smoke`, `fixed-public`, `full-public`, `custom`.
- Make "shortest 3 problems" an explicit `--smoke-shortest`, not a default for reported results.

Acceptance:

- Official score reports refuse to include leaky prompt runs.
- Every aggregate records dataset source, manifest path/hash, and prompt mode.

### 4. Fix Bench CLI Scheduling Wiring ✅

`bench swebench-pro --ctx` and `--parallel` are parsed into `SwebenchProArgs` and propagated to `LlamaServerOpts`, which passes them to llama-server as `-c <ctx>` and `--parallel <parallel>`.

Implementation targets:

- ✅ Populate `LlamaServerOpts { ctx: args.ctx, parallel: args.parallel, ... }`.
- ✅ Add tests around `build_llama_server_args()`.
- Record exact llama-server argv in `plan.json`.

Acceptance:

- ✅ `selfware bench swebench-pro --ctx 65536 --parallel 1` starts llama-server with `-c 65536 --parallel 1`.
- `plan.json` records resolved context, parallel, KV cache, tensor split, model file hash, llama-server path, and version.

### 5. Remove Or Fence Fake SWE-bench Paths ✅

`src/swebench/mod.rs` is deprecated legacy surface. It is now gated behind the `legacy-swebench` Cargo feature and its doc header explicitly warns against using it for official scoring. The orphaned sibling files (`agent_runner.rs`, `analysis.rs`, `checkpoint.rs`, `evaluator.rs`) were deleted. Production benchmark commands route through `src/bench_harness/swebench_pro` and `system_tests/swe_bench_pro/`.

Implementation targets:

- ✅ Gate `src/swebench` behind `legacy-swebench` feature.
- ✅ Delete orphaned non-compiling sibling files.
- ✅ Update docs and integration tests to route Pro runs through `bench_harness::swebench_pro` and `system_tests/swe_bench_pro/`.

Acceptance:

- ✅ No command path can report fabricated SWE-bench success.

## P0: Agent Loop Fixes For Qwen

These directly target the 60% read-loop and 0-byte patch failures.

### 6. Replace Read Blocking With Forced Mutation ✅

`ReadLoopPolicy::ForceMutation` is the default. The `force_mutation_directive` now lists all accepted mutation tools (`file_edit`, `file_multi_edit`, `file_write`, `patch_apply`) and allows one targeted verification command after an edit. The existing two-strike abort guard still fails fast with `READ_LOOP_NO_EDIT` when the model refuses to mutate.

Implementation targets:

- ✅ `ReadLoopPolicy::ForceMutation` active by default.
- ✅ At read-loop threshold, the next accepted action must be `file_edit`, `file_multi_edit`, `patch_apply`, `file_write`, or a targeted test command after an edit.
- ✅ Inject concrete tool-call templates using the most recently read file.
- ✅ Fail with `READ_LOOP_NO_EDIT` after two refusals.

Acceptance:

- Regression test: repeated `file_read` after guard produces `READ_LOOP_NO_EDIT` or a forced edit prompt, not 80 more blocked read attempts.
- Benchmark aggregate records first-edit step and read-only count before first edit.

### 7. Enforce Hard SWE Completion Gates

For SWE-bench, text summaries are never enough.

Implementation targets:

- Require at least one source-file mutation.
- Require non-empty `git diff`.
- Reject test-only patches unless explicitly allowed.
- Require verification after the last mutating tool, not before.
- Treat JSON tool results with `success: false` as failed verification even if the tool returned `Ok(Value)`.
- Add `EmptyDiff`, `TestOnlyPatch`, `StaleVerification`, `FailingTestsAccepted` failure kinds.

Acceptance:

- Exit 0 with 0B patch is marked failure.
- A verification command before the last edit does not satisfy completion.
- `cargo_test`/language test output with `success: false` cannot count as pass.

### 8. Fix Native/XML Tool History Invariants

Selfware has both native OpenAI tool calls and XML fallback. These must not mix in the same message-history shape.

Implementation targets:

- Native: assistant `tool_calls` followed by exact `role="tool"` results.
- XML/text fallback: assistant text only, then user `<tool_result>`.
- Split `native_tool_calls` from `text_fallback_tool_calls` end-to-end.
- Add tests for parsed XML fallback in streaming mode.
- Add backend tool policy: `NativeOpenAI`, `NativeNoStreamingTools`, `XmlPromptOnly`, `ToolsParamNoChoice`, `NoToolsParam`.

Acceptance:

- No parsed XML fallback call is stored as native `assistant.tool_calls`.
- Native multi-turn request history validates under strict OpenAI-compatible servers.

### 9. Make Validation Project-Aware

The error review found Rust validation running against non-Rust projects. That blocks useful Python/JS/Go edits.

Implementation targets:

- Only run Rust syntax/cargo validation when `Cargo.toml` or Rust file context warrants it.
- Add Python/JS/Go cheap syntax checks when applicable.
- Include concrete parser/compiler error in tool result.
- After 3 near-identical edit failures, force a different action path.

Acceptance:

- Python/JS/Go SWE-bench repos do not fail edits due to Rust validation.
- Edit failure loops are classified and terminated/recovered.

## P1: Improve Patch Production

### 10. Add A SWE-bench Solve Protocol

Strong harnesses do not just prompt "please solve"; they enforce a protocol.

Implementation targets:

- Add `SolvePhase`: `inspect_tests`, `localize`, `edit`, `verify`, `submit`.
- Add model-visible `task_state` tool: `task_plan_set`, `task_step_update`, `task_state_get`.
- Inject compact state every turn: candidate files, hypothesis, latest failing test, next required action.
- Add `submit_patch` sentinel tool so the harness distinguishes "finished" from "timed out with dirty tree".

Acceptance:

- A SWE run cannot complete unless phase is `submit` with non-empty source diff.
- Aggregate records phase at exit and incomplete phase on failure.

### 11. Add A Qwen3.6 SWE Prompt Profile

Selfware's current system prompt is broad and the Pro prompt is a short six-step string. Claude Code's prompts are more sectioned, concrete, and tool-oriented.

Implementation targets:

- `swebench_pro` prompt profile.
- Short Qwen response contract: valid tool call only or concise final; no prose before tool XML.
- Replace "large budget / do not rush" with "simplest correct patch; verify honestly; do not gold-plate".
- In diagnostic mode, include fail-to-pass tests; in official mode, exclude oracle fields.
- Align thinking policy: decide `enable_thinking=false` or `preserve_thinking=true` per backend and record it.

Acceptance:

- Prompt snapshot tests check no-test-edit rule, tool contract, verification requirement, and official/diagnostic leakage flag.

### 12. Upgrade Edit Tools

Claude Code's edit primitive is safer and more expressive. Selfware's FIM is a local-model advantage, but exact edits need more structure.

Implementation targets:

- Add `file_multi_edit` with ordered edits, `replace_all`, overlap checks, atomic apply, and structured hunks.
- Add stale guards to `file_edit`, `file_write`, `file_delete`, `file_fim_edit`.
- Add `patch_apply` with `git apply --check`, optional 3-way/fuzz fallback, and post-apply diff summary.
- Intercept simple `sed -i` in `shell_exec`, preview as file edit, apply directly.
- Preserve line endings/encoding where possible.

Acceptance:

- Regression tests for stale edit rejection, multi-hunk order, sed preview equivalence, FIM empty output, generated-file hints.

### 13. Replace Grep/Search With Better Localization

Selfware has powerful repo-map pieces, but Qwen may never discover them. Claude Code's `rg` wrapper is operationally better.

Implementation targets:

- Make `grep_search` use `rg` when available, with timeout, caps, VCS/build-dir excludes, binary handling, and pagination.
- Remove duplicate `GrepSearch` implementations.
- Add `lsp_diagnostics`, `lsp_workspace_symbols`, `lsp_goto_implementation`, call hierarchy.
- Add `localize_issue` tool that returns ranked files/functions/tests from problem text, tests, stack traces, BM25/code graph/LSP.
- In SWE mode, run localization before free-form read loops for lower quants.

Acceptance:

- Metrics: search calls to first relevant file, first edit file precision, tokens spent before first edit.

### 14. Add Repo-Language Verification

Cargo-centric checks are not enough for SWE-bench Pro's mixed repos.

Implementation targets:

- Infer repo language from files, package manifests, and dataset metadata.
- Generate targeted commands from repo-native tools in diagnostic mode.
- In official mode, run non-oracle repo-native smoke checks during solve; official pass/fail still comes from Docker eval.
- After edits, run cheap syntax/lint for touched files before expensive tests.
- Feed failing command output back into the next turn and require another edit before re-running.

Acceptance:

- Python/JS/Go/Rust repos get appropriate checks.
- Test failure does not become a normal successful tool result.

## P1: Bench Operations And Observability

### 15. Structured Headless Protocol

Claude Code's headless mode emits a structured protocol. Selfware's harness currently scrapes subprocess logs and diffs.

Implementation targets:

- Add `selfware -p --output-format json|stream-json`.
- Final envelope: `session_id`, `exit_status`, `stop_reason`, `num_turns`, `patch_bytes`, `patch_lines`, `usage`, `model`, `duration_ms`, `failure_mode`, `artifact_dir`.
- Add `--max-turns`, `--max-budget-tokens`, `--max-wall-secs`.
- Make `bench swebench-pro` consume structured output.

Acceptance:

- Harness no longer relies on parsing `agent.log` for core fields.

### 16. RunTrace And Diagnose

Turn artifacts are a good base, but not enough for sweep-level diagnosis.

Implementation targets:

- Emit `trace.jsonl` per run with `run_id`, `instance_id`, `quant`, `trial`, `step`, event kind, artifact paths.
- Include `ToolCallStarted/Completed`, `LlmRequest/Response`, `PatchCaptured`, `VerificationStarted/Completed`, `FailureClassified`.
- Add `selfware swebench diagnose <output-dir>`.
- Emit `diagnosis.json` per run and `diagnosis_summary.json` per sweep.

Acceptance:

- Diagnosis can histogram read loops, fake completes, timeouts, API errors, syntax/test failures, and most common failing tools.

### 17. Trial Manifest And Resume

`--skip-existing` on `.pred` is too weak.

Implementation targets:

- Add manifest states: `planned`, `boot_failed`, `clone_failed`, `running`, `agent_failed`, `patch_captured`, `evaluated`.
- Resume incomplete trials safely after GPU/server crashes.
- Record boot/clone failures in denominators.

Acceptance:

- A killed run can resume without silently dropping failed instances.

## P2: Scale The Harness

### 18. Real Subagents With Worktree Isolation

Claude Code's real subagents/worktrees are a major advantage. Selfware's coordinator workers are currently simulated.

Implementation targets:

- Replace `WorkerAgent::execute_task` stub with real `Agent` execution.
- Add per-agent `workdir` to agent/tool context; stop relying on process-global cwd.
- Add deterministic `.selfware/worktrees/agent-<id>` lifecycle.
- SWE mode roles: `localizer`, `patcher`, `verifier`.
- Keep bounded, not open-ended swarm.

Acceptance:

- Workers produce real patches/artifacts from isolated worktrees.
- Candidate selection uses verification evidence, not patch size.

### 19. Candidate Generation And Selection

Variance is high. Aider/Augment-style multiple attempts help, but score reporting must remain honest.

Implementation targets:

- Run 2-4 candidates for selected quants/settings.
- Select by official eval if available; otherwise by non-empty source diff, no test edits, syntax/test evidence, smaller diff.
- Report `pass@1` separately from `pass@k_oracle`.

Acceptance:

- No headline pass rate uses oracle selection unless labeled as upper bound.

### 20. Quant And Backend Policy

Local scheduling must be explicit for 2x4090.

Implementation targets:

- Extend `QuantSpec` with recommended ctx, max parallel, KV type, tensor split, temperature, thinking policy, backend.
- Estimate model + KV memory per GPU before boot.
- Couple `concurrency <= parallel`.
- Add backend profiles: llama.cpp for GGUF, SGLang/vLLM for HF/FP8/native tool-call experiments.
- Dynamic endpoint capability detection from `/v1/models` plus backend metadata where available.

Acceptance:

- Unsafe `ctx * parallel` combinations fail fast or downshift.
- Each run records resolved quant/backend policy.

## P2: Memory, Skills, And Sandboxing

### 21. SWE-bench Memory

Selfware has RAG and episodic memory but does not automatically use them in benchmark mode.

Implementation targets:

- Add `SwebenchInstanceMemory` keyed by repo, base commit, instance, quant, trial.
- Add repo-level lessons with leakage controls.
- Auto-index each repo before first turn.
- Inject at most 1-2k tokens of source-attributed memory.

Acceptance:

- Exact-instance memory is only reused for retries, not official fresh pass@1.
- Repo lessons are capped, attributed, and excluded from patches.

### 22. Deferred Tool And MCP Hygiene

Too many schemas hurt small models.

Implementation targets:

- Enforce deferred tools consistently; do not bypass activation in dispatch.
- Register MCP tools as deferred by default.
- Improve `tool_search` exact select, category aliases, concise activation response.
- Add local plugin manifest for skills/hooks/MCP/tool aliases.

Acceptance:

- Hallucinated deferred tool calls produce actionable "use tool_search first" feedback.

### 23. Sandbox-Required Bench Mode

Selfware's safety layer is mostly policy, not an execution boundary.

Implementation targets:

- Add `--sandbox-required` for bench mode.
- Refuse unsafe `--yolo` unless inside Docker/devcontainer/worktree with expected network policy.
- Replace global `pkill -f llama-server` with managed server pool.
- Collate audit logs into trial output.

Acceptance:

- Bench runs do not kill unrelated llama-server jobs.
- Each run records permission decisions and sandbox metadata.

## 20-Scope Component Map

| # | Component | Biggest Selfware Action |
|---|---|---|
| 1 | Tools/edit primitives | Add stale-safe `file_multi_edit`, `patch_apply`, sed preview. |
| 2 | Main loop/state | Replace ambiguous boolean turn result with structured `TurnOutcome`. |
| 3 | Context/compaction | Snip old file/tool results after ContextMap ingestion. |
| 4 | Verification | Require post-edit, repo-language verification and non-empty source diff. |
| 5 | Bench harness | Integrate official Docker eval and remove proxy scoring. |
| 6 | Model interface | Enforce native/XML message-history invariants. |
| 7 | Multi-agent | Replace simulated coordinator with real worktree agents. |
| 8 | Planning | Add model-visible task state and SWE solve protocol. |
| 9 | Permissions/sandbox | Add sandbox-required bench mode and yolo safety ordering. |
| 10 | Observability | Add RunTrace JSONL and `swebench diagnose`. |
| 11 | MCP/skills/hooks | Defer MCP tools and enforce activation. |
| 12 | Memory/RAG | Add leakage-safe SWE instance/repo memory and auto-indexing. |
| 13 | CLI/headless | Add JSON/stream-JSON result protocol and replay. |
| 14 | Recovery | Force mutation on read-loop; repair malformed Qwen tool calls. |
| 15 | Prompting | Add Qwen/SWE-specific prompt profile and snapshot tests. |
| 16 | Search/LSP | Add rg backend, LSP diagnostics, `localize_issue`. |
| 17 | Patch strategy | Add multi-hunk atomic edit and generated-code policy. |
| 18 | Hardware scheduling | Fix ctx/parallel wiring; add per-quant 2x4090 policies. |
| 19 | Official eval | Add leakage-safe official pass@1 and pass@k reporting. |
| 20 | External harnesses | Copy phase enforcement, submit action, edit-time lint, telemetry. |

## Recommended Implementation Order

1. Build health and bench wiring: compile green, `--ctx/--parallel` fixed, fake SWE path fenced.
2. Valid scoring: official eval, prompt mode separation, aggregate fields, manifest splits.
3. Read-loop unlock: forced mutation policy, hard SWE completion gate, language-aware validation.
4. Tool-call correctness: native/XML invariant, Qwen parser repair, backend policy.
5. Patch production: `file_multi_edit`, stale guards, `patch_apply`, SWE prompt profile.
6. Verification and diagnose: repo-language checks, RunTrace, `swebench diagnose`.
7. Search/localization: rg, LSP diagnostics, `localize_issue`.
8. Multi-candidate: real worktree agents, candidate selection, honest pass@1/pass@k.
9. Quant policy: 2x4090 scheduler, backend profiles, capability detection.
10. Memory/plugins/sandbox: add only after score loop is trustworthy.

## Success Metrics

Track these per quant and per instance:

- Official `resolved` rate.
- Attempted source patch rate.
- Empty patch rate.
- First edit step.
- Read-only steps before first edit.
- Tool-call parse failure rate.
- Native/XML history violation count.
- Verification-after-last-edit rate.
- Test-only patch rate.
- Timeout rate.
- API/server failure rate.
- Median prompt/completion tokens.
- Tokens to first relevant file.
- Tokens to first edit.
- Patch file count and changed source/test/generated split.

## External References

- Qwen3.6 model card and benchmark settings: https://huggingface.co/Qwen/Qwen3.6-27B-FP8
- Official SWE-bench leaderboard/docs: https://www.swebench.com/
- SWE-bench harness docs: https://www.swebench.com/SWE-bench/api/harness/
- SWE-bench Pro repository/eval script: https://github.com/scaleapi/SWE-bench_Pro-os
- mini-SWE-agent SWE-bench docs: https://mini-swe-agent.com/latest/usage/swebench/
- OpenHands evaluation harness docs: https://docs.openhands.dev/openhands/usage/developers/evaluation-harness
- Aider SWE-bench methodology: https://aider.chat/2024/05/22/swe-bench-lite.html
- Aider lint/test automation: https://aider.chat/docs/usage/lint-test.html
- Augment SWE-bench agent: https://github.com/augmentcode/augment-swebench-agent
- AutoCodeRover paper: https://arxiv.org/abs/2404.05427
- MASAI paper: https://arxiv.org/abs/2406.11638
