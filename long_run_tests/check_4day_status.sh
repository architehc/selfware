#!/bin/bash
# Quick status check for 4-day test

RESULTS_DIR=$(ls -td /home/ivo/selfware/long_run_tests/4day_results_* 2>/dev/null | head -1)

if [ -z "$RESULTS_DIR" ]; then
    echo "No active 4-day test found"
    exit 1
fi

echo "4-Day Test Status"
echo "================="
echo "Results: $RESULTS_DIR"
echo ""

# Check orchestrator log
echo "Recent Activity:"
tail -5 "$RESULTS_DIR/orchestrator.log" 2>/dev/null || echo "No log entries yet"
echo ""

# Count rounds per agent
echo "Agent Progress:"
for agent_dir in "$RESULTS_DIR"/agents/agent_*/; do
    if [ -d "$agent_dir" ]; then
        agent_id=$(basename "$agent_dir" | sed 's/agent_//')
        rounds=$(find "$agent_dir" -name "metrics.json" | wc -l)
        echo "  Agent $agent_id: $rounds rounds completed"
    fi
done
echo ""

# Show compilation stats if available
if [ -d "$RESULTS_DIR/agents" ]; then
    total_metrics=$(find "$RESULTS_DIR/agents" -name "metrics.json" 2>/dev/null | wc -l)
    if [ $total_metrics -gt 0 ]; then
        echo "Total metrics collected: $total_metrics"
        
        successful=$(grep -l "ALL_TESTS_PASS\|SOME_TESTS_FAIL\|COMPILES" "$RESULTS_DIR"/agents/agent_*/round_*/metrics.json 2>/dev/null | wc -l)
        echo "Successful compilations: $successful"
    fi
fi
