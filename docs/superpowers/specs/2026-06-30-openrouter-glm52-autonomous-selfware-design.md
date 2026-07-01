# OpenRouter-only GLM-5.2 autonomous selfware setup

**Date:** 2026-06-30  
**Scope:** Configure the selfware agent to use the OpenRouter `z-ai/glm-5.2` endpoint as its only model, enable full YOLO/autonomous mode with computer control, spawn monitor agents to catch failures early, and capture all interactions for training/audit. After setup, run a single task that reviews this PC’s resources, inspects the selfware harness, and answers the three big questions.

## System snapshot

| Resource | Value |
|----------|-------|
| OS | Ubuntu 24.04, Linux 6.17, x86_64 |
| CPU | AMD EPYC 7V73X, 64 cores, 4 NUMA nodes |
| RAM | 503 GiB total, ~481 GiB available |
| Disk | 1.8 TB total, ~573 GB free |
| GPUs | 2× NVIDIA RTX 4090 (24 GB) + Tesla PG500-216 (32 GB) |
| Local model already running | `llama-server` serving `GLM-5.2-Q4_K_M` at `127.0.0.1:8080` with external speculative draft socket `/tmp/glm52-drafter-5k.sock` |
| OpenRouter key | `OPENROUTER_API_KEY` is exported in the environment (length 73) |
| Selfware binary | Not yet built in this checkout |

## 1. Configuration

Create `selfware-openrouter-glm52.toml` in the repo root. Do **not** store the real API key in the file.

```toml
endpoint = "https://openrouter.ai/api/v1"
model = "z-ai/glm-5.2"
api_key = "$SELFWARE_API_KEY"
max_tokens = 16384
temperature = 0.1

[agent]
max_iterations = 30
step_timeout_secs = 600
token_budget = 32768
context_window = 32768
native_function_calling = false
streaming = false
min_completion_steps = 2
require_verification_before_completion = true

[retry]
max_retries = 8
base_delay_ms = 2000
max_delay_ms = 60000

[yolo]
enabled = true
max_operations = 0
max_hours = 0.0
allow_git_push = false
allow_destructive_shell = false
status_interval = 100

[safety]
allowed_paths = ["./**", "/home/rig/**", "/tmp/**"]
denied_paths = ["**/.env", "**/.env.local", "**/.ssh/**", "**/secrets/**"]
protected_branches = ["main"]
require_confirmation = []

[[safety.permissions]]
tool_pattern = "file_*"
resource_pattern = "./**"
reason = "Allow file operations inside the workspace"

[[safety.permissions]]
tool_pattern = "shell_exec"
reason = "Allow shell commands for build/test/research"

[[safety.permissions]]
tool_pattern = "cargo_*"
reason = "Allow cargo build/test/check"

[[safety.permissions]]
tool_pattern = "computer_*"
reason = "Allow desktop automation for research tasks"

[continuous_work]
enabled = true
checkpoint_interval_tools = 10
checkpoint_interval_secs = 300
auto_recovery = true
max_recovery_attempts = 3

[debug]
log_requests = true
log_responses = true
log_turns = true
log_gates = true
```

## 2. Environment & build

At runtime:

```bash
export SELFWARE_API_KEY="$OPENROUTER_API_KEY"
export DISPLAY=:1          # required for computer_* tools; verify at runtime
```

Build:

```bash
cargo build --release --all-features
```

Launch:

```bash
SELFWARE_API_KEY="$OPENROUTER_API_KEY" DISPLAY=:1 \
  ./target/release/selfware --config selfware-openrouter-glm52.toml chat
```

## 3. Computer-control readiness

- The machine is running X11 on GPU 1; the agent needs `DISPLAY` exported.
- `computer_*` tools are pre-approved via `safety.permissions`.
- Dangerous key combos (`ctrl+alt+delete`, `alt+f4`, etc.) are blocked in code.
- First use of computer control per session may still trigger a confirmation prompt depending on the current selfware build; the harness will handle it.

## 4. Verification & first autonomous task

After build, run in order:

1. `./target/release/selfware --config selfware-openrouter-glm52.toml doctor`
2. `SELFWARE_API_KEY="$OPENROUTER_API_KEY" ./target/release/selfware --config selfware-openrouter-glm52.toml llm-doctor`
3. A single headless task that reviews resources, inspects the harness, and answers the three big questions.

Task prompt (rough; final version in implementation plan):

> Review the system resources of this Linux PC (CPU/RAM/GPU/disk/installed backends), inspect the selfware harness (src/agent, src/tools, src/api, src/computer), then answer these questions:
> 1. Which local/remote model and quant fit this hardware?
> 2. How should selfware + OpenRouter + glm-5.2 be configured for autonomous coding?
> 3. Which safety / computer-control settings are appropriate?
> Also produce a short draft note on how the already-running local GLM-5.2-Q4_K_M llama-server (with external speculative draft) could be used for inference optimization if we later switch away from OpenRouter.

## 5. Conditional draft: local GLM-5.2 inference optimization

The local `llama-server` is already running with:

```bash
./llama-server \
  -m /home/rig/models/glm-5.2-q4_k_m/GLM-5.2-Q4_K_M-00001-of-00008.gguf \
  -ngl 999 -ncmoe 500 -sm layer -ts 0.10,0.45,0.45 \
  -c 8192 -t 64 -tb 64 -b 512 -ub 512 -fa on \
  -ctk q4_0 -ctv q4_0 --no-warmup \
  --host 127.0.0.1 --port 8080 \
  --spec-type external-draft --spec-draft-socket /tmp/glm52-drafter-5k.sock
```

Key flags:
- `-ts 0.10,0.45,0.45` — tensor split across GPU 0 / GPU 1 / GPU 2.
- `-ncmoe 500` — MoE expert cache / routing hotpath tuning.
- `--spec-type external-draft` + `--spec-draft-socket` — speculative decoding via a separate draft worker for lower latency.
- `-ngl 999` — offload all layers to GPU.
- `-c 8192` — context length.

If we later switch to local GLM-5.2, the selfware endpoint becomes `http://127.0.0.1:8080/v1` and the model alias is whatever `llama-server` reports (usually derived from the GGUF path).

## 6. Spawned monitor agents

During implementation and the first run, keep these watchers active:

- **Build monitor:** tail `cargo build --release` output; on failure capture the first error block and propose a fix (dependency, feature flag, env var).
- **Harness monitor:** run `doctor`/`llm-doctor`; if the OpenRouter call fails, diagnose key/model/endpoint/rate-limit and patch the config.
- **Resource monitor:** watch CPU/RAM/GPU/disk while the agent runs; warn on swap pressure or GPU OOM.
- **Patch agent:** applies minimal config/env fixes and re-runs verification. It does **not** modify protected code paths (`src/evolution/`, `src/safety/`, benchmarks, safety module) without explicit approval.

All monitors log to a shared `monitor.jsonl` in the training log directory.

## 7. Failure protocol

1. Monitor detects failure → captures logs and signals the main agent.
2. If it is a config/env issue, the patch agent fixes it inline.
3. If it is a code-level harness bug, the patch agent proposes a minimal fix; the main agent reviews and re-runs.
4. Every fix and re-run is recorded.

## 8. Training/audit data capture

- Keep selfware’s default per-turn artifacts at `<workdir>/.selfware/turns/turn_NNNN.json` (secrets are already redacted).
- The `[debug]` config enables request/response/turn logging.
- After each run, a `Stop` hook copies `.selfware/turns/*.json` and any failures log into:
  `training_logs/openrouter-glm52-<timestamp>/`
- Monitor agents append to `training_logs/openrouter-glm52-<timestamp>/monitor.jsonl`.
- Result: a replayable, redacted dataset of prompts, tool calls, outcomes, and corrections.

## Risks & mitigations

| Risk | Mitigation |
|------|------------|
| API key leak | Key lives only in env; config uses placeholder; artifacts redact secrets. |
| Destructive shell in YOLO | `allow_destructive_shell = false`; denied paths include secrets. |
| Git push to main | `allow_git_push = false`; protected branches. |
| Runaway token spend | 32k token budget, 30 max iterations, 600s step timeout. |
| Computer control mishaps | Dangerous combos blocked; `DISPLAY` must be correct. |
| Build failure | Build monitor catches it and proposes fixes. |

## Open questions

1. Is the X11 display actually `:1`? We must verify `echo $DISPLAY` at runtime.
2. Does the current selfware build require any extra system packages (e.g., `libssl-dev`, X11 libs) for computer control on this machine?
3. Will OpenRouter accept `z-ai/glm-5.2` with XML-style tool calls and `streaming = false` as configured?

## Next step

After this spec is approved, invoke the `writing-plans` skill to produce a concrete implementation plan.
