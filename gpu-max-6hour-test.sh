#!/bin/bash
# 6-Hour GPU Max-Out Test for Selfware
# Pushes GPU to maximum utilization with continuous load

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/gpu_max_test/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

# 6 hours duration
DURATION_HOURS=6
DURATION_SEC=$((DURATION_HOURS * 3600))
START_TIME=$(date +%s)
END_TIME=$((START_TIME + DURATION_SEC))

# High-intensity prompts to max out GPU
declare -a INTENSIVE_PROMPTS=(
    "Write a complete Python web framework with routing, middleware, and ORM from scratch"
    "Implement a distributed key-value store in Rust with consensus algorithm"
    "Create a full compiler for a custom programming language including lexer, parser, and codegen"
    "Build a neural network framework in C++ with CUDA support and autograd"
    "Implement a distributed tracing system with span collection and visualization"
    "Write a complete database engine with B-tree indexing and query optimization"
    "Create a real-time collaborative text editor with CRDT implementation"
    "Build a container orchestrator like Kubernetes with scheduling and networking"
    "Implement a cryptocurrency wallet with multi-sig and hardware key support"
    "Write a game engine with physics, rendering, and ECS architecture"
    "Create a video streaming server with adaptive bitrate and CDN support"
    "Build a recommendation system with matrix factorization and real-time updates"
    "Implement a full-text search engine with inverted index and ranking"
    "Write a network protocol stack from Ethernet to HTTP with async I/O"
    "Create a time-series database with compression and aggregation"
)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1" | tee -a "$RESULTS_DIR/test.log"; }
log_ok() { echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $1" | tee -a "$RESULTS_DIR/test.log"; }
log_warn() { echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $1" | tee -a "$RESULTS_DIR/test.log"; }
log_gpu() { echo -e "${MAGENTA}[$(date +%H:%M:%S)] GPU${NC} $1" | tee -a "$RESULTS_DIR/test.log"; }

# Metrics
TOTAL_TASKS=0
SUCCESS_TASKS=0
FAILED_TASKS=0

declare -a GPU_UTIL_HISTORY=()
declare -a GPU_MEM_HISTORY=()
declare -a GPU_TEMP_HISTORY=()

# Get GPU stats
get_gpu_stats() {
    if command -v nvidia-smi &> /dev/null; then
        nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw --format=csv,noheader,nounits 2>/dev/null | head -1
    else
        echo "0,0,0,0,0"
    fi
}

# Log GPU status every 5 minutes
gpu_monitor() {
    while [ $(date +%s) -lt $END_TIME ]; do
        local stats=$(get_gpu_stats)
        local util=$(echo "$stats" | cut -d',' -f1 | tr -d ' ')
        local mem_used=$(echo "$stats" | cut -d',' -f2 | tr -d ' ')
        local mem_total=$(echo "$stats" | cut -d',' -f3 | tr -d ' ')
        local temp=$(echo "$stats" | cut -d',' -f4 | tr -d ' ')
        local power=$(echo "$stats" | cut -d',' -f5 | tr -d ' ')
        local timestamp=$(date +%s)
        
        # Store for averaging
        GPU_UTIL_HISTORY+=($util)
        GPU_MEM_HISTORY+=($mem_used)
        GPU_TEMP_HISTORY+=($temp)
        
        # Calculate memory percentage
        local mem_pct=0
        if [ "$mem_total" -gt 0 ]; then
            mem_pct=$((mem_used * 100 / mem_total))
        fi
        
        # Log to file
        echo "{\"timestamp\": $timestamp, \"gpu_util\": $util, \"gpu_mem_used\": $mem_used, \"gpu_mem_total\": $mem_total, \"gpu_mem_pct\": $mem_pct, \"gpu_temp\": $temp, \"gpu_power\": $power}" >> "$RESULTS_DIR/gpu_stats.jsonl"
        
        # Print status
        log_gpu "Util: ${util}% | Mem: ${mem_used}/${mem_total} MB (${mem_pct}%) | Temp: ${temp}C | Power: ${power}W"
        
        # Check if GPU is being maxed out
        if [ "$util" -lt 50 ]; then
            log_warn "GPU utilization low (${util}%) - ramping up load..."
        elif [ "$util" -gt 90 ]; then
            log_ok "GPU maxed out at ${util}%!"
        fi
        
        sleep 300  # 5 minutes
    done
}

# Run intensive task
run_gpu_task() {
    local task_id=$1
    local prompt="${INTENSIVE_PROMPTS[$RANDOM % ${#INTENSIVE_PROMPTS[@]}]}"
    local start_time=$(date +%s)
    local log_file="$RESULTS_DIR/task_${task_id}.log"
    
    log "Starting task $task_id: ${prompt:0:50}..."
    
    # Run with max tokens to stress GPU
    timeout 600 docker run --rm \
        --name "selfware-gpu-${task_id}" \
        -e SELFWARE_MAX_TOKENS=131072 \
        -e SELFWARE_TEMPERATURE=0.7 \
        selfware:latest \
        -p "$prompt" --yolo 2>&1 > "$log_file" &
    
    echo $!
}

# Keep GPU busy with multiple concurrent tasks
max_out_gpu() {
    local active_pids=()
    local task_counter=0
    
    while [ $(date +%s) -lt $END_TIME ]; do
        # Check and replenish tasks to keep GPU busy
        local active_count=0
        for pid in "${active_pids[@]}"; do
            if kill -0 "$pid" 2>/dev/null; then
                ((active_count++))
            fi
        done
        
        # Launch new tasks to maintain 8 concurrent (max out GPU)
        while [ $active_count -lt 8 ] && [ $(date +%s) -lt $END_TIME ]; do
            ((task_counter++))
            local pid=$(run_gpu_task $task_counter)
            active_pids+=($pid)
            ((active_count++))
            ((TOTAL_TASKS++))
            log "Launched task $task_counter (Active: $active_count)"
            sleep 2
        done
        
        # Check for completed tasks
        for i in "${!active_pids[@]}"; do
            local pid=${active_pids[$i]}
            if ! kill -0 "$pid" 2>/dev/null; then
                # Task completed
                wait $pid 2>/dev/null && ((SUCCESS_TASKS++)) || ((FAILED_TASKS++))
                unset 'active_pids[$i]'
            fi
        done
        
        # Compact array
        active_pids=("${active_pids[@]}")
        
        sleep 5
    done
    
    # Wait for remaining
    log "Waiting for remaining tasks to complete..."
    for pid in "${active_pids[@]}"; do
        wait $pid 2>/dev/null && ((SUCCESS_TASKS++)) || ((FAILED_TASKS++))
    done
}

# Status display
show_status() {
    while [ $(date +%s) -lt $END_TIME ]; do
        sleep 60  # Update every minute
        
        local current=$(date +%s)
        local elapsed=$((current - START_TIME))
        local remaining=$((END_TIME - current))
        local hours=$((remaining / 3600))
        local mins=$(((remaining % 3600) / 60))
        
        # Calculate averages
        local avg_util=0
        local avg_mem=0
        if [ ${#GPU_UTIL_HISTORY[@]} -gt 0 ]; then
            local sum=0
            for val in "${GPU_UTIL_HISTORY[@]}"; do
                sum=$((sum + val))
            done
            avg_util=$((sum / ${#GPU_UTIL_HISTORY[@]}))
        fi
        if [ ${#GPU_MEM_HISTORY[@]} -gt 0 ]; then
            local sum=0
            for val in "${GPU_MEM_HISTORY[@]}"; do
                sum=$((sum + val))
            done
            avg_mem=$((sum / ${#GPU_MEM_HISTORY[@]}))
        fi
        
        clear
        echo -e "${MAGENTA}╔══════════════════════════════════════════════════════════════╗${NC}"
        echo -e "${MAGENTA}║         GPU MAX-OUT TEST - ${hours}h ${mins}m REMAINING            ║${NC}"
        echo -e "${MAGENTA}╚══════════════════════════════════════════════════════════════╝${NC}"
        echo ""
        echo -e "  Tasks Completed:  ${CYAN}$TOTAL_TASKS${NC}"
        echo -e "  Successful:       ${GREEN}$SUCCESS_TASKS${NC}"
        echo -e "  Failed:           ${RED}$FAILED_TASKS${NC}"
        if [ $TOTAL_TASKS -gt 0 ]; then
            local rate=$((SUCCESS_TASKS * 100 / TOTAL_TASKS))
            echo -e "  Success Rate:     ${rate}%"
        fi
        echo ""
        echo -e "  Avg GPU Util:     ${YELLOW}${avg_util}%${NC}"
        echo -e "  Avg GPU Mem:      ${YELLOW}${avg_mem} MB${NC}"
        echo ""
        echo -e "  Results: ${CYAN}$RESULTS_DIR${NC}"
        echo ""
        echo "Press Ctrl+C to stop"
    done
}

# Generate final report
generate_report() {
    log "Generating final report..."
    
    local report_file="$RESULTS_DIR/FINAL_GPU_MAX_REPORT.md"
    local end_time=$(date +%s)
    local total_duration=$((end_time - START_TIME))
    
    # Calculate final averages
    local avg_util=0
    local max_util=0
    local min_util=100
    
    if [ ${#GPU_UTIL_HISTORY[@]} -gt 0 ]; then
        local sum=0
        for val in "${GPU_UTIL_HISTORY[@]}"; do
            sum=$((sum + val))
            [ $val -gt $max_util ] && max_util=$val
            [ $val -lt $min_util ] && min_util=$val
        done
        avg_util=$((sum / ${#GPU_UTIL_HISTORY[@]}))
    fi
    
    cat > "$report_file" << EOF
# 6-Hour GPU Max-Out Test Report

Test Duration: $((total_duration / 3600))h $(((total_duration % 3600) / 60))m  
Start: $(date -d @$START_TIME '+%Y-%m-%d %H:%M:%S')  
End: $(date -d @$end_time '+%Y-%m-%d %H:%M:%S')  
Model: qwen3.5-27b (Qwen/Qwen3.5-27B-FP8)  
GPU: 2x RTX 4090

## Summary

Total Tasks: $TOTAL_TASKS  
Successful: $SUCCESS_TASKS  
Failed: $FAILED_TASKS  
Success Rate: $(if [ $TOTAL_TASKS -gt 0 ]; then echo "$((SUCCESS_TASKS * 100 / TOTAL_TASKS))%"; else echo "N/A"; fi)  
Throughput: $(if [ $total_duration -gt 0 ]; then echo "$((TOTAL_TASKS * 3600 / total_duration)) tasks/hr"; else echo "N/A"; fi)

## GPU Utilization

Average: ${avg_util}%  
Peak: ${max_util}%  
Minimum: ${min_util}%  
Samples: ${#GPU_UTIL_HISTORY[@]}

## Configuration

Concurrent Tasks: 8 (max)  
Max Tokens: 131072  
Temperature: 0.7  
Check Interval: 5 minutes

EOF

    log_ok "Report saved: $report_file"
}

# Cleanup
cleanup() {
    log "Stopping test and cleaning up..."
    docker ps -q --filter "name=selfware-gpu-" | xargs -r docker rm -f 2>/dev/null || true
    generate_report
    
    echo ""
    echo -e "${GREEN}✓ 6-Hour GPU Max-Out Test Complete!${NC}"
    echo -e "Results: ${CYAN}$RESULTS_DIR${NC}"
}

# Main
trap cleanup EXIT INT TERM

log "╔══════════════════════════════════════════════════════════════╗"
log "║       6-HOUR GPU MAX-OUT TEST - Starting Now                ║"
log "╚══════════════════════════════════════════════════════════════╝"
log ""
log "Duration: 6 hours"
log "Concurrent: 8 tasks (max)"
log "Check Interval: 5 minutes"
log "Results: $RESULTS_DIR"
log ""

# Start GPU monitor
gpu_monitor &
MONITOR_PID=$!

# Start status display
show_status &
STATUS_PID=$!

# Run main test
max_out_gpu

# Stop background processes
kill $MONITOR_PID 2>/dev/null || true
kill $STATUS_PID 2>/dev/null || true
