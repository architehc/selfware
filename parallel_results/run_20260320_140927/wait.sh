#!/bin/bash
RUN_DIR="$(dirname "$0")"
echo "Waiting for all instances to complete..."

wait_for_completion() {
    while true; do
        running=0
        for task in task_a task_b task_c task_d; do
            for i in 1 2 3 4; do
                pid_file="$RUN_DIR/$task/instance_$i/pid"
                if [[ -f "$pid_file" ]]; then
                    pid=$(cat "$pid_file" 2>/dev/null)
                    if kill -0 "$pid" 2>/dev/null; then
                        running=$((running + 1))
                    fi
                fi
            done
        done
        
        if [[ $running -eq 0 ]]; then
            echo "All instances completed!"
            break
        fi
        
        echo "  $running instances still running..."
        sleep 10
    done
}

wait_for_completion
echo ""
echo "Generating summary report..."
$RUN_DIR/summary.sh
