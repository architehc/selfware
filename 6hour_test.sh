#!/bin/bash
# 6-Hour Continuous Test - Max GPU Utilization
# Tests 122B endpoint continuously while debugging 27B

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG_122B="/home/ivo/selfware/selfware-evolve-122b.toml"
RESULTS="/tmp/6hour_test_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS"

TEMPLATE_DIR="/home/ivo/selfware/system_tests/projecte2e/templates"

# Test tasks of varying difficulty
declare -a TASKS=(
    "easy_calculator:Implement a calculator"
    "easy_string_ops:Implement string operations"
    "medium_json_merge:Implement JSON merge"
    "medium_bitset:Implement bitset operations"
    "viz_sparkline:Implement sparkline"
    "viz_progress_bar:Implement progress bar"
    "viz_histogram:Implement histogram"
    "codegen_task_runner:Implement task runner"
)

START_TIME=$(date +%s)
END_TIME=$((START_TIME + 21600))  # 6 hours = 21600 seconds

ITER=0
PASS_122B=0
FAIL_122B=0
PASS_27B=0
FAIL_27B=0

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  6-HOUR CONTINUOUS GPU TEST                                   ║"
echo "║  Start: $(date)                                               ║"
echo "║  End: $(date -d '+6 hours')                                   ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Results: $RESULTS"
echo ""

while [ $(date +%s) -lt $END_TIME ]; do
    ((ITER++))
    
    # Pick random task
    TASK_IDX=$((RANDOM % ${#TASKS[@]}))
    IFS=':' read -r TASK_NAME TASK_PROMPT <<< "${TASKS[$TASK_IDX]}"
    
    echo "=== Iteration $ITER - $(date +%H:%M:%S) - Task: $TASK_NAME ==="
    
    # Run on 122B
    timeout 120 $SELFWARE -c "$CONFIG_122B" -y -p "$TASK_PROMPT" \
        -C "$TEMPLATE_DIR/$TASK_NAME" > "$RESULTS/122b_${ITER}.log" 2>&1 &
    PID_122B=$!
    
    # Run on 27B (for debugging)
    timeout 120 $SELFWARE -y -p "$TASK_PROMPT" \
        -C "$TEMPLATE_DIR/$TASK_NAME" > "$RESULTS/27b_${ITER}.log" 2>&1 &
    PID_27B=$!
    
    wait $PID_122B
    if grep -q "✅ Task completed" "$RESULTS/122b_${ITER}.log"; then
        ((PASS_122B++))
        STATUS_122B="✓"
    else
        ((FAIL_122B++))
        STATUS_122B="✗"
    fi
    
    wait $PID_27B
    if grep -q "✅ Task completed" "$RESULTS/27b_${ITER}.log"; then
        ((PASS_27B++))
        STATUS_27B="✓"
    else
        ((FAIL_27B++))
        STATUS_27B="✗"
    fi
    
    echo "  122B: $STATUS_122B | 27B: $STATUS_27B"
    echo "  Stats - 122B: $PASS_122B/$((PASS_122B+FAIL_122B)) | 27B: $PASS_27B/$((PASS_27B+FAIL_27B))"
    
    # Save stats every 10 iterations
    if [ $((ITER % 10)) -eq 0 ]; then
        cat > "$RESULTS/stats.json" << EOF
{
  "iteration": $ITER,
  "timestamp": "$(date -Iseconds)",
  "122b": {"passed": $PASS_122B, "failed": $FAIL_122B},
  "27b": {"passed": $PASS_27B, "failed": $FAIL_27B}
}
EOF
    fi
    
    # Brief pause to prevent overwhelming
    sleep 1
done

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "6-HOUR TEST COMPLETE"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "122B Endpoint: $PASS_122B passed, $FAIL_122B failed"
echo "27B Endpoint:  $PASS_27B passed, $FAIL_27B failed"
echo "Total iterations: $ITER"
echo ""
echo "Results saved to: $RESULTS"
