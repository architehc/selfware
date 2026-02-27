# Selfware RSI Roadmap: From Agent to Recursive Self-Improver

## Executive Summary

This roadmap outlines the transformation of Selfware from a sophisticated AI agent harness into a true **Recursive Self-Improving (RSI) System** capable of autonomous multi-day operation with local Qwen3 Coder (1M context).

**Current RSI Level: 2/10** → **Target RSI Level: 8/10**

---

## What is True RSI?

True Recursive Self-Improvement requires:

1. **Genuine Self-Modification**: The agent can modify its own source code
2. **Self-Compilation**: Can compile and test modified code
3. **Hot-Reload**: Can deploy changes without losing state
4. **Recursive Meta-Learning**: Can improve its own improvement mechanisms
5. **Long-Term Autonomy**: Can run for days with minimal human intervention
6. **Safe Evolution**: Improvements are bounded, tested, and reversible

---

## Current State Analysis

### What Selfware Already Has (Strong Foundation)

| Component | Status | Description |
|-----------|--------|-------------|
| PDVR Cycle | ✅ | Plan-Do-Verify-Reflect cognitive loop |
| Tool System | ✅ | 54 built-in tools with safety guardrails |
| Checkpointing | ✅ | Task-level persistence and resume |
| Self-Healing | ✅ | Error classification and recovery |
| Multi-Agent | ✅ | Swarm coordination (up to 16 agents) |
| Prompt Optimization | ✅ | Learns from prompt effectiveness |
| Tool Selection Learning | ✅ | Improves tool choice over time |
| Episodic Memory | ✅ | Experience tracking |

### Critical Gaps Preventing True RSI

| Gap | Impact | Priority |
|-----|--------|----------|
| No source code self-modification | Cannot improve architecture | 🔴 P0 |
| No self-compilation | Cannot test changes | 🔴 P0 |
| No hot-reload | Loses state on restart | 🔴 P0 |
| No 1M context utilization | Cannot fit codebase in context | 🔴 P0 |
| No multi-day infrastructure | Cannot run autonomously | 🟠 P1 |
| No evolutionary fitness functions | Cannot measure improvement | 🟠 P1 |
| No safety boundaries | Risk of runaway/degradation | 🟠 P1 |

---

## Phase 1: Memory & Context Architecture (P0)

### 1.1 Hierarchical Memory System

**Goal**: Maximize 1M token context utilization for self-improvement

```
┌─────────────────────────────────────────────────────────────┐
│                    1M TOKEN CONTEXT                          │
├─────────────────────────────────────────────────────────────┤
│ WORKING MEMORY (100K / 10%)                                  │
│ • Active conversation                                        │
│ • Current task context                                       │
│ • Recently accessed code                                     │
├─────────────────────────────────────────────────────────────┤
│ EPISODIC MEMORY (200K / 20%)                                 │
│ • Session history (tiered: Critical/High/Normal/Low)        │
│ • Tool executions and results                                │
│ • Errors and learnings                                       │
├─────────────────────────────────────────────────────────────┤
│ SEMANTIC MEMORY (700K / 70%)                                 │
│ • Selfware source code (full index)                         │
│ • AST-based structure parsing                                │
│ • Vector-based semantic search                               │
│ • Dependency graph tracking                                  │
└─────────────────────────────────────────────────────────────┘
```

**Key Components**:
- `memory_hierarchy.rs`: Unified memory manager
- `working_memory.rs`: Immediate context with importance scoring
- `episodic_memory.rs`: Tiered experience storage
- `semantic_memory.rs`: Codebase indexing and retrieval

**Task-Based Token Allocation**:

| Task Type | Working | Episodic | Semantic | Best For |
|-----------|---------|----------|----------|----------|
| SelfImprovement | 10% | 10% | **70%** | Modifying own code |
| CodeAnalysis | 15% | 15% | **60%** | Understanding codebase |
| Debugging | 25% | **35%** | 30% | Error investigation |

### 1.2 Self-Referential Context Management

**Goal**: Enable agent to read and understand its own source code

```rust
pub struct SelfReferenceSystem {
    semantic: Arc<RwLock<SemanticMemory>>,
    self_model: SelfModel,        // Agent's understanding of itself
    code_cache: LruCache<String, CachedCode>,
    recent_modifications: VecDeque<CodeModification>,
}
```

**Capabilities**:
- Index entire Selfware codebase (~50K-100K tokens)
- Retrieve relevant code for self-improvement tasks
- Track recent modifications
- Build and maintain self-model

### 1.3 Implementation Files

| File | Lines | Purpose |
|------|-------|---------|
| `memory_hierarchy.rs` | 298 | Three-layer memory system |
| `token_budget.rs` | 188 | Dynamic token allocation |
| `self_reference.rs` | 307 | Self-referential context |
| `semantic_memory.rs` | 350+ | Codebase indexing |

---

## Phase 2: Self-Modification System (P0)

### 2.1 Architecture Overview

```
Analyze → Prioritize → Checkpoint → Generate → Apply → Verify → Diff Test → Hot-Reload → Record
```

### 2.2 Core Components

#### Code Analysis Engine (`code_analysis.rs`)
- AST-based analysis using `syn` crate
- Detects: complexity, missing docs, unnecessary clones, performance issues
- Calculates cyclomatic complexity
- Identifies improvement opportunities

#### AST Transformer (`code_transformer.rs`)
- Type-safe code transformations
- Safety levels: Additive, BehaviorPreserving, BehaviorChanging, Critical
- Transformations: AddFunction, ModifyFunction, ExtractFunction, PatternReplace

#### Compilation Manager (`compilation_manager.rs`)
- `cargo check`, `cargo clippy`, `cargo test` integration
- Full verification pipeline
- Warning/error parsing

#### Hot-Reload System (`hot_reload.rs`)
- Dynamic library loading with `libloading`
- State migration between versions
- File watching with `notify`
- Zero-downtime updates

#### Safety & Rollback (`safety_rollback.rs`)
- Git-based checkpointing before each modification
- Protected files: `safety/`, `Cargo.toml`, `main.rs`
- Session modification limits
- Automatic rollback on failure

### 2.3 Safety Mechanisms

| Mechanism | Description |
|-----------|-------------|
| Protected Components | Cannot modify safety/, main.rs, Cargo.toml |
| Multi-Layer Verification | Static analysis → Compile → Test → Differential |
| Git Checkpoints | Every modification creates rollback point |
| Session Limits | Max 10 modifications per session |
| Type Safety | AST-based transformations preserve types |

### 2.4 Key Dependencies

```toml
syn = { version = "2.0", features = ["full", "visit", "visit-mut"] }
quote = "1.0"
proc-macro2 = "1.0"
prettyplease = "0.2"
libloading = "0.8"
notify = "6.1"
git2 = "0.18"
```

---

## Phase 3: Multi-Day Execution Infrastructure (P1)

### 3.1 Checkpointing System

**Strategy**: Dual approach (time-based + event-based)
- Full checkpoint every 30 minutes
- Delta checkpoints every 5 minutes
- Storage: SQLite (metadata) + bincode (state) + zstd (compression)

**What to Checkpoint**:
| Component | Frequency | Size Target |
|-----------|-----------|-------------|
| Agent State | 5min + events | <10MB |
| Task Queue | Every change | - |
| Working Memory | 5min | <50MB |
| LLM Context | On demand | <100MB |

### 3.2 Process Supervision

**Watchdog Pattern**:
- Separate supervisor process monitors agent
- Heartbeat every 30s, timeout after 90s
- Exponential backoff: 1s, 2s, 4s, 8s, max 60s
- Max 5 restarts per hour

### 3.3 Resource Management

**GPU VRAM Tiers**:
| Tier | VRAM Usage | Action |
|------|------------|--------|
| Normal | <70% | Maintain current model |
| Warning | 70-85% | Reduce quantization |
| Critical | >85% | Unload model, emergency actions |

**Quantization Fallback**:
```
Qwen3-32B-Q4_K_M (18GB) → Q3_K_L (14GB) → Q2_K (10GB)
```

### 3.4 Progress Tracking

**Metrics**:
- Task throughput (completed/hour)
- Success rate (rolling 24h)
- LLM tokens/sec
- Resource usage (VRAM, RAM, CPU)
- Error counts by type

**Dashboard**: Web UI with WebSocket real-time updates

### 3.5 Recovery Workflow

```
1. DETECT: Supervisor detects crash/timeout
2. VALIDATE: Check checkpoint integrity (checksum + version)
3. RESTORE: Load state, memory, queue
4. RECONCILE: Verify task queue with execution log
5. RESUME: Continue from last known good state
6. VERIFY: Confirm successful recovery
```

---

## Phase 4: Safety & Evolution Framework (P1)

### 4.1 Safety Architecture (4-Layer Defense)

| Layer | Component | Purpose |
|-------|-----------|---------|
| Layer 1 | Containment | Filesystem/network boundaries |
| Layer 2 | Oversight | Human-in-the-loop, audit logging |
| Layer 3 | Control | Capability levels, rate limiting |
| Layer 4 | Kill Switch | Emergency shutdown, rollback |

### 4.2 Fitness Functions (Composite Scoring)

```
Total Score = 
  Code Quality (25%) +
  Test Coverage (25%) +
  Safety Score (20%) +
  Performance (15%) +
  Maintainability (15%)
```

### 4.3 Evolutionary Algorithm

```
Checkpoint → Generate Variants → Evaluate Fitness → Select Best
                                              ↓
Deploy ← Human Approval ← Validate ← Check Boundaries
```

### 4.4 Anomaly Detection

**Behavioral Anomalies**:
- File access pattern changes
- Unusual modification rates
- Unexpected command execution

**Fitness Anomalies**:
- Sudden fitness drops
- Stagnation (no improvement)
- Oscillation (cycling changes)
- Unexpected improvements (potential bugs)

### 4.5 Human Oversight

**Approval Levels**:
| Level | Requires Approval For |
|-------|----------------------|
| Strict | All modifications |
| Moderate | Critical changes, >100 lines |
| Minimal | Only safety-critical |

---

## Implementation Roadmap

### Week 1-2: Memory Architecture
- [ ] Implement hierarchical memory system
- [ ] Create token budget allocator
- [ ] Build self-reference system
- [ ] Integrate with existing cognitive module

### Week 3-4: Self-Modification Core
- [ ] Implement code analysis engine
- [ ] Build AST transformer
- [ ] Create compilation manager
- [ ] Add safety/rollback system

### Week 5-6: Hot-Reload & Integration
- [ ] Implement hot-reload system
- [ ] Add differential testing
- [ ] Integrate with agent loop
- [ ] Create self-improvement orchestrator

### Week 7-8: Long-Running Infrastructure
- [ ] Build checkpointing system
- [ ] Implement process supervision
- [ ] Add resource management
- [ ] Create progress tracking dashboard

### Week 9-10: Safety & Evolution
- [ ] Implement safety framework
- [ ] Create fitness functions
- [ ] Add anomaly detection
- [ ] Build human oversight UI

### Week 11-12: Testing & Hardening
- [ ] End-to-end RSI testing
- [ ] Safety boundary testing
- [ ] Performance optimization
- [ ] Documentation

---

## Key Dependencies to Add

```toml
[dependencies]
# Self-modification
syn = { version = "2.0", features = ["full", "visit", "visit-mut"] }
quote = "1.0"
proc-macro2 = "1.0"
prettyplease = "0.2"
libloading = "0.8"

# Infrastructure
bincode = "1.3"
rusqlite = { version = "0.30", features = ["bundled", "chrono"] }
zstd = "0.13"
nvml-wrapper = "0.10"
sysinfo = "0.30"
notify = "6.1"
git2 = "0.18"

# Monitoring
metrics = "0.22"
axum = "0.7"
tokio-tungstenite = "0.21"

# Safety
crc32fast = "1.3"
thiserror = "1.0"
```

---

## Success Metrics

| Metric | Current | Target |
|--------|---------|--------|
| RSI Level | 2/10 | 8/10 |
| Autonomous Runtime | Hours | Days |
| Context Utilization | ~8K | 900K+ |
| Self-Modification Success Rate | N/A | >70% |
| Rollback Rate | N/A | <20% |
| Human Interventions/Day | Constant | <5 |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Runaway self-improvement | Capability limits, fitness bounds, kill switch |
| Catastrophic self-modification | Git checkpoints, protected files, rollback |
| Resource exhaustion | Auto-quantization, resource monitoring |
| State corruption | Checksum validation, differential checkpoints |
| Degenerate evolution | Fitness tracking, anomaly detection |

---

## Conclusion

Selfware has a **strong foundation** for RSI with its existing cognitive architecture, tool system, and safety guardrails. The critical missing pieces are:

1. **Memory architecture** to utilize 1M context for self-reference
2. **Self-modification system** to genuinely improve source code
3. **Long-running infrastructure** for multi-day autonomy
4. **Safety framework** to bound and guide evolution

With these additions, Selfware can evolve from a sophisticated agent harness into a true recursive self-improving system that can autonomously enhance its capabilities over days of operation.

**Estimated Timeline**: 12 weeks to MVP RSI system
**Estimated Effort**: 2-3 senior Rust engineers
**Risk Level**: Medium (well-understood technical challenges)
