#!/bin/bash
# Stress Test: Max out 122B endpoint (64 concurrency)
# Runs multiple E2E tasks in parallel to test throughput

set -e

SELFWARE="/home/ivo/selfware/target/release/selfware"
CONFIG="/home/ivo/selfware/selfware-evolve-122b.toml"
RESULTS_DIR="/tmp/stress_122b_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${CYAN}╔════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     122B ENDPOINT STRESS TEST                                ║${NC}"
echo -e "${CYAN}║     Max Concurrency: 64 | Testing: 16 parallel tasks         ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Test tasks (E2E templates)
declare -a TASKS=(
    "easy_calculator:Implement a calculator with add, subtract, multiply, divide"
    "easy_string_ops:Implement the missing string operations"
    "medium_json_merge:Implement the missing JSON merge functionality"
    "viz_sparkline:Implement the sparkline visualization"
    "viz_progress_bar:Implement the progress bar visualization"
    "easy_calculator:Fix the calculator division by zero bug"
    "easy_string_ops:Fix string truncation bug"
    "medium_json_merge:Fix JSON merge for nested objects"
)

CONCURRENT=8  # Run 8 at a time (conservative for 64 max)
TOTAL=${#TASKS[@]}

echo "Tasks: $TOTAL"
echo "Concurrent: $CONCURRENT"
echo "Results: $RESULTS_DIR"
echo ""

# Function to run single task
run_task() {
    local task_name=$1
    local prompt=$2
    local idx=$3
    
    local template_dir="/home/ivo/selfware/system_tests/projecte2e/templates/$task_name"
    local log_file="$RESULTS_DIR/task_${idx}_${task_name}.log"
    
    if [ ! -d "$template_dir" ]; then
        echo "SKIP: $task_name (not found)"
        return 1
    fi
    
    local start_time=$(date +%s)
    
    if timeout 180 $SELFWARE -c "$CONFIG" -y -p "$prompt" -C "$template_dir" > "$log_file" 2>&1; then
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        
        if grep -q "✅ Task completed" "$log_file"; then
            echo "✓ Task $idx ($task_name): ${duration}s"
            echo "success:$duration" >> "$RESULTS_DIR/results.txt"
            return 0
        else
            echo "✗ Task $idx ($task_name): No completion marker"
            echo "fail:$duration" >> "$RESULTS_DIR/results.txt"
            return 1
        fi
    else
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        echo "✗ Task $idx ($task_name): Failed/timeout (${duration}s)"
        echo "fail:$duration" >> "$RESULTS_DIR/results.txt"
        return 1
    fi
}

# Export for parallel execution
export -f run_task
export SELFWARE CONFIG RESULTS_DIR

# Run tasks in parallel
echo -e "${BLUE}Starting parallel execution...${NC}"
echo ""

idx=0
for task_def in "${TASKS[@]}"; do
    IFS=':' read -r task_name prompt <<< "$task_def"
    
    # Run in background
    run_task "$task_name" "$prompt" "$idx" &
    
    ((idx++))
    
    # Limit concurrent jobs
    if [ $((idx % CONCURRENT)) -eq 0 ]; then
        wait
    fi
done

# Wait for remaining jobs
wait

# Calculate results
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}RESULTS${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

if [ -f "$RESULTS_DIR/results.txt" ]; then
    SUCCESS=$(grep -c "^success:" "$RESULTS_DIR/results.txt" 2>/dev/null || echo 0)
    FAILED=$(grep -c "^fail:" "$RESULTS_DIR/results.txt" 2>/dev/null || echo 0)
    
    # Calculate avg duration
    TOTAL_DURATION=0
    COUNT=0
    for dur in $(grep "^success:" "$RESULTS_DIR/results.txt" | cut -d: -f2); do
        TOTAL_DURATION=$((TOTAL_DURATION + dur))
        ((COUNT++))
    done
    
    if [ $COUNT -gt 0 ]; then
        AVG_DURATION=$((TOTAL_DURATION / COUNT))
    else
        AVG_DURATION=0
    fi
    
    echo -e "${GREEN}Successful: $SUCCESS / $TOTAL${NC}"
    echo -e "Failed: $FAILED / $TOTAL"
    echo "Avg duration (success): ${AVG_DURATION}s"
    echo ""
    
    # Throughput calculation
    TOTAL_TIME=$(($(date +%s) - $(stat -c %Y "$RESULTS_DIR/results.txt" 2>/dev/null || echo $(date +%s))))
    if [ $TOTAL_TIME -gt 0 ]; then
        THROUGHPUT=$(echo "scale=2; $SUCCESS * 60 / $TOTAL_TIME" | bc 2>/dev/null || echo "N/A")
        echo "Throughput: $THROUGHPUT tasks/min"
    fi
else
    echo "No results found"
fi

echo ""
echo "Logs: $RESULTS_DIR/"
echo ""
