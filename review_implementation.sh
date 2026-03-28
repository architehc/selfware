#!/bin/bash
# Review existing SWL/observability implementation

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-122b-concurrency64.toml"
RESULTS_DIR="/tmp/implementation_review_$(date +%s)"
mkdir -p "$RESULTS_DIR"

# Task 1: Review observability/telemetry.rs
TASK1="Review src/observability/telemetry.rs - the telemetry and observability implementation.

Analyze:
1. What telemetry features are implemented?
2. How is metrics collection structured?
3. Is there real-time streaming capability?
4. What integration points exist for a command center dashboard?
5. Compare against the SWL design requirements for observability

Provide specific findings:
- What's working well
- What's missing for SWL command center
- Integration recommendations
"

# Task 2: Review observability/dashboard.rs
TASK2="Review src/observability/dashboard.rs - the dashboard implementation.

Analyze:
1. What dashboard features exist?
2. Is there TUI or web interface?
3. What metrics are tracked (token usage, latency, etc)?
4. How does it compare to the SWL command center design?

Provide specific findings:
- Current capabilities
- Gaps vs SWL requirements
- Reuse opportunities
"

# Task 3: Review orchestration/workflow_dsl/
TASK3="Review src/orchestration/workflow_dsl/ - the existing workflow DSL implementation.

Files to check:
- lexer.rs
- parser.rs
- ast.rs
- runtime.rs

Analyze:
1. What workflow features exist?
2. Can this be extended for SWL or should it be replaced?
3. What's the migration path from workflow_dsl to SWL?

Provide specific findings:
- Feature comparison (workflow_dsl vs SWL)
- Code reuse opportunities
- Migration strategy
"

# Task 4: Review overall architecture fit
TASK4="Review the overall architecture for SWL integration.

Check:
- src/config/ (newly split config system)
- src/agent/execution.rs (agent execution)
- src/ui/tui/ (TUI infrastructure)

Analyze:
1. Where should SWL compiler integrate?
2. How would SWL workflows drive agent execution?
3. What's the best approach for command center integration?

Provide specific recommendations for implementation.
"

echo "Spawning 4 agents to review implementation..."
echo "Results: $RESULTS_DIR"
echo ""

run_agent() {
    local id=$1
    local task="$2"
    local output="$RESULTS_DIR/agent_$id.log"
    
    echo "[Agent $id] Starting..."
    timeout 300 $SELFWARE -c "$CONFIG" -y -p "$task" \
        -C /home/ivo/selfware > "$output" 2>&1 &
    echo $!
}

# Start all agents
PIDS=()
PIDS+=($(run_agent 01 "$TASK1"))
PIDS+=($(run_agent 02 "$TASK2"))
PIDS+=($(run_agent 03 "$TASK3"))
PIDS+=($(run_agent 04 "$TASK4"))

echo "Launched agents: ${PIDS[@]}"
echo ""
echo "Monitoring..."

# Monitor progress
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
    
    echo "Progress: $COMPLETED/4 complete, $RUNNING running"
    
    [ $RUNNING -eq 0 ] && break
done

echo ""
echo "=== REVIEW COMPLETE ==="
echo ""

# Show results
for i in 01 02 03 04; do
    echo "--- Agent $i Report ---"
    tail -30 "$RESULTS_DIR/agent_$i.log" 2>/dev/null || echo "(check $RESULTS_DIR/agent_$i.log)"
    echo ""
done

echo "Full logs: $RESULTS_DIR"
