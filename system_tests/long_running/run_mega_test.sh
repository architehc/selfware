#!/bin/bash
#
# Long-Running Mega Project Test Script
# 
# Usage:
#   ./run_mega_test.sh [project_type] [duration_hours] [agent_count]
#
# Examples:
#   ./run_mega_test.sh task_queue 6 6
#   ./run_mega_test.sh database 8 8
#   ./run_mega_test.sh microservices 4 4

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
PROJECT_TYPE="${1:-task_queue}"
DURATION_HOURS="${2:-6}"
AGENT_COUNT="${3:-6}"
CHECKPOINT_INTERVAL="${CHECKPOINT_INTERVAL:-10}"
CONFIG_FILE="${CONFIG_FILE:-}"

# Session ID
SESSION_ID="mega-$(date +%Y%m%d-%H%M%S)-$(uuidgen | cut -d'-' -f1)"

# Directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TEST_RUNS_DIR="$PROJECT_ROOT/test_runs"
SESSION_DIR="$TEST_RUNS_DIR/$SESSION_ID"

# Logging
LOG_FILE="$SESSION_DIR/session.log"

log() {
    echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1"
}

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Cleanup function
cleanup() {
    log_warn "Received interrupt signal, cleaning up..."
    
    # Save final checkpoint if possible
    if [ -d "$SESSION_DIR" ]; then
        echo "{" > "$SESSION_DIR/interrupted.json"
        echo "  \"timestamp\": \"$(date -Iseconds)\"," >> "$SESSION_DIR/interrupted.json"
        echo "  \"reason\": \"user_interrupt\"" >> "$SESSION_DIR/interrupted.json"
        echo "}" >> "$SESSION_DIR/interrupted.json"
    fi
    
    exit 130
}

trap cleanup SIGINT SIGTERM

# Print banner
cat << 'EOF'
╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║   🤖 Selfware Mega Project Test Runner                          ║
║                                                                  ║
║   Long-running validation of agentic software engineering       ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
EOF

log "Session ID: $SESSION_ID"
log "Project: $PROJECT_TYPE"
log "Duration: $DURATION_HOURS hours"
log "Agents: $AGENT_COUNT"
log "Checkpoint Interval: $CHECKPOINT_INTERVAL minutes"

# Validate inputs
case $PROJECT_TYPE in
    task_queue|database|microservices)
        log_info "Project type validated: $PROJECT_TYPE"
        ;;
    *)
        log_error "Invalid project type: $PROJECT_TYPE"
        log_error "Valid options: task_queue, database, microservices"
        exit 1
        ;;
esac

if [ "$DURATION_HOURS" -lt 1 ] || [ "$DURATION_HOURS" -gt 24 ]; then
    log_error "Duration must be between 1 and 24 hours"
    exit 1
fi

if [ "$AGENT_COUNT" -lt 2 ] || [ "$AGENT_COUNT" -gt 16 ]; then
    log_error "Agent count must be between 2 and 16"
    exit 1
fi

# Create session directory
mkdir -p "$SESSION_DIR"
log_info "Session directory: $SESSION_DIR"

# Save configuration
cat > "$SESSION_DIR/config.json" << EOF
{
  "session_id": "$SESSION_ID",
  "project_type": "$PROJECT_TYPE",
  "duration_hours": $DURATION_HOURS,
  "agent_count": $AGENT_COUNT,
  "checkpoint_interval_min": $CHECKPOINT_INTERVAL,
  "started_at": "$(date -Iseconds)",
  "selfware_version": "$(cd "$PROJECT_ROOT" && git describe --tags --always 2>/dev/null || echo 'unknown')",
  "rust_version": "$(rustc --version 2>/dev/null || echo 'unknown')"
}
EOF

# Check prerequisites
log "Checking prerequisites..."

if ! command -v python3 &> /dev/null; then
    log_error "Python 3 is required"
    exit 1
fi

if [ ! -d "$PROJECT_ROOT" ]; then
    log_error "Cannot find project root"
    exit 1
fi

log_info "All prerequisites met"

# Resolve timeout command (GNU coreutils on macOS installs as gtimeout)
if command -v timeout >/dev/null 2>&1; then
    TIMEOUT_CMD="timeout"
elif command -v gtimeout >/dev/null 2>&1; then
    TIMEOUT_CMD="gtimeout"
else
    log_error "'timeout' (or 'gtimeout') command is required. Install coreutils."
    exit 1
fi

# Build Selfware in release mode for testing
log "Building Selfware (release mode)..."
cd "$PROJECT_ROOT"
if ! cargo build --release --all-features 2>&1 | tee -a "$LOG_FILE"; then
    log_error "Build failed"
    exit 1
fi
log_info "Build complete"

# Create test configuration
# If CONFIG_FILE is provided, extract endpoint/model settings from it
ENDPOINT_LINE=""
MODEL_LINE=""
MAX_TOKENS_LINE=""
TEMPERATURE_LINE=""
SAFETY_BLOCK=""
YOLO_BLOCK=""
RETRY_BLOCK=""
RESOURCES_BLOCK=""
if [ -n "${CONFIG_FILE}" ] && [ -f "${CONFIG_FILE}" ]; then
    log_info "Merging settings from CONFIG_FILE: ${CONFIG_FILE}"
    _ep=$(grep '^endpoint' "${CONFIG_FILE}" | head -1 || true)
    _mo=$(grep '^model' "${CONFIG_FILE}" | head -1 || true)
    _mt=$(grep '^max_tokens' "${CONFIG_FILE}" | head -1 || true)
    _te=$(grep '^temperature' "${CONFIG_FILE}" | head -1 || true)
    [ -n "$_ep" ] && ENDPOINT_LINE="$_ep"
    [ -n "$_mo" ] && MODEL_LINE="$_mo"
    [ -n "$_mt" ] && MAX_TOKENS_LINE="$_mt"
    [ -n "$_te" ] && TEMPERATURE_LINE="$_te"
    # Extract step_timeout_secs if present
    _st=$(grep 'step_timeout_secs' "${CONFIG_FILE}" | head -1 | sed 's/.*= *//' || true)
    STEP_TIMEOUT="${_st:-1200}"
    # Extract native_function_calling and streaming
    _nfc=$(grep 'native_function_calling' "${CONFIG_FILE}" | head -1 || true)
    _str=$(grep '^streaming' "${CONFIG_FILE}" | head -1 || true)
else
    STEP_TIMEOUT="600"
    _nfc=""
    _str=""
fi

{
    echo "# Mega Test Session Configuration"
    echo "# Auto-generated at $(date -Iseconds)"
    [ -n "${ENDPOINT_LINE}" ] && echo "${ENDPOINT_LINE}"
    [ -n "${MODEL_LINE}" ] && echo "${MODEL_LINE}"
    [ -n "${MAX_TOKENS_LINE}" ] && echo "${MAX_TOKENS_LINE}"
    [ -n "${TEMPERATURE_LINE}" ] && echo "${TEMPERATURE_LINE}"
    echo ""
    echo "[safety]"
    echo 'allowed_paths = ["./**", "/tmp/selfware/**"]'
    echo 'denied_paths = ["**/.git/**"]'
    echo 'require_confirmation = []'
    echo ""
    echo "[agent]"
    echo "max_iterations = 10000"
    echo "step_timeout_secs = ${STEP_TIMEOUT}"
    echo "token_budget = 240000"
    [ -n "$_nfc" ] && echo "$_nfc"
    [ -n "$_str" ] && echo "$_str"
    echo ""
    echo "[yolo]"
    echo "enabled = true"
    echo "max_operations = 500"
    echo "max_hours = 8.0"
    echo "allow_git_push = false"
    echo "allow_destructive_shell = false"
    echo 'audit_log_path = "./mega-audit.log"'
    echo "status_interval = 25"
    echo ""
    echo "[continuous_work]"
    echo "enabled = true"
    echo "checkpoint_interval_tools = 10"
    echo "checkpoint_interval_secs = 300"
    echo "auto_recovery = true"
    echo "max_recovery_attempts = 5"
    echo ""
    echo "[retry]"
    echo "max_retries = 8"
    echo "base_delay_ms = 1000"
    echo "max_delay_ms = 30000"
    echo ""
    echo "[resources]"
    echo "[resources.gpu]"
    echo "monitor_interval_seconds = 10"
    echo "temperature_threshold = 90"
    echo "memory_utilization_threshold = 0.95"
    echo "throttle_on_overheat = true"
    echo ""
    echo "[resources.memory]"
    echo "warning_threshold = 0.8"
    echo "critical_threshold = 0.9"
    echo "emergency_threshold = 0.95"
    echo "monitor_interval_seconds = 10"
    echo ""
    echo "[resources.disk]"
    echo "max_usage_percent = 0.9"
    echo "maintenance_interval_seconds = 3600"
    echo "compress_after_days = 1"
    echo ""
    echo "[resources.quotas]"
    echo "max_gpu_memory_per_model = 96_000_000_000"
    echo "max_concurrent_requests = 4"
    echo "max_context_tokens = 262144"
    echo "max_queued_tasks = 1000"
    echo "max_checkpoint_size = 2_000_000_000"
} > "$SESSION_DIR/selfware.toml"

log_info "Configuration created"

# Phase definitions
log "Phase breakdown:"
echo "  Phase 1 (Bootstrap):      1 hour"
echo "  Phase 2 (Development):    2 hours"
echo "  Phase 3 (Refinement):     2 hours"
echo "  Phase 4 (Finalization):   1 hour"
echo "  ─────────────────────────────────"
echo "  Total:                    $DURATION_HOURS hours"

# Create the project prompt
case $PROJECT_TYPE in
    task_queue)
        PROJECT_PROMPT="Create a distributed task queue system named 'RedQueue' with the following components:
1. TCP server implementing Redis Serialization Protocol (RESP)
2. HTTP REST API for management
3. Async worker pool with dynamic scaling
4. Priority queues (0-255 priority levels)
5. Delayed job scheduling
6. Dead letter queue for failed jobs
7. Web dashboard for monitoring (simple HTML/JS)
8. CLI management tool
9. Comprehensive test suite (target 80% coverage)
10. Docker deployment configuration

Requirements:
- Written in Rust
- Async/await throughout
- Comprehensive error handling
- Structured logging
- Configuration via environment variables
- Health check endpoints
- Metrics export (Prometheus format)"
        ;;
    database)
        PROJECT_PROMPT="Create a simplified SQLite-compatible database engine named 'MiniDB' with:
1. B-tree storage engine for tables
2. SQL parser supporting SELECT, INSERT, UPDATE, DELETE
3. Query planner with basic optimization
4. Transaction support with ACID properties
5. Write-Ahead Logging (WAL) for durability
6. Buffer pool for caching
7. CLI client for interactive queries
8. Test suite with TPC-C style benchmarks

Requirements:
- Written in Rust
- Safe memory management
- Crash recovery
- Concurrent read access
- Configurable cache size"
        ;;
    microservices)
        PROJECT_PROMPT="Create a microservices platform named 'ServiceMesh' with:
1. Service discovery using gossip protocol
2. HTTP/gRPC load balancer
3. Circuit breaker pattern implementation
4. Distributed tracing with OpenTelemetry
5. Configuration management (hot reload)
6. Health checking and auto-failover
7. Rate limiting and quotas
8. Service mesh sidecar proxy
9. Admin dashboard
10. Integration tests

Requirements:
- Written in Rust
- Kubernetes compatible
- Prometheus metrics
- Structured logging
- Zero-downtime deployments"
        ;;
esac

# Save project prompt
echo "$PROJECT_PROMPT" > "$SESSION_DIR/project_prompt.txt"

# Monitor function
monitor_session() {
    local session_dir=$1
    local log_file=$2
    
    log_info "Starting monitoring..."
    
    while true; do
        sleep 60
        
        # Check if session is still running
        if [ -f "$session_dir/.pid" ]; then
            local pid=$(cat "$session_dir/.pid")
            if ! kill -0 "$pid" 2>/dev/null; then
                log_warn "Process $pid not found, session may have ended"
                break
            fi
        fi
        
        # Collect metrics
        local elapsed=$(($(date +%s) - $(stat -f %m "$session_dir/config.json" 2>/dev/null || gstat -c %Y "$session_dir/config.json" 2>/dev/null || date +%s)))
        local hours=$((elapsed / 3600))
        local minutes=$(((elapsed % 3600) / 60))
        
        # Count checkpoints
        local checkpoints=$(find "$session_dir/checkpoints" -name "checkpoint_*.json" 2>/dev/null | wc -l)
        
        # Log status
        log "Status: ${hours}h ${minutes}m elapsed | Checkpoints: $checkpoints"
        
        # Write metrics
        cat > "$session_dir/metrics_current.json" << EOF
{
  "timestamp": "$(date -Iseconds)",
  "elapsed_seconds": $elapsed,
  "checkpoints": $checkpoints,
  "phase": "unknown"
}
EOF
    done
}

# Run the test
log "Starting mega test..."
echo ""
echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║  Test is running... Press Ctrl+C to gracefully interrupt        ║"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo ""

# Export session info for Selfware
export SELFWARE_SESSION_ID="$SESSION_ID"
export SELFWARE_SESSION_DIR="$SESSION_DIR"
export SELFWARE_CHECKPOINT_INTERVAL="$CHECKPOINT_INTERVAL"
export SELFWARE_AUTO_RECOVERY="true"

# Start monitoring in background
monitor_session "$SESSION_DIR" "$LOG_FILE" &
MONITOR_PID=$!

# Save monitor PID
echo $MONITOR_PID > "$SESSION_DIR/.monitor_pid"

# Run the actual test using Selfware
cd "$PROJECT_ROOT"

# Create a status file
echo "running" > "$SESSION_DIR/status"

# Execute the test
if "${TIMEOUT_CMD}" "${DURATION_HOURS}h" ./target/release/selfware \
    -c "$SESSION_DIR/selfware.toml" \
    -C "$SESSION_DIR" \
    -y \
    run "$PROJECT_PROMPT" 2>&1 | tee -a "$LOG_FILE"; then
    
    log_info "Test completed successfully"
    echo "completed" > "$SESSION_DIR/status"
    EXIT_CODE=0
else
    EXIT_CODE=$?
    if [ "$EXIT_CODE" -eq 124 ]; then
        log_warn "Test reached configured duration (${DURATION_HOURS}h)"
        echo "timeout" > "$SESSION_DIR/status"
    elif [ "$EXIT_CODE" -eq 130 ] || [ "$EXIT_CODE" -eq 143 ]; then
        log_warn "Test interrupted"
        echo "interrupted" > "$SESSION_DIR/status"
    else
        log_error "Test failed (exit ${EXIT_CODE})"
        echo "failed" > "$SESSION_DIR/status"
    fi
fi

# Stop monitoring
if kill -0 "$MONITOR_PID" 2>/dev/null; then
    kill "$MONITOR_PID" 2>/dev/null || true
fi

# Generate final report
log "Generating final report..."

# Count results
CHECKPOINT_COUNT=$(find "$SESSION_DIR/checkpoints" -name "checkpoint_*.json" 2>/dev/null | wc -l)
METRIC_COUNT=$(find "$SESSION_DIR/metrics" -name "metrics_*.json" 2>/dev/null | wc -l)

# Calculate duration
END_TIME=$(date +%s)
START_TIME=$(stat -f %m "$SESSION_DIR/config.json" 2>/dev/null || gstat -c %Y "$SESSION_DIR/config.json" 2>/dev/null || date +%s)
DURATION=$((END_TIME - START_TIME))
DURATION_H=$((DURATION / 3600))
DURATION_M=$(((DURATION % 3600) / 60))

# Generate report
cat > "$SESSION_DIR/final_report.json" << EOF
{
  "session_id": "$SESSION_ID",
  "status": "$(cat $SESSION_DIR/status)",
  "started_at": "$(date -Iseconds -r $(stat -f %m "$SESSION_DIR/config.json" 2>/dev/null || gstat -c %Y "$SESSION_DIR/config.json" 2>/dev/null || echo 0))",
  "completed_at": "$(date -Iseconds)",
  "duration_seconds": $DURATION,
  "duration_formatted": "${DURATION_H}h ${DURATION_M}m",
  "project_type": "$PROJECT_TYPE",
  "agent_count": $AGENT_COUNT,
  "checkpoints_created": $CHECKPOINT_COUNT,
  "metrics_snapshots": $METRIC_COUNT,
  "exit_code": $EXIT_CODE
}
EOF

# Print summary
echo ""
echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║                        Test Complete                             ║"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo ""
echo "Session ID:    $SESSION_ID"
echo "Status:        $(cat $SESSION_DIR/status)"
echo "Duration:      ${DURATION_H}h ${DURATION_M}m"
echo "Checkpoints:   $CHECKPOINT_COUNT"
echo "Metrics:       $METRIC_COUNT"
echo "Output:        $SESSION_DIR"
echo ""

if [ $EXIT_CODE -eq 0 ]; then
    log_info "✅ Mega test completed successfully"
elif [ $EXIT_CODE -eq 124 ]; then
    log_warn "⏱️ Mega test reached duration limit"
else
    log_error "❌ Mega test failed or was interrupted"
fi

exit $EXIT_CODE
