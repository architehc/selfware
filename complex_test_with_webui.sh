#!/bin/bash
# High Complexity Test: Distributed Task Queue with Web UI Visual Feedback

set -e

mkdir -p /tmp/complex_test_taskqueue
cd /tmp/complex_test_taskqueue

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     HIGH COMPLEXITY TEST: Distributed Task Queue System       ║"
echo "║              with Real-time Web Dashboard                      ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Hardware: 2x RTX 4090 | Model: Qwen3.5-27B-FP8 | Agents: 9"
echo "Endpoint: http://localhost:8000/v1"
echo ""

# Define agent tasks with visual roles
AGENT_TASKS=(
  "🏗️ ARCHITECT: Design the system architecture for a distributed task queue. Create ARCHITECTURE.md with: 1) Component diagram (ASCII art), 2) Data flow, 3) API specifications, 4) Error handling strategy. Include a mermaid diagram showing the broker-workers-storage flow."
  "📨 BROKER: Implement src/broker.rs - a message broker that accepts tasks via TCP (port 8080) and Unix sockets, with priority queue support and load balancing."
  "⚙️ WORKER: Implement src/worker.rs - a worker pool that processes tasks concurrently with dynamic scaling (1-100 workers) based on queue depth."
  "💾 STORAGE: Implement src/storage.rs - SQLite persistence layer for task state with at-least-once delivery guarantees and retry tracking."
  "🌐 API: Implement src/api.rs - REST API for task management (/enqueue, /dequeue, /status) and WebSocket endpoint for real-time events."
  "🧪 TESTER: Create tests/integration_test.rs with comprehensive tests: concurrent enqueue, worker failover, persistence recovery, and load tests."
  "🔌 INTEGRATOR: Create src/main.rs and Cargo.toml that wires all components. Add graceful shutdown, signal handling, and component health checks."
  "📊 BENCHMARKER: Create benches/throughput.rs benchmark that measures: tasks/sec, latency percentiles, memory usage. Target: 10k tasks/sec."
  "🎨 WEB_DESIGNER: Create assets/dashboard.html - A beautiful real-time monitoring dashboard with: 1) Dark theme with neon accents, 2) Live task queue visualization with animated bars, 3) Worker status cards showing active/idle/busy, 4) Throughput graphs (using Chart.js or canvas), 5) Color-coded task status (green=success, red=failed, yellow=pending), 6) Auto-refreshing stats every second via WebSocket."
)

AGENT_EMOJIS=("🏗️" "📨" "⚙️" "💾" "🌐" "🧪" "🔌" "📊" "🎨")
AGENT_NAMES=("ARCHITECT" "BROKER" "WORKER" "STORAGE" "API" "TESTER" "INTEGRATOR" "BENCHMARKER" "WEB_DESIGNER")

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Progress display function
show_progress() {
  while true; do
    clear
    echo "╔════════════════════════════════════════════════════════════════╗"
    echo "║           TASK QUEUE SYSTEM - AGENT PROGRESS                   ║"
    echo "╠════════════════════════════════════════════════════════════════╣"
    
    for i in {0..8}; do
      LOG_FILE="/tmp/complex_test_agent_$i.log"
      NAME="${AGENT_NAMES[$i]}"
      EMOJI="${AGENT_EMOJIS[$i]}"
      
      if [ -f "$LOG_FILE" ]; then
        # Check status
        if grep -q "Done" "$LOG_FILE" 2>/dev/null; then
          STATUS="${GREEN}✅ COMPLETE${NC}"
          PROGRESS="████████████████████ 100%"
        elif grep -q "Error\|error\|FAILED" "$LOG_FILE" 2>/dev/null; then
          STATUS="${RED}❌ ERROR${NC}"
          PROGRESS="░░░░░░░░░░░░░░░░░░░░"
        elif grep -q "Executing\|Writing\|Creating" "$LOG_FILE" 2>/dev/null; then
          # Calculate rough progress based on iterations
          STEP=$(grep -o "\[[0-9]*/[0-9]*\]" "$LOG_FILE" | tail -1 | tr -d '[]' | cut -d'/' -f1 || echo "0")
          TOTAL=$(grep -o "\[[0-9]*/[0-9]*\]" "$LOG_FILE" | tail -1 | tr -d '[]' | cut -d'/' -f2 || echo "10")
          if [ -n "$STEP" ] && [ -n "$TOTAL" ] && [ "$TOTAL" -gt 0 ] 2>/dev/null; then
            PCT=$((STEP * 100 / TOTAL))
            FILLED=$((PCT / 5))
            EMPTY=$((20 - FILLED))
            PROGRESS=$(printf "%${FILLED}s" | tr ' ' '█')$(printf "%${EMPTY}s" | tr ' ' '░')
            STATUS="${YELLOW}⏳ ${PCT}%${NC}"
          else
            PROGRESS="██████░░░░░░░░░░░░░░ 30%"
            STATUS="${YELLOW}⏳ WORKING${NC}"
          fi
        else
          PROGRESS="█░░░░░░░░░░░░░░░░░░░ 5%"
          STATUS="${CYAN}🚀 STARTING${NC}"
        fi
        
        # Get last action
        LAST_ACTION=$(grep -E "(Thinking|Executing|Writing|Creating|Complete|Error)" "$LOG_FILE" 2>/dev/null | tail -1 | cut -c1-40 || echo "Initializing...")
      else
        STATUS="${BLUE}⏸️  WAITING${NC}"
        PROGRESS="░░░░░░░░░░░░░░░░░░░░"
        LAST_ACTION="Pending launch..."
      fi
      
      printf "║ %s %-12s │ %-20s │ %-30s ║\n" "$EMOJI" "$NAME" "$STATUS" "${LAST_ACTION:0:30}"
      printf "║                │ %-40s     ║\n" "$PROGRESS"
      echo "╠════════════════════════════════════════════════════════════════╣"
    done
    
    # Show endpoint metrics
    RUNNING=$(curl -s http://localhost:8000/metrics 2>/dev/null | grep "num_requests_running" | grep -v "#" | awk '{print $2}' | cut -d'.' -f1 || echo "0")
    echo "║ 🖥️  ENDPOINT: Running=$RUNNING/32 | Press Ctrl+C to stop monitoring           ║"
    echo "╚════════════════════════════════════════════════════════════════╝"
    
    sleep 2
  done
}

# Launch progress display in background
show_progress &
PROGRESS_PID=$!

# Cleanup on exit
trap "kill $PROGRESS_PID 2>/dev/null; exit" INT TERM

# Launch agents
START=$(date +%s)

for i in {0..8}; do
  (
    TASK="${AGENT_TASKS[$i]}"
    NAME="${AGENT_NAMES[$i]}"
    
    cd /home/ivo/selfware
    
    # Run the task
    SELFWARE_CONFIG=./selfware-stress-test.toml \
      timeout 600 ./target/release/selfware run "$TASK" -y \
      > /tmp/complex_test_agent_$i.log 2>&1
    
    echo "Done" >> /tmp/complex_test_agent_$i.log
    
  ) &
  
  AGENT_PIDS[$i]=$!
  sleep 3  # Stagger starts to prevent thundering herd
  
  # Limit concurrent to 8 (endpoint max)
  if [ $(( (i + 1) % 8 )) -eq 0 ]; then
    wait
  fi
done

# Wait for all agents
wait

kill $PROGRESS_PID 2>/dev/null

END=$(date +%s)
DURATION=$((END-START))

# Final results
clear
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    TEST COMPLETE                               ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Duration: ${DURATION}s"
echo ""
echo "=== Generated Files ==="
find /home/ivo/selfware -maxdepth 2 -name "*.rs" -o -name "*.md" -o -name "*.html" -o -name "Cargo.toml" 2>/dev/null | grep -E "(broker|worker|storage|api|task|queue|dashboard|ARCHITECTURE)" | head -20
echo ""
echo "=== Compilation Test ==="
if [ -f "/home/ivo/selfware/Cargo.toml" ]; then
  cd /home/ivo/selfware && cargo check 2>&1 | tail -10 || true
fi
echo ""
echo "=== Dashboard Preview ==="
if [ -f "/home/ivo/selfware/assets/dashboard.html" ]; then
  head -50 /home/ivo/selfware/assets/dashboard.html
fi
