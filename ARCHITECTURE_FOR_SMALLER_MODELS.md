# Selfware Architecture Guide for Qwen3.5-27B

> This document is a compressed, precise reference for a 27B parameter model to understand
> and modify the selfware codebase. 430K lines of Rust across 301 files.

## Critical Path: How a Task Executes

```
main.rs → cli::run() → Agent::new(config) → agent.run_task(task)
  → run_execution_loop():
    Planning: plan() → chat_streaming() → parse_tool_calls()
    Executing: execute_step_internal() → collect_tool_calls() → execute_tool_batch()
      → execute_single_tool_in_batch() or execute_parallel_tools()
        → tool.execute(args) → push_tool_result_message()
    ErrorRecovery: inject error context → retry
    Completed: final_answer() → checkpoint
```

## Module Map (by importance for modifications)

### TIER 1: Core Agent Loop (modify these most often)
| File | Lines | Purpose |
|------|-------|---------|
| `src/agent/mod.rs` | 965 | Agent struct (50 fields), initialization, confirmation policy |
| `src/agent/task_runner.rs` | 755 | `run_task()`, `run_execution_loop()` state machine |
| `src/agent/execution.rs` | 800 | `execute_step_internal()` — the inner loop body |
| `src/agent/tool_dispatch.rs` | 1220 | `execute_tool_batch()` — parallel/sequential tool execution |
| `src/agent/recovery.rs` | 598 | No-action detection, smart fallbacks, repetition detection |
| `src/agent/streaming.rs` | 519 | SSE streaming with tag suppression |
| `src/agent/loop_control.rs` | 365 | State machine: Planning→Executing→ErrorRecovery→Completed/Failed |

### TIER 2: Tool System
| File | Lines | Purpose |
|------|-------|---------|
| `src/tools/mod.rs` | 320 | `Tool` trait (Send+Sync), `ToolRegistry` |
| `src/tools/file.rs` | 800 | file_read, file_write, file_edit, file_delete, directory_tree |
| `src/tools/shell.rs` | 400 | shell_exec with safety validation |
| `src/tools/search.rs` | 600 | grep_search, glob_find, symbol_search |
| `src/tools/cargo.rs` | 500 | cargo_test, cargo_check, cargo_clippy, cargo_fmt |
| `src/tools/git.rs` | 500 | git_status, git_diff, git_commit, git_push, git_checkpoint |
| `src/tool_parser.rs` | 700 | Multi-format parser: XML, JSON, Qwen3, function calling |

### TIER 3: Context & Memory
| File | Lines | Purpose |
|------|-------|---------|
| `src/agent/context.rs` | 529 | ContextCompressor — token budget management |
| `src/agent/context_map.rs` | 1200 | 3-level context: L1(tree) → L2(skeleton) → L3(full) |
| `src/agent/context_management.rs` | 500 | trim_message_history, bulk_read, structured_summary |
| `src/memory.rs` | 729 | AgentMemory with token-aware eviction |
| `src/concurrency.rs` | 253 | ConcurrencyGovernor: 4 streams, 16 tools, 24 global |

### TIER 4: Configuration
| File | Lines | Purpose |
|------|-------|---------|
| `src/config/mod.rs` | 2000 | Config, AgentConfig, SafetyConfig, ModelProfile |
| `src/cli.rs` | 600 | clap CLI args, command dispatch |
| `src/api/mod.rs` | 1500 | ApiClient with streaming, retry, circuit breaker |
| `src/api/types.rs` | 800 | OpenAI-compatible Message, ToolCall, ChatResponse |

### TIER 5: Safety
| File | Lines | Purpose |
|------|-------|---------|
| `src/safety/checker.rs` | 1200 | Primary safety gate — validates every tool call |
| `src/safety/path_validator.rs` | 600 | Path traversal, symlink, denied/allowed list |
| `src/safety/redact.rs` | 300 | Secret redaction (17 patterns) |

## Key Patterns a Smaller Model Must Know

### 1. Tool Calling (Two Modes)
```rust
// Mode 1: XML-based (default, works with all backends)
// Tools embedded in system prompt, parsed from content
<tool>
<name>file_read</name>
<arguments>{"path": "src/main.rs"}</arguments>
</tool>

// Mode 2: Native function calling (requires backend support)
// Tools passed via API, tool_calls returned in response
config.agent.native_function_calling = true;
```

### 2. State Machine Transitions
```
Planning → Executing (valid)
Planning → ErrorRecovery (valid)
Executing → Executing (valid, increments step)
Executing → ErrorRecovery (valid)
Executing → Completed (valid)
ErrorRecovery → Executing (valid)
Planning → Completed (INVALID)
```

### 3. No-Action Loop Prevention
When the model describes intent without calling tools:
1. First 2 times: inject text correction ("call a tool now")
2. After 2: force smart fallback (auto-execute directory_tree or file_read)
3. After 5 consecutive: abort task with error
4. After 50 lifetime: hard abort

### 4. Parallel Tool Execution
Read-only tools run concurrently via `FuturesUnordered`:
- `PARALLEL_SAFE_TOOLS`: file_read, directory_tree, glob_find, grep_search, symbol_search, git_status, git_diff, git_log, lsp_*
- Write tools always sequential: file_write, file_edit, shell_exec, git_commit
- Path conflicts within parallel batch → moved to sequential

### 5. Context Budget Split
```
token_budget (default 65536):
  75% → content (files, conversation, tool results)
  20% → compression headroom
   5% → thinking/reasoning
```

### 6. Completion Gate
Task completion rejected unless:
- `min_completion_steps >= 3` steps executed
- At least one successful cargo_check/test/clippy (for Rust projects)
- Response doesn't contain confused meta-reasoning

## Dead Code to Remove (~6,773 lines)

| File | Issue | Lines |
|------|-------|-------|
| `src/agent/tool_execution.rs` | Orphan file, not declared in mod.rs | 277 |
| `src/orchestration/parallel.rs` | Module-level `allow(dead_code)`, never imported | 3,152 |
| `src/session/local_first.rs` | ~60% dead (OfflineManager, SyncManager, EdgeTask) | ~1,600 |
| `src/orchestration/integrate_cache.rs` | Doc comments only, zero executable code | 64 |
| `src/cognitive/learning.rs` | Entire module unused (CodeExplainer, Quiz) | 1,677 |
| `src/templates.rs` WORKFLOW_ORCHESTRATOR | Unused constant | 3 |

## Known Duplications

1. **Two Episode types**: `cognitive/episodic.rs::Episode` vs `cognitive/memory_hierarchy.rs::Episode` (different fields)
2. **Two Lesson types**: `cognitive/state.rs::Lesson` vs `cognitive/learning.rs::Lesson` (different purpose)
3. **Two ResourceUsage types**: `resource/mod.rs::ResourceUsage` vs `resource/quotas.rs::ResourceUsage`
4. **AutonomyLevel/RiskLevel**: defined independently in both `safety/sandbox.rs` and `safety/autonomy.rs`
5. **Tool dispatch duplication**: `tool_dispatch.rs` and `tool_execution.rs` have overlapping methods

## Feature Flags (Cargo.toml)

| Feature | Default | Gates |
|---------|---------|-------|
| tui | Yes | Ratatui dashboard, animations, demos |
| workflows | Yes | YAML workflows, parallel executor |
| resilience | Yes | Self-healing, graceful degradation |
| execution-modes | Yes | Dry-run, confirm, yolo modes |
| self-improvement | Yes | RSI loop, metrics, self-edit |
| hot-reload | **No** | Dynamic .so plugin loading |
| vlm-bench | **No** | Vision model benchmark suite |

## Key Dependencies

- **LLM**: reqwest → OpenAI-compatible `/v1/chat/completions` (streaming SSE)
- **Terminal**: reedline (input), crossterm (raw mode), colored (ANSI), ratatui (TUI)
- **Git**: git2 (libgit2 bindings) + git CLI
- **Token counting**: tokenizers (HuggingFace Qwen), tiktoken-rs fallback
- **Crypto**: aes-gcm, pbkdf2, sha2, hmac (checkpoint integrity, session encryption)
- **System**: sysinfo (RAM/disk), nvml-wrapper (GPU), xcap (screen capture)

## Config Loading Order
1. TOML file: explicit `--config` → `SELFWARE_CONFIG` env → `selfware.toml` in CWD → `~/.config/selfware/config.toml`
2. Env var overrides: `SELFWARE_ENDPOINT`, `SELFWARE_MODEL`, `SELFWARE_API_KEY`, `SELFWARE_MAX_TOKENS`
3. CLI flags: `--mode`, `--compact`, `--verbose`, `--plan`
4. API key: `SELFWARE_API_KEY` env → OS keyring → TOML file

## Prompt Template for Modifications

When asking a 27B model to modify selfware, use this structure:

```
You are modifying the selfware codebase (a Rust AI agent framework).

TARGET FILE: src/agent/execution.rs
TASK: [description]

CONTEXT:
- The Agent struct is in src/agent/mod.rs with ~50 fields
- Tool calls are parsed by src/tool_parser.rs (XML/JSON/Qwen3 formats)
- The execution loop is in src/agent/task_runner.rs::run_execution_loop()
- Tools implement the Tool trait (Send+Sync) from src/tools/mod.rs
- Safety checks happen in src/safety/checker.rs before every tool call

CONSTRAINTS:
- Do not modify src/safety/ or src/evolution/ (protected paths)
- All tool implementations must be async and implement Tool: Send + Sync
- Use cli_println! macro instead of println! (respects TUI mode)
- Run cargo check after every edit
```
