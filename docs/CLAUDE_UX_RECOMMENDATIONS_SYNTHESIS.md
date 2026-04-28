# Selfware UX Improvements — 20-Agent Codebase Synthesis

> Generated after parallel exploration of the entire selfware codebase by 20 agents.
> This document maps Claude's top 10 UX recommendations to specific findings, files, and actionable implementation paths.

---

## 1. Per-Model Auto-Config (Highest ROI)

**Claude's ask:** Detect model from `/v1/models` (or alias) and apply known-good defaults automatically. User just sets `model = "qwen3.6-27b-q4kp"` and the rest comes from a profile.

**Current state:**
- `AutoConfigurator` exists in `src/config/auto_config.rs` and probes `/models`, tests chat/thinking/streaming/function-calling, then generates a `Config` heuristically.
- `unpack.rs` does zero-touch auto-calibration: scans LM Studio/Ollama/generic ports, auto-starts Ollama, auto-pulls hardware-matched models.
- `ModelProfile` exists in `src/config/model.rs` with `endpoint`, `model`, `max_tokens`, `temperature`, `modalities`, `context_length`, `extra_body`.
- **BUT:** `ModelProfile` lacks `native_function_calling`, `streaming`, `top_p`, and `presence_penalty` fields.
- **BUT:** `chat_with_profile()` in `src/api/client.rs:553` **hardcodes `native_tool_choice = false`**, so profiles can never use native FC even if the main model supports it.
- **BUT:** No built-in static model database / registry. Auto-config relies entirely on runtime probing; there is no local lookup table of known models with recommended params.
- **BUT:** `extra_body` is the only way to set `top_p` / `presence_penalty`; they have no first-class `Config` field.

**Gaps found:**
1. No `[model_profiles."qwen3.6-*"]` TOML section or equivalent.
2. `AutoConfigurator` probes but does not match against a static knowledge base.
3. `ModelProfile` does not inherit/merge the main config's `extra_body` for profile calls.
4. Per-model `extra_body` presets (e.g., SGLang `chat_template_kwargs`) are only discovered via probing, not from a registry.

**Recommended implementation path:**
1. Add a static `model_profiles` table to `Config` (e.g., `HashMap<String, ModelProfile>` keyed by glob pattern).
2. After `Config::load()`, run a `apply_model_profile()` step that matches `config.model` against profile keys and merges profile defaults.
3. Extend `ModelProfile` with `native_function_calling`, `streaming`, `top_p`, `presence_penalty`.
4. Fix `chat_with_profile()` to respect the profile's `native_function_calling` setting.
5. Ship built-in profiles for common models (Qwen3.6, Qwen3.5-Coder, Llama 3.1, etc.) so users get good defaults out of the box.

---

## 2. Per-Turn Artifact Capture

**Claude's ask:** Make `.selfware/turns/turn_NNN.json` the default output, capturing per turn: request body, response body, finish_reason, tokens used, parsed tool calls, decision (advance/nudge/abort).

**Current state:**
- Four env vars control debug output: `SELFWARE_DEBUG`, `SELFWARE_DEBUG_RAW`, `SELFWARE_DEBUG_REQUEST`, `SELFWARE_DEBUG_GATE`.
- `SessionLogger` writes events to `~/.local/share/selfware/session_logs/{session_id}.jsonl` with `TurnStart`/`TurnEnd` events.
- `TurnStart` captures: phase, loop state, message count, estimated tokens, context window.
- `TurnEnd` captures: success, duration_ms, content chars, reasoning chars, native tool call count, message count, estimated tokens.
- **BUT:** No per-turn request/response files. Request/response bodies are only printed to stderr via env vars.
- **BUT:** `finish_reason` is **completely lost** in both streaming and non-streaming paths.
- **BUT:** Token usage is global-only (`TOTAL_PROMPT_TOKENS` / `TOTAL_COMPLETION_TOKENS` atomic counters). `log_turn_end_event` includes only `estimated_message_tokens`, not actual API-reported usage.
- **BUT:** No `.selfware/turns/` directory exists. Only `.selfware/tool_results/` has ad-hoc cached JSON files.
- **BUT:** Streaming chunks are consumed in real-time; only final `content` is retained. No raw SSE stream dump per turn.
- **BUT:** Thinking content is stripped from history and discarded after display.

**Gaps found:**
1. `AssistantStepResponse` lacks `finish_reason`, `prompt_tokens`, `completion_tokens`, `total_tokens`.
2. `StreamChunk` enum has no `FinishReason` variant; `parse_sse_event` detects it but only uses it to flush tool calls.
3. `StreamingResponse::collect()` hardcodes `finish_reason: Some("stop")`.
4. Checkpoints save task-level state, not turn-level bundles.

**Recommended implementation path:**
1. Add `finish_reason` and token fields to `AssistantStepResponse`.
2. Add `StreamChunk::FinishReason(String)` and capture usage in the streaming loop.
3. Read `response.usage` and `choice.finish_reason` in the non-streaming path.
4. Create a `TurnArtifact` struct: turn index, task_id, timestamp, request_messages, response_content, reasoning, finish_reason, prompt_tokens, completion_tokens, total_tokens, parsed_tool_calls, tool_results, decision label.
5. Write `.selfware/artifacts/<task_id>/turn_0001.json` atomically after each turn in `execute_step_internal()`.
6. Deprecate the 4 debug env vars in favor of the always-on artifact directory (low cost — just JSON writes).

---

## 3. Failure-Mode Classifier in Result

**Claude's ask:** Categorize the run end with `outcome`, `evidence`, and `advice`. Selfware knows failure modes internally but does not surface them.

**Current state:**
- `src/swebench/analysis.rs` has a rich `FailurePattern` enum: `Timeout`, `WrongFiles`, `IncompletePatch`, `TestRegression`, `SyntaxError`, `StuckLoop`, `MisunderstoodProblem`, `ToolFailure`, `TokenExhaustion`, `Other(String)`.
- `src/agent/task_runner.rs` has `classify_outcome_label()` with runtime heuristics: `"read_loop"`, `"no_write"`, `"edit_failure_loop"`, `"verification_missing"`, `"green_incomplete"`, `"invalid_tool_loop"`.
- `src/agent/learning.rs` has `classify_error_type()`: `timeout`, `permission`, `safety`, `parsing`, `network`, `execution`.
- `src/self_healing/mod.rs` has `ErrorClass`: `Network`, `Timeout`, `RateLimit`, `ResourceExhausted`, `ParseError`, `AuthError`, `SafetyViolation`, `ToolError`, `ModelError`, `Unknown`.
- **BUT:** `FailurePattern` is isolated to SWE-bench evaluation. It is not reused by the core agent loop, health monitor, or checkpoint system.
- **BUT:** `classify_outcome_label()` returns a `&'static str`, not a structured enum. It is not persisted in checkpoints or session logs.
- **BUT:** No unified `AgentTaskOutcome` enum exists in the core agent path.
- **BUT:** When a run ends, stuck-loop detector states (`consecutive_read_only_steps`, `permanently_blocked_tool_calls`, etc.) are not summarized into a final failure-mode label.

**Gaps found:**
1. No canonical `AgentTaskOutcome` enum with fine-grained variants.
2. `FailurePattern` lives in `src/swebench/` and is not consumed by `src/agent/`.
3. Checkpoints track `tool_calls`, `success`, and `error` strings, but no `failure_pattern` or `outcome_label`.
4. Infrastructure health events (`CircuitOpen`, `GPU overheating`) are invisible in the task result taxonomy.

**Recommended implementation path:**
1. Define a new `AgentTaskOutcome` enum in `src/agent/` (or `src/cognitive/self_improvement.rs`) with variants:
   - `Success`, `ReadOnlyLoop`, `EditFailureLoop`, `StuckLoop`, `VerificationMissing`, `VerificationFailed`, `SafetyBlocked`, `Timeout`, `TokenExhaustion`, `InfrastructureFailure`, `UserAborted`, `MaxIterationsExceeded`.
2. Integrate it into `AgentLoop` / `task_runner.rs` so `classify_outcome_label()` returns the enum instead of a string.
3. Persist it in `TaskCheckpoint` and session log.
4. Consume it from `swebench/analysis.rs` so `FailurePattern` maps directly to the core agent taxonomy.
5. On task end, print a structured summary:
   ```
   { "outcome": "READ_LOOP",
     "evidence": "8 consecutive read-only steps, 0 mutating tools",
     "advice": "task may be misclassified; try lowering progress_guard threshold" }
   ```

---

## 4. `--debug` Master Flag

**Claude's ask:** One CLI flag turns on all debug knobs and writes per-turn artifacts.

**Current state:**
- `--verbose` / `-v` sets `verbose_mode` and `output::VERBOSE_MODE`.
- `--show-tokens` sets `show_tokens`.
- `--plan` sets `plan_mode`.
- `RUST_LOG` or `SELFWARE_LOG_LEVEL` enables tracing.
- `SELFWARE_DEBUG` enables `output::debug_output()` and raw response dumps.
- `SELFWARE_DEBUG_REQUEST` dumps request JSON to stderr.
- `SELFWARE_DEBUG_RAW` (used with `SELFWARE_DEBUG`) dumps raw response bodies.
- `SELFWARE_DEBUG_GATE` prints gate refusal messages.
- **BUT:** `init_tracing()` runs **before** `Cli::parse()`, so a `--debug` flag cannot influence initial tracing setup without re-initializing.
- **BUT:** `--verbose` does not enable tracing.
- **BUT:** `SELFWARE_DEBUG` is undocumented in `docs/getting-started.md` and `docs/configuration.md`.
- **BUT:** No `[debug]` or `[logging]` TOML section exists.
- **BUT:** No unified debug switch exists.

**Gaps found:**
1. No `--debug` flag in `src/cli/args.rs`.
2. Tracing initialized before CLI parsing, making post-parse flags unable to affect it.
3. No config file section for debug settings.

**Recommended implementation path:**
1. Add `--debug` to `src/cli/args.rs` as a global flag.
2. Change `init_tracing()` to accept a pre-parsed hint (e.g., scan `std::env::args_os()` for `--debug` before `Cli::parse()`), or re-initialize tracing after parsing.
3. `--debug` should:
   - Set `verbose_mode = true`
   - Set `show_tokens = true`
   - Enable tracing at `debug` level
   - Enable per-turn artifact capture (see #2)
   - Document `SELFWARE_DEBUG` as the env-var equivalent
4. Optionally add `[debug]` TOML section with `verbose = true`, `log_level = "debug"`, `turn_artifacts = true`.

---

## 5. Streamline Config Precedence + `selfware config show`

**Claude's ask:** Add `selfware config show` that prints each setting with its source. Fix precedence so `SELFWARE_CONFIG` reliably overrides cwd toml.

**Current state:**
- Config load order: `--config` > `SELFWARE_CONFIG` > `./selfware.toml` > `~/.config/selfware/config.toml` > defaults.
- Env var overrides: `SELFWARE_ENDPOINT`, `SELFWARE_MODEL`, `SELFWARE_API_KEY`, `SELFWARE_MAX_TOKENS`, `SELFWARE_TEMPERATURE`, `SELFWARE_TIMEOUT`, `SELFWARE_THEME`, `SELFWARE_MODE`, `SELFWARE_LOG_LEVEL`, `SELFWARE_STRICT_PERMISSIONS`.
- CLI overrides in `cli/mod.rs`: `--mode`, `--yolo`, `--daemon`, `--compact`, `--verbose`, `--show-tokens`, `--plan`, `--theme`.
- **BUT:** `~/.selfware.toml` is **NOT** in the load path (Claude listed it as a precedence step, but `loader.rs` only checks `~/.config/selfware/config.toml`).
- **BUT:** No `selfware config show` command exists.
- **BUT:** `selfware status` shows only model, endpoint, local/remote indicator, and journal stats — not full config.
- **BUT:** No source attribution: users cannot know which config file was loaded, which values came from env vars vs file vs keyring vs computed defaults.
- **BUT:** No `config set-key` CLI command exists despite `save_api_key_to_keyring()` being implemented in `src/config/api_key.rs`.
- **BUT:** Unknown key warnings (`warn_unknown_keys()`) are ephemeral tracing logs; not collected or shown in any summary.
- **BUT:** `token_budget` auto-computed from `context_length * 3/5` is invisible to the user.

**Gaps found:**
1. No `Commands::Config` variant in `src/cli/args.rs`.
2. No `config show`, `config get`, `config set`, or `config set-key` subcommands.
3. No precedence trace / dry-run mode.
4. `~/.selfware.toml` is not implemented.

**Recommended implementation path:**
1. Add `selfware config show` subcommand that prints the fully resolved config with source attribution:
   ```
   endpoint    = http://127.0.0.1:8000/v1   [from SELFWARE_ENDPOINT]
   model       = qwen3.6-27b-q4kp           [from /tmp/foo/selfware.toml]
   temperature = 0.7                        [from /tmp/foo/selfware.toml]
   native_fc   = true                       [from /tmp/foo/selfware.toml]
   top_p       = 0.8                        [profile: qwen3.6-*]
   presence_penalty = 1.5                   [profile: qwen3.6-*]
   ```
2. Track source metadata during `Config::load()`: store a `source: ConfigSource` field (or parallel map) for each field.
3. Add `selfware config set-key` to expose the existing keyring save function.
4. Add `selfware config validate --show-sources` to surface unknown keys, computed defaults, and precedence.
5. Optionally add `~/.selfware.toml` to the load path for consistency with user expectations.

---

## 6. `selfware bench` Integrated Subcommand

**Claude's ask:** Move SWE-bench harness into selfware itself: `selfware bench swe-pro --quants Q4 --instances 5`.

**Current state:**
- `selfware bench` exists in `src/cli/args.rs` (alias `bm`) but only runs trivial LLM Q&A benchmarks via `HarnessRunner`.
- `src/swebench/` module exists with `agent_runner.rs`, `evaluator.rs`, `analysis.rs`, `checkpoint.rs`, etc.
- **BUT:** `SWEAgentRunner::solve()` in `src/swebench/agent_runner.rs` is a **placeholder loop** that does not call `agent.run_task()`.
- **BUT:** `src/swebench/mod.rs` `load_tasks()` returns **hardcoded mock data** with `warn!("STUB: load_tasks returning MOCK data")`.
- **BUT:** `TaskEnvironment` is referenced but **not defined** anywhere.
- **BUT:** `SWEOrchestrator` does not exist.
- **BUT:** `selfware swe` / `selfware eval-swebench` explicitly bails with `"SWE-bench evaluation is not implemented yet."`
- **BUT:** The most mature runner is `scripts/swebench_pro/run.py` (Python), which spawns `selfware` as a subprocess.
- **BUT:** Only `scripts/swebench_exec.py` runs real Docker test execution.

**Gaps found:**
1. No Rust dataset loader for SWE-bench Lite/Full/Pro.
2. No `TaskEnvironment` (repo clone, checkout, dependency install).
3. `SWEAgentRunner` does not actually run the agent.
4. No orchestrator for concurrent SWE-bench execution in Rust.
5. No Docker test execution from Rust side.
6. `bench` subcommand does not support SWE-bench.

**Recommended implementation path:**
1. Fix `src/swebench/mod.rs` to load real JSON/HF datasets.
2. Implement `TaskEnvironment` in `src/swebench/environment.rs` (clone repo, checkout commit, apply test patch).
3. Fix `SWEAgentRunner` to call `Agent::run_task(prompt)` with a proper SWE-bench prompt.
4. Implement `SWEOrchestrator` using `tokio::sync::Semaphore` for concurrency.
5. Keep Python boundary for Docker eval: after Rust orchestrator generates patches, invoke `scripts/swebench_exec.py` as a subprocess and ingest `exec_results.json`.
6. Extend `Commands::Bench` to parse `suite = "swebench"` and dispatch to the orchestrator.
7. Wire `CheckpointManager` so `--resume` works.
8. Connect `AnalysisEngine` to report generation.

---

## 7. Multi-Trial Built-In

**Claude's ask:** `selfware -p "..." --trials 3 --report best` runs the task 3 times and reports the best output.

**Current state:**
- `run_task()` in `src/agent/task_runner.rs` runs one linear trajectory.
- `run_swarm_task()` runs sequential phases but each phase is still a single linear run.
- `CheckpointManager` supports save/load/merge but is not wired to trial branching.
- `EditHistory` supports branching (`current_branch: Option<String>`) but is **in-memory only**.
- **BUT:** No concept of a trial ID, parent trial, or branch point in checkpoints.
- **BUT:** No automatic rollback to a previous step.
- **BUT:** Runtime guard state (`permanently_blocked_tool_calls`, `stuck_loop_warning_counts`, etc.) is **not serialized** in checkpoints.
- **BUT:** No explicit multi-trial orchestrator.
- **BUT:** The quant bench report explicitly calls out multi-trial averaging as missing.

**Gaps found:**
1. Checkpoint format is linear (single `current_step`, single `messages` vector).
2. No `rollback(step)` primitive.
3. No `clone_checkpoint_for_trial()` primitive.
4. No trial scheduler / comparator.

**Recommended implementation path:**
1. Add trial metadata to `TaskCheckpoint`: `trial_number`, `max_trials`, `trial_outcomes[]`, `rollback_target_step`, `prompt_variant`.
2. Implement `CheckpointManager::branch_for_trial(parent_checkpoint, rollback_step)` to create a new trial checkpoint without mutating the parent.
3. Serialize runtime guard state in checkpoints so resumed agents don't repeat stuck-loop patterns.
4. Add a trial orchestrator in `task_runner.rs`:
   ```rust
   for trial in 0..max_trials {
       let checkpoint = if trial == 0 {
           initial_checkpoint
       } else {
           manager.branch_for_trial(&best_so_far, rollback_step)
       };
       let outcome = run_single_trial(checkpoint).await?;
       outcomes.push(outcome);
   }
   report_best_outcome(outcomes);
   ```
5. Add `--trials` and `--report` (best/median/last) CLI flags.
6. Persist edit-history branches to disk so trial branches survive process restarts.

---

## 8. Stop-on-Pattern

**Claude's ask:** Let users configure stop strings for known confused-output patterns.

**Current state:**
- `stop: Option<Vec<String>>` exists in API types (`src/api/types.rs`) but is used only on **legacy completion requests**.
- The agent's main chat path (`src/api/client.rs`) **never passes stop sequences** to the API.
- `parse_tool_calls` operates on the final accumulated string, not on a streaming token buffer.
- Completion gate (`check_completion_gate`) runs on the **already-generated** assistant message.
- Progress guards detect stuck loops after they happen (e.g., 8 consecutive read-only steps).
- **BUT:** No stop strings are sent for tool boundaries (`<tool>`, `</tool>`), result boundaries (`<tool_result>`), or completion patterns (`Final answer:`).
- **BUT:** No user-configurable stop patterns exist.

**Gaps found:**
1. `ChatRequest` / `CompletionRequest` never sets `stop` in production.
2. No streaming stop on `<tool>` tag.
3. No config field for `agent.early_termination.abort_on_patterns`.

**Recommended implementation path:**
1. Add `[agent.early_termination]` TOML section:
   ```toml
   [agent.early_termination]
   abort_on_patterns = ["STUCK LOOP", "I cannot make progress", "I'll proceed step by step"]
   max_repeats = 3
   ```
2. Pass `stop` sequences into the chat request builder:
   - For XML mode: `stop: ["</tool>"]` (or equivalent) to truncate after tool call.
   - For native FC: rely on `finish_reason: "tool_calls"` but add fallback stop strings.
3. Add a pattern matcher in `execute_step_internal()` that scans assistant content for `abort_on_patterns` and aborts early if matched.
4. Make stop strings model-aware (different for XML vs native FC vs different model families).

---

## 9. Tool-Result Shape Should Match What the Model Expects

**Claude's ask:** When `native_function_calling=true`, feed back `{"role":"tool","tool_call_id":"...","content":"..."}` instead of XML-wrapped user messages.

**Current state:**
- When `native_function_calling = true`: tool results are pushed as `Message::tool(result_string, call_id)` with `role: "tool"`.
- When `native_function_calling = false`: tool results are pushed as `Message::user("<tool_result>...</tool_result>")`.
- **BUT:** `Message::tool()` always sets `name: None`. OpenAI-style APIs expect `role: "tool"` messages to include the `name` of the function that was called.
- **BUT:** Some backends (certain vLLM/SGLang configs) require tool results as `role: "user"` even when native FC is on.
- **BUT:** No per-model adaptation of tool-result shape.
- **BUT:** Image results in native FC mode use multimodal `MessageContent::Blocks` but the JSON blob is stripped to `"image_attached": true` — this may not be what the model expects.

**Gaps found:**
1. Tool messages never set `name` field (`src/api/types.rs:371`).
2. No per-model tool-result shape adaptation.
3. Auto-config FC probe always sends `chat_template_kwargs: {enable_thinking: false}`, assuming SGLang-style backend.

**Recommended implementation path:**
1. Store the tool name in `Message::tool()` so `name` is populated.
2. Add a `tool_result_format` field to `ModelProfile` with options: `NativeTool`, `UserXml`, `UserMarkdown`.
3. In `push_tool_result_message()`, branch on `config.agent.native_function_calling && config.resolve_model().tool_result_format == NativeTool`.
4. For backends that need `role: "user"` even with native FC, allow override via model profile.
5. Add test coverage for tool-result message shapes per backend.

---

## 10. Built-In Observability Dashboard

**Claude's ask:** Hook a small web UI to the existing `src/observability/` infrastructure for live agent loop progress, recent tool calls, stuck-loop alerts.

**Current state:**
- `src/observability/dashboard.rs` is fully coded (`ObservabilityDashboard` with `TokenTracker`, `LatencyHistogram`, `ToolTracker`, `ErrorTracker`, `SessionStats`) but explicitly marked **"EXPERIMENTAL — no call sites, candidate for removal"**.
- `src/observability/test_dashboard.rs` has TestExplorer + CoverageTracker + WatchMode but is also unwired.
- `src/ui/tui/dashboard.rs` is a working ratatui-based swarm dashboard with real-time event streaming via `TuiEvent`.
- `metrics_exporter_prometheus` exposes a pull-only endpoint on `127.0.0.1:9090` in Daemon mode.
- `tracing` writes to stderr + daily rotating file + optional OTLP.
- **BUT:** No HTTP server / web framework in `Cargo.toml` (no axum, warp, actix, etc.).
- **BUT:** No WebSocket / SSE / broadcast channel for pushing live events to a browser.
- **BUT:** `ObservabilityDashboard` and `DashboardState` are completely separate with no shared event pipeline.
- **BUT:** No general stuck-loop alert outside visual tasks and hard iteration limits.
- **BUT:** Prometheus is pull-only; browsers can't scrape it.

**Gaps found:**
1. No web framework dependency.
2. No event bus for broadcasting agent events outside the TUI.
3. `ObservabilityDashboard` has zero call sites.
4. No stuck-loop monitor for non-visual code tasks.

**Recommended implementation path:**
1. **Short term (TUI-only):** Wire `ObservabilityDashboard` into the agent event loop:
   - Every API request → `record_request()`
   - Every tool execution → `record_tool()`
   - Every error → `record_error()`
   - Add new TUI pane types: `LatencyMetrics`, `ToolStats`, `ErrorTracker`.
   - Add `/stats` slash command to query the observability backend.
2. **Medium term (web dashboard):**
   - Add `axum` (or `warp`) as an optional feature.
   - Add a `tokio::sync::broadcast` event bus to replace/augment TUI-only `EventEmitter`.
   - Expose SSE endpoint streaming serialized `AgentEvent`s.
   - Add state snapshot endpoint returning current `AgentLoop` + `DashboardState` as JSON.
   - Add stuck-loop monitor watching: iteration count, time-in-state, repeated error patterns, heartbeat age.
   - Serve minimal HTML dashboard (embed with `rust-embed` or static files).
3. **Long term:** Merge `ObservabilityDashboard` and `DashboardState` into a single `SharedRuntimeState` (Arc<RwLock<...>>) accessible by both TUI and web API.

---

## Bonus Findings from 20-Agent Exploration

### A. MCP Transport Framing Bug
- **Critical:** `src/mcp/transport.rs` reads newline-delimited JSON (`reader.lines()`), but the MCP spec (and Selfware's own server) uses **Content-Length framing**.
- **Impact:** Selfware's MCP client will likely fail to communicate with spec-compliant MCP servers.
- **Fix:** Switch client transport to Content-Length framing to match the server.

### B. MCP Tools Are Invisible by Default
- MCP tools are registered as **deferred** (`register_deferred()`), so they never appear in the system prompt.
- The LLM may never discover configured MCP tools unless it happens to call `tool_search`.
- **Fix:** Register MCP tools as **critical** (or add a dedicated "MCP tools available" note to the system prompt).

### C. Coordinator Mode Is Simulated
- `CoordinatorAgent` workers are completely stubbed; they log warnings and return placeholder text without making LLM calls.
- **Fix:** Bridge `CoordinatorAgent` with `MultiAgentChat` (which actually makes LLM calls) to power real coordinator workers.

### D. Garden TUI Is Half-Built
- `garden_view.rs` exists (932 lines) but is **not wired to the TUI event loop**.
- `docs/TUI_GARDEN_GUIDE.md` explicitly states: "There is NO integrated 'Garden TUI'."
- None of the three roadmap phases are checked off.
- **Fix:** Wire `garden_view.rs` into the TUI event loop, add Ctrl+G toggle, arrow key navigation, and agent spawn actions.

### E. Top-Level Error Formatting Is Raw
- `src/main.rs` does `eprintln!("Error: {:?}", e)` — no structured formatting, no garden metaphor, no actionable guidance.
- **Fix:** Add a human-friendly error formatter with garden metaphors, exit code, and suggested next steps.

### F. Crash Recovery Prompt Is Missing
- The UX doc recommends auto-detecting unclean shutdown and offering `[Resume] [View Journal] [Start Fresh]`.
- **Fix:** On startup, check for incomplete checkpoints and offer an interactive resume prompt.

---

## Implementation Priority Matrix

| Priority | Recommendation | Effort | Impact | Files to Touch |
|----------|----------------|--------|--------|----------------|
| P0 | 1. Per-model auto-config | Medium | Very High | `src/config/model.rs`, `src/config/loader.rs`, `src/api/client.rs` |
| P0 | 5. `config show` + precedence | Low | Very High | `src/cli/args.rs`, `src/cli/mod.rs`, `src/config/loader.rs` |
| P0 | 9. Tool-result shapes | Low | High | `src/api/types.rs`, `src/agent/message_handling.rs`, `src/config/model.rs` |
| P1 | 2. Per-turn artifact capture | Medium | High | `src/agent/execution.rs`, `src/agent/streaming.rs`, `src/agent/session_log.rs` |
| P1 | 3. Failure-mode classifier | Medium | High | `src/agent/task_runner.rs`, `src/swebench/analysis.rs`, `src/session/checkpoint.rs` |
| P1 | 4. `--debug` master flag | Low | Medium | `src/cli/args.rs`, `src/cli/mod.rs`, `src/output/mod.rs` |
| P1 | 8. Stop-on-pattern | Low | Medium | `src/config/agent.rs`, `src/agent/execution.rs`, `src/api/client.rs` |
| P2 | 6. `selfware bench` integration | High | High | `src/swebench/`, `src/cli/mod.rs`, `scripts/swebench_pro/` |
| P2 | 7. Multi-trial built-in | Medium | Medium | `src/agent/task_runner.rs`, `src/session/checkpoint.rs` |
| P2 | 10. Observability dashboard | High | Medium | `src/observability/dashboard.rs`, `src/ui/tui/`, `Cargo.toml` |

---

*Synthesis generated from 20 parallel agent explorations covering: config, agent loop, CLI, API client, observability, bench harness, safety, tool execution, session/checkpointing, message handling, output/logging, templates/prompts, doctor/diagnostics, evolution/cognitive, UI/TUI, errors/recovery, orchestration/testing, docs/UX, computer/browser, and MCP/tool bridge.*
