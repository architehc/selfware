#!/bin/bash
# Spawn agents to analyze SWL design fit in codebase

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-122b-concurrency64.toml"
RESULTS_DIR="/tmp/swl_analysis_$(date +%s)"
mkdir -p "$RESULTS_DIR"

# Task 1: Architecture - Where does SWL fit?
ARCHITECTURE_TASK="Review the Selfware codebase and determine where the Selfware Workflow Language (SWL) fits best.

SWL is a declarative-imperative hybrid language for agent orchestration with:
1. YAML-based declarative layer (agents, workflows, guardrails)
2. Embedded code blocks for imperative logic (Rust/Python)
3. Built-in real-time telemetry and observability
4. Command center dashboard integration

Analyze these integration points:
1. Should SWL be a separate compiler (swlc) or embedded in the existing Rust codebase?
2. How would SWL relate to the existing config system (src/config/)?
3. Where would the command center telemetry integrate (src/ui/tui/? new module?)
4. How would SWL workflows map to the existing agent/orchestration modules?

Review these files:
- src/config/mod.rs and submodules (newly split config system)
- src/orchestration/workflow_dsl/ (existing workflow DSL)
- src/ui/tui/ (existing TUI for potential dashboard integration)
- src/agent/execution.rs and task_runner.rs (agent execution)

Provide recommendations for:
- Module structure (where to add swl/ directory)
- Integration approach (plugin vs core)
- Reuse opportunities (what existing code can SWL leverage)
"

# Task 2: Implementation - Parser/Compiler design
IMPLEMENTATION_TASK="Design the SWL parser and compiler architecture.

SWL has these syntax requirements:
1. Declarative YAML-like structure for agents, workflows, state
2. Embedded code blocks with language annotation (language: rust/python)
3. Template expressions ${...} for variable interpolation
4. Type annotations for state schemas

Review the existing codebase for:
- src/orchestration/workflow_dsl/ (existing lexer/parser - can it be extended?)
- Cargo.toml dependencies (what parser crates are available?)

Design a parser architecture that:
- Parses declarative layer efficiently
- Handles embedded code blocks without breaking YAML parsing
- Supports type checking between declarative and imperative parts
- Compiles to Rust data structures that integrate with existing Agent types

Output specific recommendations:
- Parser crate choice (serde_yaml + custom, nom, pest, etc.)
- AST structure design
- Compilation target (should SWL compile to Rust code or be interpreted?)
"

# Task 3: Observability - Telemetry integration
OBSERVABILITY_TASK="Design the telemetry and command center integration.

The command center needs:
1. Real-time metrics streaming (WebSocket)
2. Time-series data storage (ring buffer)
3. TUI dashboard (ratatui)
4. Integration with existing Agent execution

Review:
- src/ui/tui/ (existing TUI infrastructure)
- Any existing telemetry/metrics code
- src/agent/execution.rs (where to instrument)

Design:
- How to instrument agent execution without performance impact
- Telemetry event structure
- Dashboard server architecture (axum/embedded in CLI?)
- Storage strategy (in-memory vs persistent)

Recommend:
- Where to add telemetry/ module
- How to make telemetry collection zero-cost when dashboard not running
- Integration with existing TUI vs new web-based dashboard
"

# Task 4: Existing workflow DSL - Migration or coexistence
WORKFLOW_TASK="Analyze the existing workflow_dsl implementation.

Review src/orchestration/workflow_dsl/:
- lexer.rs
- parser.rs  
- executor.rs

Determine:
1. Is the existing DSL sufficient for SWL or should it be replaced?
2. Can SWL leverage the existing lexer/parser architecture?
3. What are the feature gaps between existing DSL and SWL requirements?
4. Migration strategy if replacing (backward compatibility?)

Compare features:
- Existing: YAML workflows, steps, parallel execution
- SWL adds: Typed state, embedded code, observability, guardrails as code

Recommend:
- Evolution strategy (extend vs replace)
- Code reuse opportunities
"

echo "Spawning 4 agents to analyze SWL integration..."
echo "Results will be in: $RESULTS_DIR"
echo ""

# Launch all 4 agents concurrently
run_agent() {
    local id=$1
    local task="$2"
    local output="$RESULTS_DIR/agent_$id.log"
    
    echo "[Agent $id] Starting analysis..."
    timeout 300 $SELFWARE -c "$CONFIG" -y -p "$task" \
        -C /home/ivo/selfware > "$output" 2>&1 &
    echo $!
}

# Start all agents
PIDS=()
PIDS+=($(run_agent 01 "$ARCHITECTURE_TASK"))
PIDS+=($(run_agent 02 "$IMPLEMENTATION_TASK"))
PIDS+=($(run_agent 03 "$OBSERVABILITY_TASK"))
PIDS+=($(run_agent 04 "$WORKFLOW_TASK"))

echo "Agents launched with PIDs: ${PIDS[@]}"
echo ""
echo "Monitoring progress..."

# Monitor
for i in {1..30}; do
    sleep 10
    
    RUNNING=0
    COMPLETED=0
    
    for pid in "${PIDS[@]}"; do
        if kill -0 $pid 2>/dev/null; then
            ((RUNNING++))
        else
            ((COMPLETED++))
        fi
    done
    
    echo "Status: $COMPLETED/4 completed, $RUNNING running"
    
    if [ $RUNNING -eq 0 ]; then
        break
    fi
done

echo ""
echo "=== ANALYSIS COMPLETE ==="
echo ""

# Collect results
echo "Agent 01 (Architecture) findings:"
grep -A5 "Task completed\|RECOMMENDATION\|Module structure" "$RESULTS_DIR/agent_01.log" 2>/dev/null | tail -20 || echo "(check $RESULTS_DIR/agent_01.log)"
echo ""

echo "Agent 02 (Implementation) findings:"
grep -A5 "Task completed\|RECOMMENDATION\|Parser" "$RESULTS_DIR/agent_02.log" 2>/dev/null | tail -20 || echo "(check $RESULTS_DIR/agent_02.log)"
echo ""

echo "Agent 03 (Observability) findings:"
grep -A5 "Task completed\|RECOMMENDATION\|Telemetry" "$RESULTS_DIR/agent_03.log" 2>/dev/null | tail -20 || echo "(check $RESULTS_DIR/agent_03.log)"
echo ""

echo "Agent 04 (Workflow DSL) findings:"
grep -A5 "Task completed\|RECOMMENDATION\|Migration" "$RESULTS_DIR/agent_04.log" 2>/dev/null | tail -20 || echo "(check $RESULTS_DIR/agent_04.log)"
echo ""

echo "Full logs available in: $RESULTS_DIR"
