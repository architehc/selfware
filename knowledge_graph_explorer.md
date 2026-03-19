# Selfware Knowledge Graph Explorer

## Overview
- **Total Nodes:** 183
- **Total Edges:** 174
- **Node Types:** File (133), Module (27), Struct (14), Concept (7), Crate (1), Function (1)
- **Edge Types:** contains, uses, depends_on, implements, related_to

## Graph Statistics

### Nodes by Type
| Type | Count | Percentage |
|------|-------|------------|
| File | 133 | 72.7% |
| Module | 27 | 14.8% |
| Struct | 14 | 7.6% |
| Concept | 7 | 3.8% |
| Crate | 1 | 0.5% |
| Function | 1 | 0.5% |

## Core Structure

### Root Crate
- **selfware** (Crate - a36869551ed3)
  - Description: "Personal AI workshop — local-first agentic coding harness with 70+ tools, multi-agent swarm, evolution engine, and safety features"
  - Location: `./Cargo.toml`
  - Contains 22 direct children

### Key Modules (Direct children of selfware crate)

#### 1. agent (e5e154cc4110)
- **Description:** Core agent module with task execution, context management, checkpointing, and learning capabilities
- **Location:** `./src/agent/mod.rs`
- **Key Structs:**
  - Agent (bde2cd57feac) - Core agent orchestrating LLM reasoning
  - AgentMemory (49d68c296570) - Memory system for conversation history
- **Key Relationships:**
  - uses: ApiClient, Config, SafetyChecker, cognitive, tools, session, lsp, observability, memory
  - contains: task_runner, agent_execution, agent_streaming, agent_checkpointing, message_handling, context_management

#### 2. tools (347612c9ae64)
- **Description:** 70+ tools for file operations, git, cargo, shell, browser automation, computer control, LSP, MCP, and more
- **Location:** `./src/tools/mod.rs`
- **Key Struct:** ToolRegistry (4691ffc45303)
- **Sub-modules:**
  - File tools (file_read, file_write, file_edit, file_delete, directory_tree)
  - Git tools (git_status, git_diff, git_commit, git_push, git_checkpoint)
  - Cargo tools (cargo_check, cargo_clippy, cargo_fmt, cargo_test)
  - Shell tools (shell_exec, pty_shell)
  - Container tools (container_run, container_stop, container_build, etc.)
  - Browser tools (browser_fetch, browser_links, browser_eval, browser_screenshot, browser_pdf)
  - LSP tools (lsp_document_symbols, lsp_hover, lsp_goto_definition, lsp_find_references)
  - Knowledge tools (knowledge_add, knowledge_query, knowledge_relate, knowledge_export, knowledge_stats)
  - Vision tools (vision_analyze, vision_compare)
  - Screen capture tools (screen_capture, computer_screen)
  - Process tools (process_start, process_stop, process_restart, process_logs)
  - Package management (pip_install, npm_install, yarn_install)
  - Computer control (computer_keyboard, computer_mouse)
  - HTTP tools (http_request)
  - FIM editing (file_fim_edit)
  - Swarm dispatch (swarm_dispatch)
  - Hot reload (hot_reload)

#### 3. cognitive (93873cbf24d4)
- **Description:** Cognitive systems and AI capabilities - PDVR cycle, episodic memory, RAG, self-improvement
- **Location:** `./src/cognitive/mod.rs`
- **Key Structs:**
  - CognitiveState (96c068d89947) - PDVR cycle state management
  - TokenBudgetAllocator (a90b95e5582f) - Token budget management
  - HierarchicalMemory (deb01b6c462f) - Memory hierarchy system
  - SelfImprovementEngine (237e698700c1) - Recursive self-improvement
- **Sub-modules:**
  - rsi_orchestrator - Recursive Self-Improvement orchestrator
  - cognitive_kg - Knowledge graph implementation
  - token_budget - Token budget management
  - memory_hierarchy - Hierarchical memory system
  - self_improvement - Self-improvement engine
  - intelligence - Cognitive system intelligence core

#### 4. safety (a6f380c40c7b)
- **Description:** Multi-layer safety system with path validation, sandboxing, threat modeling, audit logging
- **Location:** `./src/safety/mod.rs`
- **Key Struct:** SafetyChecker (77394c16c431)
- **Features:** Path protection, command filtering, JSONL audit logging, permission grants
- **Sub-modules:**
  - path_validator - Path validation for safety checks
  - checker - Safety validation and risk assessment
  - threat_modeling - STRIDE threat modeling

#### 5. api (eb8705197cbc)
- **Description:** LLM API client with token budget enforcement, streaming support, and model configuration
- **Location:** `./src/api/mod.rs`
- **Key Struct:** ApiClient (16934f756243)
- **Features:** Circuit breaker pattern, token budget enforcement, streaming support

#### 6. session (3cec7dbb0650)
- **Description:** Session management with checkpointing, caching, encryption, and local-first persistence
- **Location:** `./src/session/mod.rs`
- **Features:** Checkpointing, tool result caching, LLM response caching, encryption
- **Sub-modules:**
  - session_cache - State management cache
  - session_checkpoint - Persistence checkpointing

#### 7. lsp (02647dba22d2)
- **Description:** Language Server Protocol client for semantic code intelligence
- **Location:** `./src/lsp/mod.rs`
- **Supports:** rust-analyzer, pyright, tsserver, gopls
- **Sub-modules:**
  - lsp_client - Client implementation
  - lsp_server - Server implementation

#### 8. config (e01054e1fb54)
- **Description:** Configuration management with resource quotas, model settings, and semantic feature flags
- **Location:** `./src/config/mod.rs`
- **Key Struct:** Config (97094ed6f4c1)
- **Features:** TOML loading, API settings, agent behavior, safety settings

#### 9. cli (0b2fcfd9e446)
- **Description:** Command-line interface with interactive REPL, TUI dashboard, and CLI commands
- **Location:** `./src/cli.rs`
- **Sub-modules:**
  - command_registry - Command parsing and validation
  - completer - Input completion system
  - highlighter - Syntax highlighting

#### 10. mcp (49b7bcc303b7)
- **Description:** Model Context Protocol client and server for tool bridging
- **Location:** `./src/mcp/mod.rs`
- **Features:** Client mode (connect to external tools), Server mode (expose selfware tools)

#### 11. ui (4850e4aecf3a)
- **Description:** TUI dashboard with animations, garden view, swarm visualization, diff viewer
- **Location:** `./src/ui/mod.rs`
- **Sub-modules:**
  - tui_widgets - TUI widget implementations

#### 12. hooks (08c83091e0bb)
- **Description:** Event-driven hook system for automation
- **Location:** `./src/hooks/mod.rs`
- **Key Struct:** HookRegistry (7cadff40b28f)
- **Events:** PreToolUse, PostToolUse, Stop

#### 13. memory (7d29fe2e30f8)
- **Description:** Working memory system for agent state and context retention
- **Location:** `./src/memory.rs`
- **Key Struct:** AgentMemory (49d68c296570)

#### 14. tokens (a2346ee119bd)
- **Description:** Token counting, estimation, and management for LLM context windows
- **Location:** `./src/tokens.rs`
- **Features:** Token estimation, context pruning, budget management, drift tracking

#### 15. resource (264974467493)
- **Description:** Resource governance with memory, disk, GPU monitoring and quota enforcement
- **Location:** `./src/resource/mod.rs`

#### 16. supervision (5dc0198017ea)
- **Description:** Agent supervision with circuit breaker, health monitoring, and fault tolerance
- **Location:** `./src/supervision/mod.rs`

#### 17. observability (db3528230e0b)
- **Description:** Telemetry, metrics, logging, carbon tracking, and dashboard
- **Location:** `./src/observability/mod.rs`

#### 18. analysis (ab35373c9f1b)
- **Description:** Code analysis with vector store, BM25 search, workspace graph, tech debt tracking
- **Location:** `./src/analysis/mod.rs`

#### 19. orchestration (385b56c7fd69)
- **Description:** Multi-agent swarm, workflows, planning, and parallel execution orchestration
- **Location:** `./src/orchestration/mod.rs`

#### 20. devops (0c079ab4c8cc)
- **Description:** Container and process management for Docker/Podman
- **Location:** `./src/devops/mod.rs`

#### 21. computer (a4f580f9a9d4)
- **Description:** Computer control with keyboard, mouse, screen capture, and window management
- **Location:** `./src/computer/mod.rs`

#### 22. evolution (b5cb96ff0f67)
- **Description:** Evolution engine for recursive self-improvement
- **Location:** `./src/evolution/mod.rs`

#### 23. doctor (38cfca6aa7d8)
- **Description:** System dependency checker and LLM backend diagnostics tool
- **Location:** `./src/doctor.rs`

## Key Concepts

### 1. PDVR Cycle (e290de605082)
- **Description:** Plan-Do-Verify-Reflect cognitive cycle for structured reasoning
- **Implemented by:** Agent struct

### 2. Multi-Agent Swarm (c8193a5b68e1)
- **Description:** Up to 16 concurrent agents with consensus voting and role specialization

### 3. MCP (be0a20a5ff4b)
- **Description:** Model Context Protocol for tool bridging and external integration

### 4. LSP (a40d874b92a9)
- **Description:** Language Server Protocol integration for semantic code intelligence

### 5. CLI (4a158f55060f)
- **Description:** Interactive command-line interface with chat mode, run mode, doctor, init, multi-chat, MCP server modes

### 6. Knowledge Tools (5784a64f30a6)
- **Description:** Knowledge graph tools for managing the codebase knowledge base

### 7. CheckpointSystem (f6cfbb206271)
- **Description:** Checkpoint system for task resumption and session persistence

## Key Functions

### Agent::run_task (d6680659381c)
- **Description:** Main entry point for running agent tasks
- **Location:** `src/agent/mod.rs`
- **Orchestrates:** Full workflow from task parsing to completion

## Relationship Types

### contains
- Hierarchical structure (crate contains modules, modules contain files/structs)
- Example: selfware --contains--> agent, tools, cognitive, safety

### uses
- Dependency relationships between components
- Example: agent --uses--> ApiClient, tools, cognitive, safety

### depends_on
- Stronger dependency relationship
- Example: main.rs --depends_on--> config, session

### implements
- Implementation relationships
- Example: Agent --implements--> PDVR Cycle

### related_to
- Conceptual relationships
- Example: Agent::run_task --related_to--> Agent

## Navigation Guide

### Finding Related Components
1. **Search by node ID:** Use the ID (e.g., bde2cd57feac) to find specific nodes
2. **Search by name:** Partial match on node names (e.g., "agent", "tool")
3. **Filter by type:** Focus on specific node types (File, Module, Struct, Concept)
4. **Follow relationships:** Trace contains/uses/depends_on edges to understand dependencies

### Common Queries
- "What does the agent module use?" → Follow edges from agent (e5e154cc4110)
- "What files are in the tools module?" → Follow contains from tools (347612c9ae64)
- "Which modules depend on cognitive?" → Find edges where to_id = cognitive (93873cbf24d4)
- "Show all structs" → Filter nodes by type = "struct"

## Export Information
- **Exported:** 2026-03-17T15:32:52.920527026+00:00
- **File:** knowledge_graph.json
- **Format:** JSON with nodes and edges arrays

## Usage Examples

### Query Nodes by Type
```
Type: File - 133 nodes (src/**/*.rs files)
Type: Module - 27 nodes (mod.rs files)
Type: Struct - 14 nodes (key data structures)
Type: Concept - 7 nodes (architectural concepts)
Type: Crate - 1 node (selfware root)
Type: Function - 1 node (Agent::run_task)
```

### Explore Key Paths
1. **Core Agent Flow:**
   - Agent (struct) → uses → ApiClient, ToolRegistry, SafetyChecker, CognitiveState
   - Agent → implements → PDVR Cycle
   - Agent::run_task → orchestrates → full workflow

2. **Tool Execution Path:**
   - Agent → uses → ToolRegistry
   - ToolRegistry → contains → 70+ tool implementations
   - Tools → validated by → SafetyChecker

3. **Cognitive Flow:**
   - Agent → uses → cognitive module
   - cognitive → contains → CognitiveState, TokenBudgetAllocator, HierarchicalMemory
   - cognitive → enables → PDVR cycle, self-improvement

4. **Safety Layer:**
   - Agent → uses → SafetyChecker
   - SafetyChecker → validates → paths, commands, permissions
   - safety → contains → path_validator, threat_modeling

## Next Steps for Exploration
1. Add more function/method nodes for detailed API documentation
2. Add relationship types for trait implementations
3. Track function calls between modules
4. Add test coverage information
5. Map feature flags to modules
6. Track imports/exports between crates
