#!/usr/bin/env bash
#
# Quick 5-minute test to verify monitoring works before 2-hour run

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SESSION_ID="quicktest-$(date +%Y%m%d-%H%M%S)"
TEST_DIR="$PROJECT_ROOT/test_runs/$SESSION_ID"
mkdir -p "$TEST_DIR"/logs "$TEST_DIR"/metrics "$TEST_DIR"/checkpoints

echo "Quick Monitor Test - Session: $SESSION_ID"
echo "Test Directory: $TEST_DIR"
echo ""

# Create config
cat > "$TEST_DIR/config.json" << EOF
{
  "session_id": "$SESSION_ID",
  "project_name": "quicktest",
  "scenario": "test",
  "duration_hours": 0.1,
  "monitor_interval_seconds": 5,
  "started_at": "$(date -Iseconds)"
}
EOF

METRICS_FILE="$TEST_DIR/metrics/metrics.jsonl"

# Simulate a test process
echo "Simulating test process..."
(
    for i in {1..60}; do
        sleep 1
        echo "Working... iteration $i" >> "$TEST_DIR/logs/main.log"
        
        # Create occasional checkpoints
        if [ $((i % 10)) -eq 0 ]; then
            echo "{\"checkpoint\": $i, \"time\": \"$(date -Iseconds)\"}" > "$TEST_DIR/checkpoints/cp_$i.json"
        fi
    done
    echo "completed" > "$TEST_DIR/status"
) &
TEST_PID=$!
echo $TEST_PID > "$TEST_DIR/.pid"

# Simulate metrics collection
echo "Simulating metrics collection..."
(
    start_time=$(date +%s)
    iteration=0
    while kill -0 $TEST_PID 2>/dev/null; do
        sleep 5
        iteration=$((iteration + 1))
        current_time=$(date +%s)
        elapsed=$((current_time - start_time))
        
        checkpoints=$(find "$TEST_DIR/checkpoints" -name "*.json" 2>/dev/null | wc -l)
        log_lines=$(wc -l < "$TEST_DIR/logs/main.log" 2>/dev/null || echo "0")
        
        cat >> "$METRICS_FILE" << EOF
{"timestamp": "$(date -Iseconds)", "iteration": $iteration, "elapsed_seconds": $elapsed, "checkpoints": $checkpoints, "log_lines": $log_lines}
EOF
        echo "[$iteration] Elapsed: ${elapsed}s, Checkpoints: $checkpoints, Log lines: $log_lines"
    done
) &
MONITOR_PID=$!

echo ""
echo "Test PID: $TEST_PID"
echo "Monitor PID: $MONITOR_PID"
echo ""
echo "Running for ~60 seconds (press Ctrl+C to stop)..."
echo ""

# Wait for completion
wait $TEST_PID 2>/dev/null || true
sleep 2

# Show results
echo ""
echo "=== Test Complete ==="
echo ""
echo "Metrics collected:"
cat "$METRICS_FILE" | wc -l
echo ""
echo "Sample metrics:"
head -3 "$METRICS_FILE"
echo "..."
tail -3 "$METRICS_FILE"
echo ""
echo "Checkpoints created:"
ls -la "$TEST_DIR/checkpoints/"
echo ""
echo "Log file:"
tail -5 "$TEST_DIR/logs/main.log"
echo ""
echo "Test directory: $TEST_DIR"
