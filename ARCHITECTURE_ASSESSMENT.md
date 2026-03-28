# Selfware Architecture Assessment

## Executive Summary

This document provides a comprehensive architectural analysis of the Selfware codebase, focusing on the `src/agent/`, `src/api/`, and `src/config/` modules. The assessment evaluates coupling, cohesion, module boundaries, and identifies areas for improvement.

**Assessment Date**: 2026-03-27  
**Scope**: Core agent execution, API integration, and configuration management

---

## 1. System Overview

### 1.1 Project Context
Selfware is a local-first, agentic coding harness for LLMs featuring:
- 70+ tools for file operations, browser automation, vision, and system control
- Multi-agent swarm capabilities
- Self-improvement and evolution engine
- Context compression and knowledge graph management

### 1.2 Core Architecture Patterns
- **Modular Monolith**: Single binary with clear module boundaries
- **Dependency Injection**: Config-driven initialization
- **Event-Driven Agent Loop**: State machine with cognitive phases
- **Resilience Patterns**: Circuit breakers, retry logic, self-healing

---

## 2. Module Analysis

### 2.1 `src/agent/` Module

#### 2.1.1 Structure
```
src/agent/
├── mod.rs              # Core Agent struct, 3000+ lines
├── execution.rs        # Main execution loop, 103K lines
├── tool_dispatch.rs    # Tool invocation, 92K lines
├── tool_execution.rs   # Tool parsing & execution
├── tool_collect.rs     # Tool call extraction
├── verification.rs     # Verification gate logic
├── recovery.rs         # Loop detection & recovery
├── message_handling.rs # Message building & context
├── context_files.rs    # Context file management
├── context_map.rs      # Context compression
├── session_log.rs      # Session tracking
├── interactive.rs      # Interactive mode
└── types.rs            # Agent types (re-exported)
```

#### 2.1.2 Key Components

**Agent Struct** (`mod.rs`)
- **Responsibilities**: Orchestrates execution, manages state, coordinates submodules
- **Dependencies**: 25+ cross-module imports
- **Coupling Level**: HIGH - tightly integrated with all subsystems

**Execution Loop** (`execution.rs`)
- **Responsibilities**: Main agent loop, step execution, state transitions
- **Key Features**:
  - ESC listener for stdin interruption
  - No-action prompt handling
  - Checkpoint integration
  - Token budget management

**Tool Dispatch** (`tool_dispatch.rs`)
- **Responsibilities**: Tool invocation, confirmation handling, result processing
- **Constants**:
  - `TOOL_CONFIRM_ARGS_PREVIEW_CHARS: 240`
  - `TOOL_CONFIRM_FAILURE_HINT_CHARS: 400`
- **Integrations**: `cognitive::self_improvement`, `checkpoint::ToolCallLog`, `api::types::Message`

**Verification Gate** (`verification.rs`)
- **Responsibilities**: Pre-completion verification checks
- **Features**:
  - Project-type-aware verification (Rust vs non-Rust)
  - Visual assertion integration
  - Loop detection

#### 2.1.3 Coupling Analysis

**High Coupling Areas**:
1. `Agent` struct depends on:
   - `api::ApiClient`, `api::types::Message`
   - `checkpoint::CheckpointManager`
   - `cognitive::CognitiveState`, `cognitive::self_improvement`
   - `config::Config`
   - `hooks::HookRegistry`
   - `memory::AgentMemory`
   - `safety::SafetyChecker`
   - `session::ChatStore`, `session::EditHistory`
   - `tools::ToolRegistry`
   - `verification::VerificationGate`

2. **Circular Dependency Risk**: 
   - `agent/mod.rs` → `api/client.rs` → `api/types.rs` → `agent/types.rs` (re-export)
   - This is managed via careful re-exporting but creates tight coupling

**Module Dependencies**:
```
agent/execution.rs
  ├── agent/mod.rs (self)
  ├── api/types::Message
  ├── checkpoint::ToolCallLog
  ├── cognitive::CyclePhase
  └── errors::AgentError

agent/tool_dispatch.rs
  ├── api/types::Message
  ├── checkpoint::ToolCallLog
  ├── cognitive::self_improvement::Outcome
  ├── errors::AgentError
  └── hooks::HookContext

agent/verification.rs
  ├── checkpoint::VisualAssertion
  └── cognitive::CyclePhase
```

#### 2.1.4 Cohesion Assessment

**Strengths**:
- Clear separation of concerns (dispatch vs execution vs verification)
- Specialized modules for specific responsibilities
- Good use of submodules (`context::`, `loop_control::`, `planning::`, `tui_events::`)

**Weaknesses**:
- `mod.rs` is a "god module" with 3000+ lines
- Some cross-cutting concerns (error handling, logging) are duplicated
- Type re-exports create implicit coupling

---

### 2.2 `src/api/` Module

#### 2.2.1 Structure
```
src/api/
├── mod.rs          # Module exports, client traits
├── client.rs       # HTTP client implementation
├── types.rs        # OpenAI-compatible types
├── streaming.rs    # SSE parsing, chunk accumulation
└── tests.rs        # Unit tests
```

#### 2.2.2 Key Components

**ApiClient** (`client.rs`)
- **Responsibilities**: HTTP client for OpenAI-compatible APIs
- **Features**:
  - Retry logic with jitter
  - Circuit breaker integration (`supervision::circuit_breaker`)
  - Token estimation (`tokens::estimate_messages_tokens`)
  - Streaming support
  - Thinking mode handling

**Type System** (`types.rs`)
- **Responsibilities**: OpenAI API type definitions
- **Key Types**: `Message`, `ToolCall`, `ChatResponse`, `Choice`, `Usage`
- **Special Features**: Custom deserializer for `MessageContent` null handling

**Streaming** (`streaming.rs`)
- **Responsibilities**: SSE parsing, tool call accumulation
- **Features**:
  - Concurrent task limiting (semaphore)
  - Tool call state management
  - Error handling with `ApiError`

#### 2.2.3 Coupling Analysis

**Dependencies**:
```
api/client.rs
  ├── errors::ApiError
  ├── supervision::circuit_breaker
  ├── tokens::estimate_*
  ├── api/streaming::StreamingResponse
  └── api/types::*

api/streaming.rs
  ├── errors::ApiError
  └── api/types::*
```

**Coupling Level**: MODERATE
- Clean separation from `agent/` via trait boundaries
- Depends on `errors/` and `supervision/` but these are well-defined
- Token estimation creates dependency on `tokens/` module

#### 2.2.4 Cohesion Assessment

**Strengths**:
- Single responsibility for each submodule
- Clear trait boundaries (`LlmClient`)
- Good error handling with typed errors

**Weaknesses**:
- `types.rs` grows with each new API feature
- Streaming logic could be more modular

---

### 2.3 `src/config/` Module

#### 2.3.1 Structure
```
src/config/
├── mod.rs              # Main config, re-exports
├── loader.rs           # TOML loading, normalization
├── validation.rs       # Configuration validation
├── agent.rs            # Agent behavior config
├── api_key.rs          # API key management
├── auto_config.rs      # Auto-configuration detection
├── model.rs            # Model profiles, RedactedString
├── resources.rs        # Resource limits
├── safety.rs           # Safety guardrails
├── types.rs            # Common types, defaults
└── tests.rs            # Unit tests
```

#### 2.3.2 Key Components

**AgentConfig** (`agent.rs`)
- **Responsibilities**: Agent behavior settings
- **Key Fields**:
  - `max_iterations: 100`
  - `step_timeout_secs: 300`
  - `token_budget: 0` (0 = derive from `max_tokens`)
  - `token_safety_margin: 8192` (8K)
  - `native_function_calling: false`
  - `streaming: true`
  - `min_completion_steps: 3`
  - `require_verification_before_completion: true`
  - `require_visual_verification: false`
  - Context ratios: `content: 0.75`, `compression: 0.20`, `thinking: 0.05`
  - `compression_detail: "signatures"`

**Config** (`mod.rs`, `loader.rs`)
- **Responsibilities**: Top-level configuration, validation, loading
- **Features**:
  - TOML parsing
  - API key resolution (env, keyring, file)
  - Auto-detection of backend type
  - Validation with detailed error messages

#### 2.3.3 Coupling Analysis

**Dependencies**:
```
config/agent.rs
  └── config/types::default_*

config/loader.rs
  ├── api_key::*
  ├── model::*
  ├── types::ExecutionMode
  └── Config (self)

config/validation.rs
  ├── api_key::is_local_endpoint
  └── Config (self)
```

**Coupling Level**: LOW
- Self-contained module
- Minimal external dependencies
- Clean separation via `Config` struct

#### 2.3.4 Cohesion Assessment

**Strengths**:
- Excellent separation of concerns
- Each submodule has clear responsibility
- Good use of default functions for testability
- `RedactedString` prevents accidental secret logging

**Weaknesses**:
- None significant

---

## 3. Cross-Module Dependencies

### 3.1 Dependency Graph

```
┌─────────────┐
│   config/   │
│   (LOW)     │
└──────┬──────┘
       │ (used by)
       ▼
┌─────────────┐
│   agent/    │
│  (HIGH)     │
└──────┬──────┘
       │ (uses)
       ▼
┌─────────────┐
│    api/     │
│  (MODERATE) │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│  errors/    │
│  supervision│
└─────────────┘
```

### 3.2 Critical Dependency Paths

1. **Execution Path**:
   ```
   config::Config
     → agent::Agent
       → api::ApiClient
         → supervision::circuit_breaker
   ```

2. **Tool Execution Path**:
   ```
   agent::tool_dispatch
     → agent::tool_execution
       → tools::* (external module)
   ```

3. **Verification Path**:
   ```
   agent::verification
     → checkpoint::VisualAssertion
     → cognitive::CyclePhase
   ```

### 3.3 Coupling Hotspots

**High-Risk Areas**:
1. **`agent/mod.rs`**: 25+ cross-module imports, potential god object
2. **Type Re-exports**: `agent::types` re-exports from `tool_collect`, `recovery`
3. **Error Types**: `errors::AgentError` used everywhere, potential monolith

**Mitigation Strategies**:
- Consider splitting `agent/mod.rs` into smaller orchestrator structs
- Use traits to reduce direct dependencies
- Consider error type specialization per module

---

## 4. Architectural Patterns

### 4.1 Strengths

1. **Clear Module Boundaries**: Each module has a well-defined purpose
2. **Configuration-Driven**: `Config` struct provides centralized control
3. **Resilience Patterns**: Circuit breakers, retries, self-healing
4. **Type Safety**: Strong typing with custom types (`RedactedString`)
5. **Extensibility**: Hook system, tool registry, plugin architecture

### 4.2 Weaknesses

1. **Tight Coupling in Agent**: `Agent` struct knows too much
2. **Large Files**: `execution.rs` (103K), `tool_dispatch.rs` (92K)
3. **Implicit Dependencies**: Type re-exports create hidden coupling
4. **Error Monolith**: Single error type for all concerns
5. **Test Complexity**: High coupling makes unit testing difficult

### 4.3 Code Quality Metrics

| Module | Lines | Complexity | Test Coverage |
|--------|-------|------------|---------------|
| `agent/` | ~300K | HIGH | Moderate |
| `api/` | ~20K | MODERATE | Good |
| `config/` | ~15K | LOW | Good |

---

## 5. Recommendations

### 5.1 Immediate Actions (High Priority)

1. **Split `agent/mod.rs`**:
   - Extract `ExecutionEngine` for loop logic
   - Extract `ToolCoordinator` for tool management
   - Extract `StateManager` for state transitions

2. **Reduce Type Re-exports**:
   - Define clear ownership of types
   - Use explicit imports instead of re-exports

3. **Error Type Specialization**:
   - `AgentError`, `ApiError`, `ConfigError`, `ToolError`
   - Use `thiserror` for better error handling

### 5.2 Medium-Term Improvements

1. **Introduce Facade Pattern**:
   - Create high-level interfaces for complex subsystems
   - Reduce direct dependencies in `Agent`

2. **Dependency Injection**:
   - Use trait objects for testability
   - Enable mocking in unit tests

3. **Module Boundaries**:
   - Define clear public APIs for each module
   - Use `#[doc(hidden)]` for internal types

### 5.3 Long-Term Vision

1. **Plugin Architecture**:
   - Allow external tool providers
   - Support custom execution strategies

2. **Microservices Readiness**:
   - Design for potential service separation
   - Define clear IPC boundaries

3. **Observability**:
   - Structured logging with correlation IDs
   - Distributed tracing for agent steps

---

## 6. Module Health Summary

### 6.1 `src/agent/` - **NEEDS REFACTORING**
- **Coupling**: HIGH ⚠️
- **Cohesion**: MODERATE
- **Maintainability**: MODERATE
- **Action Required**: Split large modules, reduce dependencies

### 6.2 `src/api/` - **HEALTHY**
- **Coupling**: MODERATE ✓
- **Cohesion**: HIGH ✓
- **Maintainability**: GOOD ✓
- **Action Required**: Monitor for type bloat

### 6.3 `src/config/` - **EXCELLENT**
- **Coupling**: LOW ✓
- **Cohesion**: HIGH ✓
- **Maintainability**: EXCELLENT ✓
- **Action Required**: None

---

## 7. Next Steps

1. **Create Refactoring Plan**:
   - Prioritize `agent/mod.rs` split
   - Define new module boundaries
   - Create migration path

2. **Add Architecture Tests**:
   - Dependency graph validation
   - Module boundary enforcement
   - Circular dependency detection

3. **Documentation**:
   - Update module-level docs
   - Create architecture decision records (ADRs)
   - Document cross-module contracts

---

## Appendix A: Key File Locations

| Component | File | Lines |
|-----------|------|-------|
| Agent Core | `src/agent/mod.rs` | 3000+ |
| Execution | `src/agent/execution.rs` | 103K |
| Tool Dispatch | `src/agent/tool_dispatch.rs` | 92K |
| API Client | `src/api/client.rs` | ~2K |
| Config | `src/config/mod.rs` | ~150 |
| Agent Config | `src/config/agent.rs` | 112 |

## Appendix B: Glossary

- **Coupling**: Degree of interdependence between modules
- **Cohesion**: How closely related the responsibilities of a module are
- **God Object**: An object that knows too much or does too much
- **Facade Pattern**: Provides simplified interface to complex subsystem
- **Dependency Injection**: Passing dependencies rather than creating them

---

*Assessment completed by Selfware architecture analysis tool*  
*Last updated: 2026-03-27*