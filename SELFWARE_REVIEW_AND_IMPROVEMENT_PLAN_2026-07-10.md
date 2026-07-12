# Selfware review and improvement plan

Date: 2026-07-10  
Review scope: architecture, agent loop, tool execution, safety/privacy, configuration, build health, CLI/TUI user experience, and a live OpenRouter GLM-5.2 user simulation.

## Executive verdict

Selfware can launch against OpenRouter and GLM-5.2, stream responses, invoke native tools, display a dashboard, request permission, write a file, and read it back. Those are meaningful working foundations.

It is not ready for a production or security-sensitive release. The simulated one-file task exposed incorrect text entry, a broken help surface, incomplete session clearing, repeated requests for a denied and explicitly prohibited shell command, a non-code verification loop, and an in-flight run that did not stop after `SIGINT` or `SIGTERM`. The architecture review also found release-blocking workspace-isolation and tool-policy drift, while the security review found paths that can expose the API key to child processes and untrusted repository hooks.

Recommended release posture: block the next production release until Phase 0 through Phase 3 below are complete and the listed acceptance tests pass.

## Functional progress recheck — 2026-07-11

The core constrained-file harness flow is now functional in the working tree. A fresh before/after simulation used the explicit OpenRouter endpoint and `z-ai/glm-5.2`, a clean temporary workspace, an eight-turn cap, and the exact task:

> Create a file named `user-check_1+2=3.txt` containing exactly `visual validation passed` using `file_write`. Do not run shell commands or use `pty_shell`. Do not change any other file.

Before the current fixes, Selfware wrote the correct 24-byte file but incorrectly treated the negated `pty_shell` mention as a required tool. It attempted the prohibited tool, repeated completion prose, and aborted at Step 9/8 with `MAX_ITERATIONS`. The omitted artifact flag also created nine hidden `.selfware/turns/*.json` transcripts.

After the fixes, GLM-5.2 completed in Step 4/8 through the intended sequence: `file_write` permission, successful write, same-path `file_read`, and accepted completion. It did not request or execute a shell tool. A filesystem truth check found only the original `README.md` and the requested file; no hidden turn artifacts were created. The target contains exactly 24 bytes with no trailing newline.

| Recheck step | Health | Result |
|---:|---|---|
| 1. Launch current dashboard | Degraded | The TUI starts and accepts input, but its top status strip is still border-only. |
| 2. Enter punctuation-heavy filename | Good | `_`, `+`, `=`, and `-` survive exact input; Unicode editing regressions also pass. |
| 3. Inspect mutation permission | Good | The modal is responsive, clears its area, wraps details, and keeps allow/deny controls visible. |
| 4. Approve `file_write` | Good | Only the requested 24-byte text file is written. |
| 5. Enforce non-code verification | Good | Completion guidance requests `file_read`, not a build, test, shell, or PTY command. |
| 6. Complete after readback | Good | Fresh same-path readback satisfies the artifact gate; the run completes within the turn bound. |
| 7. Inspect workspace side effects | Good | No `.selfware/turns` directory appears when the config omits `disable_turn_artifacts`. |
| 8. Cancel and exit | Degraded | Typed Agent cancellation no longer emits completion, but a live provider request stalled before response headers still left the TUI owner alive beyond 13 seconds after the renderer restored. In-flight shutdown remains open. |

Fresh evidence:

- [before: readable dashboard](artifacts/functional-recheck-2026-07-11/before/01-dashboard.png)
- [before: clipped permission modal](artifacts/functional-recheck-2026-07-11/before/02-permission-modal.png)
- [before: max-iteration failure](artifacts/functional-recheck-2026-07-11/before/04-max-iterations.png)
- [after: dashboard](artifacts/functional-recheck-2026-07-11/after/01-dashboard.png)
- [after: readable permission modal](artifacts/functional-recheck-2026-07-11/after/02-permission-modal.png)
- [after: readback completion](artifacts/functional-recheck-2026-07-11/after/03-complete-direct.png)

Implemented in this functional slice:

- artifact-aware completion from checkpoint tool receipts, including untracked non-code paths and fresh full-file readback;
- negation precedence and natural-language shell-category prohibition for both `shell_exec` and `pty_shell`;
- exact denied-call retry memory so the same permission is not requested again;
- typed cancellation with checkpoint persistence and no false `Completed` event;
- a responsive, clearing, wrapped permission overlay;
- suppression of raw safety output while Ratatui owns the screen; and
- privacy-safe turn-artifact deserialization defaults.

Focused regressions cover the exact live filename/task, stale and partial readbacks, mixed source/artifact tasks, shell policy, denial memory, cancellation semantics, signal notification, modal layout at wide and compact sizes, and omitted artifact configuration. The focused set passes; `cargo build --bin selfware` also passes. Two unrelated pre-existing unused-import warnings remain.

This does not clear the broader production release bar. The next functional priorities are in-flight provider cancellation and TUI ownership, the unified tool/safety registry, removal of process-global working-directory mutation, hook environment isolation and repository trust, activation enforcement in every dispatch path, multiline `/help`, truthful connectivity/status UI, resize handling, and real ASCII/no-color behavior.

## What was validated

### OpenRouter GLM-5.2

The runtime was always launched with an explicit config override because the repository-local `selfware.toml` selects a different local Qwen endpoint. The effective runtime configuration was:

- Endpoint: `https://openrouter.ai/api/v1`
- Model: `z-ai/glm-5.2`
- Context length: 1,048,576
- Native function calling: enabled
- Streaming: enabled
- API key: present; the value was never printed

The live `llm-doctor` run passed:

- configuration sanity;
- endpoint reachability;
- configured-model discovery;
- context-window agreement;
- a real connection test;
- native tool calling;
- SSE streaming; and
- acceptance of the configured thinking parameters.

It warned that the configured model is text-only for Selfware's vision feature. The doctor also misclassified OpenRouter as `sglang`, printed all 345 available models, described GLM-5.2 as an unknown quality tier, and recommended an SGLang-only optimization. These are diagnostic-quality issues, not connection failures.

Validated command:

```bash
./target/release/selfware \
  --config "$HOME/.config/selfware/config.toml" \
  llm-doctor
```

### Build and test health

| Check | Result | Notes |
|---|---|---|
| `cargo check --all-targets` | Passed | Warnings remain. |
| `cargo fmt --all -- --check` | Failed | Widespread formatting drift in source, tests, examples, and experiments. |
| CI-equivalent Clippy with warnings denied | Failed | Review run reported 21 library and 65 test errors; includes a mutex guard held across `await`. |
| Targeted safety tests | Failed | 76 passed, 1 stale protected-branch expectation failed. |
| Turn-artifact unit tests | Passed | 8 passed, but secret fields and file modes are not covered. |
| `cargo test --workspace --all-features` | Hung | Compiled and began 9,384 library tests, then remained stuck for more than 20 minutes; several tool-dispatch tests were already reported as running for more than 60 seconds. It was interrupted. |
| `selfware validate` | Not functional | The command is explicitly a placeholder and always bails. |

The repository declares Rust 1.85, while code and CI use APIs/toolchains from 1.91 or newer. The declared MSRV is therefore not currently credible.

## Simulated user flow and visual audit

The flow ran in an isolated temporary workspace using the release binary and live GLM-5.2. Screenshots were captured from the actual terminal window and inspected before acceptance.

| Step | User action | Health | Result |
|---:|---|---|---|
| 1 | Launch the dashboard | Degraded | Dashboard rendered, but the top status area contained only a border/title and the UI claimed model connectivity without having tested it. |
| 2 | Request `/help` | Broken | Multiline help became a clipped run-on paragraph and damaged the visible layout. |
| 3 | Ask GLM-5.2 to read a file | Partial | `file_read` ran successfully, but the UI exposed no path/result details and reported completion even though the requested assertion was not met. |
| 4 | Ask for a one-line text file | Broken | `-` was consumed as a global animation shortcut, changing `user-check.txt` to `usercheck.txt`; the permission modal clipped its raw JSON. |
| 5 | Approve the write | Partial | The file was written and read back correctly, but the agent continued after announcing completion. |
| 6 | Deny an unrequested shell verification | Broken | Denial caused more reads and another semantically equivalent shell request. |
| 7 | Deny again | Broken/nonterminating | The task reached Step 11/100 for a one-file write and immediately requested shell access again. |
| 8 | Stop an in-flight run | Broken | `SIGINT` and `SIGTERM` did not stop the active TUI run; cleanup required `SIGKILL`. An idle `/quit` did exit cleanly. |
| 9 | Launch with `--ascii --no-color --theme high-contrast` | Degraded | The TUI still rendered emoji and the same colored palette. |

### Step 1 — dashboard start

![Dashboard start](artifacts/visual-audit-2026-07-10/01-dashboard-start.png)

Strengths: the chat-first hierarchy is understandable, input is visible, and secondary garden/tool/log panes are easy to locate.

Risks: the status strip is allocated too little height, empty panes consume substantial space, garden labels clip, and `Connected to model` is logged unconditionally. The layout allocates 5% to status in `src/ui/tui/layout.rs:414-420`, while the bordered status widget requires at least three rows.

### Step 2 — help

![Help](artifacts/visual-audit-2026-07-10/02-help.png)

The help content is unusable. `/help` creates a multiline system message at `src/ui/tui/mod.rs:1393-1428`, but the chat wrapper at `src/ui/tui/mod.rs:1839-1912` does not split embedded newlines into separate Ratatui lines.

### Step 3 — read-only tool turn

![Read tool response](artifacts/visual-audit-2026-07-10/03-read-tool-response.png)

The model/tool round trip completed, but tool feedback is only `[file_read] started/completed`; it omits the target path, result summary, verification status, and expandable detail. The old broken help message continued to dominate the viewport.

### Step 4 — write permission

![Write permission](artifacts/visual-audit-2026-07-10/04-permission.png)

The permission gate correctly stopped the mutation. The decision surface itself is unsafe: the fixed 60x8 modal clips the path/arguments, does not wrap or scroll, and does not clear the background before drawing (`src/ui/tui/dashboard_widgets.rs:626-678`). A user cannot reliably inspect the full operation before approving it.

### Step 5 — prohibited shell request

![Shell permission](artifacts/visual-audit-2026-07-10/05-shell-permission.png)

After write/readback success and multiple completion statements, the agent requested `shell_exec` even though the task said not to run shell commands. Natural-language tool-policy extraction only recognizes narrow phrases and exact backticked tool names (`src/agent/tool_dispatch.rs:464-485,586-601`).

### Step 6 — denial recovery

![Denial recovery](artifacts/visual-audit-2026-07-10/06-denial-recovery.png)

The denial did not become a task-scoped policy. The agent continued with `directory_tree` and repeated `file_read` calls. A `.txt` output is classified as a mutation, but the completion gate demands a supported source-code diff and formal build/test verification (`src/agent/verification.rs:472-528,646-659`). Readback verification of a plain-text artifact cannot satisfy that gate.

### Step 7 — repeated permission request

![Repeated permission](artifacts/visual-audit-2026-07-10/07-repeat-permission.png)

The second shell command was semantically equivalent but textually different. Exact-signature repetition logic did not catch it. There is no task-scoped `deny shell`, semantic denial cooldown, or `cancel task` choice.

### Step 9 — accessibility flags

![Accessibility flags](artifacts/visual-audit-2026-07-10/08-accessibility-flags.png)

The screenshot confirms that the TUI does not honor the CLI's ASCII/no-color intent. Screenshot evidence cannot establish keyboard accessibility, screen-reader semantics, contrast ratios, zoom/reflow behavior, or full WCAG compliance; those require terminal-level and assistive-technology testing.

## Prioritized findings

### P0 — release blockers

#### 1. Child processes and hooks can expose the API key

`process_start` accepts arbitrary commands, the initial process spawn inherits Selfware's entire environment, and immediate-exit output is returned to the agent/log path (`src/tools/process.rs:97-190`, `src/devops/process_manager.rs:536-550,727-766`). The restart path clears the environment, proving inconsistent handling.

Repository-local `selfware.toml` files are automatically discovered, and project hooks execute through `sh -c` with the inherited environment and outside the normal safety/confirmation path (`src/config/loader.rs:210-241`, `src/hooks/shell_handler.rs:92-113`, `src/agent/tool_dispatch.rs:2157-2165`). Opening an untrusted checkout can therefore become code execution with access to host secrets.

Required fix:

- call `env_clear()` for every child process and hook;
- add back a minimal documented allowlist (`PATH`, `HOME`, locale, and explicit task variables);
- reject or redact sensitive names and values in requested child environments and outputs;
- require an explicit repository-trust decision before enabling project hooks;
- route hooks through the same permission and audit pipeline as model tools; and
- add a sentinel-secret regression test proving the value never reaches child stdout, logs, artifacts, or model context.

#### 2. TUI input changes requests and crashes on Unicode

Global handlers intercept `+`, `=`, `-`, and `_` before normal character insertion (`src/ui/tui/mod.rs:1317-1329`). In the live test, this changed the requested filename. `z` is also globally intercepted. Separately, the cursor counts characters while `String::insert/remove` require byte indexes (`src/ui/tui/app.rs:471-503`), so multibyte input can panic.

Required fix:

- scope shortcuts to a command/navigation mode or require modifiers;
- let every printable character reach the input editor while it has focus;
- store the cursor as a byte index or use a grapheme-aware text buffer;
- cover insert, delete, arrows, paste, composition, and emoji; and
- ensure a TUI worker panic restores the terminal and terminates the owner process with a nonzero result.

#### 3. Non-code mutations cannot complete safely

The verification gate treats creation of `.txt`, `.md`, JSON, configuration, or generated artifacts as a source-code repair. It then requires a supported source extension and formal test/build evidence. The live task repeatedly requested shell commands after correct file readback and after explicit denial.

Required fix:

- classify outcomes as source edit, test edit, configuration edit, documentation edit, generated artifact, or external side effect;
- define verification policies per class;
- accept exact readback/hash/size/schema checks for non-code artifacts;
- condition or remove the unconditional `has_written_any_file` build/test gate;
- parse user tool prohibitions into task-scoped tool categories;
- record denial as a typed outcome with `deny once`, `deny category for task`, and `cancel task`; and
- suppress semantically equivalent denied retries.

#### 4. Tool registration and tool safety are separate, contradictory catalogs

Tools are registered in `src/tools/mod.rs:360-529`, while safety uses a separate manual tool-name switch in `src/safety/checker/validation.rs:47-268`. Unknown names fail closed. Registered tools missing from safety include critical/deferred editing, PTY, worktree, LSP, analysis, RadarCam, and knowledge tools. The production `pty_shell` path is one confirmed example.

Required fix:

- make a mandatory `ToolSpec` part of registration;
- include risk, mutation scope, path/argument validators, permission policy, activation state, cacheability, parallel safety, timeout, environment policy, and verification contribution;
- fail registration/CI when any tool lacks policy; and
- run contract tests through the Agent dispatcher, not by calling tools directly.

#### 5. Process-global working directory breaks isolation and concurrency

Subagents and worktree tools call `std::env::set_current_dir` and retain process-global state (`src/agent/subagent.rs:85-167`, `src/tools/git_worktree.rs:28-100`). A global mutex serializes nominally concurrent subagents, while parent/unrelated tools do not share the same protection and can run in the wrong worktree.

Required fix:

- introduce immutable `WorkspaceContext { root, task_root, worktree }`;
- inject it into every Agent and Tool execution;
- resolve filesystem arguments against it;
- set subprocess cwd explicitly; and
- prohibit `set_current_dir` after process initialization.

#### 6. Workspace path checks allow sibling-prefix escape

The fallback path check compares normalized strings with `starts_with(working_dir)` (`src/safety/path_validator.rs:471-474`). A workspace such as `/home/rig/selfware` can therefore match `/home/rig/selfware-evil/...`.

Required fix: canonicalize existing parents and use component-aware `Path::starts_with`, including correct handling for not-yet-created targets. Add exact-root, child, `..`, symlink, and sibling-prefix tests.

#### 7. Active runs do not cancel or shut down reliably

The live TUI run remained alive after `SIGINT` and `SIGTERM` and required `SIGKILL`. Code review also found cancellation recorded as abandoned but returned as `Ok(())`, allowing the wrapper to emit completion (`src/agent/task_runner.rs:593-605,703-713`).

Required fix:

- return `RunOutcome::{Completed, Failed, Cancelled, Paused}`;
- propagate a shared cancellation token through model streaming, tool execution, confirmation waits, persistence, and TUI ownership;
- persist and flush a paused checkpoint before returning `Cancelled`;
- make the ten-second force-exit watchdog independent of component handlers; and
- test `/quit`, Escape/cancel, Ctrl+C, SIGINT, SIGTERM, and cancellation during model/tool/permission phases.

### P1 — high priority

#### 8. Parallel execution bypasses the normal tool pipeline

Parallel dispatch calls `tool.execute` directly and skips parts of the sequential pipeline: activation enforcement, cache behavior, snapshots, permits, progress events, timeouts, and hooks (`src/agent/tool_dispatch.rs:1828-1945,2191-2257,3283-3434`).

Build one `ToolExecutor::execute(context, call) -> ToolOutcome`; sequential and parallel modes should schedule only this method. Apply one concurrency governor to tools and model streams.

#### 9. Deferred activation is bypassed at execution time

The registry exposes an activation-aware lookup, but execution uses unrestricted lookup (`src/tools/mod.rs:595-607`, `src/agent/tool_dispatch.rs:2201,3307`). XML tool calls can therefore guess deferred names while native calls see a different contract.

Use activated lookup in every path and return a structured discovery hint for inactive tools.

#### 10. Session boundaries are ambiguous

`/clear` only calls `app.clear_chat()` (`src/ui/tui/mod.rs:1380-1384`, `src/ui/tui/app.rs:218-227`); it does not clear the Agent's model conversation. The next live answer included the prior task after the UI said chat was cleared. Agent reuse also retains messages/context across top-level tasks.

Create explicit operations:

- `clear view` for display only;
- `new task/session` for a fresh model/task state; and
- `follow up` for intentional conversational continuity.

The UI copy must state which operation occurred.

#### 11. Turn artifacts and runtime state are not private by default

Turn artifacts are enabled by default; the request is sanitized, but raw response, reasoning, tool calls, and decision data are stored without explicit private modes (`src/agent/execution.rs:333-386`, `src/agent/turn_artifacts.rs:121-173`). Audit/checkpoint files have similar mode gaps.

Recursively redact all persisted fields, use atomic `0600` files inside `0700` directories, make verbose reasoning retention opt-in, document retention/cleanup, and test secrets in every field.

#### 12. Provider abstraction is not truly injectable

Agent owns a concrete API client, and the nominal LLM trait exposes a reqwest-specific streaming response. OpenRouter/SGLang behavior lives in shared code, which contributed to `llm-doctor` misclassification.

Inject `Arc<dyn ModelClient>`, return a boxed event stream, and implement provider adapters for capabilities, request/response translation, headers, error taxonomy, reasoning, tool calls, and diagnostics. Add a first-class OpenRouter GLM-5.2 adapter/test profile.

#### 13. Persistence is race-prone and can lose tail data

Concurrent Agents rewrite shared learning state, checkpoint HMAC initialization has a first-use race, some persistence is detached, and logging uses an unbounded channel without an explicit terminal flush.

Create one process-owned bounded persistence service with append/journal or SQLite semantics, atomic compaction, serialized merge operations, `OnceLock` key initialization, and `flush_and_shutdown`.

#### 14. The core TUI surfaces need responsive and truthful rendering

Required fixes:

- render a loading shell before Agent initialization and workspace scanning;
- allocate a fixed minimum of three rows for status;
- derive connection state from a real check and use `checking/connected/offline/error` states;
- split multiline content before wrapping;
- clear, wrap, and scroll help/permission/other overlays;
- show structured permission fields: tool, target, action, risk, full command/diff;
- add expandable tool target/result details;
- make pane focus order deterministic;
- make garden details responsive;
- propagate ASCII/no-color/high-contrast preferences throughout the TUI; and
- stop displaying raw Markdown delimiters in normal messages.

### P2 — structural cleanup

After the release blockers and execution core are fixed:

- consolidate the two `ShellExec` implementations and run contracts against the registered production type;
- replace string-matched recovery with typed errors and retry metadata;
- fail closed on corrupt checkpoints instead of silently fabricating a blank task;
- move synchronous checkpoint I/O out of the async loop;
- consolidate messages, token accounting, compression, and context maps into one `ConversationState`;
- register internal context tools so XML and native modes have the same tool surface;
- bound bulk context reads by files, bytes, and concurrency;
- make the state machine the actual source of truth for terminal outcomes;
- either reuse the core Agent runner for `MultiAgentChat` or rename it as a tool-less parallel panel;
- require `ValidatedConfig` for programmatic Agent construction;
- make multi-file edit genuinely atomic or rename/document its partial-commit semantics;
- remove tracked runtime key material, generated swarm workspaces, dependency caches, and `node_modules`; and
- align README, SECURITY, and PRIVACY claims with actual egress, redaction, sandbox, and screenshot behavior.

## Phased implementation plan

### Phase 0 — containment and release freeze (1-2 engineering days)

Deliverables:

1. Clear/allowlist child and hook environments.
2. Disable untrusted project hooks until a repository-trust prompt exists.
3. Fix component-aware workspace containment.
4. Default turn artifacts to off or fully redact and lock their permissions.
5. Add temporary CI inventory checks that fail on registered tools without safety policies.
6. Remove/rotate tracked runtime HMAC keys and generated caches.

Exit criteria:

- sentinel `SELFWARE_API_KEY` cannot appear in child stdout, hook output, logs, checkpoints, or turn artifacts;
- sibling-prefix and symlink escape tests pass;
- opening an untrusted repository cannot execute a hook before explicit approval; and
- repository secret/history scan is clean or every finding is explicitly dispositioned.

### Phase 1 — input, permission, and TUI crash safety (2-4 days)

Deliverables:

1. Grapheme-safe input buffer and cursor.
2. Contextual/modifier-based shortcuts that do not consume normal typing.
3. Structured permission modal with clear background, wrapping, scrolling, and full details.
4. Typed permission outcomes: allow once, allow category, deny once, deny category, cancel task.
5. Panic-safe terminal restoration and owner-process teardown.

Exit criteria:

- filenames containing `-`, `_`, `+`, and `=` reach the Agent unchanged;
- Unicode insert/delete/navigation/paste tests pass;
- a 200-character command and long path remain reviewable at 80x24;
- one denial prevents semantically equivalent retries for the selected scope; and
- a forced TUI worker panic restores the terminal and returns nonzero without leaving a live process.

### Phase 2 — task semantics, verification, and cancellation (3-5 days)

Deliverables:

1. Typed task/output classification.
2. Verification strategies per output class.
3. Natural-language tool-category restrictions.
4. Typed `RunOutcome` and end-to-end cancellation.
5. Fresh `TaskSession` versus explicit follow-up semantics.

Exit criteria:

- the exact live scenario—create a text file, prohibit shell, verify by readback—completes within a small fixed turn/token bound;
- no shell prompt is shown;
- task success asserts the requested filename and exact bytes, not merely loop termination;
- `/clear` or `new session` behavior matches its copy and tests; and
- SIGINT/SIGTERM during generation, tools, and permissions exit within the documented grace period.

### Phase 3 — unify tools and workspace isolation (1-2 weeks)

Deliverables:

1. `WorkspaceContext` with no runtime global-cwd mutation.
2. Mandatory `ToolSpec` catalog.
3. One `ToolExecutor` for sequential/parallel/native/XML execution.
4. Typed `ToolOutcome` instead of failures hidden in arbitrary JSON.
5. One activation and concurrency policy.

Exit criteria:

- every registered tool passes policy/schema/activation/benign-dispatch contracts;
- native and XML tool matrices expose and enforce the same capabilities;
- three subagents execute concurrently in isolated worktrees without changing parent cwd;
- parallel and sequential runs emit the same hooks, metrics, timeout, permission, and cache behavior; and
- `concurrency.max_tools` and stream limits are observed under load.

### Phase 4 — provider, context, and persistence boundaries (1-2 weeks)

Deliverables:

1. Injectable `ModelClient` and provider adapters.
2. First-class OpenRouter capability/diagnostic handling.
3. Canonical `ConversationState` and explicit task/follow-up lifecycles.
4. Bounded persistence worker with atomic merge/flush.
5. Race-free HMAC key initialization and fail-closed recovery.

Exit criteria:

- deterministic mock streaming/tool/error tests require no network;
- OpenRouter GLM-5.2 connection, streaming, reasoning, native tool, denial, retry, and long-context tests pass;
- diagnostics no longer label OpenRouter as SGLang or recommend backend-inapplicable flags;
- concurrent Agents cannot lose learning/checkpoint/log updates; and
- crash/restart preserves task identity and rejects corrupt state safely.

### Phase 5 — visual quality and accessibility (3-5 days)

Deliverables:

1. Loading, connected, offline, working, permission, error, cancelled, and complete states.
2. Responsive status/garden/tool/log layouts.
3. Correct multiline Markdown rendering and expandable tool cards.
4. ASCII, no-color, and high-contrast TUI themes.
5. Deterministic keyboard focus and visible focus indicators.

Exit criteria:

- TestBackend/PTY golden tests pass at 80x24, 120x35, and 160x50;
- `/help`, permission, long output, resize, and scrolling preserve pane chrome;
- all important states are understandable without color alone;
- ASCII mode contains no emoji or box glyphs outside its allowed set;
- keyboard-only flow covers input, pane focus, modal decisions, cancellation, and exit; and
- measured contrast and zoom/reflow checks meet the chosen accessibility target.

### Phase 6 — restore the release pipeline (about 1 week)

Deliverables:

1. Apply formatting and fix Clippy warnings/errors.
2. Fix or quarantine the full-suite hangs with issue links and timeouts.
3. Align declared MSRV, CI toolchain, and API usage.
4. Run integration-feature tests in CI.
5. Add `cargo audit`, `cargo deny`, secret scanning, and dependency/action pinning.
6. Remove generated artifacts from Git and add robust ignores.
7. Replace placeholder `selfware validate` with real UI/runtime validation or remove it from the public CLI.
8. Update README/SECURITY/PRIVACY and ship current screenshots.

Exit criteria:

- format, check, Clippy, unit, integration, E2E, visual, security, and MSRV jobs are green;
- no test can run indefinitely without a bounded timeout and diagnostic artifact;
- dependency advisories are fixed or explicitly risk-accepted with expiry;
- release artifacts reproduce from pinned inputs; and
- documentation claims are exercised by automated tests where practical.

## Minimum regression matrix

| Area | Required cases |
|---|---|
| Input | ASCII punctuation, Unicode, emoji, combining marks, paste, arrows, backspace, long lines |
| Viewports | 80x24, 100x30, 120x35, 160x50, live resize, zoom/reflow |
| Permissions | allow once, deny once, deny category, cancel; long paths/commands; repeated semantic request |
| Tasks | read-only answer, source edit, test-only edit, text artifact, config edit, delete, external side effect |
| Models | deterministic mock, GLM-5.2 native, XML fallback, streaming/non-streaming, rate limit, timeout, malformed tool call |
| Safety | sibling-prefix path, symlink, `..`, protected branch, untrusted hooks, env-secret sentinel, PTY/process parity |
| Concurrency | parent + subagents, multiple worktrees, parallel tools, cancellation during parallel work, persistence contention |
| Recovery | user denial, model retry, network retry, corrupt checkpoint, restart, SIGINT, SIGTERM, worker panic |
| Output | text, JSON, stream JSON, CLI, TUI, ASCII, no-color, high-contrast |

## Definition of done

Selfware should not be called production-ready until all of the following are true:

- no host secret is inherited or persisted outside an explicit allowlist;
- every tool has one registered, tested execution/safety policy;
- every Agent/tool operates in an immutable workspace context;
- task success, failure, cancellation, and pause are typed and observable;
- a denied category is respected for the rest of its selected scope;
- non-code tasks terminate with appropriate verification;
- the TUI preserves exact input, survives Unicode, and exits reliably;
- all visual states remain legible at the supported minimum viewport;
- CI format/Clippy/test/integration/security/MSRV gates are green; and
- the OpenRouter GLM-5.2 journey passes without shell-policy violations, repeated permissions, or forced termination.

## Evidence limits

The live audit covered one isolated TUI task plus diagnostics and selected build/test paths. It did not exhaust every command, multi-agent behavior, external integration, editor extension, browser/computer tool, operating system, terminal emulator, or assistive technology. Screenshot findings establish visible layout and content issues only; they do not prove full accessibility compliance. Dependency versions flagged for advisory review still need an installed, current `cargo audit`/`cargo deny` run before release decisions are finalized.
