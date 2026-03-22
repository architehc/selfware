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
