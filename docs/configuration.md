# Configuration Reference

Selfware is configured via TOML files. Config is loaded from (in order of priority):

1. `--config <path>` CLI argument
2. `SELFWARE_CONFIG` environment variable
3. `selfware.toml` in the current directory
4. `~/.config/selfware/config.toml`

## Precedence Chain

Once a config file is selected, individual *values* are resolved through a
cascade. Higher layers override lower layers:

```
built-in defaults  <  model-defaults profile  <  TOML file  <  env var  <  CLI flag
```

| Layer | Example source | Notes |
|-------|----------------|-------|
| 1. Built-in defaults | `default_endpoint()`, `default_temperature()` | Compiled in |
| 2. Model-defaults profile | Built-in rule matched by the configured model name (e.g. GLM-5.2, Qwen3.6) | Fills only fields you did **not** set explicitly via TOML or env |
| 3. TOML file | `selfware.toml` top-level fields | Loaded once |
| 4. Env var | `SELFWARE_TEMPERATURE=0.2` | Per-process |
| 5. CLI flag | `selfware --max-turns 20` | Per-invocation; only some settings have flags |

There are no `--endpoint` / `--model` / `--temperature` CLI flags — use the
`SELFWARE_ENDPOINT` / `SELFWARE_MODEL` / `SELFWARE_TEMPERATURE` environment
variables for per-invocation overrides. The CLI flags that do adjust
configuration are `--mode` / `-y` / `--daemon` (execution mode),
`--profile <name>` (built-in profiles `architect`, `swarm-8`, `batch-16`,
`batch-32`, `visual`, `quick` — overrides `max_tokens`, `temperature`,
iteration and timeout limits), `--max-turns`, `--max-budget-tokens`,
`--max-wall-secs`, `--max-cost-usd` (run bounds), and `--theme` / `--compact`
/ `--verbose` / `--show-tokens` (display).

For example, given:

```toml
# selfware.toml
endpoint    = "http://localhost:8000/v1"
model       = "Qwen/Qwen3.5-27B"
temperature = 0.7
```

```bash
SELFWARE_TEMPERATURE=0.5 selfware --max-turns 20 run "..."
```

…the agent ends up with:

- `endpoint = "http://localhost:8000/v1"` and `model = "Qwen/Qwen3.5-27B"` *(from the TOML file; no env var or CLI flag overrides them)*
- `temperature = 0.5` *(from the env var, which beats the TOML value)*
- `agent.max_iterations = 20` *(from the `--max-turns` CLI flag)*

Run `selfware config show` to print the effective configuration annotated
with the source (default / TOML / env / CLI) of every value.

## Top-Level Settings

```toml
# API endpoint URL (must start with http:// or https://)
# Default: "https://openrouter.ai/api/v1"
endpoint = "http://localhost:8000/v1"

# Model identifier sent to the API
# Default: "z-ai/glm-5.2"
model = "Qwen/Qwen3-Coder-Next-FP8"

# Maximum tokens in the model response
# Default: 65536
max_tokens = 65536

# Sampling temperature (0.0 = deterministic, higher = more creative)
# Default: 1.0
temperature = 1.0

# API key (prefer env var or keyring instead -- see below)
# api_key = "sk-..."
```

### API Key Resolution

The API key is resolved in this priority order:

1. `SELFWARE_API_KEY` environment variable (highest priority, never persisted)
2. System keyring if a key is already stored there
3. `api_key` field in config file (plaintext on disk -- warns, errors in strict mode)

```bash
# Set via environment (recommended for CI/scripts)
export SELFWARE_API_KEY="sk-..."

# Selfware reads from the system keyring automatically if a key is already stored there.
# This repo does not expose a dedicated CLI command for writing to the keyring.
```

### Extra Body Fields

Merge arbitrary fields into every chat-completion API request. Useful for backend-specific extensions:

```toml
[extra_body]
chat_template_kwargs = { enable_thinking = false }
```

### `context_mode`

Evolve workspace context tier. `"auto"` (default) measures each tier and picks
the richest one fitting `context_fit_ratio` of the usable window; or pin a tier:
`"map"`, `"lite"`, `"compact"`, `"full"`, `"full_extended"`.

### `context_fit_ratio`

Fraction of the usable window (`context_length` minus output reserve) the
composed context may occupy in `auto` mode. Default `0.70`, range `0.1..=1.0`.

## `[agent]` -- Agent Behavior

```toml
[agent]
# Maximum tool-call iterations per task
# Default: 100
max_iterations = 100

# Timeout per agent step in seconds
# Default: 300
step_timeout_secs = 300

# Token budget for a single session
# Default: 0 (derived at load time as 60% of context_length)
token_budget = 500000

# Use native function calling (requires backend support like sglang --tool-call-parser)
# When false (default), tools are embedded in the system prompt
# Default: false
native_function_calling = false

# Stream LLM responses as they arrive
# Default: true
streaming = true

# Minimum steps before the agent can declare task completion
# Prevents premature self-termination
# Default: 3
min_completion_steps = 3

# Require at least one verification (cargo_check/cargo_test/cargo_clippy)
# before accepting task completion
# Default: true
require_verification_before_completion = true
```

## `[safety]` -- Safety Guardrails

```toml
[safety]
# Glob patterns for paths the agent may read/write
# Default: ["./**"]
allowed_paths = ["./**"]

# Glob patterns for paths the agent must never touch
# Default: ["**/.env", "**/.env.local", "**/.ssh/**", "**/secrets/**"]
denied_paths = [
    "**/.env",
    "**/.env.local",
    "**/.ssh/**",
    "**/secrets/**",
]

# Git branches that cannot be pushed to directly
# Default: ["main", "master"]
# Note: this guards `git push` only (the git_push tool and explicit
# `git push <remote> <branch>` via shell_exec) — committing directly on a
# protected branch is NOT blocked.
protected_branches = ["main", "master"]

# Tools that require user confirmation before execution
# Default: ["git_push", "file_delete", "shell_exec"]
require_confirmation = ["git_push", "file_delete", "shell_exec"]

# Reject config files with group/world-readable permissions (mode & 0o077 != 0)
# Also activated via SELFWARE_STRICT_PERMISSIONS=1
# Default: false
strict_permissions = false

# Pre-authorized permission grants to reduce confirmation prompts
# See examples below
permissions = []
```

### Permission Grants

Pre-authorize specific tools to skip confirmation prompts:

```toml
[[safety.permissions]]
tool_pattern = "file_*"
resource_pattern = "./src/**"
reason = "Allow all file operations in src/"

[[safety.permissions]]
tool_pattern = "shell_exec"
expires_at = "2025-12-31T23:59:59Z"
reason = "Temporary shell access for this sprint"

[[safety.permissions]]
tool_pattern = "cargo_*"
reason = "Always allow cargo commands"
```

Fields:
- `tool_pattern` -- tool name or glob (e.g., `"file_*"`, `"shell_exec"`)
- `resource_pattern` -- optional path glob the grant applies to
- `expires_at` -- optional ISO 8601 expiry timestamp
- `reason` -- optional human-readable justification

## `[ui]` -- User Interface

```toml
[ui]
# Color theme: "amber", "ocean", "minimal", "high-contrast"
# Default: "amber"
theme = "amber"

# Enable spinner and progress bar animations
# Default: true
animations = true

# Animation speed multiplier (1.0 = normal, 2.0 = faster)
# Default: 1.0
animation_speed = 1.0

# Start in compact mode (less visual chrome)
# Default: false
compact_mode = false

# Start in verbose mode (detailed tool output)
# Default: false
verbose_mode = false

# Always show token usage after responses
# Default: false
show_tokens = false
```

## `[retry]` -- API Retry Settings

```toml
[retry]
# Maximum retries before failing
# Default: 5
max_retries = 5

# Initial delay before first retry (milliseconds)
# Default: 1000
base_delay_ms = 1000

# Upper bound for retry delay (milliseconds)
# Default: 60000
max_delay_ms = 60000
```

## `[yolo]` -- YOLO Mode Settings

```toml
[yolo]
# Enable YOLO mode by default
# Default: false
enabled = false

# Maximum operations before requiring check-in (0 = unlimited)
# Default: 0
max_operations = 0

# Maximum hours before requiring check-in (0 = unlimited)
# Default: 0.0
max_hours = 0.0

# Allow git push in YOLO mode
# Default: true
allow_git_push = true

# Allow destructive shell commands (rm -rf, etc.) in YOLO mode
# Default: false
allow_destructive_shell = false

# Path to JSONL audit log file
# audit_log_path = "/tmp/selfware-audit.jsonl"

# Send status updates every N operations
# Default: 100
status_interval = 100
```

## `[continuous_work]` -- Long-Running Sessions

```toml
[continuous_work]
# Enable periodic checkpointing
# Default: true
enabled = true

# Save checkpoint after this many tool calls
# Default: 10
checkpoint_interval_tools = 10

# Save checkpoint after this many seconds
# Default: 300
checkpoint_interval_secs = 300

# Enable automatic recovery on failure
# Default: true
auto_recovery = true

# Maximum recovery attempts per failure
# Default: 3
max_recovery_attempts = 3
```

## `[[hooks]]` -- Event Hooks

See [Hooks Guide](hooks.md) for full documentation.

```toml
[[hooks]]
event = "PostToolUse"
match_tools = ["file_write", "file_edit"]
command = "cargo fmt -- {path}"
timeout_secs = 30

[[hooks]]
event = "Stop"
command = "cargo test --quiet 2>&1 | tail -5"
timeout_secs = 120
```

## `[mcp]` -- MCP Server Connections

See [MCP Guide](mcp.md) for full documentation.

```toml
[[mcp.servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "ghp_..." }
init_timeout_secs = 30
```

## `[qa]` -- Quality Assurance

```toml
[qa]
# QA profile: "standard", "strict", or "minimal"
# Default: "standard"
profile = "standard"

# Maximum auto-fix iterations for lint/format errors
# Default: 3
auto_fix_iterations = 3

# Maximum retry iterations for test failures
# Default: 2
test_retry_iterations = 2
```

**Profiles:**

| Profile | Checks | Use Case |
|---------|--------|----------|
| `standard` | syntax + format + lint + test | Normal development |
| `strict` | All checks + security audit + higher thresholds | Production code |
| `minimal` | Syntax + type check only | Fast iteration |

**Quality Grades:**

| Grade | Score |
|-------|-------|
| S | >= 95% |
| A | >= 90% |
| B | >= 80% |
| C | >= 70% |
| D | >= 60% |
| F | < 60% |

## `[models.*]` -- Named Model Profiles

Define multiple LLM backends for different purposes. The top-level
`endpoint`/`model`/... fields define the primary (`default`) profile; named
profiles are picked up automatically by the features that need them — for
example, the vision/screenshot tools use a `[models.vision]` profile when one
is defined and vision-capable. There is no CLI flag to switch between named
profiles.

```toml
[models.coder]
endpoint = "http://localhost:8000/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"
max_tokens = 65536
temperature = 1.0
modalities = ["text"]
context_length = 131072

[models.vision]
endpoint = "http://localhost:8001/v1"
model = "Qwen/Qwen2.5-VL-72B"
api_key = "sk-..."
max_tokens = 8192
temperature = 0.7
modalities = ["text", "vision"]
context_length = 32768
```

A `"default"` profile is auto-generated from the top-level `endpoint`/`model`/`api_key` fields if not explicitly defined.

Profile fields:
- `endpoint` -- API endpoint URL
- `model` -- model identifier
- `api_key` -- optional per-profile API key
- `max_tokens` -- max response tokens (default: 65536)
- `temperature` -- sampling temperature (default: 1.0)
- `modalities` -- `["text"]` or `["text", "vision"]` (default: `["text"]`)
- `context_length` -- context window in tokens (default: 1048576)
- `extra_body` -- per-profile extra request fields

## `[resources]` -- Resource Limits

```toml
[resources]
# (Internal resource management settings -- typically left at defaults)
```

## `[evolution]` -- Self-Improvement Daemon

```toml
[evolution]
# Source files containing prompt construction logic
prompt_logic = []
# Source files containing tool implementations
tool_code = []
# Source files containing cognitive architecture
cognitive = []
# Config keys the agent can modify
config_keys = []
```

## Feature Flags (Cargo)

Selfware uses Cargo feature flags to gate optional modules. Enable them with `cargo build --features <flag>` (or `cargo install --git https://github.com/architehc/selfware --features <flag>`).

Enabled by default: `tui`, `resilience`, `execution-modes`, `log-analysis`, `tokens`, `self-improvement`, `consolidation`.

| Feature | Description |
|---------|-------------|
| `extras` | Convenience flag: all default modules plus `hot-reload`, `vlm-bench`, `bench-harness` |
| `tui` | TUI dashboard mode with animations and demos |
| `resilience` | Error recovery and self-healing |
| `execution-modes` | Auto-edit, yolo, daemon modes |
| `log-analysis` | Log file analysis tools |
| `tokens` | Token counting and budget tracking |
| `self-improvement` | Evolution and self-improvement daemon |
| `consolidation` | Memory consolidation ("sleep") system |
| `hot-reload` | Dynamic library hot-reload (security-sensitive, off by default) |
| `vlm-bench` | Vision LLM benchmarking |
| `bench-harness` | Concurrent benchmark harness for throughput testing |
| `redis` | Redis backend for workflow state (requires a Redis server) |
| `vendored-openssl` | Vendored OpenSSL for cross-compilation (used by release builds) |
| `system-tests` | System-level E2E tests (require a live LLM endpoint; not included in `extras`) |
| `integration` | Integration tests (not included in `extras`) |
| `context-select`, `tool-verify` | Benchmark experiments under `experiments/` |

The `system-tests` feature is intended for manual testing against a live backend. It is not enabled by default or by `extras` because it requires a running LLM endpoint.

Avoid `--all-features`: it additionally compiles the test-only features above and is not covered by CI (which builds and tests with `--features extras`).

## Environment Variables

All environment variables override their corresponding config file values.

| Variable | Description | Example |
|----------|-------------|---------|
| `SELFWARE_API_KEY` | API authentication key | `sk-...` |
| `SELFWARE_ENDPOINT` | API endpoint URL | `http://localhost:8000/v1` |
| `SELFWARE_MODEL` | Model identifier | `Qwen/Qwen3-Coder-Next-FP8` |
| `SELFWARE_MAX_TOKENS` | Max response tokens | `65536` |
| `SELFWARE_TEMPERATURE` | Sampling temperature | `1.0` |
| `SELFWARE_TIMEOUT` | Step timeout in seconds | `300` |
| `SELFWARE_THEME` | UI color theme | `amber` |
| `SELFWARE_MODE` | Execution mode | `normal`, `auto-edit`, `yolo`, `daemon` |
| `SELFWARE_LOG_LEVEL` | Log level (fallback for `RUST_LOG`) | `info`, `debug`, `trace` |
| `SELFWARE_CONFIG` | Config file path override | `/path/to/config.toml` |
| `SELFWARE_STRICT_PERMISSIONS` | Enforce strict file permissions | `1` |
| `SELFWARE_ASCII` | ASCII-only output (no emoji) | `1` |
| `NO_COLOR` | Disable colored output (standard) | `1` |

## Complete Example Config

```toml
# ~/.config/selfware/config.toml

endpoint = "http://192.168.1.170:8000/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"
max_tokens = 65536
temperature = 1.0

[agent]
max_iterations = 100
step_timeout_secs = 300
token_budget = 500000
streaming = true
native_function_calling = false

[safety]
allowed_paths = ["./**"]
denied_paths = ["**/.env", "**/.ssh/**"]
protected_branches = ["main"]
require_confirmation = ["git_push", "file_delete"]

[[safety.permissions]]
tool_pattern = "cargo_*"
reason = "Always allow cargo commands"

[ui]
theme = "amber"
animations = true
compact_mode = false

[retry]
max_retries = 5
base_delay_ms = 1000

[yolo]
enabled = false
allow_destructive_shell = false

[continuous_work]
enabled = true
checkpoint_interval_tools = 10

[[hooks]]
event = "PostToolUse"
match_tools = ["file_write", "file_edit"]
command = "cargo fmt -- {path}"
timeout_secs = 30

[[hooks]]
event = "Stop"
command = "cargo test --quiet 2>&1 | tail -5"
timeout_secs = 120

[qa]
profile = "standard"

[[mcp.servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "ghp_..." }
```
