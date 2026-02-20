#!/bin/bash
#
# Flexible Monitored System Test for Selfware
# Usage: ./run_monitored_test.sh [duration_minutes] [project] [scenario]

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

# Configuration
DURATION_MIN="${1:-30}"  # Default 30 minutes
PROJECT="${2:-redqueue}"
SCENARIO="${3:-bootstrap}"
MONITOR_INTERVAL=30  # Always 30 seconds as requested

DURATION_SECS=$((DURATION_MIN * 60))
SESSION_ID="test-$(date +%Y%m%d-%H%M%S)-${DURATION_MIN}min"

# Directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TEST_DIR="$PROJECT_ROOT/test_runs/$SESSION_ID"
LOG_DIR="$TEST_DIR/logs"
METRICS_DIR="$TEST_DIR/metrics"
CHECKPOINT_DIR="$TEST_DIR/checkpoints"
WORK_DIR="$TEST_DIR/work"

# Files
MAIN_LOG="$LOG_DIR/main.log"
METRICS_FILE="$METRICS_DIR/metrics.jsonl"
STATUS_FILE="$TEST_DIR/status"
PID_FILE="$TEST_DIR/.pid"

# Initialize
mkdir -p "$LOG_DIR" "$METRICS_DIR" "$CHECKPOINT_DIR" "$WORK_DIR"

# Logging
log() { echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1" | tee -a "$MAIN_LOG"; }
log_info() { echo -e "${GREEN}[INFO]${NC} $1" | tee -a "$MAIN_LOG"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1" | tee -a "$MAIN_LOG"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1" | tee -a "$MAIN_LOG"; }
log_phase() { echo -e "${MAGENTA}[PHASE]${NC} $1" | tee -a "$MAIN_LOG"; }

# Cleanup
cleanup() {
    local code=$?
    log_warn "Cleaning up..."
    echo "completed" > "$STATUS_FILE" 2>/dev/null || true
    
    # Generate report
    local end=$(date +%s)
    local start=$(stat -c %Y "$TEST_DIR/config.json" 2>/dev/null || echo "$end")
    local dur=$((end - start))
    
    cat > "$TEST_DIR/final_report.json" << EOF
{
  "session_id": "$SESSION_ID",
  "duration_seconds": $dur,
  "exit_code": $code,
  "status": "$(cat $STATUS_FILE 2>/dev/null || echo 'unknown')"
}
EOF
    exit $code
}
trap cleanup EXIT INT TERM

# Banner
cat << EOF
╔══════════════════════════════════════════════════════════════════════════════╗
║                                                                              ║
║   🤖 Selfware Monitored System Test                                         ║
║                                                                              ║
║   Duration: $DURATION_MIN minutes  |  Monitoring: Every $MONITOR_INTERVAL seconds  ║
║   Project: $PROJECT  |  Scenario: $SCENARIO                                    ║
║                                                                              ║
╚══════════════════════════════════════════════════════════════════════════════╝
EOF

log "Session ID: $SESSION_ID"
log "Test Directory: $TEST_DIR"

# Save config
cat > "$TEST_DIR/config.json" << EOF
{
  "session_id": "$SESSION_ID",
  "project": "$PROJECT",
  "scenario": "$SCENARIO",
  "duration_minutes": $DURATION_MIN,
  "monitor_interval_seconds": $MONITOR_INTERVAL,
  "started_at": "$(date -Iseconds)"
}
EOF

# Check binary
SELFWARE_BIN="$PROJECT_ROOT/target/release/selfware"
if [ ! -f "$SELFWARE_BIN" ]; then
    log_info "Building selfware..."
    cd "$PROJECT_ROOT"
    cargo build --release 2>&1 | tee "$LOG_DIR/build.log"
fi
log_info "Using binary: $SELFWARE_BIN"

# Create test config
cat > "$TEST_DIR/selfware.toml" << EOF
endpoint = "http://localhost:8888/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"
max_tokens = 98304
temperature = 1.0

[safety]
allowed_paths = ["$TEST_DIR/work/**", "$PROJECT_ROOT/**"]
denied_paths = ["**/.env", "**/secrets/**", "**/.ssh/**"]
protected_branches = ["main", "master"]

[agent]
max_iterations = $((DURATION_MIN * 5))
step_timeout_secs = 300
token_budget = 500000

[continuous_work]
enabled = true
checkpoint_interval_tools = 20
checkpoint_interval_secs = 180
auto_recovery = true
max_recovery_attempts = 3

[yolo]
enabled = true
max_hours = $((DURATION_MIN / 60 + 1))
allow_git_push = false
allow_destructive_shell = false
audit_log_path = "$TEST_DIR/audit.log"
status_interval = 20
EOF

# Define task based on scenario
case "$SCENARIO" in
    fix-calculator)
        # Copy broken calculator project
        cp -r "$PROJECT_ROOT/system_tests/projecte2e/work/easy_calculator" "$WORK_DIR/"
        cd "$WORK_DIR/easy_calculator"
        TASK="Fix the bugs in this calculator library. The tests are failing. 
Run 'cargo test' to see the failures, then fix the code so all tests pass.
Don't change the tests, only fix the implementation in src/lib.rs."
        ;;
    fix-string-ops)
        cp -r "$PROJECT_ROOT/system_tests/projecte2e/work/easy_string_ops" "$WORK_DIR/"
        cd "$WORK_DIR/easy_string_ops"
        TASK="Fix the string manipulation functions. Run 'cargo test' to see failures.
Fix src/lib.rs so all tests pass. The tests check for proper handling of 
Unicode, edge cases, and error conditions."
        ;;
    fix-scheduler)
        cp -r "$PROJECT_ROOT/system_tests/projecte2e/work/hard_scheduler" "$WORK_DIR/"
        cd "$WORK_DIR/hard_scheduler"
        TASK="This task scheduler has bugs in its priority queue implementation.
Run 'cargo test' to see failures. Fix the concurrency issues and 
logic bugs in src/lib.rs. Pay attention to race conditions and 
proper task ordering."
        ;;
    fix-event-bus)
        cp -r "$PROJECT_ROOT/system_tests/projecte2e/work/hard_event_bus" "$WORK_DIR/"
        cd "$WORK_DIR/hard_event_bus"
        TASK="Fix the event bus implementation. It has issues with event handling,
subscriber management, and async processing. Run 'cargo test' to see failures.
Fix all issues in the src/ directory."
        ;;
    new-project|bootstrap|*)
        cd "$WORK_DIR"
        TASK="Create a new Rust project called '$PROJECT' with:
1. Cargo workspace setup
2. Core library with error handling (thiserror/anyhow)
3. Async runtime (tokio) setup
4. Tracing/logging infrastructure
5. Configuration management
6. Health check endpoint
7. Comprehensive test suite
8. CI/CD setup (GitHub Actions workflow)
9. Dockerfile for containerization
10. README with usage examples

Focus on production-ready code with proper documentation."
        ;;
esac

echo "$TASK" > "$TEST_DIR/task.txt"
log "Task saved"

# Monitor function
monitor_progress() {
    local start=$(date +%s)
    local iter=0
    
    while true; do
        sleep $MONITOR_INTERVAL
        iter=$((iter + 1))
        
        local now=$(date +%s)
        local elapsed=$((now - start))
        local remain=$((DURATION_SECS - elapsed))
        local pct=$((elapsed * 100 / DURATION_SECS))
        
        # Check main process
        if [ -f "$PID_FILE" ]; then
            local pid=$(cat "$PID_FILE" 2>/dev/null || echo "")
            if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
                log_warn "Main process ended"
                break
            fi
        fi
        
        # Time check
        if [ $elapsed -ge $DURATION_SECS ]; then
            log_info "Time limit reached"
            break
        fi
        
        # Collect metrics
        local cpu=$(top -bn1 2>/dev/null | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1 || echo "0")
        local mem=$(free -m 2>/dev/null | awk 'NR==2{printf "%.1f", $3*100/$2}' || echo "0")
        local cps=$(find "$CHECKPOINT_DIR" -name "*.json" 2>/dev/null | wc -l)
        local logs=$(wc -l < "$MAIN_LOG" 2>/dev/null || echo "0")
        
        local commits=0
        local branch="none"
        if [ -d "$WORK_DIR/.git" ]; then
            commits=$(cd "$WORK_DIR" && git rev-list --count HEAD 2>/dev/null || echo "0")
            branch=$(cd "$WORK_DIR" && git branch --show-current 2>/dev/null || echo "none")
        fi
        
        # Write metrics
        cat >> "$METRICS_FILE" << EOF
{"t":"$(date -Iseconds)","i":$iter,"e":$elapsed,"p":$pct,"cpu":"$cpu","mem":"$mem","cp":$cps,"log":$logs,"git":$commits}
EOF
        
        # Progress bar
        local fw=30
        local fl=$((pct * fw / 100))
        local bar=""
        for ((i=0; i<fl; i++)); do bar+="█"; done
        for ((i=fl; i<fw; i++)); do bar+="░"; done
        
        printf "\r${CYAN}[%s]${NC} %3d%% [%s] %02d:%02d | CP:%d | Git:%d" \
            "$(date '+%H:%M:%S')" "$pct" "$bar" "$((elapsed/60))" "$((elapsed%60))" "$cps" "$commits"
        
        # Detailed log every 2 min
        if [ $((iter % 4)) -eq 0 ]; then
            log "Elapsed: $((elapsed/60))min, CPU:${cpu}%, MEM:${mem}%, CPs:$cps, Commits:$commits"
        fi
    done
    
    echo ""
    log_info "Monitor stopped"
}

# Main execution
main() {
    log_info "Starting monitored test"
    echo "running" > "$STATUS_FILE"
    
    # Start monitor
    monitor_progress &
    local mon_pid=$!
    echo $mon_pid > "$TEST_DIR/.monitor_pid"
    
    # Record main PID
    echo $$ > "$PID_FILE"
    
    log_phase "Executing: $SCENARIO"
    
    # Run with timeout
    if timeout "${DURATION_MIN}m" "$SELFWARE_BIN" run "$TASK" \
        --config "$TEST_DIR/selfware.toml" \
        --work-dir "$WORK_DIR" 2>&1 | tee -a "$MAIN_LOG"; then
        
        log_info "Selfware completed successfully"
        echo "completed" > "$STATUS_FILE"
        code=0
    else
        code=$?
        if [ $code -eq 124 ]; then
            log_warn "Test timed out"
            echo "timeout" > "$STATUS_FILE"
        else
            log_error "Exit code: $code"
            echo "failed" > "$STATUS_FILE"
        fi
    fi
    
    # Stop monitor
    kill $mon_pid 2>/dev/null || true
    wait $mon_pid 2>/dev/null || true
    
    return $code
}

main "$@"
