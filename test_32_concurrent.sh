#!/bin/bash
# 32 Concurrent Selfware Agents Test
# Max out the 2x RTX 4090 endpoint

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     32 CONCURRENT SELFWARE AGENTS TEST                        ║"
echo "║     2x RTX 4090 | Qwen3.5-27B-FP8 | Max Load Test            ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

TEST_DIR="/tmp/32_concurrent_test_$(date +%s)"
mkdir -p "$TEST_DIR"

echo "📁 Test Directory: $TEST_DIR"
echo "🎯 Target: 32 parallel selfware agents"
echo "⏱️  Max Duration: 5 minutes per agent"
echo ""

# Different tasks for variety
TASKS=(
  "Create a simple HTML button with hover effect"
  "Write a CSS animation for a loading spinner"
  "Create a dark-themed card component"
  "Write JavaScript for a toggle switch"
  "Create a responsive navigation bar"
  "Write CSS for a gradient text effect"
  "Create a simple modal dialog"
  "Write JavaScript for form validation"
  "Create a progress bar component"
  "Write CSS for a glassmorphism effect"
  "Create a tooltip component"
  "Write JavaScript for a countdown timer"
  "Create a toggle button with animation"
  "Write CSS for a neon glow effect"
  "Create a simple accordion menu"
  "Write JavaScript for image lazy loading"
  "Create a star rating component"
  "Write CSS for a floating action button"
  "Create a breadcrumb navigation"
  "Write JavaScript for dark mode toggle"
  "Create a notification toast component"
  "Write CSS for a shimmer loading effect"
  "Create a tab component"
  "Write JavaScript for smooth scrolling"
  "Create a badge component"
  "Write CSS for a 3D card flip"
  "Create a search input with icon"
  "Write JavaScript for character counter"
  "Create a pagination component"
  "Write CSS for an animated underline"
  "Create a dropdown menu"
  "Write JavaScript for local storage helper"
)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Results tracking
SUCCESS_COUNT=0
FAIL_COUNT=0
START_TIME=$(date +%s)

# Progress display function
show_progress() {
  while true; do
    clear
    echo "╔════════════════════════════════════════════════════════════════╗"
    echo "║           32 CONCURRENT AGENTS - LIVE STATUS                  ║"
    echo "╠════════════════════════════════════════════════════════════════╣"
    
    COMPLETED=$(ls "$TEST_DIR"/agent_*.done 2>/dev/null | wc -l)
    FAILED=$(ls "$TEST_DIR"/agent_*.fail 2>/dev/null | wc -l)
    RUNNING=$((SUCCESS_COUNT - COMPLETED - FAILED))
    
    # Progress bar
    PCT=$((COMPLETED * 100 / 32))
    FILLED=$((PCT / 5))
    BAR=$(printf "%${FILLED}s" | tr ' ' '█')$(printf "%$((20 - FILLED))s" | tr ' ' '░')
    
    echo "║ Progress: [$BAR] $PCT%                           ║"
    echo "║                                                                ║"
    printf "║ ${GREEN}✅ Success: %2d${NC}  ${RED}❌ Failed: %2d${NC}  ${YELLOW}⏳ Pending: %2d${NC}              ║\n" $COMPLETED $FAILED $((32 - COMPLETED - FAILED))
    echo "║                                                                ║"
    
    # Check endpoint
    RUNNING_REQS=$(curl -s http://localhost:8000/metrics 2>/dev/null | grep "num_requests_running" | grep -v "#" | awk '{print $2}' | cut -d'.' -f1 || echo "0")
    THROUGHPUT=$(curl -s http://localhost:8000/metrics 2>/dev/null | grep "generation_throughput" | grep -v "#" | awk '{print $2}' | cut -d'.' -f1 || echo "0")
    
    printf "║ 🖥️  Endpoint: Running=%s/32 | Throughput=%s tok/s          ║\n" "$RUNNING_REQS" "$THROUGHPUT"
    echo "║                                                                ║"
    
    # Recent completions
    echo "║ Recent Activity:                                               ║"
    for i in $(seq 28 32); do
      if [ -f "$TEST_DIR/agent_${i}.done" ]; then
        printf "║   ${GREEN}✓ Agent-%2d completed${NC}                                        ║\n" $i
      elif [ -f "$TEST_DIR/agent_${i}.fail" ]; then
        printf "║   ${RED}✗ Agent-%2d failed${NC}                                           ║\n" $i
      fi
    done
    
    ELAPSED=$(($(date +%s) - START_TIME))
    printf "║                                                                ║"
    printf "\n║ ⏱️  Elapsed: %02d:%02d                                           ║\n" $((ELAPSED/60)) $((ELAPSED%60))
    echo "╚════════════════════════════════════════════════════════════════╝"
    
    sleep 2
  done
}

# Launch progress display
show_progress &
PROGRESS_PID=$!

# Cleanup function
cleanup() {
  kill $PROGRESS_PID 2>/dev/null || true
}
trap cleanup EXIT

echo "🚀 Launching 32 agents..."

# Launch 32 agents
for i in {0..31}; do
  (
    AGENT_ID=$i
    TASK="${TASKS[$i]}"
    LOG_FILE="$TEST_DIR/agent_${AGENT_ID}.log"
    
    # Run selfware
    cd /home/ivo/selfware
    if SELFWARE_CONFIG=./selfware-stress-test.toml timeout 300 \
       ./target/release/selfware run "$TASK" -y > "$LOG_FILE" 2>&1; then
      touch "$TEST_DIR/agent_${AGENT_ID}.done"
    else
      touch "$TEST_DIR/agent_${AGENT_ID}.fail"
    fi
  ) &
  
  # Small delay to prevent thundering herd
  sleep 0.5
done

echo "All 32 agents launched!"
echo ""

# Wait for all to complete
echo "⏳ Waiting for completion (max 5 minutes)..."
wait

# Cleanup progress display
kill $PROGRESS_PID 2>/dev/null || true

# Calculate results
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

SUCCESS_COUNT=$(ls "$TEST_DIR"/agent_*.done 2>/dev/null | wc -l)
FAIL_COUNT=$(ls "$TEST_DIR"/agent_*.fail 2>/dev/null | wc -l)

# Final report
clear
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    TEST COMPLETE                               ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "📊 RESULTS:"
echo "   Duration: ${DURATION}s"
echo "   Success: $SUCCESS_COUNT/32"
echo "   Failed: $FAIL_COUNT/32"
echo "   Success Rate: $(( SUCCESS_COUNT * 100 / 32 ))%"
echo ""

# Check endpoint final status
echo "🖥️  ENDPOINT FINAL STATUS:"
curl -s http://localhost:8000/metrics 2>/dev/null | grep -E "(throughput|running|kv_cache)" | grep -v "#" | head -5 | while read line; do
  echo "   $line"
done

echo ""
echo "📁 LOGS: $TEST_DIR/agent_*.log"
echo ""

# Sample outputs
echo "📄 SAMPLE OUTPUTS:"
for i in 0 16 31; do
  if [ -f "$TEST_DIR/agent_${i}.log" ]; then
    echo ""
    echo "Agent-$i:"
    grep -E "(Created|Writing|Complete|Error)" "$TEST_DIR/agent_${i}.log" | tail -3 | head -1 || echo "  (See full log)"
  fi
done

echo ""
echo "═══════════════════════════════════════════════════════════════"
