# Selfware Knowledge Graph Summary

## Overview
Successfully built a comprehensive knowledge graph for the `selfware` Rust project, capturing the codebase architecture, key components, and their relationships.

## Statistics
- **Total Nodes**: 46
- **Total Edges**: 53
- **Node Types**:
  - Crate: 1 (selfware)
  - Modules: 23
  - Structs: 14
  - Concepts: 7
  - Functions: 1

## Key Components Captured

### Core Crate
- **selfware** - Local-first AI workshop with 70+ tools, multi-agent swarm, evolution engine

### Major Modules (23)
1. **agent** - Core agent orchestrating LLM reasoning with tool execution
2. **api** - API client for LLM backends (Ollama, local servers)
3. **tools** - 70+ tools for file, git, cargo, shell, browser automation, etc.
4. **cognitive** - RSI orchestrator, knowledge graph, memory hierarchy, token budget
5. **safety** - Multi-layer safety system with path validation and audit logging
6. **orchestration** - Multi-agent orchestration with consensus voting
7. **devops** - Container and process management for Docker/Podman
8. **cli** - Interactive command-line interface
9. **config** - Configuration management with resource quotas
10. **mcp** - Model Context Protocol (client and server)
11. **lsp** - Language Server Protocol for semantic code intelligence
12. **analysis** - Code analysis with vector store and workspace graph
13. **observability** - Telemetry, tracing, and metrics
14. **session** - Checkpointing, caching, encryption, persistence
15. **evolution** - Recursive self-improvement engine
16. **ui** - TUI dashboard with animations and garden view
17. **computer** - Computer control and automation
18. **hooks** - Event-driven automation system
19. **supervision** - Circuit breaker and health monitoring
20. **resource** - Memory, disk, GPU monitoring and quota enforcement
21. **tokens** - Token counting and estimation
22. **memory** - Working memory system
23. **doctor** - System dependency checker

### Key Structs (14)
1. **Agent** - Core agent orchestrating LLM reasoning
2. **ApiClient** - API client for LLM backends
3. **ToolRegistry** - Registry of 70+ tools
4. **SafetyChecker** - Multi-layer safety system
5. **AgentMemory** - Agent memory system
6. **Config** - Configuration system
7. **HookRegistry** - Event-driven hook system
8. **Doctor** - System dependency checker
9. **SelfImprovementEngine** - Recursive self-improvement
10. **CognitiveState** - Cognitive state management
11. **HierarchicalMemory** - Memory hierarchy system
12. **TokenBudgetAllocator** - Token budget allocation
13. **VerificationGate** - Code validation gate
14. **ErrorAnalyzer** - Intelligent error analysis

### Key Concepts (7)
1. **PDVR Cycle** - Plan-Do-Verify-Reflect cognitive cycle
2. **Multi-Agent Swarm** - Multi-agent system with consensus voting
3. **MCP** - Model Context Protocol
4. **LSP** - Language Server Protocol
5. **Knowledge Tools** - Knowledge graph tools
6. **CLI** - Command-line interface modes
7. **CheckpointSystem** - Task resumption system

### Key Functions (1)
1. **Agent::run_task** - Main entry point for running agent tasks

## Relationship Types
- **contains** - Module/function containment hierarchy
- **uses** - Component usage dependencies
- **depends_on** - Module dependencies
- **implements** - Concept implementation
- **related_to** - General relationships

## Architecture Insights

### Core Agent Flow
```
Agent
├── uses ApiClient (LLM interaction)
├── uses ToolRegistry (70+ tools)
├── uses SafetyChecker (validation)
├── uses AgentMemory (state management)
├── uses Config (behavior settings)
├── uses HookRegistry (event automation)
├── uses CognitiveState (PDVR cycle)
├── uses SelfImprovementEngine (learning)
├── uses VerificationGate (code validation)
├── uses ErrorAnalyzer (error handling)
├── uses HierarchicalMemory (memory management)
├── uses TokenBudgetAllocator (context management)
└── implements PDVR Cycle
```

### Module Dependencies
- **agent** depends on:
  - safety (multi-layer protection)
  - cognitive (reasoning and learning)
  - tools (execution capabilities)
  - tokens (context management)

### Key Architectural Patterns
1. **Event-Driven**: Hook system for PreToolUse, PostToolUse, Stop events
2. **Multi-Agent**: Up to 16 concurrent agents with consensus voting
3. **Self-Improving**: Evolutionary mutation with fitness evaluation
4. **Local-First**: Zero data egress, runs entirely on local hardware
5. **Safe-by-Design**: Multi-layer validation with JSONL audit logging
6. **Persistent**: Checkpoint system for task resumption

## Knowledge Graph Usage
The exported knowledge graph (`knowledge_graph.json`) can be used for:
- Code navigation and exploration
- Impact analysis for refactoring
- Documentation generation
- Onboarding new developers
- Architecture visualization

## Files
- `knowledge_graph.json` - Complete exported knowledge graph (838 lines)
- This summary document - Human-readable overview

## Verification
- ✅ Cargo check: 0 errors, 0 warnings
- ✅ All 46 nodes successfully added
- ✅ All 53 relationships established
- ✅ Knowledge graph exported successfully
