# Long-Running Mega Project Test Plan

## Overview

This document outlines a comprehensive, multi-hour test plan for validating Selfware's agentic capabilities on large-scale, realistic software engineering projects. The test simulates extended development sessions to verify:

- Task persistence and recovery
- Memory management over time
- Checkpoint reliability
- Multi-agent coordination stability
- Resource efficiency
- Error handling and self-healing

## Test Duration

**Total Duration**: 4-8 hours (configurable)
**Recommended**: 6 hours for full validation

## Test Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Mega Project Test Architecture                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Phase 1 (1hr)      Phase 2 (2hr)      Phase 3 (2hr)      Phase 4   │
│  Bootstrap         → Development      → Refinement      → Finalize │
│                                                                      │
│  • Project setup    • Feature impl    • Bug fixes        • Polish  │
│  • Agent swarm init • Tests added     • Performance      • Docs    │
│  • Initial plan     • Integration     • Security audit   • Release │
│                                                                      │
│  Checkpoint every 10 minutes / 100 tool calls / 500K tokens         │
│  Auto-recovery on failures                                          │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Test Project Specifications

### Option A: Distributed Task Queue System
**Complexity**: High | **Lines of Code**: ~5,000-8,000 | **Files**: 25-40

Components:
- Async message broker with multiple protocols
- Worker pool with dynamic scaling
- Web dashboard for monitoring
- CLI management tool
- Integration tests suite

### Option B: Database Engine (Simplified)
**Complexity**: Very High | **Lines of Code**: ~8,000-12,000 | **Files**: 40-60

Components:
- B-tree storage engine
- SQL parser and query planner
- Transaction manager
- WAL (Write-Ahead Logging)
- Network protocol handler

### Option C: Microservices Platform
**Complexity**: High | **Lines of Code**: ~6,000-10,000 | **Files**: 30-50

Components:
- Service discovery
- Load balancer
- Circuit breaker
- Distributed tracing
- Configuration management

## Test Phases

### Phase 1: Bootstrap & Architecture (60 minutes)

**Objectives**:
- Initialize project structure
- Design system architecture
- Set up CI/CD pipeline
- Create initial documentation

**Agent Assignment**:
| Agent | Role | Task |
|-------|------|------|
| Archie | Architect | System design, module boundaries |
| Cody-1 | Coder | Project setup, build system |
| Cody-2 | Coder | Core abstractions, interfaces |
| Doc | Documenter | README, API docs structure |

**Success Criteria**:
- [ ] Project compiles successfully
- [ ] All modules have defined interfaces
- [ ] CI pipeline passes
- [ ] Architecture document complete

**Checkpoints**: Every 15 minutes

---

### Phase 2: Core Development (120 minutes)

**Objectives**:
- Implement core functionality
- Add comprehensive tests
- Integrate components
- Performance baseline

**Agent Assignment**:
| Agent | Role | Task |
|-------|------|------|
| Cody-1 | Coder | Core engine implementation |
| Cody-2 | Coder | Protocol handlers |
| Tessa-1 | Tester | Unit tests, edge cases |
| Tessa-2 | Tester | Integration tests |
| DevOps | DevOps | Docker setup, deployment scripts |
| Security | Security | Threat modeling, initial audit |

**Sub-phases**:

#### Hour 2.1: Core Implementation (60 min)
- Implement main business logic
- Add error handling
- Create configuration system

#### Hour 2.2: Testing & Integration (60 min)
- Write unit tests (target: 80% coverage)
- Integration tests
- Performance benchmarks

**Success Criteria**:
- [ ] Core features functional
- [ ] Test coverage ≥ 70%
- [ ] Integration tests pass
- [ ] No critical security issues

**Checkpoints**: Every 20 minutes

---

### Phase 3: Refinement & Hardening (120 minutes)

**Objectives**:
- Bug fixes from testing
- Performance optimization
- Security hardening
- Documentation completion

**Agent Assignment**:
| Agent | Role | Task |
|-------|------|------|
| Reviewer | Reviewer | Code review, quality gates |
| Cody-3 | Coder | Bug fixes, optimizations |
| Security | Security | Full security audit |
| Performance | Performance | Profiling, optimization |
| Tessa-3 | Tester | E2E tests, load testing |

**Sub-phases**:

#### Hour 3.1: Bug Fixes & Review (60 min)
- Address all critical bugs
- Code review all modules
- Refactor where needed

#### Hour 3.2: Performance & Security (60 min)
- Profile and optimize hotspots
- Security audit fixes
- Load testing

**Success Criteria**:
- [ ] All critical/high bugs fixed
- [ ] Performance targets met
- [ ] Security scan clean
- [ ] E2E tests pass

**Checkpoints**: Every 20 minutes

---

### Phase 4: Finalization (60 minutes)

**Objectives**:
- Final polish
- Documentation completion
- Release preparation
- Final validation

**Agent Assignment**:
| Agent | Role | Task |
|-------|------|------|
| Doc | Documenter | Complete all documentation |
| DevOps | DevOps | Release automation |
| Reviewer | Reviewer | Final code review |
| Cody-Final | Coder | Final fixes |

**Success Criteria**:
- [ ] Documentation 100% complete
- [ ] All tests passing
- [ ] Release artifacts ready
- [ ] Final checkpoint validates

**Checkpoints**: Every 10 minutes

---

## Checkpoint Strategy

### Automatic Checkpoints

Triggered by:
1. **Time-based**: Every 10 minutes
2. **Tool call threshold**: Every 100 tool calls
3. **Token threshold**: Every 500K tokens consumed
4. **Phase completion**: At end of each phase

### Checkpoint Contents

```rust
pub struct SessionCheckpoint {
    pub timestamp: u64,
    pub session_id: String,
    pub phase: TestPhase,
    pub agent_states: Vec<AgentState>,
    pub working_memory: HashMap<String, String>,
    pub file_system_state: FileSystemSnapshot,
    pub test_results: TestResults,
    pub metrics: SessionMetrics,
    pub git_commit: Option<String>,
}
```

### Recovery Scenarios

| Failure Point | Recovery Action | Expected Time |
|--------------|-----------------|---------------|
| Tool timeout | Retry with backoff, escalate to manual | 30s-5min |
| LLM error | Switch model, retry, log error | 10s-2min |
| Memory pressure | Compress context, checkpoint, continue | 1-2min |
| Agent deadlock | Vote resolution, human intervention | 1-5min |
| System crash | Restore from last checkpoint | 30s-1min |

## Monitoring & Observability

### Real-time Metrics

```rust
pub struct SessionMetrics {
    // Time
    pub elapsed_seconds: u64,
    pub estimated_remaining: u64,
    
    // Tokens
    pub total_tokens_consumed: u64,
    pub tokens_per_minute: f64,
    
    // Progress
    pub tasks_completed: usize,
    pub tasks_remaining: usize,
    pub test_pass_rate: f64,
    
    // Code
    pub lines_of_code: usize,
    pub test_coverage: f64,
    pub files_modified: usize,
    
    // Agents
    pub agent_utilization: Vec<AgentUtilization>,
    pub consensus_votes_cast: usize,
    
    // Health
    pub memory_usage_mb: usize,
    pub checkpoint_count: usize,
    pub errors_encountered: usize,
}
```

### Dashboard Updates

Every 30 seconds, update:
- Progress percentage
- Current agent activities
- Test pass/fail counts
- Token consumption rate
- Estimated completion time

## Resource Limits

### Per-Agent Limits

| Resource | Limit | Action on Exceed |
|----------|-------|------------------|
| Tokens/hour | 200K | Throttle, queue tasks |
| Tool calls/hour | 500 | Batch operations |
| Memory context | 128K | Compress, summarize |
| Idle time | 10 min | Reassign tasks |

### System-wide Limits

| Resource | Limit |
|----------|-------|
| Total tokens | 5M |
| Parallel agents | 8 max |
| Disk space | 2GB |
| Test runtime | 30 min max |

## Error Handling Strategy

### Classification

```rust
pub enum ErrorSeverity {
    Transient,      // Retry immediately
    Recoverable,    // Retry with backoff
    Degraded,       // Continue with reduced functionality
    Critical,       // Pause for intervention
    Fatal,          // Stop session
}
```

### Auto-Recovery Actions

| Error Type | Severity | Action |
|------------|----------|--------|
| Network timeout | Transient | Retry 3x, then backoff |
| LLM rate limit | Recoverable | Wait, retry, switch model |
| Parse failure | Recoverable | Retry with different prompt |
| Test failure | Degraded | Log, continue, flag for review |
| Disk full | Critical | Pause, alert, cleanup |
| Consensus deadlock | Critical | Escalate to human |
| Data corruption | Fatal | Stop, restore checkpoint |

## Success Criteria

### Must Have (Critical)

- [ ] Session completes without fatal errors
- [ ] All checkpoints successfully created
- [ ] Recovery from at least 1 simulated failure
- [ ] Final code compiles and passes tests
- [ ] Documentation complete

### Should Have (Important)

- [ ] Test coverage ≥ 80%
- [ ] All critical bugs fixed
- [ ] Performance within 20% of target
- [ ] Security scan with no high/critical issues
- [ ] Auto-recovery from 3+ error scenarios

### Nice to Have (Bonus)

- [ ] Test coverage ≥ 90%
- [ ] Performance exceeds target
- [ ] Zero manual interventions
- [ ] All agents maintain trust score > 0.8
- [ ] Session completes 20% under budget

## Test Execution Script

```bash
#!/bin/bash
# long_running_test.sh

set -e

SESSION_ID=$(uuidgen)
PROJECT_TYPE=${1:-"task_queue"}
DURATION_HOURS=${2:-6}
AGENT_COUNT=${3:-6}

echo "Starting Mega Project Test"
echo "Session: $SESSION_ID"
echo "Project: $PROJECT_TYPE"
echo "Duration: ${DURATION_HOURS}h"
echo "Agents: $AGENT_COUNT"

# Setup
export SELFWARE_SESSION_ID=$SESSION_ID
export SELFWARE_CHECKPOINT_INTERVAL=600
export SELFWARE_AUTO_RECOVERY=true
export SELFWARE_MAX_DURATION=$((DURATION_HOURS * 3600))

# Create project directory
mkdir -p "test_runs/$SESSION_ID"
cd "test_runs/$SESSION_ID"

# Initialize project based on type
case $PROJECT_TYPE in
    task_queue)
        selfware run "Create a distributed task queue system with Redis-compatible protocol, async workers, and web dashboard"
        ;;
    database)
        selfware run "Create a simplified SQLite-compatible database engine with B-tree storage, transactions, and WAL"
        ;;
    microservices)
        selfware run "Create a microservices platform with service discovery, load balancing, and distributed tracing"
        ;;
esac

# Monitor loop
for hour in $(seq 1 $DURATION_HOURS); do
    echo "Hour $hour of $DURATION_HOURS"
    
    # Checkpoint validation
    selfware checkpoint verify
    
    # Metrics snapshot
    selfware metrics export "metrics_hour_${hour}.json"
    
    # Health check
    if ! selfware health check; then
        echo "Health check failed, attempting recovery..."
        selfware recover
    fi
    
    # Progress update
    selfware status --format=json > "status_hour_${hour}.json"
    
    sleep 3600
done

# Final validation
selfware test --suite=full
selfware docs verify
selfware security scan

# Generate report
selfware report generate --output="final_report.html"

echo "Test complete: $SESSION_ID"
```

## Post-Test Analysis

### Metrics to Analyze

1. **Efficiency Metrics**
   - Tokens per line of code
   - Tool calls per task
   - Time to completion
   - Checkpoint frequency

2. **Quality Metrics**
   - Bug density
   - Test coverage trend
   - Code review findings
   - Documentation completeness

3. **System Metrics**
   - Recovery success rate
   - Agent utilization
   - Consensus efficiency
   - Memory pressure points

### Report Generation

```rust
pub struct TestReport {
    pub session_id: String,
    pub project_type: String,
    pub duration_seconds: u64,
    pub final_status: SessionStatus,
    pub metrics: SessionMetrics,
    pub checkpoint_history: Vec<CheckpointSummary>,
    pub error_log: Vec<ErrorEvent>,
    pub agent_performance: Vec<AgentReport>,
    pub code_quality: CodeQualityReport,
    pub recommendations: Vec<String>,
}
```

## Continuous Improvement

After each mega test:

1. **Review Checkpoints**: Identify slow points
2. **Analyze Failures**: Categorize and prioritize fixes
3. **Tune Parameters**: Adjust token limits, timeouts
4. **Update Prompts**: Improve system prompts based on results
5. **Enhance Recovery**: Add new recovery strategies

## Schedule Recommendation

| Frequency | Test Type | Duration | Purpose |
|-----------|-----------|----------|---------|
| Weekly | Mini (simplified) | 1 hour | Regression testing |
| Monthly | Standard | 4 hours | Feature validation |
| Quarterly | Mega (full) | 8 hours | Comprehensive validation |
| Release | Mega + stress | 12 hours | Release readiness |

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| LLM API downtime | Local fallback models |
| Resource exhaustion | Monitoring + limits |
| Infinite loops | Timeout guards |
| Data loss | Frequent checkpoints |
| Cost overruns | Token budgets + alerts |
| Scope creep | Strict phase gates |

---

## Appendix A: Sample Project Definition

### Task Queue System Detailed Spec

```yaml
name: "RedQueue"
description: "Redis-compatible distributed task queue"
requirements:
  protocols:
    - RESP (Redis Serialization Protocol)
    - HTTP REST API
    - gRPC (optional)
  
  features:
    - Priority queues (0-255)
    - Delayed jobs
    - Scheduled jobs
    - Dead letter queue
    - Job retry with backoff
    - Worker heartbeats
    - Horizontal scaling
  
  performance:
    - 10K jobs/sec throughput
    - <10ms latency p99
    - 99.99% availability
  
  components:
    - server: TCP/HTTP API server
    - worker: Job processor
    - scheduler: Delayed job handler
    - web: Dashboard UI
    - cli: Management tool
```

## Appendix B: Checkpoint Validation Checklist

```markdown
## Checkpoint Validation

- [ ] Git repository clean state
- [ ] All files saved
- [ ] Tests compile
- [ ] No uncommitted critical changes
- [ ] Agent states serialized
- [ ] Memory snapshot complete
- [ ] Metrics logged
- [ ] No active tool calls
- [ ] Consensus votes resolved
- [ ] Error queue empty
```

## Appendix C: Troubleshooting Guide

### Common Issues

**Issue**: Agent deadlock on decision
**Solution**: Force vote with priority override, log for review

**Issue**: Token budget exhaustion
**Solution**: Compress context, summarize completed work

**Issue**: Test suite timeout
**Solution**: Run tests in parallel, reduce test data size

**Issue**: Checkpoint corruption
**Solution**: Restore from previous checkpoint, log incident

**Issue**: Memory leak in agent
**Solution**: Restart agent, transfer context, investigate

---

*This test plan is versioned. Update when test infrastructure changes.*
