#!/bin/bash
# Quick 32 concurrent test - simpler version

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-122b-concurrency64.toml"
RESULTS="/tmp/quick_32_$(date +%s)"
mkdir -p "$RESULTS"

echo "Quick 32 Concurrent Agent Test"
echo "=============================="
echo ""

# Simple task
TASK="List the top 5 files by line count in src/ and briefly describe what each does"

# Launch 32 agents with stagger
echo "Launching 32 agents..."
for i in {1..32}; do
    timeout 120 $SELFWARE -c "$CONFIG" -y -p "$TASK" -C /home/ivo/selfware > "$RESULTS/agent_$i.log" 2>&1 &
    echo -n "."
    sleep 0.3
done
echo ""
echo "Launched. Waiting..."

# Wait for completion
for i in {1..60}; do
    RUNNING=$(pgrep -f "selfware.*-c.*concurrency64" | wc -l)
    COMPLETED=$(ls "$RESULTS"/*.log 2>/dev/null | wc -l)
    echo "Progress: $COMPLETED/32 completed, $RUNNING running"
    
    if [ $RUNNING -eq 0 ] && [ $COMPLETED -eq 32 ]; then
        break
    fi
    sleep 5
done

# Results
echo ""
echo "=============================="
echo "Results:"
SUCCESS=$(grep -l "Task completed" "$RESULTS"/*.log 2>/dev/null | wc -l)
FAILED=$(grep -l "Failed\|ERROR\|panic" "$RESULTS"/*.log 2>/dev/null | wc -l)
echo "Success: $SUCCESS"
echo "Failed: $FAILED"
echo "Results in: $RESULTS"
