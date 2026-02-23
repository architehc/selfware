#!/bin/bash
#
# 2-Hour Monitored System Test for Selfware
# Monitors progress every 30 seconds
#
# Usage: ./run_2h_monitored.sh [project_name] [scenario]

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
PROJECT_NAME="${1:-redqueue}"
SCENARIO="${2:-bootstrap}"
DURATION_HOURS=2
DURATION_SECS=$((DURATION_HOURS * 3600))
MONITOR_INTERVAL=30
SESSION_ID="2htest-$(date +%Y%m%d-%H%M%S)"

# Directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TEST_DIR="$PROJECT_ROOT/test_runs/$SESSION_ID"
LOG_DIR="$TEST_DIR/logs"
METRICS_DIR="$TEST_DIR/metrics"
CHECKPOINT_DIR="$TEST_DIR/checkpoints"

# Files
MAIN_LOG="$LOG_DIR/main.log"
MONITOR_LOG="$LOG_DIR/monitor.log"
METRICS_FILE="$METRICS_DIR/metrics.jsonl"
STATUS_FILE="$TEST_DIR/status"
PID_FILE="$TEST_DIR/.pid"

# Initialize directories
mkdir -p "$LOG_DIR" "$METRICS_DIR" "$CHECKPOINT_DIR"

# Logging functions
log() {
    echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1" | tee -a "$MAIN_LOG"
}

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1" | tee -a "$MAIN_LOG"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1" | tee -a "$MAIN_LOG"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1" | tee -a "$MAIN_LOG"
}

log_metric() {
    echo -e "${CYAN}[METRIC]${NC} $1" | tee -a "$MAIN_LOG"
}

log_phase() {
    echo -e "${MAGENTA}[PHASE]${NC} $1" | tee -a "$MAIN_LOG"
}

# Map process exit code to persisted session status.
status_from_exit() {
    local code=$1
    case "$code" in
        0) echo "completed" ;;
        124) echo "timeout" ;;
        130|143) echo "interrupted" ;;
        *) echo "failed" ;;
    esac
}

# Cleanup handler
cleanup() {
    local exit_code=$?

    # Prevent trap re-entry while we finalize artifacts.
    trap - EXIT INT TERM

    local current_status=""
    if [ -f "$STATUS_FILE" ]; then
        current_status="$(cat "$STATUS_FILE" 2>/dev/null || true)"
    fi
    if [ -z "$current_status" ] || [ "$current_status" = "running" ]; then
        status_from_exit "$exit_code" > "$STATUS_FILE"
    fi

    log_warn "Cleaning up... (exit code: $exit_code)"
    generate_final_report "$exit_code"

    exit "$exit_code"
}

trap cleanup EXIT INT TERM

# Banner
cat << 'EOF'
╔══════════════════════════════════════════════════════════════════════════════╗
║                                                                              ║
║   🤖 Selfware 2-Hour Monitored System Test                                  ║
║                                                                              ║
║   Duration: 2 hours  |  Monitoring: Every 30 seconds                        ║
║                                                                              ║
╚══════════════════════════════════════════════════════════════════════════════╝
EOF

log "Session ID: $SESSION_ID"
log "Project: $PROJECT_NAME"
log "Scenario: $SCENARIO"
log "Duration: $DURATION_HOURS hours"
log "Monitor Interval: $MONITOR_INTERVAL seconds"
log "Test Directory: $TEST_DIR"

# Save configuration
cat > "$TEST_DIR/config.json" << EOF
{
  "session_id": "$SESSION_ID",
  "project_name": "$PROJECT_NAME",
  "scenario": "$SCENARIO",
  "duration_hours": $DURATION_HOURS,
  "monitor_interval_seconds": $MONITOR_INTERVAL,
  "started_at": "$(date -Iseconds)",
  "test_directory": "$TEST_DIR",
  "hostname": "$(hostname)",
  "pid": $$
}
EOF

# Check prerequisites
log "Checking prerequisites..."

if [ ! -f "$PROJECT_ROOT/Cargo.toml" ]; then
    log_error "Cannot find project root at $PROJECT_ROOT"
    exit 1
fi

# Check if selfware binary exists or needs building
if [ -f "$PROJECT_ROOT/target/release/selfware" ]; then
    log_info "Using existing selfware binary"
else
    log_info "Building selfware (release mode)..."
    cd "$PROJECT_ROOT"
    cargo build --release 2>&1 | tee "$LOG_DIR/build.log"
    if [ ! -f "$PROJECT_ROOT/target/release/selfware" ]; then
        log_error "Build failed - binary not found"
        exit 1
    fi
    log_info "Build complete"
fi

SELFWARE_BIN="$PROJECT_ROOT/target/release/selfware"

# Create test configuration
cat > "$TEST_DIR/selfware.toml" << 'EOF'
endpoint = "http://localhost:8888/v1"
model = "Qwen/Qwen3-Coder-Next-FP8"
max_tokens = 98304
temperature = 1.0

[safety]
allowed_paths = ["./**", "/home/thread/**"]
denied_paths = ["**/.env", "**/secrets/**", "**/.ssh/**"]
protected_branches = ["main", "master"]

[agent]
max_iterations = 500
step_timeout_secs = 600
token_budget = 1000000

[continuous_work]
enabled = true
checkpoint_interval_tools = 25
checkpoint_interval_secs = 300
auto_recovery = true
max_recovery_attempts = 3

[retry]
max_retries = 5
base_delay_ms = 1000
max_delay_ms = 60000

[yolo]
enabled = true
max_operations = 0
max_hours = 2.5
allow_git_push = false
allow_destructive_shell = false
audit_log_path = "./audit.log"
status_interval = 25
EOF

log_info "Configuration created"

# Define test scenarios
case "$SCENARIO" in
    bootstrap)
        TASK="Create a Rust project called '$PROJECT_NAME' with:
1. Clean project structure with cargo workspace
2. Core library crate with error handling setup
3. Async runtime configuration (tokio)
4. Comprehensive logging (tracing)
5. Configuration management
6. Health check endpoint
7. Basic CI/CD setup (GitHub Actions)
8. Initial test suite
9. Documentation (README, CONTRIBUTING)
10. Docker setup for development

Focus on production-ready structure with proper separation of concerns."
        ;;
    feature)
        TASK="Implement a task queue system in Rust called '$PROJECT_NAME' with:
1. Redis-compatible RESP protocol parser
2. In-memory queue with priority support
3. Async worker pool
4. Delayed job scheduling
5. Dead letter queue
6. Metrics export
7. Comprehensive tests
8. Benchmark suite

Use tokio for async, serde for serialization, and thiserror/anyhow for errors."
        ;;
    refactor)
        TASK="Refactor the existing codebase to improve:
1. Extract common functionality into modules
2. Add proper error propagation
3. Implement structured logging throughout
4. Add comprehensive documentation
5. Increase test coverage to 80%+
6. Optimize performance bottlenecks
7. Add metrics and observability
8. Clean up technical debt

Maintain backward compatibility while improving code quality."
        ;;
    *)
        TASK="Build a Rust project called '$PROJECT_NAME'. Implement core functionality with tests, documentation, and proper error handling."
        ;;
esac

echo "$TASK" > "$TEST_DIR/task.txt"
log "Task saved to $TEST_DIR/task.txt"

# Monitoring function
monitor_progress() {
    local start_time=$(date +%s)
    local iteration=0
    
    log_info "Monitor started (PID: $$)"
    
    while true; do
        sleep $MONITOR_INTERVAL
        iteration=$((iteration + 1))
        
        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))
        local elapsed_min=$((elapsed / 60))
        local remaining=$((DURATION_SECS - elapsed))
        local remaining_min=$((remaining / 60))
        local percent=$((elapsed * 100 / DURATION_SECS))
        
        # Check if main process is still running
        if [ -f "$PID_FILE" ]; then
            local main_pid=$(cat "$PID_FILE" 2>/dev/null || echo "")
            if [ -n "$main_pid" ] && ! kill -0 "$main_pid" 2>/dev/null; then
                log_warn "Main process $main_pid not found, stopping monitor"
                break
            fi
        fi
        
        # Check if time is up
        if [ $elapsed -ge $DURATION_SECS ]; then
            log_info "Time limit reached ($DURATION_HOURS hours)"
            break
        fi
        
        # Collect system metrics
        local cpu_usage=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1 || echo "0")
        local mem_usage=$(free -m | awk 'NR==2{printf "%.1f", $3*100/$2}' || echo "0")
        local disk_usage=$(df -h "$TEST_DIR" | awk 'NR==2 {print $5}' | tr -d '%' || echo "0")
        
        # Collect selfware-specific metrics
        local checkpoints=$(find "$CHECKPOINT_DIR" -name "*.json" 2>/dev/null | wc -l)
        local log_lines=$(wc -l < "$MAIN_LOG" 2>/dev/null || echo "0")
        local audit_lines=$(wc -l < "$TEST_DIR/audit.log" 2>/dev/null || echo "0")
        
        # Check git status if repo exists
        local git_commits=0
        local git_branch="none"
        if [ -d "$TEST_DIR/work/.git" ]; then
            git_commits=$(cd "$TEST_DIR/work" && git rev-list --count HEAD 2>/dev/null || echo "0")
            git_branch=$(cd "$TEST_DIR/work" && git branch --show-current 2>/dev/null || echo "unknown")
        fi
        
        # Check for build artifacts
        local has_build=false
        if [ -f "$TEST_DIR/work/Cargo.toml" ]; then
            if [ -d "$TEST_DIR/work/target" ]; then
                has_build=true
            fi
        fi
        
        # Write structured metrics
        local metric_json=$(cat << EOF
{
  "timestamp": "$(date -Iseconds)",
  "iteration": $iteration,
  "elapsed_seconds": $elapsed,
  "elapsed_minutes": $elapsed_min,
  "remaining_seconds": $remaining,
  "remaining_minutes": $remaining_min,
  "percent_complete": $percent,
  "system": {
    "cpu_percent": "${cpu_usage:-0}",
    "memory_percent": "${mem_usage:-0}",
    "disk_percent": "${disk_usage:-0}"
  },
  "progress": {
    "checkpoints": $checkpoints,
    "log_lines": $log_lines,
    "audit_entries": $audit_lines,
    "git_commits": $git_commits,
    "git_branch": "$git_branch",
    "has_build": $has_build
  },
  "status": "running"
}
EOF
)
        echo "$metric_json" >> "$METRICS_FILE"
        
        # Display progress bar
        local bar_width=40
        local filled=$((percent * bar_width / 100))
        local empty=$((bar_width - filled))
        local bar=""
        for ((i=0; i<filled; i++)); do bar+="█"; done
        for ((i=0; i<empty; i++)); do bar+="░"; done
        
        # Print status line
        printf "\r${CYAN}[%s]${NC} %3d%% | %s | %02dh %02dm elapsed | %02dm remaining | CP: %d | Commits: %d" \
            "$(date '+%H:%M:%S')" "$percent" "$bar" "$((elapsed_min/60))" "$((elapsed_min%60))" \
            "$remaining_min" "$checkpoints" "$git_commits"
        
        # Log detailed metrics every 2 minutes (every 4th iteration)
        if [ $((iteration % 4)) -eq 0 ]; then
            log_metric "Elapsed: ${elapsed_min}min, CPU: ${cpu_usage}%, MEM: ${mem_usage}%, Checkpoints: $checkpoints, Commits: $git_commits"
        fi
        
        # Check for errors in logs
        if grep -i "error\|fail\|panic" "$MAIN_LOG" 2>/dev/null | tail -5 | grep -q .; then
            local recent_errors=$(grep -ic "error\|fail\|panic" "$MAIN_LOG" 2>/dev/null || echo "0")
            if [ "$recent_errors" -gt 0 ]; then
                log_warn "Detected $recent_errors errors in recent log entries"
            fi
        fi
    done
    
    echo "" # Newline after progress bar
    log_info "Monitor stopped after $iteration iterations"
}

# Generate final report
generate_final_report() {
    local exit_code=$1
    local end_time=$(date +%s)
    local start_time=$(stat -c %Y "$TEST_DIR/config.json" 2>/dev/null || echo "$end_time")
    local duration=$((end_time - start_time))
    local duration_min=$((duration / 60))
    
    log_info "Generating final report..."
    
    # Count metrics
    local total_metrics=$(wc -l < "$METRICS_FILE" 2>/dev/null || echo "0")
    local total_checkpoints=$(find "$CHECKPOINT_DIR" -name "*.json" 2>/dev/null | wc -l)
    local total_logs=$(wc -l < "$MAIN_LOG" 2>/dev/null || echo "0")
    
    # Check git status
    local git_stats="{}"
    if [ -d "$TEST_DIR/work/.git" ]; then
        local commits=$(cd "$TEST_DIR/work" && git rev-list --count HEAD 2>/dev/null || echo "0")
        local files=$(cd "$TEST_DIR/work" && git ls-files 2>/dev/null | wc -l || echo "0")
        local loc=$(cd "$TEST_DIR/work" && find . -name "*.rs" -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}' || echo "0")
        git_stats="{\"commits\": $commits, \"tracked_files\": $files, \"rust_loc\": $loc}"
    fi
    
    # Generate report
    cat > "$TEST_DIR/final_report.json" << EOF
{
  "session_id": "$SESSION_ID",
  "project_name": "$PROJECT_NAME",
  "scenario": "$SCENARIO",
  "status": "$(cat $STATUS_FILE 2>/dev/null || echo 'unknown')",
  "exit_code": $exit_code,
  "timing": {
    "started_at": "$(date -d @$start_time -Iseconds)",
    "completed_at": "$(date -d @$end_time -Iseconds)",
    "duration_seconds": $duration,
    "duration_minutes": $duration_min
  },
  "metrics_summary": {
    "total_snapshots": $total_metrics,
    "monitor_interval_seconds": $MONITOR_INTERVAL,
    "checkpoints_created": $total_checkpoints,
    "log_lines": $total_logs
  },
  "git_statistics": $git_stats,
  "files": {
    "config": "$TEST_DIR/config.json",
    "main_log": "$MAIN_LOG",
    "metrics": "$METRICS_FILE",
    "final_report": "$TEST_DIR/final_report.json"
  }
}
EOF

    # Print summary
    echo ""
    echo "╔══════════════════════════════════════════════════════════════════════════════╗"
    echo "║                         🎯 Test Complete                                     ║"
    echo "╚══════════════════════════════════════════════════════════════════════════════╝"
    echo ""
    echo "Session ID:     $SESSION_ID"
    echo "Project:        $PROJECT_NAME"
    echo "Scenario:       $SCENARIO"
    echo "Duration:       ${duration_min} minutes ($(printf '%.1f' $(echo "$duration_min / 60" | bc -l)) hours)"
    echo "Exit Code:      $exit_code"
    echo "Metrics:        $total_metrics snapshots"
    echo "Checkpoints:    $total_checkpoints"
    echo ""
    echo "Output Directory: $TEST_DIR"
    echo ""
    
    if [ $exit_code -eq 0 ]; then
        echo -e "${GREEN}✅ Test completed successfully${NC}"
    else
        echo -e "${RED}❌ Test failed or was interrupted${NC}"
    fi
}

# Main execution
main() {
    log_info "Starting 2-hour monitored test"
    echo "running" > "$STATUS_FILE"
    
    # Create workspace directory
    mkdir -p "$TEST_DIR/work"
    cd "$TEST_DIR/work"
    
    # Start monitor in background
    monitor_progress &
    local monitor_pid=$!
    echo $monitor_pid > "$TEST_DIR/.monitor_pid"
    log_info "Monitor started (PID: $monitor_pid)"
    
    # Record main process PID
    echo $$ > "$PID_FILE"
    
    # Run selfware with timeout
    log_phase "Starting selfware execution..."

    local exit_code=0
    if timeout "${DURATION_HOURS}h" "$SELFWARE_BIN" \
        --config "$TEST_DIR/selfware.toml" \
        --workdir "$TEST_DIR/work" \
        --yolo \
        run "$TASK" 2>&1 | tee -a "$MAIN_LOG"; then
        
        log_info "Selfware completed successfully"
        echo "completed" > "$STATUS_FILE"
        exit_code=0
    else
        exit_code=$?
        if [ $exit_code -eq 124 ]; then
            log_warn "Test timed out after $DURATION_HOURS hours"
            echo "timeout" > "$STATUS_FILE"
        else
            log_error "Selfware failed with exit code $exit_code"
            echo "failed" > "$STATUS_FILE"
        fi
    fi
    
    # Stop monitor
    if kill -0 "$monitor_pid" 2>/dev/null; then
        kill "$monitor_pid" 2>/dev/null || true
        wait "$monitor_pid" 2>/dev/null || true
    fi
    
    return "$exit_code"
}

# Run main
main "$@"
