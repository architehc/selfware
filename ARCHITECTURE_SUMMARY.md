# Selfware Architecture Summary

## Project Overview

Selfware is an **agentic coding harness** for local LLMs with the following key characteristics:

- **~197k lines** of Rust code across 199 source files
- **54 built-in tools** for file, git, cargo, search, shell operations
- **Multi-agent swarm** supporting up to 16 concurrent agents
- **TUI dashboard** built with ratatui
- **Evolution engine** for recursive self-improvement
- **Comprehensive safety** with multi-layer validation

---

## Module Architecture

```
src/
├── agent/          # Core agent loop (PDVR cycle), checkpointing, execution
│   ├── loop_control.rs      # State machine, iteration tracking
│   ├── execution.rs         # Tool execution, API interaction
│   ├── task_runner.rs       # Main task loops, swarm orchestration
│   ├── checkpointing.rs     # Persistence, resume, delta compression
│   ├── context.rs           # Context compression with LLM summarization
│   └── streaming.rs         # Real-time response handling
│
├── tools/          # 54 tool implementations
│   ├── mod.rs               # Tool trait, registry, pagination
│   ├── file.rs              # File read/write/edit/delete with atomic operations
│   ├── git.rs               # Git operations with safety checks
│   ├── search.rs            # Grep, glob, symbol search with regex cache
│   ├── shell.rs             # Shell execution with dangerous pattern blocking
│   ├── fim.rs               # Fill-in-the-Middle AI code editing
│   └── knowledge.rs         # Knowledge base operations
│
├── safety/         # Multi-layer security framework
│   ├── checker.rs           # Central safety coordinator
│   ├── path_validator.rs    # TOCTOU-protected path validation
│   ├── scanner.rs           # Secret and vulnerability detection
│   ├── sandbox.rs           # Docker-based isolation
│   ├── yolo.rs              # YOLO mode with audit logging
│   └── threat_modeling.rs   # STRIDE-based threat analysis
│
├── cognitive/      # Cognitive architecture
│   ├── memory_hierarchy.rs  # 1M token 3-layer memory (working/episodic/semantic)
│   ├── rag.rs               # Retrieval-Augmented Generation
│   ├── token_budget.rs      # Dynamic token allocation
│   ├── self_improvement.rs  # Prompt optimization, tool learning
│   ├── rsi_orchestrator.rs  # Recursive Self-Improvement (stubbed)
│   └── knowledge_graph.rs   # Codebase relationship tracking
│
├── orchestration/  # Multi-agent coordination
│   ├── swarm.rs             # Agent swarm with consensus voting
│   ├── multiagent.rs        # Multi-agent chat interface
│   ├── parallel.rs          # Dependency-aware parallel execution
│   ├── planning.rs          # Hierarchical goal decomposition (PDVR)
│   ├── workflow_dsl/        # Custom workflow language
│   │   ├── lexer.rs
│   │   ├── parser.rs
│   │   ├── ast.rs
│   │   └── runtime.rs
│   └── workflows.rs         # YAML workflow engine
│
├── ui/             # User interface
│   ├── style.rs             # Terminal styling, themes
│   ├── theme.rs             # 10 built-in color themes
│   ├── animations.rs        # Animation framework (1,503 lines)
│   ├── garden.rs            # Code health visualization
│   └── tui/                 # Ratatui dashboard
│       ├── app.rs           # Main TUI application
│       ├── dashboard_widgets.rs
│       └── garden_view.rs
│
├── evolution/      # Self-improvement engine (feature-gated)
│   ├── daemon.rs            # Main evolution loop
│   ├── fitness.rs           # SAB-based fitness function
│   ├── sandbox.rs           # Isolated evaluation
│   ├── tournament.rs        # Parallel hypothesis evaluation
│   └── telemetry.rs         # Performance profiling
│
├── api/            # LLM API client
│   ├── mod.rs               # Async client with streaming
│   └── types.rs             # Request/response types
│
├── config/         # Configuration management
│   ├── mod.rs               # Main config (1,000+ lines)
│   ├── typed.rs             # Schema-based config (UNUSED - 1,168 lines)
│   └── resources.rs         # Resource limits
│
├── session/        # Persistence
│   ├── checkpoint.rs        # Task checkpointing
│   ├── chat_store.rs        # Conversation storage
│   └── local_first.rs       # Local-first sync (2,527 lines)
│
└── [other modules]
    ├── analysis/            # BM25 search, vector store, code graph
    ├── observability/       # Telemetry, metrics, carbon tracking
    ├── devops/              # Container support, process management
    ├── supervision/         # Circuit breaker, health checks
    └── testing/             # Verification, contract testing
```

---

## Key Design Patterns

### 1. PDVR Cognitive Cycle
```
    ╭─────────╮         ╭─────────╮
    │  PLAN   │────────▶│   DO    │
    ╰─────────╯         ╰─────────╯
         ▲                    │
         │                    ▼
    ╭─────────╮         ╭─────────╮
    │ REFLECT │◀────────│ VERIFY  │
    ╰─────────╯         ╰─────────╯
```

### 2. Safety Layers
```
Request → Path Guardian → Command Sentinel → Protected Groves → Execute
```

### 3. Evolution Flow
```
Generate Hypotheses → Safety Filter → Sandbox Eval → Select Winner → Apply
```

### 4. Memory Hierarchy (1M tokens)
```
┌─────────────────────────────────────────────────────────────┐
│                     SEMANTIC MEMORY                         │
│                    (~700K tokens)                           │
│              Codebase and long-term knowledge               │
├─────────────────────────────────────────────────────────────┤
│                     EPISODIC MEMORY                         │
│                    (~200K tokens)                           │
│         Recent experiences with tiered importance           │
├─────────────────────────────────────────────────────────────┤
│                     WORKING MEMORY                          │
│                    (~100K tokens)                           │
│            Immediate conversation context                   │
└─────────────────────────────────────────────────────────────┘
```

---

## Feature Flags

| Feature | Description | Status |
|---------|-------------|--------|
| `tui` | TUI dashboard with animations | Working |
| `workflows` | Workflow automation | Working |
| `resilience` | Self-healing and recovery | Working |
| `execution-modes` | Dry-run, confirm, yolo modes | Working |
| `cache` | Response caching | Working |
| `self-improvement` | Evolution engine | **Stubbed** |
| `vlm-bench` | Visual benchmark suite | Feature-gated |
| `hot-reload` | Dynamic library reload | Security-sensitive |

---

## Test Infrastructure

```
tests/
├── unit/              # 11 modules, ~3,500 lines
├── integration/       # 12 modules, ~4,600 lines
├── e2e-projects/      # Test fixtures (minimal)
├── prop_*.rs          # Property-based tests (proptest)
└── [other tests]

system_tests/
├── projecte2e/        # SAB benchmark (20 scenarios)
└── long_running/      # 4-8 hour stress tests
```

**Test Metrics:**
- ~6,400 unit tests
- ~82% line coverage
- Property-based tests for parser and safety
- SAB benchmark with 12 scenarios

---

## Critical Issues Summary

| Category | Count | Files |
|----------|-------|-------|
| Blocking I/O in async | 2 | `agent/execution.rs`, `agent/checkpointing.rs` |
| Security bypass | 1 | `tools/file.rs` |
| Prompt injection | 1 | `tools/fim.rs` |
| Stubbed functionality | 2 | `cognitive/rsi_orchestrator.rs`, `config/typed.rs` |
| Race conditions | 2 | `safety/path_validator.rs` |
| Missing validation | 5+ | `config/mod.rs` |

---

## Performance Characteristics

| Aspect | Value | Notes |
|--------|-------|-------|
| Compile time | High | ~197k lines, consider workspace split |
| Memory usage | Moderate | Token cache, vector store |
| Async efficiency | Good | Tokio-based, some blocking I/O issues |
| TUI framerate | 30 FPS | Throttled to prevent CPU waste |
| Regex cache | 64 entries | Bounded to prevent memory growth |

---

## Dependencies (Key)

| Category | Crates |
|----------|--------|
| Async runtime | tokio |
| TUI | ratatui, crossterm |
| HTTP client | reqwest |
| Serialization | serde, toml, serde_json |
| Git | git2 |
| Vector search | hnsw_rs |
| CLI | clap |
| Tracing | tracing, tracing-subscriber |

---

## Safety Invariants

1. **Evolution engine cannot modify:**
   - Its own fitness function
   - The SAB benchmark suite
   - The safety module itself

2. **Path validation:**
   - O_NOFOLLOW atomic open
   - Symlink chain limits (max 40)
   - Unicode homoglyph detection

3. **Command filtering:**
   - 10+ dangerous pattern regexes
   - Base64 execution detection
   - Environment variable injection blocking

---

## Recommendations Summary

### Immediate (P0)
1. Fix blocking I/O in async contexts
2. Fix test mode security bypass
3. Fix FIM instruction injection
4. Remove or integrate dead config code

### Short-term (P1)
1. Fix symlink race conditions
2. Consolidate duplicate memory systems
3. Add API spawning limits
4. Improve shell parser

### Long-term (P2)
1. Implement actual semantic search
2. Add AST parser for knowledge graph
3. Create mock LLM server for CI
4. Split oversized modules

---

*Generated: 2026-03-06*  
*Analyst: Claude Code CLI*
