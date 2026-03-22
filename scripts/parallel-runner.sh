#!/bin/bash
# Parallel Selfware Runner
# Runs 4 different tasks with 4 concurrent instances each (16 total instances)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SELFWARE_BIN="$PROJECT_DIR/target/release/selfware"
OUTPUT_DIR="$PROJECT_DIR/parallel_results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RUN_DIR="$OUTPUT_DIR/run_$TIMESTAMP"

# Ensure binary exists
if [[ ! -f "$SELFWARE_BIN" ]]; then
    echo "Error: selfware binary not found at $SELFWARE_BIN"
    echo "Run: cargo build --release"
    exit 1
fi

# Create output directories
mkdir -p "$RUN_DIR"/{task_a,task_b,task_c,task_d}/{instance_1,instance_2,instance_3,instance_4}

# Define 4 different task types based on TASKS.md and codebase improvement areas
TASK_A="Analyze and optimize the memory hierarchy in src/cognitive/memory_hierarchy.rs - look for caching inefficiencies and suggest improvements"
TASK_B="Review and improve error handling in src/agent/execution.rs - identify panics and convert to proper error propagation"
TASK_C="Check async/await usage across the codebase for blocking operations and suggest fixes using block_in_place where appropriate"
TASK_D="Review the swarm orchestration in src/orchestration/swarm.rs for task lifecycle issues and suggest fixes"

echo "=========================================="
echo "  Parallel Selfware Runner"
echo "=========================================="
echo "Timestamp: $TIMESTAMP"
echo "Output: $RUN_DIR"
echo ""
echo "Task A (4 instances): Memory Hierarchy Optimization"
echo "Task B (4 instances): Error Handling Improvements"
echo "Task C (4 instances): Async/Blocking I/O Review"
echo "Task D (4 instances): Swarm Task Lifecycle"
echo ""
echo "Starting 16 parallel instances..."
echo "=========================================="

# Function to run a single instance
run_instance() {
    local task_name=$1
    local task_desc=$2
    local instance_id=$3
    local output_file="$RUN_DIR/$task_name/instance_$instance_id/output.log"
    local error_file="$RUN_DIR/$task_name/instance_$instance_id/error.log"
    
    echo "[$(date +%H:%M:%S)] Starting $task_name instance $instance_id"
    
    # Run selfware with yolo mode for automation
    "$SELFWARE_BIN" \
        --config "$PROJECT_DIR/selfware.toml" \
        --mode yolo \
        --workdir "$PROJECT_DIR" \
        run "$task_desc" \
        > "$output_file" 2> "$error_file" &
    
    local pid=$!
    echo $pid > "$RUN_DIR/$task_name/instance_$instance_id/pid"
    echo "[$(date +%H:%M:%S)] $task_name instance $instance_id started (PID: $pid)"
}

# Launch all 16 instances in parallel
for i in 1 2 3 4; do
    run_instance "task_a" "$TASK_A" "$i"
    run_instance "task_b" "$TASK_B" "$i"
    run_instance "task_c" "$TASK_C" "$i"
    run_instance "task_d" "$TASK_D" "$i"
done

echo ""
echo "All 16 instances launched!"
echo ""

# Create monitor script
cat > "$RUN_DIR/monitor.sh" << 'EOF'
#!/bin/bash
RUN_DIR="$(dirname "$0")"
echo "=========================================="
echo "  Parallel Run Monitor"
echo "=========================================="
echo ""

for task in task_a task_b task_c task_d; do
    echo "=== $task ==="
    for i in 1 2 3 4; do
        pid_file="$RUN_DIR/$task/instance_$i/pid"
        if [[ -f "$pid_file" ]]; then
            pid=$(cat "$pid_file" 2>/dev/null)
            if kill -0 "$pid" 2>/dev/null; then
                echo "  instance_$i: RUNNING (PID: $pid)"
            else
                exit_code=$(cat "$RUN_DIR/$task/instance_$i/exit_code" 2>/dev/null || echo "unknown")
                echo "  instance_$i: FINISHED (exit: $exit_code)"
            fi
        else
            echo "  instance_$i: NOT STARTED"
        fi
    done
    echo ""
done

echo "=========================================="
echo "Log files: $RUN_DIR/{task}/instance_{n}/output.log"
echo "=========================================="
EOF
chmod +x "$RUN_DIR/monitor.sh"

# Create wait script
cat > "$RUN_DIR/wait.sh" << EOF
#!/bin/bash
RUN_DIR="\$(dirname "\$0")"
echo "Waiting for all instances to complete..."

wait_for_completion() {
    while true; do
        running=0
        for task in task_a task_b task_c task_d; do
            for i in 1 2 3 4; do
                pid_file="\$RUN_DIR/\$task/instance_\$i/pid"
                if [[ -f "\$pid_file" ]]; then
                    pid=\$(cat "\$pid_file" 2>/dev/null)
                    if kill -0 "\$pid" 2>/dev/null; then
                        running=\$((running + 1))
                    fi
                fi
            done
        done
        
        if [[ \$running -eq 0 ]]; then
            echo "All instances completed!"
            break
        fi
        
        echo "  \$running instances still running..."
        sleep 10
    done
}

wait_for_completion
echo ""
echo "Generating summary report..."
\$RUN_DIR/summary.sh
EOF
chmod +x "$RUN_DIR/wait.sh"

# Create summary script
cat > "$RUN_DIR/summary.sh" << 'EOF'
#!/bin/bash
RUN_DIR="$(dirname "$0")"
SUMMARY_FILE="$RUN_DIR/summary_report.md"

echo "# Parallel Selfware Run Summary" > "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"
echo "**Date:** $(date)" >> "$SUMMARY_FILE"
echo "" >> "$SUMMARY_FILE"

for task in task_a task_b task_c task_d; do
    echo "## $task" >> "$SUMMARY_FILE"
    echo "" >> "$SUMMARY_FILE"
    
    for i in 1 2 3 4; do
        instance_dir="$RUN_DIR/$task/instance_$i"
        output_file="$instance_dir/output.log"
        error_file="$instance_dir/error.log"
        
        echo "### Instance $i" >> "$SUMMARY_FILE"
        echo "" >> "$SUMMARY_FILE"
        
        # Check if output exists and has content
        if [[ -f "$output_file" ]]; then
            lines=$(wc -l < "$output_file" 2>/dev/null || echo "0")
            size=$(du -h "$output_file" 2>/dev/null | cut -f1 || echo "0")
            echo "- **Status:** Completed" >> "$SUMMARY_FILE"
            echo "- **Output lines:** $lines" >> "$SUMMARY_FILE"
            echo "- **Output size:** $size" >> "$SUMMARY_FILE"
            
            # Extract key findings (last 50 lines)
            echo "" >> "$SUMMARY_FILE"
            echo "**Key output (last 30 lines):**" >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
            tail -30 "$output_file" 2>/dev/null >> "$SUMMARY_FILE" || echo "(no output)" >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
        else
            echo "- **Status:** No output file" >> "$SUMMARY_FILE"
        fi
        
        # Check for errors
        if [[ -f "$error_file" && -s "$error_file" ]]; then
            echo "" >> "$SUMMARY_FILE"
            echo "**Errors:**" >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
            cat "$error_file" >> "$SUMMARY_FILE"
            echo '```' >> "$SUMMARY_FILE"
        fi
        
        echo "" >> "$SUMMARY_FILE"
    done
done

echo "Summary report saved to: $SUMMARY_FILE"
cat "$SUMMARY_FILE"
EOF
chmod +x "$RUN_DIR/summary.sh"

# Create stop script
cat > "$RUN_DIR/stop.sh" << 'EOF'
#!/bin/bash
RUN_DIR="$(dirname "$0")"
echo "Stopping all running instances..."

for task in task_a task_b task_c task_d; do
    for i in 1 2 3 4; do
        pid_file="$RUN_DIR/$task/instance_$i/pid"
        if [[ -f "$pid_file" ]]; then
            pid=$(cat "$pid_file" 2>/dev/null)
            if kill -0 "$pid" 2>/dev/null; then
                echo "  Stopping $task instance $i (PID: $pid)"
                kill "$pid" 2>/dev/null || true
            fi
        fi
    done
done

echo "All instances stopped."
EOF
chmod +x "$RUN_DIR/stop.sh"

echo "=========================================="
echo "  Parallel Runner Started!"
echo "=========================================="
echo ""
echo "Run directory: $RUN_DIR"
echo ""
echo "Useful commands:"
echo "  $RUN_DIR/monitor.sh    - Check status of all instances"
echo "  $RUN_DIR/wait.sh       - Wait for completion and generate report"
echo "  $RUN_DIR/summary.sh    - Generate summary report"
echo "  $RUN_DIR/stop.sh       - Stop all instances"
echo ""
echo "To wait for completion and see results:"
echo "  $RUN_DIR/wait.sh"
