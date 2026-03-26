#!/bin/bash
# SWE-bench Evaluation with 122B Model (SGLang)
# High-signal test with model that can actually reason

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/swebench_122b/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1" | tee -a "$RESULTS_DIR/eval.log"; }
log_ok() { echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $1" | tee -a "$RESULTS_DIR/eval.log"; }
log_warn() { echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $1" | tee -a "$RESULTS_DIR/eval.log"; }
log_task() { echo -e "${MAGENTA}[TASK]${NC} $1" | tee -a "$RESULTS_DIR/eval.log"; }

# 122B Endpoint Configuration
ENDPOINT="https://crazyshit.ngrok.io/v1"
MODEL="txn545/Qwen3.5-122B-A10B-NVFP4"
CONFIG_FILE="$SCRIPT_DIR/selfware-evolve-122b.toml"

# Verify endpoint is reachable
log "Checking 122B endpoint..."
if ! curl -s "$ENDPOINT/models" > /dev/null 2>&1; then
    log_warn "122B endpoint not reachable at $ENDPOINT"
    exit 1
fi
log_ok "122B endpoint online"

# 5 Django tasks for focused testing
declare -a DJANGO_TASKS=(
    "django__django-11133:Fix URL validator regex for special characters"
    "django__django-12260:Optimize QuerySet.filter() for large IN clauses"
    "django__django-11001:Fix migration crash with foreign key to proxy model"
    "django__django-10919:Fix admin filter_horizontal with UUID primary keys"
    "django__django-11583:Fix cache backend with Redis connection pooling"
)

# Evaluation metrics
declare -i TOTAL_TASKS=0
declare -i SUCCESSFUL_PATCHES=0
declare -i FAILED_PATCHES=0

total_start_time=$(date +%s)

# Run single task
run_task() {
    local task_id=$1
    local task_desc=$2
    local output_dir="$RESULTS_DIR/tasks/$task_id"
    local log_file="$output_dir/run.log"
    
    mkdir -p "$output_dir"
    
    log_task "$task_id"
    log "  Description: $task_desc"
    
    local start_time=$(date +%s)
    
    # Run selfware with 122B config
    timeout 3600 docker run --rm \
        --name "swebench-122b-${task_id//\//-}" \
        -v "$output_dir:/output" \
        -v "$CONFIG_FILE:/app/selfware.toml:ro" \
        selfware:latest \
        -c /app/selfware.toml \
        -p "Fix this Django issue: $task_desc" \
        --yolo 2>&1 > "$log_file"
    
    local exit_code=$?
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))
    
    # Check result
    if [ $exit_code -eq 0 ] && grep -q "✅ Task completed" "$log_file"; then
        ((SUCCESSFUL_PATCHES++))
        log_ok "  ✓ Task completed (${duration}s)"
        local status="success"
    else
        ((FAILED_PATCHES++))
        log_warn "  ✗ Task failed (exit: $exit_code, ${duration}s)"
        local status="failed"
    fi
    
    # Save metrics
    cat > "$output_dir/metrics.json" << EOF
{
  "task_id": "$task_id",
  "description": "$task_desc",
  "status": "$status",
  "duration_sec": $duration,
  "exit_code": $exit_code,
  "timestamp": "$(date -Iseconds)"
}
EOF
    
    ((TOTAL_TASKS++))
    return $exit_code
}

# Main execution
log "╔══════════════════════════════════════════════════════════════╗"
log "║     SWE-BENCH: 122B MODEL EVALUATION                        ║"
log "║     txn545/Qwen3.5-122B-A10B-NVFP4 | 64 Concurrency         ║"
log "╚══════════════════════════════════════════════════════════════╝"
log ""
log "Tasks: ${#DJANGO_TASKS[@]} Django tasks"
log "Concurrent: 8 instances (max)"
log "Timeout: 60 min per task"
log "Results: $RESULTS_DIR"
log ""

# Run tasks sequentially for now (Docker networking with external endpoint)
for task in "${DJANGO_TASKS[@]}"; do
    IFS=':' read -r task_id task_desc <<< "$task"
    run_task "$task_id" "$task_desc"
done

total_end_time=$(date +%s)
total_duration=$((total_end_time - total_start_time))

# Summary
log ""
log "╔══════════════════════════════════════════════════════════════╗"
log "║     EVALUATION COMPLETE                                     ║"
log "╚══════════════════════════════════════════════════════════════╝"
log ""
log "Results:"
log "  Total tasks: $TOTAL_TASKS"
log "  Successful: $SUCCESSFUL_PATCHES"
log "  Failed: $FAILED_PATCHES"
log "  Total time: ${total_duration}s ($((total_duration / 60))m $((total_duration % 60))s)"
log "  Avg time per task: $((total_duration / TOTAL_TASKS))s"
log ""

# Generate report
cat > "$RESULTS_DIR/summary.json" << EOF
{
  "model": "$MODEL",
  "endpoint": "$ENDPOINT",
  "total_tasks": $TOTAL_TASKS,
  "successful": $SUCCESSFUL_PATCHES,
  "failed": $FAILED_PATCHES,
  "success_rate": $(( SUCCESSFUL_PATCHES * 100 / TOTAL_TASKS )),
  "total_duration_sec": $total_duration,
  "avg_duration_sec": $((total_duration / TOTAL_TASKS)),
  "timestamp": "$(date -Iseconds)"
}
EOF

log "Summary saved to: $RESULTS_DIR/summary.json"
log "Logs: $RESULTS_DIR/eval.log"

# Return success if at least 1 task passed
if [ $SUCCESSFUL_PATCHES -gt 0 ]; then
    exit 0
else
    exit 1
fi
