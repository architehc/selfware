# Selfware Gap Analysis: Achieving "Claude Code Lean" Experience

## Executive Summary

This analysis compares Selfware (v0.1.4) against leading agentic coding systems—Claude Code, Cursor, GitHub Copilot, Continue.dev, and Aider—to identify capability gaps and differentiation opportunities. Selfware positions itself as "software that improves itself, locally run, privately owned" with a multi-agent swarm architecture built in Rust.

**Key Finding**: Selfware has a solid technical foundation with 54 tools, multi-agent swarm support, and comprehensive safety features. However, it lacks critical UX, integration, and ecosystem features that define the "Claude Code lean" experience. The gap is primarily in user experience polish, IDE integration, and extensibility mechanisms rather than core agentic capabilities.

---

## 1. CLAUDE CODE ANALYSIS

### 1.1 Core Features & Capabilities

| Feature | Description | Maturity |
|---------|-------------|----------|
| **File Operations** | Read, Edit, Write, Delete with atomic operations | Production |
| **Bash Execution** | Shell commands with dangerous pattern detection | Production |
| **Git Integration** | Commit, branch, diff, PR creation | Production |
| **Code Search** | Grep, Glob for file discovery | Production |
| **LSP Support** | Language Server Protocol (v2.0.74+) | Production |
| **Web Tools** | WebSearch, WebFetch for research | Production |
| **MCP Integration** | Model Context Protocol for external tools | Production |
| **Subagents** | Specialized AI instances with isolated contexts | Production |
| **Hooks System** | 15 lifecycle events with 3 handler types | Production |
| **Plan Mode** | Read-only planning before execution | Production |
| **Extended Thinking** | Adaptive reasoning (Opus 4.6+) | Production |
| **Skills** | Domain-specific knowledge packages | Production |
| **Plugins** | Bundle and share configurations | Production |
| **Named Sessions** | Persistent sessions with resume | Production |

### 1.2 Architecture & Design Patterns

**Terminal-Native Design**: Claude Code operates directly in the terminal with full filesystem access. This is its key architectural differentiator—it's not an IDE extension but a standalone harness.

**State Persistence**:
- CLAUDE.md: Project-level persistent instructions
- MEMORY.md: Auto-generated session memory (first 200 lines loaded)
- Session memory: `~/.claude/session-memory/[session-id].md`
- Named sessions: `claude --resume session-name`

**Context Management**:
- 200K context window (Claude models)
- Context compaction with user-controlled focus
- Topic files for on-demand loading
- LSP for semantic code understanding

**Safety Model**:
- Permission prompts for destructive operations
- PreToolUse hooks for blocking
- Dangerous pattern detection in Bash
- Configurable auto-approval levels

### 1.3 User Interaction Model

```
Three Modes:
1. Normal: Interactive with permission prompts
2. Plan Mode (Shift+Tab x2): Read-only planning
3. Auto-Accept: Non-interactive execution

Extended Thinking:
- think: Light analysis
- think hard: Deeper reasoning
- think harder: Extended problem-solving
- ultrathink: Maximum depth
```

### 1.4 Tool Use & Extension System

**MCP (Model Context Protocol)**:
- Open standard for AI-tool integration
- 50+ community servers available
- GitHub, Brave Search, Playwright, PostgreSQL, Filesystem
- Three transport modes: stdio, SSE, HTTP

**Custom Tools**:
- Slash commands: `.claude/commands/*.md`
- Dynamic arguments via `$ARGUMENTS`
- Skills: Domain-specific instructions
- Plugins: Bundle commands, subagents, hooks, MCP configs

### 1.5 Context Management Approach

**Three-Layer Memory** (from research):
1. **Working Memory**: Active conversation context
2. **Episodic Memory**: Session history and learnings
3. **Semantic Memory**: Project knowledge and patterns

**Context Engineering**:
- CLAUDE.md for explicit context declaration
- Auto memory for discovered patterns
- Subagents for isolated exploration
- Skills for domain expertise

### 1.6 Safety & Permission Model

| Layer | Mechanism |
|-------|-----------|
| PreToolUse Hooks | Block/validate before execution |
| Permission Prompts | User approval for destructive ops |
| Dangerous Patterns | Built-in regex detection |
| Configurable Rules | `.claude/settings.json` |
| YOLO Mode | Override with audit logging |

---

## 2. COMPETITIVE LANDSCAPE

### 2.1 Cursor IDE

**Positioning**: AI-native IDE (fork of VS Code)

**Key Features**:
- Tab-based autocomplete
- Composer: Multi-file editing
- Agent mode: Autonomous task execution
- Chat with codebase context
- @-mentions for symbols/files
- Image-to-code (Figma integration)
- MCP support (added recently)
- Terminal integration

**Strengths**:
- Polished UX
- Fast autocomplete
- Good multi-file editing
- Strong TypeScript support

**Weaknesses**:
- Proprietary IDE (migration required)
- $20/month
- Limited model choice
- No hooks system

**Differentiation**: Cursor is an IDE with AI, not an AI harness.

### 2.2 GitHub Copilot

**Positioning**: AI pair programmer (IDE extension)

**Key Features**:
- Code completion (inline suggestions)
- Copilot Chat (Q&A)
- Agent Mode (autonomous execution)
- Multi-model support (Claude, Gemini, OpenAI)
- MCP support (public preview)
- PR summaries, commit message generation
- Security vulnerability detection

**Strengths**:
- Wide IDE support
- GitHub integration
- Enterprise features
- Large user base

**Weaknesses**:
- Limited context understanding
- Less autonomous than Claude Code
- Vendor lock-in

**Pricing**: $19/month (Pro), $39/month (Pro+)

### 2.3 Continue.dev

**Positioning**: Open-source AI coding assistant

**Key Features**:
- Chat, Autocomplete, Edit, Actions
- Any LLM (OpenAI, Claude, Mistral, local)
- VS Code and JetBrains support
- Embeddings + reranking for context
- Plan Mode (read-only exploration)
- Agent Mode (autonomous execution)
- Local deployment option

**Strengths**:
- Fully open-source (26K+ stars)
- Model agnostic
- Local deployment
- Free for solo use ($10/team/month)

**Weaknesses**:
- Less polished than Cursor
- Autocomplete sometimes unreliable
- No hooks system
- Smaller ecosystem

**Differentiation**: "Your code, your AI, your rules"

### 2.4 Aider

**Positioning**: Terminal-based AI pair programming

**Key Features**:
- Editor-agnostic (works with any IDE)
- LLM-agnostic (OpenAI, Anthropic, local)
- Git-native (auto-commits with messages)
- Repo map (architecture understanding)
- Architect mode (two-pass: plan then implement)
- Voice coding (Whisper integration)
- Slash commands (/add, /drop, /ask, /diff, /undo)

**Strengths**:
- Works with any editor
- Excellent Git integration
- Strong benchmarks (90% on coding exercises)
- Self-hosting friendly

**Weaknesses**:
- Terminal-only (no IDE integration)
- No hooks or MCP
- Smaller feature set

**Differentiation**: "Git-native by design"

### 2.5 Feature Comparison Matrix

| Feature | Claude Code | Cursor | Copilot | Continue | Aider | Selfware |
|---------|-------------|--------|---------|----------|-------|----------|
| Terminal-native | ✅ | ❌ | ❌ | ❌ | ✅ | ✅ |
| IDE Integration | ✅ | ✅ (fork) | ✅ (ext) | ✅ (ext) | ❌ | ❌ |
| Multi-agent | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Hooks System | ✅ (15 events) | ✅ (6 events) | ❌ | ❌ | ❌ | ❌ |
| MCP Support | ✅ | ✅ | ✅ (preview) | ❌ | ❌ | ❌ |
| Plan Mode | ✅ | ✅ | ❌ | ✅ | ✅ (architect) | ❌ |
| LSP Support | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| Auto-commits | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ |
| Local LLMs | ✅ | ❌ | ❌ | ✅ | ✅ | ✅ |
| Open Source | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| Memory System | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Swarm | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |

---

## 3. SELFWARE CURRENT STATE ANALYSIS

### 3.1 Existing Capabilities

Based on documentation analysis (v0.1.4):

**Core Architecture**:
- ~197K lines of Rust code
- 54 built-in tools (file, git, cargo, search, shell)
- Multi-agent swarm (up to 16 concurrent agents)
- TUI dashboard (ratatui-based)
- PDVR cycle (Plan-Do-Verify-Reflect)

**Agent System**:
- Six specialized agents: Architect, Coder, Tester, Reviewer, DevOps, Security
- Each agent runs its own PDVR cycle
- Shared memory with consensus voting
- Task queue coordination

**Safety Features**:
- Multi-layer validation
- Path protection with TOCTOU validation
- Command filtering (dangerous pattern blocking)
- Secret and vulnerability detection
- Docker-based sandboxing
- YOLO mode with audit logging

**Cognitive Architecture**:
- 1M token 3-layer memory (working/episodic/semantic)
- RAG (Retrieval-Augmented Generation)
- Token budget management
- Knowledge graph for codebase relationships

**Persistence**:
- Checkpoint system for long-running tasks
- Task checkpointing with delta compression
- Local-first sync

**Observability**:
- Telemetry and metrics
- Carbon tracking
- Garden view (code health visualization)

### 3.2 Unique Differentiators

1. **Evolution Engine**: Recursive self-improvement with fitness functions
2. **Swarm Architecture**: True multi-agent consensus (not just subagents)
3. **Local-First**: Designed for on-premise/private deployment
4. **Rust Foundation**: Memory safety, performance
5. **VLM Support**: Vision-language model integration
6. **Science/Benchmarking**: SAB (Selfware Agentic Benchmark) with 20 scenarios

### 3.3 Current Gaps

**Critical Missing Features**:
1. No IDE integration (VS Code, JetBrains)
2. No MCP support
3. No hooks system
4. No LSP integration
5. No Plan Mode equivalent
6. No named sessions/resume
7. No plugin ecosystem
8. Limited documentation

**UX Gaps**:
1. No visual diff viewer
2. No inline suggestions
3. No @-mentions for symbols
4. No chat history UI
5. No command palette

---

## 4. GAP IDENTIFICATION FOR SELFWARE

### 4.1 Core Feature Gaps

| Gap | Claude Code | Selfware | Priority |
|-----|-------------|----------|----------|
| **IDE Integration** | VS Code, JetBrains plugins | None | CRITICAL |
| **MCP Support** | Full protocol | None | HIGH |
| **Hooks System** | 15 events, 3 handler types | None | HIGH |
| **LSP Integration** | Native tool | None | HIGH |
| **Plan Mode** | Shift+Tab x2 | None | MEDIUM |
| **Named Sessions** | `claude --resume` | Checkpoints only | MEDIUM |
| **Extended Thinking** | Adaptive reasoning | Fixed cycles | MEDIUM |
| **Skills System** | Domain packages | Knowledge graph only | LOW |
| **Plugin Ecosystem** | Marketplace | None | LOW |

### 4.2 UX/Interaction Gaps

| Gap | Impact | Effort |
|-----|--------|--------|
| Visual diff viewer | High | Medium |
| Inline code suggestions | High | High |
| @-mentions with autocomplete | Medium | Medium |
| Chat history persistence | Medium | Low |
| Command palette | Medium | Low |
| File tree navigation | Low | Low |

### 4.3 Architecture Gaps

| Gap | Claude Code | Selfware |
|-----|-------------|----------|
| Context window | 200K tokens | 1M (hierarchy) |
| Context compaction | User-controlled | Automatic |
| Session memory | Structured markdown | Checkpoint-based |
| Tool discovery | Dynamic MCP | Static 54 tools |
| Extensibility | Plugins, MCP, hooks | Config only |

### 4.4 What "Lean" Means vs Full-Featured

**Claude Code Lean** should include:
- ✅ Terminal-native operation
- ✅ Core file/bash/git tools
- ✅ Basic safety/permissions
- ✅ Session persistence
- ✅ Simple configuration
- ❌ IDE integration (optional)
- ❌ MCP ecosystem (optional)
- ❌ Complex hooks (optional)
- ❌ Plugin marketplace (optional)

**Current Selfware is**: Feature-heavy but UX-light
- Has swarm, evolution, VLM (complex)
- Lacks basic IDE integration, MCP (fundamental)

---

## 5. FEATURE RECOMMENDATIONS

### 5.1 Must-Have Features for MVP

**P0: Critical for Parity**

1. **IDE Integration (VS Code Extension)**
   - Inline diff viewing
   - Selection context sharing
   - File reference shortcuts
   - Diagnostic sharing
   - Estimated effort: 4-6 weeks

2. **MCP Client Support**
   - stdio transport (local servers)
   - Tool discovery and registration
   - Permission management
   - Estimated effort: 2-3 weeks

3. **Basic Hooks System**
   - PreToolUse (blocking)
   - PostToolUse (formatting)
   - Stop (verification)
   - Command handlers only
   - Estimated effort: 2-3 weeks

4. **LSP Client Integration**
   - goToDefinition
   - findReferences
   - documentSymbol
   - Estimated effort: 3-4 weeks

5. **Plan Mode**
   - Read-only exploration phase
   - Planning before execution
   - Estimated effort: 1-2 weeks

**P1: High Value**

6. **Named Sessions**
   - `selfware --resume <name>`
   - Session listing
   - Estimated effort: 1 week

7. **Auto-commits**
   - Git commit after each change
   - AI-generated messages
   - Estimated effort: 1 week

8. **Extended Thinking Toggle**
   - User-controlled reasoning depth
   - Estimated effort: 1 week

### 5.2 Nice-to-Have Differentiators

**Leverage Selfware's Strengths**:

1. **Swarm Mode for IDE**
   - Multiple agents visible in UI
   - Agent status dashboard
   - Consensus visualization

2. **Evolution Integration**
   - Self-improving prompts
   - Fitness tracking UI
   - Benchmark results

3. **VLM in IDE**
   - Image-to-code from screenshots
   - UI mockup implementation
   - Diagram understanding

4. **Garden View in IDE**
   - Code health visualization
   - Technical debt heatmap
   - File relationship graph

### 5.3 Features to Intentionally Exclude (for Leanness)

**Don't Build**:
1. Full IDE (like Cursor) - stay harness-focused
2. Plugin marketplace - use MCP instead
3. Cloud hosting - stay local-first
4. Proprietary models - stay model-agnostic
5. Complex workflow DSL - simplify

### 5.4 Integration Opportunities

**High-Impact Integrations**:

1. **Continue.dev Extension**
   - Selfware as backend for Continue
   - Leverage Continue's IDE support
   - Estimated effort: 2-3 weeks

2. **MCP Server Ecosystem**
   - Use existing servers (GitHub, Playwright, etc.)
   - Don't rebuild
   - Immediate capability expansion

3. **Aider-style Git Integration**
   - Auto-commits with messages
   - Repo map for context
   - Estimated effort: 1-2 weeks

---

## 6. TECHNICAL STACK RECOMMENDATIONS

### 6.1 Enabling "Claude Code Lean"

**Core Stack** (keep Rust):
- ✅ Rust for performance and safety
- ✅ Tokio for async runtime
- ✅ Ratatui for TUI
- ✅ Serde for serialization

**Additions Needed**:

1. **LSP Client**: `tower-lsp` or `lsp-types`
   - Industry standard
   - Good Rust ecosystem

2. **MCP Client**: Build or adapt
   - Protocol is JSON-RPC based
   - Can reuse existing implementations

3. **VS Code Extension**: TypeScript
   - Standard VS Code APIs
   - Language Server Protocol

4. **Hooks System**: Embedded scripting
   - Rhai (Rust-embedded scripting)
   - Or WASM plugins
   - Keep it simple

### 6.2 Architectural Decisions

**Recommended Architecture**:

```
┌─────────────────────────────────────────────────────────────┐
│                    User Interfaces                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │ Terminal │  │ VS Code  │  │ JetBrains│                  │
│  │   TUI    │  │ Extension│  │  Plugin  │                  │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘                  │
└───────┼─────────────┼─────────────┼────────────────────────┘
        │             │             │
        └─────────────┴─────────────┘
                      │
        ┌─────────────┴─────────────┐
        │      Selfware Core        │
        │  ┌─────────────────────┐  │
        │  │   Agent Runtime     │  │
        │  │  (PDVR + Swarm)     │  │
        │  └─────────────────────┘  │
        │  ┌─────────────────────┐  │
        │  │    Tool Registry    │  │
        │  │ (54 built-in + MCP) │  │
        │  └─────────────────────┘  │
        │  ┌─────────────────────┐  │
        │  │    Hook Engine      │  │
        │  └─────────────────────┘  │
        │  ┌─────────────────────┐  │
        │  │   LSP Client        │  │
        │  └─────────────────────┘  │
        └───────────────────────────┘
```

### 6.3 Common Pitfalls to Avoid

**Don't**:
1. Build a full IDE (Cursor already exists)
2. Create proprietary protocols (use MCP)
3. Over-engineer the hook system (start simple)
4. Ignore IDE integration (terminal-only is limiting)
5. Neglect documentation (adoption barrier)
6. Skip LSP (code intelligence is table stakes)

**Do**:
1. Focus on harness, not editor
2. Embrace open standards (MCP, LSP)
3. Prioritize VS Code integration (largest market)
4. Keep local-first philosophy
5. Leverage swarm as differentiator
6. Maintain Rust performance advantage

---

## 7. IMPLEMENTATION ROADMAP

### Phase 1: Foundation (Weeks 1-4)
- [ ] LSP client integration
- [ ] Basic hooks system (3 events)
- [ ] MCP client support
- [ ] Documentation overhaul

### Phase 2: IDE Integration (Weeks 5-10)
- [ ] VS Code extension
- [ ] Inline diff viewer
- [ ] Selection context sharing
- [ ] Diagnostic integration

### Phase 3: Polish (Weeks 11-14)
- [ ] Plan Mode
- [ ] Named sessions
- [ ] Auto-commits
- [ ] Extended thinking toggle

### Phase 4: Differentiation (Weeks 15-20)
- [ ] Swarm visualization in IDE
- [ ] Garden view integration
- [ ] Evolution dashboard
- [ ] VLM in IDE

---

## 8. SUMMARY & RECOMMENDATIONS

### Key Findings

1. **Selfware has strong technical foundations** but lacks UX polish and ecosystem integration
2. **The "Claude Code lean" experience requires**: IDE integration, MCP, hooks, LSP
3. **Selfware's differentiators** (swarm, evolution, VLM) are compelling but need IDE exposure
4. **The gap is primarily UX, not core agentic capabilities**

### Strategic Recommendations

1. **Prioritize VS Code integration** - This is the biggest adoption barrier
2. **Implement MCP client** - Leverage existing ecosystem instead of building
3. **Add basic hooks** - Critical for workflow automation
4. **Keep swarm as flagship feature** - No competitor has true multi-agent consensus
5. **Maintain local-first philosophy** - Key differentiator vs cloud tools

### Success Metrics

- [ ] VS Code extension with 1000+ installs
- [ ] 10+ MCP servers working
- [ ] Plan Mode with user adoption
- [ ] Named sessions for long-running tasks
- [ ] Documentation completeness

---

*Analysis completed: Based on Selfware v0.1.4, Claude Code v2.1.x, and competitive landscape as of March 2026.*
