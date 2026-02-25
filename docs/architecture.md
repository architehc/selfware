# Selfware Architecture

This document maps the main runtime flow and module boundaries for contributors.

## High-Level Module Map

```text
┌───────────────────────────────────────────────────────────────────────┐
│                               CLI / UI                               │
│  src/main.rs, src/input, src/ui, src/orchestration                  │
└───────────────────────────────┬───────────────────────────────────────┘
                                │
                                ▼
┌───────────────────────────────────────────────────────────────────────┐
│                              Agent Core                              │
│  src/agent (loop control, planning, context compression, execution)  │
└───────────────┬───────────────────────────────┬───────────────────────┘
                │                               │
                ▼                               ▼
┌──────────────────────────────┐      ┌────────────────────────────────┐
│         Tool System          │      │            API Layer           │
│  src/tools + ToolRegistry    │      │  src/api (chat + streaming)    │
└───────────────┬──────────────┘      └────────────────────────────────┘
                │
                ▼
┌───────────────────────────────────────────────────────────────────────┐
│                      Safety + Verification Guardrails                 │
│  src/safety (checker/path validator/scanner)                          │
│  src/testing/verification.rs (cargo/test/lint verification gate)      │
└───────────────────────────────────────────────────────────────────────┘
```

## PDVR Cycle (Plan, Do, Verify, Reflect)

```text
User Task
  │
  ▼
Plan
  - Build prompt + contextual state
  - Decide next actions / tools
  │
  ▼
Do
  - Parse tool calls (native FC or parser fallback)
  - Safety-check each call
  - Execute tool calls
  │
  ▼
Verify
  - Run verification gate for code-changing actions
  - Record pass/fail signals
  │
  ▼
Reflect
  - Update episodic/working memory
  - Decide whether to continue or finalize
```

## Tool Registration Flow

```text
ToolRegistry::new()
  ├─ registers built-in tool implementations
  ├─ exposes schemas/descriptions to model prompts
  └─ resolves name -> tool instance at execution time
```

Execution path:

1. Model emits tool call (native function call or parsed format).
2. `SafetyChecker` validates call arguments and shell/path risk.
3. `ToolRegistry` resolves the tool by name.
4. Tool executes and result is fed back to the model as tool result context.

## Safety Validation Pipeline

```text
Incoming ToolCall
  │
  ├─ file_* tools  ─► PathValidator
  │                  - path traversal checks
  │                  - denied/allowed glob checks
  │                  - symlink chain safety
  │
  ├─ shell_exec    ─► command pattern scanner
  │                  - destructive command regexes
  │                  - chain/shell-obfuscation detection
  │
  └─ git_push      ─► force-push guard
```

## Self-Healing Recovery (feature: `resilience`)

```text
Error occurs in agent loop
  │
  ├─ ErrorClass::classify()  ─► Network | Timeout | RateLimit | ParseError | ...
  │
  ├─ SelfHealingEngine::handle_error()
  │    ├─ Select strategy (learned or class default)
  │    ├─ Execute via RecoveryExecutor
  │    │    ├─ Retry with exponential backoff (base * 2^attempt ±25%, cap 30s)
  │    │    ├─ RestoreCheckpoint (revert to last known-good state)
  │    │    ├─ ClearCache / ResetState
  │    │    └─ Custom actions (compress_context, reduce_tool_set, switch_parsing_mode)
  │    └─ Escalate if primary strategy fails
  │
  └─ Agent loop resumes or aborts (bounded by max_recovery_attempts)
```

## Key Contributor Notes

- `src/main.rs` should stay a thin CLI entrypoint that delegates to library modules.
- `src/agent/mod.rs` is the largest critical path file; refactors should preserve loop behavior and safety semantics.
- Prefer adding behavior through focused submodules (`agent/*`, `safety/*`, `self_healing/*`) rather than growing central files.
- Feature-gated modules (`self_healing`, `tui`, `tokens`) must be guarded with `#[cfg(feature = "...")]`.

## Design Rationale & FAQ

### Why PDVR instead of ReAct?
The Plan-Do-Verify-Reflect cycle is designed for high-stakes autonomous coding. Unlike ReAct (Reason+Act), which is a "tight" loop, PDVR adds an explicit **Verify** step that allows the system to catch its own mistakes (e.g., failed compilations) before proceeding, and a **Reflect** step to update its long-term mental model.

### Dual Parsing: XML and Native Function Calling
Selfware supports both native function calling (OpenAI/Qwen style) and XML-based tool tags. This ensures compatibility across a wide range of backends. The `tool_parser` acts as a unified translation layer, presenting a consistent interface to the `Agent` regardless of how the model emitted the action.

### Feature-Flag Decomposition
The system is highly modular to support various deployment targets:
- `tui`: Desktop/CLI interactive use.
- `resilience`: Long-running server-side "daemon" use where self-healing is critical.
- `tokens`: Advanced token tracking for cost-sensitive environments.

### Checkpoint Format
Checkpoints use a JSON-based format that captures the full mental state, including episodic memory. This allows a task started on one machine to be resumed on another with its "lessons learned" intact. Atomic writes (write-then-rename) are enforced to prevent state corruption during crashes.
