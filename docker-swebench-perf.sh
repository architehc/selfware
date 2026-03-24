#!/bin/bash
# SWE-bench Pro Performance Test - 4 Docker Instances
# Benchmarks selfware on real software engineering tasks

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/swebench_perf_results"
mkdir -p "$RESULTS_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date +%H:%M:%S)]${NC} $1"; }
log_ok() { echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $1"; }
log_warn() { echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $1"; }
log_info() { echo -e "${CYAN}[$(date +%H:%M:%S)] ℹ${NC} $1"; }

# SWE-bench task categories for each instance
# Each instance gets a different type of task to test various capabilities

# Instance 1: Code Generation & Bug Fixes (Python)
TASKS_INSTANCE_1='[
  {"id": "django-1234", "repo": "django/django", "problem": "Fix QuerySet.filter() to handle Q objects correctly", "difficulty": "medium"},
  {"id": "pandas-5678", "repo": "pandas-dev/pandas", "problem": "Fix DataFrame.merge() index handling", "difficulty": "hard"},
  {"id": "requests-9012", "repo": "psf/requests", "problem": "Add timeout parameter to Session", "difficulty": "easy"}
]'

# Instance 2: API Design & Refactoring
TASKS_INSTANCE_2='[
  {"id": "flask-3456", "repo": "pallets/flask", "problem": "Refactor routing to support async handlers", "difficulty": "hard"},
  {"id": "fastapi-7890", "repo": "tiangolo/fastapi", "problem": "Add WebSocket middleware support", "difficulty": "medium"},
  {"id": "pytest-1234", "repo": "pytest-dev/pytest", "problem": "Implement parallel test execution", "difficulty": "hard"}
]'

# Instance 3: Testing & Validation
TASKS_INSTANCE_3='[
  {"id": "scikit-5678", "repo": "scikit-learn/scikit-learn", "problem": "Fix cross-validation scoring", "difficulty": "medium"},
  {"id": "numpy-9012", "repo": "numpy/numpy", "problem": "Add type stubs for array operations", "difficulty": "easy"},
  {"id": "matplotlib-3456", "repo": "matplotlib/matplotlib", "problem": "Fix figure rendering in headless mode", "difficulty": "medium"}
]'

# Instance 4: Documentation & Optimization
TASKS_INSTANCE_4='[
  {"id": "tensorflow-7890", "repo": "tensorflow/tensorflow", "problem": "Optimize gradient computation for sparse tensors", "difficulty": "hard"},
  {"id": "pytorch-1234", "repo": "pytorch/pytorch", "problem": "Add JIT compilation for custom ops", "difficulty": "hard"},
  {"id": "transformers-5678", "repo": "huggingface/transformers", "problem": "Implement attention caching for generation", "difficulty": "medium"}
]'

# Save tasks to files
save_tasks() {
    echo "$TASKS_INSTANCE_1" > "$RESULTS_DIR/tasks_instance_1.json"
    echo "$TASKS_INSTANCE_2" > "$RESULTS_DIR/tasks_instance_2.json"
    echo "$TASKS_INSTANCE_3" > "$RESULTS_DIR/tasks_instance_3.json"
    echo "$TASKS_INSTANCE_4" > "$RESULTS_DIR/tasks_instance_4.json"
    log_ok "Task files created"
}

# Run SWE-bench evaluation for a single instance
run_swebench_instance() {
    local instance_id=$1
    local task_file="$RESULTS_DIR/tasks_instance_${instance_id}.json"
    local output_dir="$RESULTS_DIR/instance_${instance_id}"
    local log_file="$output_dir/run.log"
    
    mkdir -p "$output_dir"
    
    log "Starting instance-${instance_id} with $(cat "$task_file" | grep -c '"id"') tasks"
    
    local start_time=$(date +%s.%N)
    
    # Run containerized selfware with SWE-bench
    docker run --rm \
        --name "swebench-${instance_id}" \
        --network host \
        -e SELFWARE_ENDPOINT=https://crazyshit.ngrok.io/v1 \
        -e SELFWARE_MODEL=txn545/Qwen3.5-122B-A10B-NVFP4 \
        -e SELFWARE_MAX_TOKENS=131072 \
        -e SELFWARE_TEMPERATURE=0.2 \
        -e SELFWARE_TIMEOUT=600 \
        -v "$RESULTS_DIR:/results" \
        -v "$task_file:/tasks.json:ro" \
        selfware:latest \
        swe-bench -f /tasks.json -o "/results/instance_${instance_id}/results.json" 2>&1 | tee "$log_file" &
    
    local pid=$!
    echo $pid > "$output_dir/pid"
    
    log_info "instance-${instance_id} started (PID: $pid)"
}

# Wait for all instances with progress monitoring
wait_for_all_instances() {
    log "Monitoring 4 concurrent instances..."
    
    local all_done=false
    local iterations=0
    local max_iterations=300  # 5 minutes max
    
    while [ $all_done = false ] && [ $iterations -lt $max_iterations ]; do
        all_done=true
        clear
        echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
        echo -e "${CYAN}║     SWE-bench Performance Test - Running 4 Instances        ║${NC}"
        echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
        echo ""
        
        for i in 1 2 3 4; do
            local pid_file="$RESULTS_DIR/instance_${i}/pid"
            local log_file="$RESULTS_DIR/instance_${i}/run.log"
            
            if [ -f "$pid_file" ]; then
                local pid=$(cat "$pid_file" 2>/dev/null || echo "?")
                if kill -0 "$pid" 2>/dev/null; then
                    all_done=false
                    local status="${YELLOW}RUNNING${NC}"
                    local progress="$(tail -5 "$log_file" 2>/dev/null | grep -E '(Processing|Completed|tokens)' | tail -1 || echo '...')"
                else
                    local status="${GREEN}DONE${NC}"
                    local progress="Completed"
                fi
                
                echo -e "  Instance-${i}: $status - $progress"
            fi
        done
        
        echo ""
        echo "Elapsed: ${iterations}s | Model: txn545/Qwen3.5-122B-A10B-NVFP4"
        echo "Press Ctrl+C to stop early"
        
        sleep 1
        iterations=$((iterations + 1))
    done
    
    echo ""
    log_ok "All instances completed"
}

# Collect and analyze results
collect_results() {
    log "Collecting results from all instances..."
    
    local report_file="$RESULTS_DIR/swebench_report.md"
    local json_summary="$RESULTS_DIR/summary.json"
    
    # Start JSON summary
    echo '{"instances": [' > "$json_summary"
    
    cat > "$report_file" << 'EOF'
# SWE-bench Pro Performance Report

## Test Overview

**Model**: txn545/Qwen3.5-122B-A10B-NVFP4  
**Endpoint**: https://crazyshit.ngrok.io/v1  
**Test Type**: 4-Instance Concurrent SWE-bench Evaluation  
**Timestamp**: $(date)

## Instance Configuration

| Instance | Focus Area | Tasks | Max Tokens | Temp |
|----------|-----------|-------|------------|------|
| 1 | Code Generation & Bug Fixes | 3 | 131072 | 0.2 |
| 2 | API Design & Refactoring | 3 | 131072 | 0.2 |
| 3 | Testing & Validation | 3 | 131072 | 0.2 |
| 4 | Documentation & Optimization | 3 | 131072 | 0.2 |

## Results by Instance

EOF

    local first=true
    for i in 1 2 3 4; do
        local result_file="$RESULTS_DIR/instance_${i}/results.json"
        local log_file="$RESULTS_DIR/instance_${i}/run.log"
        
        echo -e "\n### Instance-${i}\n" >> "$report_file"
        
        if [ -f "$result_file" ]; then
            # Extract metrics from result file
            local tasks_completed=$(grep -c '"status": "completed"' "$result_file" 2>/dev/null || echo "0")
            local tasks_failed=$(grep -c '"status": "failed"' "$result_file" 2>/dev/null || echo "0")
            local total_tasks=$((tasks_completed + tasks_failed))
            
            echo "- **Tasks**: $total_tasks total ($tasks_completed completed, $tasks_failed failed)" >> "$report_file"
            
            # Extract timing if available
            if grep -q '"duration_ms"' "$result_file" 2>/dev/null; then
                local avg_time=$(grep -o '"duration_ms": [0-9]*' "$result_file" | awk '{sum+=$2; count++} END {print sum/count/1000 "s"}')
                echo "- **Avg Time**: $avg_time" >> "$report_file"
            fi
            
            # Add to JSON
            if [ "$first" = true ]; then
                first=false
            else
                echo "," >> "$json_summary"
            fi
            echo -n "{\"instance\": $i, \"completed\": $tasks_completed, \"failed\": $tasks_failed}" >> "$json_summary"
            
        else
            echo "- **Status**: No results file found" >> "$report_file"
        fi
        
        # Add log excerpt
        echo -e "\n**Log Excerpt**:\n" >> "$report_file"
        echo '```' >> "$report_file"
        tail -10 "$log_file" 2>/dev/null | head -20 >> "$report_file" || echo "(no log available)" >> "$report_file"
        echo '```' >> "$report_file"
    done
    
    # Close JSON
    echo ']}' >> "$json_summary"
    
    # Add benchmark comparison
    cat >> "$report_file" << 'EOF'

## Performance Benchmarks

| Metric | Target | Instance-1 | Instance-2 | Instance-3 | Instance-4 |
|--------|--------|------------|------------|------------|------------|
| Tasks/Hour | >6 | TBD | TBD | TBD | TBD |
| Success Rate | >70% | TBD | TBD | TBD | TBD |
| Avg Tokens/sec | >30 | TBD | TBD | TBD | TBD |
| Cost/Task | <$0.50 | TBD | TBD | TBD | TBD |

## Feature Implementation Plan

Based on results, the following features will be prioritized:

1. **Auto-scaling**: Adjust concurrent instances based on endpoint capacity
2. **Task Routing**: Route tasks to instances based on complexity
3. **Checkpointing**: Save progress every N tokens for long tasks
4. **Result Aggregation**: Merge results from all instances

## Next Steps

- [ ] Analyze token usage patterns
- [ ] Implement caching for similar tasks
- [ ] Add real-time metrics dashboard
- [ ] Optimize prompts based on task type

EOF

    log_ok "Report generated: $report_file"
    log_info "JSON summary: $json_summary"
}

# Cleanup function
cleanup() {
    log "Cleaning up containers..."
    for i in 1 2 3 4; do
        docker rm -f "swebench-${i}" 2>/dev/null || true
    done
}

# Main execution
main() {
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║   SWE-bench Pro Performance Test - 4 Concurrent Instances   ║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    
    # Setup
    cleanup
    save_tasks
    
    # Start all 4 instances
    log "Launching 4 instances with different task categories..."
    run_swebench_instance 1
    run_swebench_instance 2
    run_swebench_instance 3
    run_swebench_instance 4
    
    # Wait and monitor
    wait_for_all_instances
    
    # Collect results
    collect_results
    
    # Summary
    echo ""
    echo -e "${GREEN}✓ SWE-bench performance test complete!${NC}"
    echo ""
    echo -e "Results location: ${CYAN}$RESULTS_DIR/${NC}"
    echo "  - Markdown report: swebench_report.md"
    echo "  - JSON summary: summary.json"
    echo "  - Instance logs: instance_*/run.log"
    echo ""
    
    # Quick stats
    echo "Quick Stats:"
    for i in 1 2 3 4; do
        local result_file="$RESULTS_DIR/instance_${i}/results.json"
        if [ -f "$result_file" ]; then
            local size=$(du -h "$result_file" 2>/dev/null | cut -f1)
            echo "  Instance-${i}: $size of results"
        else
            echo "  Instance-${i}: No results"
        fi
    done
}

# Signal handlers
trap cleanup EXIT INT TERM

main "$@"
