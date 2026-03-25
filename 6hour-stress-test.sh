#!/bin/bash
# 6-Hour Selfware Docker Stress Test
# Tests vLLM endpoint with continuous load

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/6hour_test_results/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

# Test duration: 6 hours
DURATION_HOURS=6
DURATION_SEC=$((DURATION_HOURS * 3600))
START_TIME=$(date +%s)
END_TIME=$((START_TIME + DURATION_SEC))

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1" | tee -a "$RESULTS_DIR/test.log"; }
log_ok() { echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $1" | tee -a "$RESULTS_DIR/test.log"; }
log_warn() { echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $1" | tee -a "$RESULTS_DIR/test.log"; }
log_info() { echo -e "${CYAN}[$(date +%H:%M:%S)] ℹ${NC} $1" | tee -a "$RESULTS_DIR/test.log"; }

# Test prompts of varying complexity
declare -a PROMPTS=(
    "Write a Python function to calculate fibonacci numbers"
    "Create a Rust struct for a binary tree with insert and search methods"
    "Write a regex pattern to validate email addresses"
    "Implement a simple LRU cache in Python"
    "Create a bash script to monitor disk usage and alert on threshold"
    "Write a SQL query to find top 10 customers by order value"
    "Implement quicksort algorithm in Go"
    "Create a Docker Compose file for a web app with Redis and PostgreSQL"
    "Write a JavaScript function to debounce user input"
    "Create a Python class for a thread-safe counter"
    "Implement binary search in C++"
    "Write a nginx config for reverse proxy with SSL"
    "Create a systemd service file for a Python script"
    "Write a Kubernetes deployment YAML for a web service"
    "Implement merge sort with O(n log n) complexity"
    "Create a Makefile for a C project with multiple targets"
    "Write a Python decorator for timing function execution"
    "Implement a simple HTTP server in Python without frameworks"
    "Create a git pre-commit hook for linting"
    "Write a Terraform config for AWS EC2 instance"
)

# Metrics counters
declare -i TOTAL_REQUESTS=0
declare -i SUCCESSFUL_REQUESTS=0
declare -i FAILED_REQUESTS=0
TOTAL_TOKENS_IN=0
TOTAL_TOKENS_OUT=0

# Run a single test iteration
run_iteration() {
    local instance_id=$1
    local prompt="${PROMPTS[$RANDOM % ${#PROMPTS[@]}]}"
    local iteration=$2
    local start_time=$(date +%s%N)
    local log_file="$RESULTS_DIR/instance_${instance_id}/iter_${iteration}.log"
    
    mkdir -p "$(dirname "$log_file")"
    
    # Run selfware with yolo mode for automation
    timeout 300 docker run --rm \
        --name "selfware-${instance_id}-${iteration}" \
        -v "$RESULTS_DIR:/results" \
        selfware:latest \
        -p "$prompt" --yolo 2>&1 > "$log_file" &
    
    local pid=$!
    wait $pid 2>/dev/null || true
    
    local end_time=$(date +%s%N)
    local duration=$(( (end_time - start_time) / 1000000 ))  # ms
    
    # Extract metrics from log
    if grep -q "completed\|success" "$log_file" 2>/dev/null; then
        ((SUCCESSFUL_REQUESTS++))
        echo "{\"instance\": $instance_id, \"iteration\": $iteration, \"duration_ms\": $duration, \"status\": \"success\", \"timestamp\": \"$(date -Iseconds)\"}" >> "$RESULTS_DIR/metrics.jsonl"
    else
        ((FAILED_REQUESTS++))
        echo "{\"instance\": $instance_id, \"iteration\": $iteration, \"duration_ms\": $duration, \"status\": \"failed\", \"timestamp\": \"$(date -Iseconds)\"}" >> "$RESULTS_DIR/metrics.jsonl"
    fi
    
    ((TOTAL_REQUESTS++))
}

# Monitor system resources
monitor_resources() {
    while [ $(date +%s) -lt $END_TIME ]; do
        local timestamp=$(date +%s)
        local cpu=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1)
        local mem=$(free -m | awk 'NR==2{printf "%.2f", $3*100/$2}')
        
        echo "{\"timestamp\": $timestamp, \"cpu_percent\": \"$cpu\", \"memory_percent\": \"$mem\"}" >> "$RESULTS_DIR/resource_usage.jsonl"
        
        sleep 30
    done
}

# Status reporter
report_status() {
    while [ $(date +%s) -lt $END_TIME ]; do
        sleep 300  # Every 5 minutes
        
        local current=$(date +%s)
        local elapsed=$((current - START_TIME))
        local remaining=$((END_TIME - current))
        local hours_left=$((remaining / 3600))
        local mins_left=$(((remaining % 3600) / 60))
        
        clear
        echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
        echo -e "${CYAN}║          6-HOUR SELFWARE STRESS TEST - RUNNING              ║${NC}"
        echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
        echo ""
        echo -e "  Elapsed:     $((elapsed / 3600))h $(((elapsed % 3600) / 60))m"
        echo -e "  Remaining:   ${hours_left}h ${mins_left}m"
        echo ""
        echo -e "  Total Requests:    $TOTAL_REQUESTS"
        echo -e "  Successful:        ${GREEN}$SUCCESSFUL_REQUESTS${NC}"
        echo -e "  Failed:            ${RED}$FAILED_REQUESTS${NC}"
        if [ $TOTAL_REQUESTS -gt 0 ]; then
            local rate=$((SUCCESSFUL_REQUESTS * 100 / TOTAL_REQUESTS))
            echo -e "  Success Rate:      ${rate}%"
        fi
        echo ""
        echo -e "  Results: ${YELLOW}$RESULTS_DIR${NC}"
        echo ""
        echo "Press Ctrl+C to stop early"
    done
}

# Main test loop
main_loop() {
    log "Starting 6-hour stress test"
    log_info "Results: $RESULTS_DIR"
    log_info "End time: $(date -d @$END_TIME '+%H:%M:%S')"
    
    # Start monitoring
    monitor_resources &
    local monitor_pid=$!
    
    # Start reporting
    report_status &
    local report_pid=$!
    
    local iteration=0
    
    while [ $(date +%s) -lt $END_TIME ]; do
        # Run 4 concurrent instances
        for i in 1 2 3 4; do
            run_iteration $i $iteration &
        done
        
        # Wait for batch to complete
        wait
        
        iteration=$((iteration + 1))
        
        # Brief pause between batches
        sleep 5
    done
    
    # Stop monitoring
    kill $monitor_pid 2>/dev/null || true
    kill $report_pid 2>/dev/null || true
}

# Generate final report
generate_report() {
    log "Generating final report..."
    
    local report_file="$RESULTS_DIR/FINAL_6HOUR_REPORT.md"
    local end_time=$(date +%s)
    local total_duration=$((end_time - START_TIME))
    
    cat > "$report_file" << EOF
# 6-Hour Selfware Stress Test Report

**Start Time**: $(date -d @$START_TIME '+%Y-%m-%d %H:%M:%S')  
**End Time**: $(date -d @$end_time '+%Y-%m-%d %H:%M:%S')  
**Duration**: $((total_duration / 3600))h $(((total_duration % 3600) / 60))m  
**Model**: qwen3.5-27b (Qwen/Qwen3.5-27B-FP8)  
**Endpoint**: http://localhost:8000/v1

---

## Summary

| Metric | Value |
|--------|-------|
| Total Requests | $TOTAL_REQUESTS |
| Successful | $SUCCESSFUL_REQUESTS |
| Failed | $FAILED_REQUESTS |
| Success Rate | $(if [ $TOTAL_REQUESTS -gt 0 ]; then echo "$((SUCCESSFUL_REQUESTS * 100 / TOTAL_REQUESTS))%"; else echo "N/A"; fi) |
| Throughput | $(if [ $total_duration -gt 0 ]; then echo "$((TOTAL_REQUESTS * 3600 / total_duration)) req/hr"; else echo "N/A"; fi) |

## Test Configuration

- **Parallel Instances**: 4
- **Test Duration**: 6 hours
- **Prompt Variety**: ${#PROMPTS[@]} different prompts
- **Timeout per Request**: 300 seconds
- **Mode**: Headless with auto-approval (--yolo)

## Prompts Tested

EOF

    for i in "${!PROMPTS[@]}"; do
        echo "$((i+1)). ${PROMPTS[$i]}" >> "$report_file"
    done

    cat >> "$report_file" << 'EOF'

## Raw Data

- Metrics: `metrics.jsonl`
- Resource Usage: `resource_usage.jsonl`
- Logs: `instance_*/iter_*.log`

## Next Steps

1. Analyze `metrics.jsonl` for performance trends
2. Check `resource_usage.jsonl` for system bottlenecks
3. Review failed requests for error patterns
4. Compare against baseline benchmarks

---

*Generated by Selfware 6-Hour Stress Test Framework*
EOF

    log_ok "Report generated: $report_file"
}

# Cleanup
cleanup() {
    log "Cleaning up..."
    docker ps -q --filter "name=selfware-" | xargs -r docker rm -f 2>/dev/null || true
    generate_report
    log_ok "Test complete! Results in: $RESULTS_DIR"
}

# Main
trap cleanup EXIT INT TERM

main_loop
